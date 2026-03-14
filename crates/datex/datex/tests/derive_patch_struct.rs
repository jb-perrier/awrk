use awrk_datex::codec::{DecodeConfig, Encoder, decode_value_full};
use awrk_datex::{Patch, PatchValidate};

#[derive(Debug, PartialEq, Patch)]
struct Player {
    hp: u64,
    name: String,
    alive: bool,
}

#[derive(Clone, Copy)]
enum PatchVal<'a> {
    U64(u64),
    String(&'a str),
}

fn encode_patch_map(entries: &[(u64, PatchVal<'_>)]) -> Vec<u8> {
    let mut items = entries.to_vec();
    items.sort_unstable_by_key(|(k, _)| *k);

    let mut enc = Encoder::new();
    enc.map(items.len() as u32, |w| {
        for (k, v) in items {
            w.entry(
                |enc| {
                    enc.u64(k);
                    Ok(())
                },
                |enc| {
                    match v {
                        PatchVal::U64(x) => enc.u64(x),
                        PatchVal::String(s) => {
                            enc.string(s)?;
                        }
                    }
                    Ok(())
                },
            )?;
        }
        Ok(())
    })
    .expect("encode patch map");
    enc.into_inner()
}

#[test]
fn derive_struct_patch_applies_known_keys_only() {
    let ty = awrk_datex_schema::type_id(core::any::type_name::<Player>());
    let fid_hp = awrk_datex_schema::field_id(ty, "hp").0;
    let fid_name = awrk_datex_schema::field_id(ty, "name").0;

    let buf = encode_patch_map(&[
        (fid_hp, PatchVal::U64(99)),
        (fid_name, PatchVal::String("bob")),
        (123456789, PatchVal::U64(0)),
    ]);

    let patch = decode_value_full(&buf, DecodeConfig::default()).expect("decode patch");

    let mut p = Player {
        hp: 1,
        name: "alice".to_string(),
        alive: true,
    };

    p.wire_patch(patch).expect("patch");

    assert_eq!(
        p,
        Player {
            hp: 99,
            name: "bob".to_string(),
            alive: true
        }
    );
}

#[test]
fn derive_struct_patch_validate_does_not_mutate_and_rejects_type_mismatch() {
    let ty = awrk_datex_schema::type_id(core::any::type_name::<Player>());
    let fid_hp = awrk_datex_schema::field_id(ty, "hp").0;

    let ok_buf = encode_patch_map(&[(fid_hp, PatchVal::U64(5))]);
    let ok_patch = decode_value_full(&ok_buf, DecodeConfig::default()).expect("decode ok");

    let p = Player {
        hp: 1,
        name: "alice".to_string(),
        alive: true,
    };

    p.wire_patch_validate(ok_patch).expect("validate ok");
    assert_eq!(p.hp, 1);

    let bad_buf = encode_patch_map(&[(fid_hp, PatchVal::String("nope"))]);
    let bad_patch = decode_value_full(&bad_buf, DecodeConfig::default()).expect("decode bad");

    let err = p.wire_patch_validate(bad_patch).expect_err("must reject");
    assert!(format!("{err}").contains("expected unsigned int"));
}
