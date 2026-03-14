use awrk_datex::codec::decode::{DecodeConfig, decode_value_full};
use awrk_datex::codec::tags;
use awrk_datex::value::SerializedValueRef;

#[test]
fn decode_string_scalar() {
    let mut buf = vec![tags::TAG_STRING];
    buf.extend_from_slice(&(5u32).to_be_bytes());
    buf.extend_from_slice(b"hello");

    let value = decode_value_full(&buf, DecodeConfig::default()).expect("decode string");
    assert_eq!(value, SerializedValueRef::String("hello"));
}

#[test]
fn decode_string_len8_scalar() {
    let mut buf = vec![tags::TAG_STRING_LEN8];
    buf.push(5u8);
    buf.extend_from_slice(b"hello");

    let value = decode_value_full(&buf, DecodeConfig::default()).expect("decode string");
    assert_eq!(value, SerializedValueRef::String("hello"));
}

#[test]
fn decode_bytes_len16_scalar() {
    let payload = vec![0xABu8; 300];
    let mut buf = vec![tags::TAG_BYTES_LEN16];
    buf.extend_from_slice(&(300u16).to_be_bytes());
    buf.extend_from_slice(&payload);

    let value = decode_value_full(&buf, DecodeConfig::default()).expect("decode bytes");
    assert_eq!(value, SerializedValueRef::Bytes(&payload));
}

#[test]
fn decode_other_integer_widths() {
    let buf = [tags::TAG_U8, 250];
    let value = decode_value_full(&buf, DecodeConfig::default()).expect("decode u8");
    assert_eq!(value, SerializedValueRef::U8(250));

    let buf = [tags::TAG_I8, 0x80];
    let value = decode_value_full(&buf, DecodeConfig::default()).expect("decode i8");
    assert_eq!(value, SerializedValueRef::I8(-128));

    let mut buf = vec![tags::TAG_U16];
    buf.extend_from_slice(&500u16.to_be_bytes());
    let value = decode_value_full(&buf, DecodeConfig::default()).expect("decode u16");
    assert_eq!(value, SerializedValueRef::U16(500));

    let mut buf = vec![tags::TAG_I16];
    buf.extend_from_slice(&(-1234i16).to_be_bytes());
    let value = decode_value_full(&buf, DecodeConfig::default()).expect("decode i16");
    assert_eq!(value, SerializedValueRef::I16(-1234));

    let mut buf = vec![tags::TAG_U32];
    buf.extend_from_slice(&1_000_000u32.to_be_bytes());
    let value = decode_value_full(&buf, DecodeConfig::default()).expect("decode u32");
    assert_eq!(value, SerializedValueRef::U32(1_000_000));

    let mut buf = vec![tags::TAG_I32];
    buf.extend_from_slice(&(-9_999i32).to_be_bytes());
    let value = decode_value_full(&buf, DecodeConfig::default()).expect("decode i32");
    assert_eq!(value, SerializedValueRef::I32(-9_999));
}
