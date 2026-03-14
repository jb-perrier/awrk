use awrk_datex::codec::Encoder;
use awrk_datex::codec::decode::{DecodeConfig, decode_value_full};
use awrk_datex::value::SerializedValueRef;
use awrk_datex::{Decode, Encode, Patch, PatchValidate, WireError};

#[test]
fn wire_encode_for_byte_slice_and_decode_vec_u8() {
    let mut enc = Encoder::new();
    let slice: &[u8] = &[1, 2, 3];
    slice.wire_encode(&mut enc).expect("encode [u8]");

    let v = decode_value_full(enc.as_slice(), DecodeConfig::default()).expect("decode");
    let out: Vec<u8> = Vec::<u8>::wire_decode(v).expect("decode bytes");
    assert_eq!(out, vec![1, 2, 3]);
}

#[test]
fn mismatch_errors_for_wide_int_and_float_and_bytes() {
    let err = <u64 as Decode>::wire_decode(SerializedValueRef::I64(1)).expect_err("u64 mismatch");
    assert_eq!(err, WireError::Malformed("expected unsigned int"));

    let err = <i64 as Decode>::wire_decode(SerializedValueRef::U64(1)).expect_err("i64 mismatch");
    assert_eq!(err, WireError::Malformed("expected signed int"));

    let err = <f32 as Decode>::wire_decode(SerializedValueRef::F64(1.0)).expect_err("f32 mismatch");
    assert_eq!(err, WireError::Malformed("expected f32"));

    let err = <f64 as Decode>::wire_decode(SerializedValueRef::F32(1.0)).expect_err("f64 mismatch");
    assert_eq!(err, WireError::Malformed("expected f64"));

    let err =
        <Vec<u8> as Decode>::wire_decode(SerializedValueRef::U64(1)).expect_err("bytes mismatch");
    assert_eq!(err, WireError::Malformed("expected bytes"));
}

#[test]
fn patch_validate_mismatch_errors_for_floats() {
    let f: f32 = 0.0;
    let err = f
        .wire_patch_validate(SerializedValueRef::F64(1.0))
        .expect_err("validate mismatch");
    assert_eq!(err, WireError::Malformed("expected f32"));

    let d: f64 = 0.0;
    let err = d
        .wire_patch_validate(SerializedValueRef::F32(1.0))
        .expect_err("validate mismatch");
    assert_eq!(err, WireError::Malformed("expected f64"));
}

#[test]
fn patch_vec_element_decode_failure_propagates() {
    let mut v: Vec<u16> = vec![1, 2];

    let mut enc = Encoder::new();
    enc.array(1, |w| w.string("nope")).expect("encode");

    let patch = decode_value_full(enc.as_slice(), DecodeConfig::default()).expect("decode");
    let err = v.wire_patch(patch).expect_err("patch must fail");
    assert_eq!(err, WireError::Malformed("expected unsigned int"));
}
