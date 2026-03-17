use std::collections::BTreeMap;

use awrk_datex::codec::encode::{EncodeConfig, Encoder};
use awrk_datex::error::Result;
use awrk_datex::value::SerializedValueRef;
use awrk_datex::{Decode, Encode, WireError};

use crate::{RpcEnvelopeRef, RpcProcId, encode_invoke_result_with_config};

type Handler = Box<
    dyn for<'a> Fn(SerializedValueRef<'a>, &mut Encoder) -> std::result::Result<(), String>
        + Send
        + Sync,
>;

type HandlerWithCtx<C> = Box<
    dyn for<'a> Fn(&mut C, SerializedValueRef<'a>, &mut Encoder) -> std::result::Result<(), String>
        + Send
        + Sync,
>;

pub struct RpcRegistry {
    schema: awrk_datex_schema::SchemaBuilder,
    handlers: BTreeMap<RpcProcId, Handler>,
    proc_names: BTreeMap<RpcProcId, String>,
}

pub struct RpcRegistryWithCtx<C> {
    schema: awrk_datex_schema::SchemaBuilder,
    handlers: BTreeMap<RpcProcId, HandlerWithCtx<C>>,
    proc_names: BTreeMap<RpcProcId, String>,
}

impl RpcRegistry {
    pub fn new() -> Self {
        let mut this = Self {
            schema: awrk_datex_schema::SchemaBuilder::new(),
            handlers: BTreeMap::new(),
            proc_names: BTreeMap::new(),
        };
        this.register_builtin_get_schema();
        this
    }

    pub fn schema_builder_mut(&mut self) -> &mut awrk_datex_schema::SchemaBuilder {
        &mut self.schema
    }

    pub fn register_type<T: awrk_datex_schema::Schema>(&mut self) -> awrk_datex_schema::TypeId {
        <T as awrk_datex_schema::Schema>::wire_schema(&mut self.schema)
    }

    pub fn register_typed<A, R, F>(&mut self, name: &str, f: F) -> RpcProcId
    where
        for<'a> A: Decode<'a> + awrk_datex_schema::Schema,
        R: Encode + awrk_datex_schema::Schema,
        F: Fn(A) -> std::result::Result<R, String> + Send + Sync + 'static,
    {
        let proc_id = RpcProcId(awrk_datex_schema::proc_id(name).0);
        self.ensure_proc_name_available(proc_id, name);

        let args_type = <A as awrk_datex_schema::Schema>::wire_schema(&mut self.schema);
        let result_type = <R as awrk_datex_schema::Schema>::wire_schema(&mut self.schema);
        self.schema.register_proc(name, args_type, result_type);

        self.proc_names.insert(proc_id, name.to_string());

        self.handlers.insert(
            proc_id,
            Box::new(move |args, enc| {
                let decoded = A::wire_decode(args).map_err(|e| format!("{e}"))?;
                let result = f(decoded)?;
                result.wire_encode(enc).map_err(|e| format!("{e}"))?;
                Ok(())
            }),
        );

        proc_id
    }

    pub fn handle_envelope<'a>(
        &self,
        env: RpcEnvelopeRef<'a>,
        config: EncodeConfig,
    ) -> Result<Vec<u8>> {
        match env {
            RpcEnvelopeRef::Invoke { id, proc_id, args } => {
                self.handle_invoke(id, proc_id, args, config)
            }
            RpcEnvelopeRef::InvokeResult { .. } => {
                Err(WireError::Malformed("cannot handle InvokeResult"))
            }
        }
    }

    pub fn handle_invoke<'a>(
        &self,
        id: u64,
        proc_id: RpcProcId,
        args: SerializedValueRef<'a>,
        config: EncodeConfig,
    ) -> Result<Vec<u8>> {
        if proc_id.0 == awrk_datex_schema::PROC_ID_GET_SCHEMA.0 {
            let schema_bytes = match self.schema_bytes() {
                Ok(b) => b,
                Err(e) => {
                    return encode_invoke_result_with_config(id, false, config, |enc| {
                        enc.string(&e)?;
                        Ok(())
                    });
                }
            };

            let _ = args;
            return encode_invoke_result_with_config(id, true, config, |enc| {
                enc.bytes(&schema_bytes)?;
                Ok(())
            });
        }

        let Some(handler) = self.handlers.get(&proc_id) else {
            return encode_invoke_result_with_config(id, false, config, |enc| {
                enc.string("unknown procedure")?;
                Ok(())
            });
        };

        let mut handler_error: Option<String> = None;
        let ok_buf =
            encode_invoke_result_with_config(id, true, config, |enc| match handler(args, enc) {
                Ok(()) => Ok(()),
                Err(msg) => {
                    handler_error = Some(msg);
                    Err(WireError::Malformed("handler error"))
                }
            });

        if let Some(msg) = handler_error {
            return encode_invoke_result_with_config(id, false, config, |enc| {
                enc.string(&msg)?;
                Ok(())
            });
        }

        ok_buf
    }

    pub fn schema_bytes(&self) -> std::result::Result<Vec<u8>, String> {
        let schema = self.schema.build_clone()?;
        schema.encode().map_err(|e| format!("{e}"))
    }

    pub fn schema_snapshot(&self) -> std::result::Result<awrk_datex_schema::OwnedSchema, String> {
        self.schema.build_clone()
    }

    fn register_builtin_get_schema(&mut self) {
        let args_type = <() as awrk_datex_schema::Schema>::wire_schema(&mut self.schema);
        let result_type = <Vec<u8> as awrk_datex_schema::Schema>::wire_schema(&mut self.schema);

        self.schema.register_proc_with_id(
            awrk_datex_schema::PROC_ID_GET_SCHEMA,
            "awrk.get_schema",
            args_type,
            result_type,
        );
        self.proc_names.insert(
            RpcProcId(awrk_datex_schema::PROC_ID_GET_SCHEMA.0),
            "awrk.get_schema".to_string(),
        );
    }

    fn ensure_proc_name_available(&self, proc_id: RpcProcId, name: &str) {
        if let Some(existing) = self.proc_names.get(&proc_id) {
            assert!(
                existing == name,
                "duplicate RPC procedure id for {name}: already registered as {existing}"
            );
        }
    }
}

impl<C> RpcRegistryWithCtx<C> {
    pub fn new() -> Self {
        let mut this = Self {
            schema: awrk_datex_schema::SchemaBuilder::new(),
            handlers: BTreeMap::new(),
            proc_names: BTreeMap::new(),
        };
        this.register_builtin_get_schema();
        this
    }

    pub fn schema_builder_mut(&mut self) -> &mut awrk_datex_schema::SchemaBuilder {
        &mut self.schema
    }

    pub fn register_type<T: awrk_datex_schema::Schema>(&mut self) -> awrk_datex_schema::TypeId {
        <T as awrk_datex_schema::Schema>::wire_schema(&mut self.schema)
    }

    pub fn register_typed<A, R, F>(&mut self, name: &str, f: F) -> RpcProcId
    where
        for<'a> A: Decode<'a> + awrk_datex_schema::Schema,
        R: Encode + awrk_datex_schema::Schema,
        F: Fn(&mut C, A) -> std::result::Result<R, String> + Send + Sync + 'static,
    {
        let proc_id = RpcProcId(awrk_datex_schema::proc_id(name).0);
        self.ensure_proc_name_available(proc_id, name);

        let args_type = <A as awrk_datex_schema::Schema>::wire_schema(&mut self.schema);
        let result_type = <R as awrk_datex_schema::Schema>::wire_schema(&mut self.schema);
        self.schema.register_proc(name, args_type, result_type);

        self.proc_names.insert(proc_id, name.to_string());

        self.handlers.insert(
            proc_id,
            Box::new(move |ctx, args, enc| {
                let decoded = A::wire_decode(args).map_err(|e| format!("{e}"))?;
                let result = f(ctx, decoded)?;
                result.wire_encode(enc).map_err(|e| format!("{e}"))?;
                Ok(())
            }),
        );

        proc_id
    }

    pub fn handle_envelope<'a>(
        &self,
        ctx: &mut C,
        env: crate::RpcEnvelopeRef<'a>,
        config: EncodeConfig,
    ) -> Result<Vec<u8>> {
        match env {
            crate::RpcEnvelopeRef::Invoke { id, proc_id, args } => {
                self.handle_invoke(ctx, id, proc_id, args, config)
            }
            crate::RpcEnvelopeRef::InvokeResult { .. } => {
                Err(WireError::Malformed("cannot handle InvokeResult"))
            }
        }
    }

    pub fn handle_invoke<'a>(
        &self,
        ctx: &mut C,
        id: u64,
        proc_id: RpcProcId,
        args: SerializedValueRef<'a>,
        config: EncodeConfig,
    ) -> Result<Vec<u8>> {
        if proc_id.0 == awrk_datex_schema::PROC_ID_GET_SCHEMA.0 {
            let schema_bytes = match self.schema_bytes() {
                Ok(b) => b,
                Err(e) => {
                    return encode_invoke_result_with_config(id, false, config, |enc| {
                        enc.string(&e)?;
                        Ok(())
                    });
                }
            };

            let _ = args;
            return encode_invoke_result_with_config(id, true, config, |enc| {
                enc.bytes(&schema_bytes)?;
                Ok(())
            });
        }

        let Some(handler) = self.handlers.get(&proc_id) else {
            return encode_invoke_result_with_config(id, false, config, |enc| {
                enc.string("unknown procedure")?;
                Ok(())
            });
        };

        let mut handler_error: Option<String> = None;
        let ok_buf = encode_invoke_result_with_config(id, true, config, |enc| {
            match handler(ctx, args, enc) {
                Ok(()) => Ok(()),
                Err(msg) => {
                    handler_error = Some(msg);
                    Err(WireError::Malformed("handler error"))
                }
            }
        });

        if let Some(msg) = handler_error {
            return encode_invoke_result_with_config(id, false, config, |enc| {
                enc.string(&msg)?;
                Ok(())
            });
        }

        ok_buf
    }

    pub fn schema_bytes(&self) -> std::result::Result<Vec<u8>, String> {
        let schema = self.schema.build_clone()?;
        schema.encode().map_err(|e| format!("{e}"))
    }

    pub fn schema_snapshot(&self) -> std::result::Result<awrk_datex_schema::OwnedSchema, String> {
        self.schema.build_clone()
    }

    fn register_builtin_get_schema(&mut self) {
        let args_type = <() as awrk_datex_schema::Schema>::wire_schema(&mut self.schema);
        let result_type = <Vec<u8> as awrk_datex_schema::Schema>::wire_schema(&mut self.schema);

        self.schema.register_proc_with_id(
            awrk_datex_schema::PROC_ID_GET_SCHEMA,
            "awrk.get_schema",
            args_type,
            result_type,
        );
        self.proc_names.insert(
            RpcProcId(awrk_datex_schema::PROC_ID_GET_SCHEMA.0),
            "awrk.get_schema".to_string(),
        );
    }

    fn ensure_proc_name_available(&self, proc_id: RpcProcId, name: &str) {
        if let Some(existing) = self.proc_names.get(&proc_id) {
            assert!(
                existing == name,
                "duplicate RPC procedure id for {name}: already registered as {existing}"
            );
        }
    }
}
