use awrk_datex::codec::Encoder;
use awrk_datex::codec::decode::{DecodeConfig, decode_value_full};
use awrk_datex::value::SerializedValueRef;
use awrk_datex::{Decode, Patch, PatchValidate, WireError};

#[test]
fn wire_decode_conversions_for_int_families() {
    // Unsigned conversions from smaller widths.
    let a: u32 = <u32 as Decode>::wire_decode(SerializedValueRef::U16(10)).expect("u32 from u16");
    assert_eq!(a, 10);

    // Signed conversions from smaller widths.
    let b: i32 = <i32 as Decode>::wire_decode(SerializedValueRef::I16(-10)).expect("i32 from i16");
    assert_eq!(b, -10);
}

#[test]
fn wire_patch_validate_then_patch_for_string_and_bytes_vec_decode() {
    let mut s = "old".to_string();
    s.wire_patch_validate(SerializedValueRef::String("new"))
        .expect("validate");
    s.wire_patch(SerializedValueRef::String("new"))
        .expect("patch");
    assert_eq!(s, "new".to_string());

    // Vec<u8> decode expects Bytes, not Array.
    let mut enc = Encoder::new();
    enc.bytes(&[9, 8]).expect("encode bytes");
    let v = decode_value_full(enc.as_slice(), DecodeConfig::default()).expect("decode");
    let b: Vec<u8> = Vec::<u8>::wire_decode(v).expect("decode vec bytes");
    assert_eq!(b, vec![9, 8]);
}

#[test]
fn vec_u8_patch_uses_array_semantics() {
    let mut v: Vec<u8> = vec![1, 2];

    // Patch is an array of u8, not bytes.
    let mut enc = Encoder::new();
    enc.array(3, |w| {
        w.u8(7)?;
        w.u8(8)?;
        w.u8(9)?;
        Ok(())
    })
    .expect("encode");

    let patch = decode_value_full(enc.as_slice(), DecodeConfig::default()).expect("decode");
    v.wire_patch_validate(patch.clone()).expect("validate");
    v.wire_patch(patch).expect("patch");
    assert_eq!(v, vec![7, 8, 9]);
}

#[test]
fn decode_type_mismatch_messages_for_more_primitives() {
    let err = <u16 as Decode>::wire_decode(SerializedValueRef::Bool(true)).expect_err("mismatch");
    assert_eq!(err, WireError::Malformed("expected unsigned int"));

    let err = <i16 as Decode>::wire_decode(SerializedValueRef::U64(1)).expect_err("mismatch");
    assert_eq!(err, WireError::Malformed("expected signed int"));
}
