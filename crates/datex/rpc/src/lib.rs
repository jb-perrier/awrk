use awrk_datex::codec::decode::{DecodeConfig, decode_value};
use awrk_datex::codec::encode::{EncodeConfig, Encoder};
use awrk_datex::error::{Result, WireError};
use awrk_datex::value::SerializedValueRef;

mod registry;

pub use registry::{RpcRegistry, RpcRegistryWithCtx};

pub const MSG_INVOKE: u8 = 0x72;
pub const MSG_INVOKE_RESULT: u8 = 0x73;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RpcProcId(pub u64);

#[derive(Debug, Clone, PartialEq)]
pub enum RpcEnvelopeRef<'a> {
    Invoke {
        id: u64,
        proc_id: RpcProcId,
        args: SerializedValueRef<'a>,
    },
    InvokeResult {
        id: u64,
        ok: bool,
        value: SerializedValueRef<'a>,
    },
}

pub fn decode_envelope<'a>(buf: &'a [u8], config: DecodeConfig) -> Result<RpcEnvelopeRef<'a>> {
    let tag = *buf.first().ok_or(WireError::UnexpectedEof)?;
    let mut cursor = 1usize;

    let message = match tag {
        MSG_INVOKE => {
            let id = read_u64(buf, &mut cursor)?;
            let proc_id = RpcProcId(read_u64(buf, &mut cursor)?);
            let args_buf = buf.get(cursor..).ok_or(WireError::UnexpectedEof)?;
            let (args, used) = decode_value(args_buf, config)?;
            cursor += used;
            RpcEnvelopeRef::Invoke { id, proc_id, args }
        }
        MSG_INVOKE_RESULT => {
            let id = read_u64(buf, &mut cursor)?;
            let ok = match *buf.get(cursor).ok_or(WireError::UnexpectedEof)? {
                0 => false,
                1 => true,
                _ => return Err(WireError::Malformed("invoke_result ok flag must be 0 or 1")),
            };
            cursor += 1;

            let value_buf = buf.get(cursor..).ok_or(WireError::UnexpectedEof)?;
            let (value, used) = decode_value(value_buf, config)?;
            cursor += used;

            RpcEnvelopeRef::InvokeResult { id, ok, value }
        }
        _ => return Err(WireError::InvalidTag(tag)),
    };

    if cursor != buf.len() {
        return Err(WireError::Malformed("trailing bytes after envelope"));
    }

    Ok(message)
}

pub fn encode_invoke<F>(id: u64, proc_id: RpcProcId, f: F) -> Result<Vec<u8>>
where
    F: FnOnce(&mut Encoder) -> Result<()>,
{
    encode_invoke_with_config(id, proc_id, EncodeConfig::default(), f)
}

pub fn encode_invoke_with_config<F>(
    id: u64,
    proc_id: RpcProcId,
    config: EncodeConfig,
    f: F,
) -> Result<Vec<u8>>
where
    F: FnOnce(&mut Encoder) -> Result<()>,
{
    let mut buf = Vec::new();
    buf.push(MSG_INVOKE);
    buf.extend_from_slice(&id.to_be_bytes());
    buf.extend_from_slice(&proc_id.0.to_be_bytes());

    let mut enc = Encoder::from_vec(buf, config);
    f(&mut enc)?;
    Ok(enc.into_inner())
}

pub fn encode_invoke_result<F>(id: u64, ok: bool, f: F) -> Result<Vec<u8>>
where
    F: FnOnce(&mut Encoder) -> Result<()>,
{
    encode_invoke_result_with_config(id, ok, EncodeConfig::default(), f)
}

pub fn encode_invoke_result_with_config<F>(
    id: u64,
    ok: bool,
    config: EncodeConfig,
    f: F,
) -> Result<Vec<u8>>
where
    F: FnOnce(&mut Encoder) -> Result<()>,
{
    let mut buf = Vec::new();
    buf.push(MSG_INVOKE_RESULT);
    buf.extend_from_slice(&id.to_be_bytes());
    buf.push(if ok { 1 } else { 0 });

    let mut enc = Encoder::from_vec(buf, config);
    f(&mut enc)?;
    Ok(enc.into_inner())
}

fn read_u64(buf: &[u8], cursor: &mut usize) -> Result<u64> {
    let end = cursor.checked_add(8).ok_or(WireError::LengthOverflow)?;
    let bytes = buf.get(*cursor..end).ok_or(WireError::UnexpectedEof)?;
    *cursor = end;
    Ok(u64::from_be_bytes(bytes.try_into().unwrap()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RpcRegistry, RpcRegistryWithCtx};

    #[test]
    fn decode_invoke_envelope() {
        let proc_id = RpcProcId(42);
        let mut args = vec![awrk_datex::codec::tags::TAG_U64];
        args.extend_from_slice(&7u64.to_be_bytes());

        let mut buf = vec![MSG_INVOKE];
        buf.extend_from_slice(&5u64.to_be_bytes());
        buf.extend_from_slice(&proc_id.0.to_be_bytes());
        buf.extend_from_slice(&args);

        let envelope = decode_envelope(&buf, DecodeConfig::default()).expect("decode envelope");
        assert_eq!(
            envelope,
            RpcEnvelopeRef::Invoke {
                id: 5,
                proc_id,
                args: SerializedValueRef::U64(7),
            }
        );
    }

    #[test]
    fn encode_invoke_envelope_roundtrip() {
        let proc_id = RpcProcId(42);
        let buf = encode_invoke(5, proc_id, |enc| {
            enc.u64(7);
            Ok(())
        })
        .expect("encode invoke");

        let decoded = decode_envelope(&buf, DecodeConfig::default()).expect("decode envelope");
        assert_eq!(
            decoded,
            RpcEnvelopeRef::Invoke {
                id: 5,
                proc_id,
                args: SerializedValueRef::U8(7),
            }
        );
    }

    #[test]
    fn registry_get_schema_returns_schema_bytes() {
        let mut reg = RpcRegistry::new();
        reg.register_typed::<u64, u64, _>("demo.echo", |x| Ok(x));

        let buf = encode_invoke(
            5,
            RpcProcId(awrk_datex_schema::PROC_ID_GET_SCHEMA.0),
            |enc| {
                enc.unit();
                Ok(())
            },
        )
        .expect("encode invoke");

        let env = decode_envelope(&buf, DecodeConfig::default()).expect("decode invoke");
        let result_buf = reg
            .handle_envelope(env, EncodeConfig::default())
            .expect("handle invoke");

        let env = decode_envelope(&result_buf, DecodeConfig::default()).expect("decode result");
        let RpcEnvelopeRef::InvokeResult { ok, value, .. } = env else {
            panic!("expected invoke_result");
        };
        assert!(ok);

        let schema_bytes = value.as_bytes().expect("bytes payload");
        assert_eq!(&schema_bytes[0..8], b"UPISCHM2");
        assert_eq!(
            u32::from_be_bytes(schema_bytes[8..12].try_into().unwrap()),
            2
        );

        let schema = awrk_datex_schema::decode_schema(schema_bytes).expect("decode schema");
        assert!(
            schema
                .procedures
                .contains_key(&awrk_datex_schema::PROC_ID_GET_SCHEMA)
        );
        assert!(
            schema
                .procedures
                .contains_key(&awrk_datex_schema::proc_id("demo.echo"))
        );
    }

    #[test]
    fn registry_unknown_proc_returns_error_result() {
        let reg = RpcRegistry::new();
        let buf = encode_invoke(5, RpcProcId(999), |enc| {
            enc.unit();
            Ok(())
        })
        .expect("encode invoke");

        let env = decode_envelope(&buf, DecodeConfig::default()).expect("decode invoke");
        let result_buf = reg
            .handle_envelope(env, EncodeConfig::default())
            .expect("handle invoke");
        let env = decode_envelope(&result_buf, DecodeConfig::default()).expect("decode result");
        let RpcEnvelopeRef::InvokeResult { ok, value, .. } = env else {
            panic!("expected invoke_result");
        };
        assert!(!ok);
        assert_eq!(value.as_str().unwrap(), "unknown procedure");
    }

    #[test]
    fn registry_with_ctx_can_mutate_context() {
        #[derive(Default)]
        struct Ctx {
            hits: u64,
        }

        let mut reg: RpcRegistryWithCtx<Ctx> = RpcRegistryWithCtx::new();
        reg.register_typed::<u64, u64, _>("demo.bump", |ctx, x| {
            ctx.hits += 1;
            Ok(x + ctx.hits)
        });

        let buf = encode_invoke(
            5,
            RpcProcId(awrk_datex_schema::proc_id("demo.bump").0),
            |enc| {
                enc.u64(10);
                Ok(())
            },
        )
        .expect("encode invoke");

        let env = decode_envelope(&buf, DecodeConfig::default()).expect("decode invoke");
        let mut ctx = Ctx::default();
        let result_buf = reg
            .handle_envelope(&mut ctx, env, EncodeConfig::default())
            .expect("handle invoke");

        assert_eq!(ctx.hits, 1);

        let env = decode_envelope(&result_buf, DecodeConfig::default()).expect("decode result");
        let RpcEnvelopeRef::InvokeResult { ok, value, .. } = env else {
            panic!("expected invoke_result");
        };
        assert!(ok);
        assert_eq!(value.as_u64().unwrap(), 11);
    }
}
