use awrk_datex::codec::decode::{DecodeConfig, decode_value_full};
use awrk_datex::codec::encode::{EncodeConfig, Encoder};
use awrk_datex::codec::tags;
use awrk_datex::value::SerializedValueRef;

#[test]
fn encoder_misc_helpers_are_exercised() {
    let mut enc = Encoder::new();
    enc.reserve(128);
    enc.bool(true);

    let bytes = enc.into_inner();
    assert!(!bytes.is_empty());

    // from_vec appends to an existing buffer
    let mut enc2 = Encoder::from_vec(vec![0xAA], EncodeConfig::default());
    enc2.u8(1);
    assert_eq!(enc2.as_slice()[0], 0xAA);
}

#[test]
fn seq_writer_typed_methods_all_work() {
    let mut enc = Encoder::new();

    enc.array(12, |w| {
        w.bool(true)?;
        w.u8(1)?;
        w.u16(2)?;
        w.u32(3)?;
        w.u64(4)?;
        w.i8(-1)?;
        w.i16(-2)?;
        w.i32(-3)?;
        w.i64(-4)?;
        w.f32(1.25)?;
        w.f64(-2.5)?;
        w.string("hi")?;
        Ok(())
    })
    .expect("encode array");

    let value = decode_value_full(enc.as_slice(), DecodeConfig::default()).expect("decode");
    let a = value.as_array().expect("array");
    assert_eq!(a.len(), 12);

    let mut got = Vec::new();
    let mut it = a.iter();
    while let Some(v) = it.next() {
        got.push(v.unwrap());
    }
    it.finish().unwrap();

    assert_eq!(
        got,
        vec![
            SerializedValueRef::Bool(true),
            SerializedValueRef::U8(1),
            SerializedValueRef::U8(2),
            SerializedValueRef::U8(3),
            SerializedValueRef::U8(4),
            SerializedValueRef::I8(-1),
            SerializedValueRef::I8(-2),
            SerializedValueRef::I8(-3),
            SerializedValueRef::I8(-4),
            SerializedValueRef::F32(1.25),
            SerializedValueRef::F64(-2.5),
            SerializedValueRef::String("hi"),
        ]
    );
}

#[test]
fn seq_writer_value_allows_manual_encoder_use() {
    let mut enc = Encoder::new();

    enc.array(1, |w| {
        w.value(|enc| {
            enc.bytes(&[1, 2, 3])?;
            Ok(())
        })
    })
    .expect("encode array");

    let value = decode_value_full(enc.as_slice(), DecodeConfig::default()).expect("decode");
    let a = value.as_array().expect("array");
    let mut it = a.iter();
    assert_eq!(
        it.next().transpose().unwrap(),
        Some(SerializedValueRef::Bytes(&[1, 2, 3]))
    );
    assert!(it.next().is_none());
    it.finish().unwrap();
}

#[test]
fn array_layout_is_count_payload_len_payload() {
    let mut enc = Encoder::new();
    enc.array(2, |w| {
        w.bool(true)?;
        w.bool(false)?;
        Ok(())
    })
    .expect("encode array");

    let bytes = enc.as_slice();

    // Layout (compact): TAG_ARRAY_LEN8 + count:u8 + payload_len:u8 + payload
    assert_eq!(bytes[0], tags::TAG_ARRAY_LEN8);
    assert_eq!(bytes[1], 2);
    assert_eq!(bytes[2], 2);
    assert_eq!(bytes[3], tags::TAG_BOOL_TRUE);
    assert_eq!(bytes[4], tags::TAG_BOOL_FALSE);
}

#[test]
fn encoder_chooses_smallest_len_variant_for_string_and_bytes() {
    let mut enc = Encoder::new();
    enc.string("hello").unwrap();
    let bytes = enc.into_inner();
    assert_eq!(bytes[0], tags::TAG_STRING_LEN8);
    assert_eq!(bytes[1], 5);

    let mut enc = Encoder::new();
    let s = "a".repeat(300);
    enc.string(&s).unwrap();
    let bytes = enc.into_inner();
    assert_eq!(bytes[0], tags::TAG_STRING_LEN16);
    assert_eq!(u16::from_be_bytes(bytes[1..3].try_into().unwrap()), 300);

    let mut enc = Encoder::new();
    let big = vec![0u8; 70_000];
    enc.bytes(&big).unwrap();
    let bytes = enc.into_inner();
    assert_eq!(bytes[0], tags::TAG_BYTES);
    assert_eq!(u32::from_be_bytes(bytes[1..5].try_into().unwrap()), 70_000);
}

#[test]
fn encoder_chooses_smallest_len_variant_for_array_and_map_headers() {
    // ARRAY_LEN16 due to payload_len > u8::MAX.
    let mut enc = Encoder::new();
    enc.array(1, |w| w.bytes(&vec![0u8; 300]))
        .expect("encode array");
    let bytes = enc.as_slice();
    assert_eq!(bytes[0], tags::TAG_ARRAY_LEN16);
    assert_eq!(u16::from_be_bytes(bytes[1..3].try_into().unwrap()), 1);

    // MAP_LEN16 due to payload_len > u8::MAX.
    let mut enc = Encoder::new();
    enc.map(1, |m| {
        m.entry(|e| e.string("k"), |e| e.bytes(&vec![0u8; 300]))
    })
    .expect("encode map");
    let bytes = enc.as_slice();
    assert_eq!(bytes[0], tags::TAG_MAP_LEN16);
    assert_eq!(u16::from_be_bytes(bytes[1..3].try_into().unwrap()), 1);

    // ARRAY_LEN32 due to payload_len > u16::MAX.
    let mut enc = Encoder::new();
    enc.array(1, |w| w.bytes(&vec![0u8; 70_000]))
        .expect("encode array");
    let bytes = enc.as_slice();
    assert_eq!(bytes[0], tags::TAG_ARRAY);
    assert_eq!(u32::from_be_bytes(bytes[1..5].try_into().unwrap()), 1);
}
