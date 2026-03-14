use awrk_datex::codec::{DecodeConfig, Encoder, decode_value_full};
use awrk_datex::value::SerializedValueRef;
use awrk_datex::{Patch, PatchValidate};

#[test]
fn vec_patch_replaces_contents_from_array() {
    let mut v: Vec<u64> = vec![10, 20];

    let mut enc = Encoder::new();
    enc.array(3, |w| {
        w.u64(1)?;
        w.u64(2)?;
        w.u64(3)?;
        Ok(())
    })
    .expect("encode array");

    let patch = decode_value_full(enc.as_slice(), DecodeConfig::default()).expect("decode");

    v.wire_patch_validate(patch.clone()).expect("validate");
    v.wire_patch(patch).expect("patch");

    assert_eq!(v, vec![1, 2, 3]);
}

#[test]
fn vec_patch_rejects_non_array() {
    let v: Vec<u64> = vec![];
    let err = v
        .wire_patch_validate(SerializedValueRef::U64(1))
        .expect_err("must reject");
    assert!(format!("{err}").contains("expected array"));
}
