use awrk_datex::codec::Encoder;
use awrk_datex::codec::decode::{DecodeConfig, decode_value_full};
use awrk_datex::value::SerializedValueRef;
use awrk_datex::{Decode, Encode, Patch, PatchValidate, WireError};

#[test]
fn wire_encode_emits_expected_tags_for_primitives() {
    {
        let mut enc = Encoder::new();
        true.wire_encode(&mut enc).expect("encode");
        let bytes = enc.into_inner();
        let v = decode_value_full(&bytes, DecodeConfig::default()).expect("decode");
        assert_eq!(v, SerializedValueRef::Bool(true));
    }

    {
        let mut enc = Encoder::new();
        1u8.wire_encode(&mut enc).expect("encode");
        let bytes = enc.into_inner();
        let v = decode_value_full(&bytes, DecodeConfig::default()).expect("decode");
        assert_eq!(v, SerializedValueRef::U8(1));
    }
    {
        let mut enc = Encoder::new();
        2u16.wire_encode(&mut enc).expect("encode");
        let bytes = enc.into_inner();
        let v = decode_value_full(&bytes, DecodeConfig::default()).expect("decode");
        assert_eq!(v, SerializedValueRef::U8(2));
    }
    {
        let mut enc = Encoder::new();
        3u32.wire_encode(&mut enc).expect("encode");
        let bytes = enc.into_inner();
        let v = decode_value_full(&bytes, DecodeConfig::default()).expect("decode");
        assert_eq!(v, SerializedValueRef::U8(3));
    }
    {
        let mut enc = Encoder::new();
        4u64.wire_encode(&mut enc).expect("encode");
        let bytes = enc.into_inner();
        let v = decode_value_full(&bytes, DecodeConfig::default()).expect("decode");
        assert_eq!(v, SerializedValueRef::U8(4));
    }

    {
        let mut enc = Encoder::new();
        (-1i8).wire_encode(&mut enc).expect("encode");
        let bytes = enc.into_inner();
        let v = decode_value_full(&bytes, DecodeConfig::default()).expect("decode");
        assert_eq!(v, SerializedValueRef::I8(-1));
    }
    {
        let mut enc = Encoder::new();
        (-2i16).wire_encode(&mut enc).expect("encode");
        let bytes = enc.into_inner();
        let v = decode_value_full(&bytes, DecodeConfig::default()).expect("decode");
        assert_eq!(v, SerializedValueRef::I8(-2));
    }
    {
        let mut enc = Encoder::new();
        (-3i32).wire_encode(&mut enc).expect("encode");
        let bytes = enc.into_inner();
        let v = decode_value_full(&bytes, DecodeConfig::default()).expect("decode");
        assert_eq!(v, SerializedValueRef::I8(-3));
    }
    {
        let mut enc = Encoder::new();
        (-4i64).wire_encode(&mut enc).expect("encode");
        let bytes = enc.into_inner();
        let v = decode_value_full(&bytes, DecodeConfig::default()).expect("decode");
        assert_eq!(v, SerializedValueRef::I8(-4));
    }

    {
        let mut enc = Encoder::new();
        1.25f32.wire_encode(&mut enc).expect("encode");
        let bytes = enc.into_inner();
        let v = decode_value_full(&bytes, DecodeConfig::default()).expect("decode");
        assert_eq!(v, SerializedValueRef::F32(1.25));
    }
    {
        let mut enc = Encoder::new();
        (-2.5f64).wire_encode(&mut enc).expect("encode");
        let bytes = enc.into_inner();
        let v = decode_value_full(&bytes, DecodeConfig::default()).expect("decode");
        assert_eq!(v, SerializedValueRef::F64(-2.5));
    }
}

#[test]
fn more_integer_range_and_mismatch_cases() {
    // Unsigned out of range
    let err = <u16 as Decode>::wire_decode(SerializedValueRef::U64(u16::MAX as u64 + 1))
        .expect_err("oor");
    assert_eq!(err, WireError::Malformed("integer out of range"));

    let err = <u32 as Decode>::wire_decode(SerializedValueRef::U64(u32::MAX as u64 + 1))
        .expect_err("oor");
    assert_eq!(err, WireError::Malformed("integer out of range"));

    // Signed out of range
    let err = <i16 as Decode>::wire_decode(SerializedValueRef::I64(i16::MAX as i64 + 1))
        .expect_err("oor");
    assert_eq!(err, WireError::Malformed("integer out of range"));

    let err = <i32 as Decode>::wire_decode(SerializedValueRef::I64(i32::MIN as i64 - 1))
        .expect_err("oor");
    assert_eq!(err, WireError::Malformed("integer out of range"));

    // Patch uses decode semantics
    let mut x: u32 = 0;
    x.wire_patch_validate(SerializedValueRef::U64(10))
        .expect("validate");
    x.wire_patch(SerializedValueRef::U64(10)).expect("patch");
    assert_eq!(x, 10);

    let mut y: i32 = 0;
    y.wire_patch_validate(SerializedValueRef::I64(-10))
        .expect("validate");
    y.wire_patch(SerializedValueRef::I64(-10)).expect("patch");
    assert_eq!(y, -10);
}

#[test]
fn compact_integer_encoding_can_be_enabled_per_encoder() {
    // u64 gets narrowed on the wire.
    let mut enc = Encoder::with_config(awrk_datex::codec::EncodeConfig {
        compact_ints: true,
        ..Default::default()
    });
    4u64.wire_encode(&mut enc).expect("encode u64");
    let v = decode_value_full(enc.as_slice(), DecodeConfig::default()).expect("decode");
    assert_eq!(v, SerializedValueRef::U8(4));
    let roundtrip: u64 = u64::wire_decode(v).expect("decode u64 from narrowed");
    assert_eq!(roundtrip, 4);

    // u64 that doesn't fit u16 narrows to u32.
    let mut enc2 = Encoder::with_config(awrk_datex::codec::EncodeConfig {
        compact_ints: true,
        ..Default::default()
    });
    70_000u64.wire_encode(&mut enc2).expect("encode u64");
    let v2 = decode_value_full(enc2.as_slice(), DecodeConfig::default()).expect("decode");
    assert_eq!(v2, SerializedValueRef::U32(70_000));
    let roundtrip2: u64 = u64::wire_decode(v2).expect("decode u64 from narrowed");
    assert_eq!(roundtrip2, 70_000);

    // i64 gets narrowed on the wire.
    let mut enc3 = Encoder::with_config(awrk_datex::codec::EncodeConfig {
        compact_ints: true,
        ..Default::default()
    });
    (-1i64).wire_encode(&mut enc3).expect("encode i64");
    let v3 = decode_value_full(enc3.as_slice(), DecodeConfig::default()).expect("decode");
    assert_eq!(v3, SerializedValueRef::I8(-1));
    let roundtrip3: i64 = i64::wire_decode(v3).expect("decode i64 from narrowed");
    assert_eq!(roundtrip3, -1);

    // u32 gets narrowed on the wire.
    let mut enc4 = Encoder::with_config(awrk_datex::codec::EncodeConfig {
        compact_ints: true,
        ..Default::default()
    });
    250u32.wire_encode(&mut enc4).expect("encode u32");
    let v4 = decode_value_full(enc4.as_slice(), DecodeConfig::default()).expect("decode");
    assert_eq!(v4, SerializedValueRef::U8(250));
    let roundtrip4: u32 = u32::wire_decode(v4).expect("decode u32 from narrowed");
    assert_eq!(roundtrip4, 250);

    // u32 that doesn't fit u8 narrows to u16.
    let mut enc5 = Encoder::with_config(awrk_datex::codec::EncodeConfig {
        compact_ints: true,
        ..Default::default()
    });
    60_000u32.wire_encode(&mut enc5).expect("encode u32");
    let v5 = decode_value_full(enc5.as_slice(), DecodeConfig::default()).expect("decode");
    assert_eq!(v5, SerializedValueRef::U16(60_000));
    let roundtrip5: u32 = u32::wire_decode(v5).expect("decode u32 from narrowed");
    assert_eq!(roundtrip5, 60_000);

    // u16 gets narrowed on the wire.
    let mut enc6 = Encoder::with_config(awrk_datex::codec::EncodeConfig {
        compact_ints: true,
        ..Default::default()
    });
    200u16.wire_encode(&mut enc6).expect("encode u16");
    let v6 = decode_value_full(enc6.as_slice(), DecodeConfig::default()).expect("decode");
    assert_eq!(v6, SerializedValueRef::U8(200));
    let roundtrip6: u16 = u16::wire_decode(v6).expect("decode u16 from narrowed");
    assert_eq!(roundtrip6, 200);

    // i32 and i16 get narrowed on the wire.
    let mut enc7 = Encoder::with_config(awrk_datex::codec::EncodeConfig {
        compact_ints: true,
        ..Default::default()
    });
    (-1000i32).wire_encode(&mut enc7).expect("encode i32");
    let v7 = decode_value_full(enc7.as_slice(), DecodeConfig::default()).expect("decode");
    assert_eq!(v7, SerializedValueRef::I16(-1000));
    let roundtrip7: i32 = i32::wire_decode(v7).expect("decode i32 from narrowed");
    assert_eq!(roundtrip7, -1000);

    let mut enc8 = Encoder::with_config(awrk_datex::codec::EncodeConfig {
        compact_ints: true,
        ..Default::default()
    });
    (-1i16).wire_encode(&mut enc8).expect("encode i16");
    let v8 = decode_value_full(enc8.as_slice(), DecodeConfig::default()).expect("decode");
    assert_eq!(v8, SerializedValueRef::I8(-1));
    let roundtrip8: i16 = i16::wire_decode(v8).expect("decode i16 from narrowed");
    assert_eq!(roundtrip8, -1);
}
