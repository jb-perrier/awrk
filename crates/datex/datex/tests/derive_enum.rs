use awrk_datex::codec::{DecodeConfig, Encoder, decode_value_full};
use awrk_datex::{Decode, Encode, Patch, PatchValidate};

#[derive(Debug, PartialEq, Encode, Decode, Patch)]
enum MyEnum {
    A,
    B(u64),
    C(String),
    D(u64, String),
    E {
        entity: u64,
        #[awrk_datex(rename = "window_title")]
        title: String,
    },
}

#[test]
fn derive_enum_roundtrip_unit_and_payload() {
    let cases = vec![
        MyEnum::A,
        MyEnum::B(42),
        MyEnum::C("hi".to_string()),
        MyEnum::D(7, "tuple".to_string()),
        MyEnum::E {
            entity: 9,
            title: "named".to_string(),
        },
    ];

    for case in cases {
        let mut enc = Encoder::new();
        case.wire_encode(&mut enc).expect("encode");

        let value = decode_value_full(enc.as_slice(), DecodeConfig::default()).expect("decode");
        let decoded = MyEnum::wire_decode(value).expect("decode MyEnum");
        assert_eq!(decoded, case);
    }
}

#[test]
fn derive_enum_decode_rejects_wrong_shape() {
    let mut enc = Encoder::new();
    enc.map(2, |w| {
        w.entry(
            |enc| {
                enc.u64(0);
                Ok(())
            },
            |enc| {
                enc.bool(true);
                Ok(())
            },
        )?;
        w.entry(
            |enc| {
                enc.u64(1);
                Ok(())
            },
            |enc| {
                enc.u64(1);
                Ok(())
            },
        )?;
        Ok(())
    })
    .expect("encode");

    let value = decode_value_full(enc.as_slice(), DecodeConfig::default()).expect("decode");
    let err = MyEnum::wire_decode(value).expect_err("must reject");
    assert!(format!("{err}").contains("exactly one entry"));
}

#[test]
fn derive_enum_decode_rejects_unknown_variant() {
    let mut enc = Encoder::new();
    enc.map(1, |w| {
        w.entry(
            |enc| {
                enc.u64(99);
                Ok(())
            },
            |enc| {
                enc.bool(true);
                Ok(())
            },
        )
    })
    .expect("encode");

    let value = decode_value_full(enc.as_slice(), DecodeConfig::default()).expect("decode");
    let err = MyEnum::wire_decode(value).expect_err("must reject");
    assert!(format!("{err}").contains("unknown enum variant"));
}

#[test]
fn derive_enum_decode_rejects_unit_variant_wrong_payload() {
    let mut enc = Encoder::new();
    enc.map(1, |w| {
        // Variant 0 (A) must carry Bool(true)
        w.entry(
            |enc| {
                enc.u64(0);
                Ok(())
            },
            |enc| {
                enc.u64(123);
                Ok(())
            },
        )
    })
    .expect("encode");

    let value = decode_value_full(enc.as_slice(), DecodeConfig::default()).expect("decode");
    let err = MyEnum::wire_decode(value).expect_err("must reject");
    assert!(format!("{err}").contains("unexpected unit variant payload"));
}

#[test]
fn derive_enum_patch_and_validate_use_decode_semantics() {
    let mut v = MyEnum::B(1);

    let mut enc = Encoder::new();
    MyEnum::E {
        entity: 88,
        title: "ok".to_string(),
    }
    .wire_encode(&mut enc)
    .expect("encode patch value");
    let patch = decode_value_full(enc.as_slice(), DecodeConfig::default()).expect("decode");

    v.wire_patch_validate(patch.clone()).expect("validate");
    v.wire_patch(patch).expect("patch");

    assert_eq!(
        v,
        MyEnum::E {
            entity: 88,
            title: "ok".to_string(),
        }
    );
}

#[test]
fn derive_enum_decode_rejects_tuple_variant_wrong_length() {
    let mut enc = Encoder::new();
    enc.map(1, |w| {
        w.entry(
            |enc| {
                enc.u64(3);
                Ok(())
            },
            |enc| {
                enc.array(1, |w| {
                    w.value(|enc| {
                        enc.u64(123);
                        Ok(())
                    })?;
                    Ok(())
                })
            },
        )
    })
    .expect("encode");

    let value = decode_value_full(enc.as_slice(), DecodeConfig::default()).expect("decode");
    let err = MyEnum::wire_decode(value).expect_err("must reject");
    assert!(format!("{err}").contains("tuple length mismatch"));
}

#[test]
fn derive_enum_decode_rejects_named_variant_wrong_shape() {
    let mut enc = Encoder::new();
    enc.map(1, |w| {
        w.entry(
            |enc| {
                enc.u64(4);
                Ok(())
            },
            |enc| {
                enc.bool(true);
                Ok(())
            },
        )
    })
    .expect("encode");

    let value = decode_value_full(enc.as_slice(), DecodeConfig::default()).expect("decode");
    let err = MyEnum::wire_decode(value).expect_err("must reject");
    assert!(format!("{err}").contains("expected map"));
}
