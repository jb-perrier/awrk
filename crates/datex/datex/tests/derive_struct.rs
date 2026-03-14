use awrk_datex::codec::{DecodeConfig, Encoder, decode_value_full};
use awrk_datex::value::SerializedValueRef;
use awrk_datex::{Decode, Encode};

#[derive(Debug, PartialEq, Encode, Decode)]
struct Foo {
    a: u64,
    b: String,
    c: bool,
}

#[derive(Debug, PartialEq, Encode, Decode)]
struct HasOptional {
    a: u64,
    b: Option<u64>,
    c: bool,
}

#[test]
fn derive_struct_roundtrip() {
    let foo = Foo {
        a: 123,
        b: "hello".to_string(),
        c: true,
    };

    let mut enc = Encoder::new();
    foo.wire_encode(&mut enc).expect("encode");

    let value = decode_value_full(enc.as_slice(), DecodeConfig::default()).expect("decode");
    let decoded = Foo::wire_decode(value).expect("decode Foo");

    assert_eq!(decoded, foo);
}

#[test]
fn derive_struct_encodes_as_map_u64_sorted_by_field_id() {
    let foo = Foo {
        a: 1,
        b: "x".to_string(),
        c: false,
    };

    let mut enc = Encoder::new();
    foo.wire_encode(&mut enc).expect("encode");

    let value = decode_value_full(enc.as_slice(), DecodeConfig::default()).expect("decode");
    let map = value.as_map().expect("map");

    let ty = awrk_datex_schema::type_id(core::any::type_name::<Foo>());
    let fid_a = awrk_datex_schema::field_id(ty, "a").0;
    let fid_b = awrk_datex_schema::field_id(ty, "b").0;
    let fid_c = awrk_datex_schema::field_id(ty, "c").0;

    let mut got_keys = Vec::new();
    let mut it = map.iter_pairs();
    while let Some(entry) = it.next() {
        let (k, _v) = entry.expect("pair");
        got_keys.push(k.as_u64().expect("u64 key"));
    }
    it.finish().expect("finish");

    let mut expected = vec![fid_a, fid_b, fid_c];
    expected.sort_unstable();
    assert_eq!(got_keys, expected);

    let find = |key: u64| -> Option<SerializedValueRef<'_>> {
        let mut found = None;
        let mut it = map.iter_pairs();
        while let Some(entry) = it.next() {
            let (k, v) = entry.ok()?;
            if k.as_u64() == Some(key) {
                found = Some(v);
            }
        }
        it.finish().ok()?;
        found
    };

    assert_eq!(find(fid_a), Some(SerializedValueRef::U8(1)));
    assert_eq!(find(fid_b), Some(SerializedValueRef::String("x")));
    assert_eq!(find(fid_c), Some(SerializedValueRef::Bool(false)));
}

#[test]
fn derive_struct_decode_errors_on_missing_field() {
    let ty = awrk_datex_schema::type_id(core::any::type_name::<Foo>());
    let fid_a = awrk_datex_schema::field_id(ty, "a").0;
    let fid_c = awrk_datex_schema::field_id(ty, "c").0;

    let mut keys = vec![fid_a, fid_c];
    keys.sort_unstable();

    let mut enc = Encoder::new();
    enc.map(2, |w| {
        for &fid in &keys {
            w.entry(
                |enc| {
                    enc.u64(fid);
                    Ok(())
                },
                |enc| {
                    if fid == fid_a {
                        enc.u64(10);
                    } else {
                        enc.bool(true);
                    }
                    Ok(())
                },
            )?;
        }
        Ok(())
    })
    .expect("encode missing-field map");

    let value = decode_value_full(enc.as_slice(), DecodeConfig::default()).expect("decode");
    let err = Foo::wire_decode(value).expect_err("should fail");
    assert!(format!("{err}").contains("missing struct field"));
}

#[test]
fn derive_struct_allows_missing_optional_field() {
    let ty = awrk_datex_schema::type_id(core::any::type_name::<HasOptional>());
    let fid_a = awrk_datex_schema::field_id(ty, "a").0;
    let fid_c = awrk_datex_schema::field_id(ty, "c").0;

    // Deliberately omit field `b`.
    let mut keys = vec![fid_a, fid_c];
    keys.sort_unstable();

    let mut enc = Encoder::new();
    enc.map(2, |w| {
        for &fid in &keys {
            w.entry(
                |enc| {
                    enc.u64(fid);
                    Ok(())
                },
                |enc| {
                    if fid == fid_a {
                        enc.u64(10);
                    } else {
                        enc.bool(true);
                    }
                    Ok(())
                },
            )?;
        }
        Ok(())
    })
    .expect("encode map");

    let value = decode_value_full(enc.as_slice(), DecodeConfig::default()).expect("decode");
    let decoded = HasOptional::wire_decode(value).expect("decode HasOptional");
    assert_eq!(
        decoded,
        HasOptional {
            a: 10,
            b: None,
            c: true
        }
    );
}

#[test]
fn derive_struct_decodes_optional_null_as_none() {
    let ty = awrk_datex_schema::type_id(core::any::type_name::<HasOptional>());
    let fid_a = awrk_datex_schema::field_id(ty, "a").0;
    let fid_b = awrk_datex_schema::field_id(ty, "b").0;
    let fid_c = awrk_datex_schema::field_id(ty, "c").0;

    let mut keys = vec![fid_a, fid_b, fid_c];
    keys.sort_unstable();

    let mut enc = Encoder::new();
    enc.map(3, |w| {
        for &fid in &keys {
            w.entry(
                |enc| {
                    enc.u64(fid);
                    Ok(())
                },
                |enc| {
                    if fid == fid_a {
                        enc.u64(1);
                    } else if fid == fid_b {
                        enc.null();
                    } else {
                        enc.bool(false);
                    }
                    Ok(())
                },
            )?;
        }
        Ok(())
    })
    .expect("encode map");

    let value = decode_value_full(enc.as_slice(), DecodeConfig::default()).expect("decode");
    let decoded = HasOptional::wire_decode(value).expect("decode HasOptional");
    assert_eq!(
        decoded,
        HasOptional {
            a: 1,
            b: None,
            c: false
        }
    );
}
