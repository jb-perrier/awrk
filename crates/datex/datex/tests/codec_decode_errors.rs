use awrk_datex::codec::decode::{DecodeConfig, decode_value_full};
use awrk_datex::codec::tags;
use awrk_datex::{WireError, WireError::Malformed};

#[test]
fn decode_value_full_rejects_trailing_bytes() {
    let buf = [tags::TAG_BOOL_TRUE, 0];
    let err = decode_value_full(&buf, DecodeConfig::default()).expect_err("trailing bytes");
    assert_eq!(err, Malformed("trailing bytes after value"));
}

#[test]
fn decode_rejects_invalid_tag() {
    let buf = [0xFF];
    let err = decode_value_full(&buf, DecodeConfig::default()).expect_err("invalid tag");
    assert_eq!(err, WireError::InvalidTag(0xFF));
}

#[test]
fn decode_rejects_invalid_utf8_string() {
    let mut buf = vec![tags::TAG_STRING];
    buf.extend_from_slice(&(2u32).to_be_bytes());
    buf.extend_from_slice(&[0xFF, 0xFF]);
    let err = decode_value_full(&buf, DecodeConfig::default()).expect_err("invalid utf8");
    assert_eq!(err, WireError::InvalidUtf8);
}

#[test]
fn decode_array_rejects_payload_len_out_of_bounds() {
    // ARRAY(count=0, payload_len=1) but no payload bytes.
    let mut buf = vec![tags::TAG_ARRAY];
    buf.extend_from_slice(&(0u32).to_be_bytes());
    buf.extend_from_slice(&(1u32).to_be_bytes());

    let err = decode_value_full(&buf, DecodeConfig::default()).expect_err("payload out of bounds");
    assert_eq!(err, WireError::UnexpectedEof);
}

#[test]
fn decode_map_rejects_payload_len_out_of_bounds() {
    // MAP(count=0, payload_len=1) but no payload bytes.
    let mut buf = vec![tags::TAG_MAP];
    buf.extend_from_slice(&(0u32).to_be_bytes());
    buf.extend_from_slice(&(1u32).to_be_bytes());

    let err = decode_value_full(&buf, DecodeConfig::default()).expect_err("payload out of bounds");
    assert_eq!(err, WireError::UnexpectedEof);
}

#[test]
fn array_iter_finish_rejects_not_enough_elements() {
    // ARRAY(count=2, payload=[true]).
    let mut buf = vec![tags::TAG_ARRAY];
    buf.extend_from_slice(&(2u32).to_be_bytes());
    buf.extend_from_slice(&(1u32).to_be_bytes());
    buf.push(tags::TAG_BOOL_TRUE);

    let v = decode_value_full(&buf, DecodeConfig::default()).expect("decode array header");
    let a = v.as_array().expect("array");
    let mut it = a.iter();
    assert_eq!(
        it.next().unwrap().unwrap(),
        awrk_datex::value::SerializedValueRef::Bool(true)
    );
    let err = it.finish().expect_err("not enough elements");
    assert_eq!(err, Malformed("not enough elements in container"));
}

#[test]
fn array_iter_finish_rejects_trailing_payload_bytes() {
    // ARRAY(count=1, payload=[true, 0x00]).
    let mut buf = vec![tags::TAG_ARRAY];
    buf.extend_from_slice(&(1u32).to_be_bytes());
    buf.extend_from_slice(&(2u32).to_be_bytes());
    buf.push(tags::TAG_BOOL_TRUE);
    buf.push(0);

    let v = decode_value_full(&buf, DecodeConfig::default()).expect("decode array header");
    let a = v.as_array().expect("array");
    let mut it = a.iter();
    assert_eq!(
        it.next().unwrap().unwrap(),
        awrk_datex::value::SerializedValueRef::Bool(true)
    );
    assert!(it.next().is_none());
    let err = it.finish().expect_err("trailing bytes");
    assert_eq!(err, Malformed("trailing bytes in container payload"));
}

#[test]
fn map_iter_rejects_missing_value_after_key() {
    // MAP(count=1, payload=[true]) => missing value.
    let mut buf = vec![tags::TAG_MAP];
    buf.extend_from_slice(&(1u32).to_be_bytes());
    buf.extend_from_slice(&(1u32).to_be_bytes());
    buf.push(tags::TAG_BOOL_TRUE);

    let v = decode_value_full(&buf, DecodeConfig::default()).expect("decode map header");
    let m = v.as_map().expect("map");
    let mut it = m.iter_pairs();
    let err = it.next().unwrap().unwrap_err();
    assert_eq!(err, Malformed("missing value after map key"));
}

#[test]
fn recursion_limit_is_enforced_on_lazy_container_access() {
    // array with one bool element; decode ok at top-level, but element decode fails when depth=0.
    let mut buf = vec![tags::TAG_ARRAY];
    buf.extend_from_slice(&(1u32).to_be_bytes());
    buf.extend_from_slice(&(1u32).to_be_bytes());
    buf.push(tags::TAG_BOOL_TRUE);

    let v = decode_value_full(&buf, DecodeConfig { max_depth: 1 }).expect("decode array");
    let a = v.as_array().expect("array");
    let err = a
        .iter()
        .next()
        .expect("one element")
        .expect_err("depth exceeded in element decode");
    assert_eq!(err, WireError::RecursionLimitExceeded);
}
