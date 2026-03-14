use awrk_datex::value::SerializedValueRef;
use awrk_datex::{Patch, PatchValidate, WireError};

#[test]
fn patch_u8_out_of_range_is_error() {
    let mut v: u8 = 0;
    let err = v
        .wire_patch(SerializedValueRef::U64(256))
        .expect_err("out of range");
    assert_eq!(err, WireError::Malformed("integer out of range"));
}

#[test]
fn patch_i8_out_of_range_is_error() {
    let mut v: i8 = 0;
    let err = v
        .wire_patch(SerializedValueRef::I64(200))
        .expect_err("out of range");
    assert_eq!(err, WireError::Malformed("integer out of range"));
}

#[test]
fn patch_bool_type_mismatch_is_error() {
    let mut v = false;
    let err = v
        .wire_patch(SerializedValueRef::U64(1))
        .expect_err("type mismatch");
    assert_eq!(err, WireError::Malformed("expected bool"));
}

#[test]
fn validate_then_patch_works_for_primitives() {
    let v: u16 = 0;
    v.wire_patch_validate(SerializedValueRef::U64(42))
        .expect("validate");

    let mut v2: u16 = 0;
    v2.wire_patch(SerializedValueRef::U64(42)).expect("patch");
    assert_eq!(v2, 42);
}

#[test]
fn validate_detects_type_mismatch_for_string() {
    let s = String::new();
    let err = s
        .wire_patch_validate(SerializedValueRef::U64(1))
        .expect_err("string mismatch");
    assert_eq!(err, WireError::Malformed("expected string"));
}
