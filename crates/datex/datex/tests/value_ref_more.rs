use awrk_datex::codec::decode::{DecodeConfig, decode_value_full};
use awrk_datex::codec::tags;
use awrk_datex::value::{SerializedValueRef, ValueKind};

#[test]
fn empty_array_and_map_are_supported() {
    // Empty array: count=0, payload_len=0.
    let mut buf = vec![tags::TAG_ARRAY];
    buf.extend_from_slice(&(0u32).to_be_bytes());
    buf.extend_from_slice(&(0u32).to_be_bytes());

    let v = decode_value_full(&buf, DecodeConfig::default()).expect("decode array");
    let a = v.as_array().expect("array");
    assert!(a.is_empty());
    assert_eq!(a.len(), 0);
    let mut it = a.iter();
    assert!(it.next().is_none());
    it.finish().unwrap();

    // Empty map: count=0, payload_len=0.
    let mut buf2 = vec![tags::TAG_MAP];
    buf2.extend_from_slice(&(0u32).to_be_bytes());
    buf2.extend_from_slice(&(0u32).to_be_bytes());

    let v2 = decode_value_full(&buf2, DecodeConfig::default()).expect("decode map");
    let m = v2.as_map().expect("map");
    assert!(m.is_empty());
    assert_eq!(m.len(), 0);
    let mut it = m.iter_pairs();
    assert!(it.next().is_none());
    it.finish().unwrap();
}

#[test]
fn value_kind_covers_all_variants() {
    let scalars = [
        (SerializedValueRef::Null, ValueKind::Null),
        (SerializedValueRef::Unit, ValueKind::Unit),
        (SerializedValueRef::Bool(true), ValueKind::Bool),
        (SerializedValueRef::U8(1), ValueKind::U8),
        (SerializedValueRef::U16(2), ValueKind::U16),
        (SerializedValueRef::U32(3), ValueKind::U32),
        (SerializedValueRef::U64(4), ValueKind::U64),
        (SerializedValueRef::I8(-1), ValueKind::I8),
        (SerializedValueRef::I16(-2), ValueKind::I16),
        (SerializedValueRef::I32(-3), ValueKind::I32),
        (SerializedValueRef::I64(-4), ValueKind::I64),
        (SerializedValueRef::F32(1.0), ValueKind::F32),
        (SerializedValueRef::F64(2.0), ValueKind::F64),
        (SerializedValueRef::String("s"), ValueKind::String),
        (SerializedValueRef::Bytes(&[1]), ValueKind::Bytes),
    ];

    for (v, k) in scalars {
        assert_eq!(v.kind(), k);
    }

    // Array and map kinds via decoded empty containers.
    let mut array_buf = vec![tags::TAG_ARRAY];
    array_buf.extend_from_slice(&(0u32).to_be_bytes());
    array_buf.extend_from_slice(&(0u32).to_be_bytes());
    let a = decode_value_full(&array_buf, DecodeConfig::default()).expect("decode");
    assert_eq!(a.kind(), ValueKind::Array);

    let mut map_buf = vec![tags::TAG_MAP];
    map_buf.extend_from_slice(&(0u32).to_be_bytes());
    map_buf.extend_from_slice(&(0u32).to_be_bytes());
    let m = decode_value_full(&map_buf, DecodeConfig::default()).expect("decode");
    assert_eq!(m.kind(), ValueKind::Map);
}
