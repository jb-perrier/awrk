use awrk_datex::codec::decode::{DecodeConfig, decode_value_full};
use awrk_datex::codec::encode::Encoder;
use awrk_datex::value::SerializedValueRef;
use awrk_datex::{Decode, Encode, Patch, PatchValidate};

#[test]
fn unit_roundtrips_via_traits() {
    let mut enc = Encoder::new();
    ().wire_encode(&mut enc).expect("encode unit");

    let v = decode_value_full(enc.as_slice(), DecodeConfig::default()).expect("decode");
    assert_eq!(v, SerializedValueRef::Unit);

    let u: () = <() as Decode>::wire_decode(v).expect("decode unit");
    let _ = u;
}

#[test]
fn option_none_encodes_as_null_and_decodes() {
    let mut enc = Encoder::new();
    let v: Option<u64> = None;
    v.wire_encode(&mut enc).expect("encode option none");

    let decoded = decode_value_full(enc.as_slice(), DecodeConfig::default()).expect("decode");
    assert_eq!(decoded, SerializedValueRef::Null);

    let out: Option<u64> = <Option<u64> as Decode>::wire_decode(decoded).expect("decode");
    assert_eq!(out, None);
}

#[test]
fn option_patch_can_set_and_clear() {
    // Start as None, patch with a concrete value.
    let mut x: Option<u8> = None;

    let mut enc = Encoder::new();
    7u8.wire_encode(&mut enc).expect("encode");
    let patch = decode_value_full(enc.as_slice(), DecodeConfig::default()).expect("decode");

    x.wire_patch(patch).expect("patch set");
    assert_eq!(x, Some(7));

    // Validate and then clear with NULL.
    x.wire_patch_validate(SerializedValueRef::Null)
        .expect("validate clear");
    x.wire_patch(SerializedValueRef::Null).expect("patch clear");
    assert_eq!(x, None);
}
