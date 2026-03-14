use awrk_datex::WireError;
use awrk_datex::codec::Encoder;

#[test]
fn encode_array_too_many_elements() {
    let mut enc = Encoder::new();
    let err = enc
        .array(1, |w| {
            w.u64(1)?;
            w.u64(2)?;
            Ok(())
        })
        .expect_err("must reject too many elements");

    assert_eq!(err, WireError::Malformed("too many elements in container"));
}

#[test]
fn encode_array_not_enough_elements() {
    let mut enc = Encoder::new();
    let err = enc
        .array(2, |w| {
            w.u64(1)?;
            Ok(())
        })
        .expect_err("must reject not enough elements");

    assert_eq!(
        err,
        WireError::Malformed("not enough elements in container")
    );
}

#[test]
fn encode_array_element_cannot_be_empty() {
    let mut enc = Encoder::new();
    let err = enc
        .array(1, |w| w.value(|_enc| Ok(())))
        .expect_err("must reject empty element");

    assert_eq!(err, WireError::Malformed("container element is empty"));
}

#[test]
fn encode_map_too_many_entries() {
    let mut enc = Encoder::new();
    let err = enc
        .map(1, |w| {
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
            w.entry(
                |enc| {
                    enc.u64(2);
                    Ok(())
                },
                |enc| {
                    enc.u64(2);
                    Ok(())
                },
            )?;
            Ok(())
        })
        .expect_err("must reject too many entries");

    assert_eq!(err, WireError::Malformed("too many entries in map"));
}

#[test]
fn encode_map_not_enough_entries() {
    let mut enc = Encoder::new();
    let err = enc
        .map(2, |w| {
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
        .expect_err("must reject not enough entries");

    assert_eq!(err, WireError::Malformed("not enough entries in map"));
}

#[test]
fn encode_map_rejects_duplicate_keys() {
    let mut enc = Encoder::new();
    let err = enc
        .map(2, |w| {
            w.entry(
                |enc| {
                    enc.u64(2);
                    Ok(())
                },
                |enc| {
                    enc.u64(1);
                    Ok(())
                },
            )?;
            w.entry(
                |enc| {
                    enc.u64(2);
                    Ok(())
                },
                |enc| {
                    enc.u64(2);
                    Ok(())
                },
            )?;
            Ok(())
        })
        .expect_err("must reject duplicate keys");

    assert_eq!(err, WireError::Malformed("duplicate map key"));
}

#[test]
fn encode_map_entry_value_cannot_be_empty() {
    let mut enc = Encoder::new();
    let err = enc
        .map(1, |w| {
            w.entry(
                |enc| {
                    enc.u64(1);
                    Ok(())
                },
                |_enc| Ok(()),
            )
        })
        .expect_err("must reject empty value");

    assert_eq!(err, WireError::Malformed("map entry value is empty"));
}

#[test]
fn encode_map_entry_key_cannot_be_empty() {
    let mut enc = Encoder::new();
    let err = enc
        .map(1, |w| {
            w.entry(
                |_enc| Ok(()),
                |enc| {
                    enc.u64(1);
                    Ok(())
                },
            )
        })
        .expect_err("must reject empty key");

    assert_eq!(err, WireError::Malformed("map entry key is empty"));
}
