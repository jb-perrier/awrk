use awrk_datex::codec::decode::{DecodeConfig, decode_value_full};
use awrk_datex::codec::tags;
use awrk_datex::value::SerializedValueRef;

#[test]
fn decode_array_and_iter() {
    let mut value0 = vec![tags::TAG_U64];
    value0.extend_from_slice(&1u64.to_be_bytes());
    let mut value1 = vec![tags::TAG_U64];
    value1.extend_from_slice(&2u64.to_be_bytes());

    let mut payload = Vec::new();
    payload.extend_from_slice(&value0);
    payload.extend_from_slice(&value1);

    let mut buf = vec![tags::TAG_ARRAY];
    buf.extend_from_slice(&(2u32).to_be_bytes());
    buf.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    buf.extend_from_slice(&payload);

    let decoded = decode_value_full(&buf, DecodeConfig::default()).expect("decode array");
    let array = decoded.as_array().expect("array");
    assert_eq!(array.len(), 2);

    let mut it = array.iter();
    assert_eq!(
        it.next().transpose().expect("v0"),
        Some(SerializedValueRef::U64(1))
    );
    assert_eq!(
        it.next().transpose().expect("v1"),
        Some(SerializedValueRef::U64(2))
    );
    assert!(it.next().is_none());
    it.finish().expect("finish");
}

#[test]
fn decode_array_allows_heterogeneous_items() {
    let mut value0 = vec![tags::TAG_U64];
    value0.extend_from_slice(&1u64.to_be_bytes());
    let mut value1 = vec![tags::TAG_STRING];
    value1.extend_from_slice(&(2u32).to_be_bytes());
    value1.extend_from_slice(b"ok");

    let mut payload = Vec::new();
    payload.extend_from_slice(&value0);
    payload.extend_from_slice(&value1);

    let mut buf = vec![tags::TAG_ARRAY];
    buf.extend_from_slice(&(2u32).to_be_bytes());
    buf.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    buf.extend_from_slice(&payload);

    let decoded = decode_value_full(&buf, DecodeConfig::default()).expect("should decode");
    let array = decoded.as_array().expect("array");

    let mut it = array.iter();
    assert_eq!(
        it.next().transpose().expect("v0"),
        Some(SerializedValueRef::U64(1))
    );
    assert_eq!(
        it.next().transpose().expect("v1"),
        Some(SerializedValueRef::String("ok"))
    );
    assert!(it.next().is_none());
    it.finish().expect("finish");
}
