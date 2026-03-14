use awrk_datex::codec::decode::{DecodeConfig, decode_value_full};
use awrk_datex::codec::encode::{EncodeConfig, Encoder, encode_value};
use awrk_datex::value::SerializedValueRef;
use awrk_datex::{Decode, Encode, Patch, PatchValidate};

fn roundtrip_value(value: &SerializedValueRef<'_>) -> SerializedValueRef<'static> {
    let bytes = encode_value(value).expect("encode_value");
    let decoded = decode_value_full(&bytes, DecodeConfig::default()).expect("decode");
    // Make it owned so the return doesn't borrow `bytes`.
    match decoded {
        SerializedValueRef::Null => SerializedValueRef::Null,
        SerializedValueRef::Unit => SerializedValueRef::Unit,
        SerializedValueRef::Bool(v) => SerializedValueRef::Bool(v),
        SerializedValueRef::U8(v) => SerializedValueRef::U8(v),
        SerializedValueRef::U16(v) => SerializedValueRef::U16(v),
        SerializedValueRef::U32(v) => SerializedValueRef::U32(v),
        SerializedValueRef::U64(v) => SerializedValueRef::U64(v),
        SerializedValueRef::I8(v) => SerializedValueRef::I8(v),
        SerializedValueRef::I16(v) => SerializedValueRef::I16(v),
        SerializedValueRef::I32(v) => SerializedValueRef::I32(v),
        SerializedValueRef::I64(v) => SerializedValueRef::I64(v),
        SerializedValueRef::F32(v) => SerializedValueRef::F32(v),
        SerializedValueRef::F64(v) => SerializedValueRef::F64(v),
        SerializedValueRef::String(s) => {
            SerializedValueRef::String(Box::leak(s.to_string().into_boxed_str()))
        }
        SerializedValueRef::Bytes(b) => {
            SerializedValueRef::Bytes(Box::leak(b.to_vec().into_boxed_slice()))
        }
        SerializedValueRef::Array(_) | SerializedValueRef::Map(_) => {
            panic!("array/map are tested separately")
        }
    }
}

#[test]
fn encode_value_ref_roundtrips_all_scalar_variants() {
    assert_eq!(
        roundtrip_value(&SerializedValueRef::Null),
        SerializedValueRef::Null
    );
    assert_eq!(
        roundtrip_value(&SerializedValueRef::Unit),
        SerializedValueRef::Unit
    );

    assert_eq!(
        roundtrip_value(&SerializedValueRef::Bool(false)),
        SerializedValueRef::Bool(false)
    );
    assert_eq!(
        roundtrip_value(&SerializedValueRef::Bool(true)),
        SerializedValueRef::Bool(true)
    );

    assert_eq!(
        roundtrip_value(&SerializedValueRef::U8(1)),
        SerializedValueRef::U8(1)
    );
    assert_eq!(
        roundtrip_value(&SerializedValueRef::U16(2)),
        SerializedValueRef::U16(2)
    );
    assert_eq!(
        roundtrip_value(&SerializedValueRef::U32(3)),
        SerializedValueRef::U32(3)
    );
    assert_eq!(
        roundtrip_value(&SerializedValueRef::U64(4)),
        SerializedValueRef::U64(4)
    );

    assert_eq!(
        roundtrip_value(&SerializedValueRef::I8(-1)),
        SerializedValueRef::I8(-1)
    );
    assert_eq!(
        roundtrip_value(&SerializedValueRef::I16(-2)),
        SerializedValueRef::I16(-2)
    );
    assert_eq!(
        roundtrip_value(&SerializedValueRef::I32(-3)),
        SerializedValueRef::I32(-3)
    );
    assert_eq!(
        roundtrip_value(&SerializedValueRef::I64(-4)),
        SerializedValueRef::I64(-4)
    );

    assert_eq!(
        roundtrip_value(&SerializedValueRef::F32(1.25)),
        SerializedValueRef::F32(1.25)
    );
    assert_eq!(
        roundtrip_value(&SerializedValueRef::F64(-2.5)),
        SerializedValueRef::F64(-2.5)
    );

    assert_eq!(
        roundtrip_value(&SerializedValueRef::String("hi")),
        SerializedValueRef::String("hi")
    );
    assert_eq!(
        roundtrip_value(&SerializedValueRef::Bytes(&[1u8, 2u8, 3u8])),
        SerializedValueRef::Bytes(&[1u8, 2u8, 3u8])
    );
}

#[test]
fn encoder_value_ref_hits_array_raw_and_map_raw_paths() {
    // First build an array + map using the structured writers.
    let mut enc = Encoder::new();
    enc.array(2, |w| {
        w.bool(true)?;
        w.u64(9)?;
        Ok(())
    })
    .expect("encode array");

    let array_value = decode_value_full(enc.as_slice(), DecodeConfig::default()).expect("decode");
    let SerializedValueRef::Array(array_ref) = array_value else {
        panic!("expected array")
    };

    let mut enc2 = Encoder::new();
    enc2.value_ref(&SerializedValueRef::Array(array_ref))
        .expect("encode raw array");
    let _ = decode_value_full(enc2.as_slice(), DecodeConfig::default()).expect("decode raw");

    let mut enc3 = Encoder::new();
    enc3.map(2, |w| {
        w.entry(
            |enc| {
                enc.u64(1);
                Ok(())
            },
            |enc| {
                enc.string("a")?;
                Ok(())
            },
        )?;
        w.entry(
            |enc| {
                enc.u64(2);
                Ok(())
            },
            |enc| {
                enc.bytes(&[9, 8])?;
                Ok(())
            },
        )?;
        Ok(())
    })
    .expect("encode map");

    let map_value =
        decode_value_full(enc3.as_slice(), DecodeConfig::default()).expect("decode map");
    let SerializedValueRef::Map(map_ref) = map_value else {
        panic!("expected map")
    };

    let mut enc4 = Encoder::new();
    enc4.value_ref(&SerializedValueRef::Map(map_ref))
        .expect("encode raw map");
    let _ = decode_value_full(enc4.as_slice(), DecodeConfig::default()).expect("decode raw map");
}

#[test]
fn encoder_recursion_limit_can_be_enforced() {
    let mut enc = Encoder::with_config(EncodeConfig {
        max_depth: 0,
        ..Default::default()
    });
    let err = enc
        .value_ref(&SerializedValueRef::Bool(true))
        .expect_err("depth=0 must fail");
    assert_eq!(err.to_string(), "value nesting limit exceeded");
}

#[test]
fn traits_more_roundtrips_and_patches() {
    // Encode for str/String and bytes.
    let mut enc = Encoder::new();
    "hello".wire_encode(&mut enc).expect("encode str");
    let v = decode_value_full(enc.as_slice(), DecodeConfig::default()).expect("decode");
    let s: String = String::wire_decode(v).expect("decode String");
    assert_eq!(s, "hello".to_string());

    let mut enc2 = Encoder::new();
    let bytes: Vec<u8> = vec![1, 2, 3];
    bytes.wire_encode(&mut enc2).expect("encode bytes vec");
    let v2 = decode_value_full(enc2.as_slice(), DecodeConfig::default()).expect("decode");
    let decoded: Vec<u8> = Vec::<u8>::wire_decode(v2).expect("decode bytes");
    assert_eq!(decoded, vec![1, 2, 3]);

    // Patch/validate for floats.
    let mut f: f32 = 0.0;
    f.wire_patch_validate(SerializedValueRef::F32(1.5))
        .expect("validate f32");
    f.wire_patch(SerializedValueRef::F32(1.5))
        .expect("patch f32");
    assert_eq!(f, 1.5);

    let mut d: f64 = 0.0;
    d.wire_patch_validate(SerializedValueRef::F64(-1.25))
        .expect("validate f64");
    d.wire_patch(SerializedValueRef::F64(-1.25))
        .expect("patch f64");
    assert_eq!(d, -1.25);

    // Unsigned decode accepts smaller widths.
    let u: u16 = <u16 as Decode>::wire_decode(SerializedValueRef::U8(7)).expect("u16 from u8");
    assert_eq!(u, 7);
}
