use awrk_datex::codec::decode::{DecodeConfig, decode_value_full};
use awrk_datex::codec::tags;
use awrk_datex::value::SerializedValueRef;
use awrk_datex::{WireError, WireError::Malformed};

#[test]
fn array_iter_reads_values_and_finish_ok() {
    let mut buf = vec![tags::TAG_ARRAY];
    buf.extend_from_slice(&(1u32).to_be_bytes());
    buf.extend_from_slice(&(1u32).to_be_bytes());
    buf.push(tags::TAG_BOOL_TRUE);

    let v = decode_value_full(&buf, DecodeConfig::default()).expect("decode");
    let a = v.as_array().expect("array");

    let mut it = a.iter();
    assert_eq!(
        it.next().transpose().unwrap(),
        Some(SerializedValueRef::Bool(true))
    );
    assert!(it.next().is_none());
    it.finish().unwrap();
}

#[test]
fn array_finish_rejects_trailing_bytes_in_payload() {
    // One element, but payload contains two values: Bool(true) then Bool(false).
    let mut buf = vec![tags::TAG_ARRAY];
    buf.extend_from_slice(&(1u32).to_be_bytes());
    buf.extend_from_slice(&(2u32).to_be_bytes());
    buf.push(tags::TAG_BOOL_TRUE);
    buf.push(tags::TAG_BOOL_FALSE);

    let v = decode_value_full(&buf, DecodeConfig::default()).expect("decode");
    let a = v.as_array().expect("array");

    let mut it = a.iter();
    assert_eq!(
        it.next().transpose().unwrap(),
        Some(SerializedValueRef::Bool(true))
    );
    assert!(it.next().is_none());
    let err = it.finish().expect_err("must reject trailing bytes");
    assert_eq!(err, Malformed("trailing bytes in container payload"));
}

#[test]
fn map_iter_reads_pairs_and_finish_ok() {
    let mut entry = Vec::new();
    entry.push(tags::TAG_U64);
    entry.extend_from_slice(&5u64.to_be_bytes());
    entry.push(tags::TAG_BOOL_TRUE);

    let mut buf = vec![tags::TAG_MAP];
    buf.extend_from_slice(&(1u32).to_be_bytes());
    buf.extend_from_slice(&(entry.len() as u32).to_be_bytes());
    buf.extend_from_slice(&entry);

    let v = decode_value_full(&buf, DecodeConfig::default()).expect("decode");
    let m = v.as_map().expect("map");

    let mut it = m.iter_pairs();
    assert_eq!(
        it.next().transpose().unwrap(),
        Some((SerializedValueRef::U64(5), SerializedValueRef::Bool(true)))
    );
    assert!(it.next().is_none());
    it.finish().unwrap();
}

#[test]
fn map_finish_rejects_trailing_bytes_in_payload() {
    // One entry, but payload contains an extra value at the end.
    let mut entry = Vec::new();
    entry.push(tags::TAG_U64);
    entry.extend_from_slice(&1u64.to_be_bytes());
    entry.push(tags::TAG_BOOL_TRUE);
    entry.push(tags::TAG_BOOL_FALSE);

    let mut buf = vec![tags::TAG_MAP];
    buf.extend_from_slice(&(1u32).to_be_bytes());
    buf.extend_from_slice(&(entry.len() as u32).to_be_bytes());
    buf.extend_from_slice(&entry);

    let v = decode_value_full(&buf, DecodeConfig::default()).expect("decode");
    let m = v.as_map().expect("map");

    let mut it = m.iter_pairs();
    assert_eq!(
        it.next().transpose().unwrap(),
        Some((SerializedValueRef::U64(1), SerializedValueRef::Bool(true)))
    );
    assert!(it.next().is_none());
    let err = it.finish().expect_err("must reject trailing bytes");
    assert_eq!(err, Malformed("trailing bytes in container payload"));
}

#[test]
fn value_kind_and_as_helpers() {
    let v = SerializedValueRef::U8(7);
    assert_eq!(v.as_u64(), Some(7));
    assert_eq!(v.as_i64(), None);
    assert_eq!(v.as_str(), None);
    assert_eq!(v.as_bytes(), None);

    let s = SerializedValueRef::String("hi");
    assert_eq!(s.as_str(), Some("hi"));
    assert_eq!(s.as_u64(), None);

    let b = SerializedValueRef::Bytes(&[1, 2]);
    assert_eq!(b.as_bytes(), Some(&[1, 2][..]));

    assert_eq!(
        WireError::InvalidUtf8.to_string(),
        "invalid utf-8 string data"
    );
}
