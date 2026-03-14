use crate::{
    EnumRepr, PROC_ID_GET_SCHEMA, PrimitiveKind, Schema, SchemaBuilder, TypeKind, decode_schema,
    string_id,
};

#[test]
fn schema_roundtrip_encode_decode_v2() {
    let mut b = SchemaBuilder::new();

    let any = b.register_opaque_type("upi::Any");
    let u64_ty = <u64 as Schema>::wire_schema(&mut b);
    let string_ty = <String as Schema>::wire_schema(&mut b);
    let bytes_ty = <Vec<u8> as Schema>::wire_schema(&mut b);
    let opt_u64_ty = <Option<u64> as Schema>::wire_schema(&mut b);
    let vec_string_ty = <Vec<String> as Schema>::wire_schema(&mut b);
    let map_ty = <std::collections::BTreeMap<String, u64> as Schema>::wire_schema(&mut b);

    let health = b.register_struct_type(
        "demo::Health",
        [
            ("current", u64_ty, 0u32),
            ("max", opt_u64_ty, 0u32),
            ("name", string_ty, 0u32),
        ],
    );

    let data = b.register_tuple_type("demo::Data", vec![bytes_ty, vec_string_ty, map_ty]);

    let e = b.register_enum_type_with_repr(
        "demo::E",
        EnumRepr::IndexKeyedSingleEntryMap,
        [("A", None), ("B", Some(any))],
    );

    let _ = (data, e);

    b.register_proc_with_id(PROC_ID_GET_SCHEMA, "awrk.get_schema", any, any);
    b.register_proc("awrk.list_entities", any, any);
    b.register_proc("demo.get_health", health, health);

    let schema = b.build().expect("schema build");
    let encoded = schema.encode().expect("schema encode");
    let decoded = decode_schema(&encoded).expect("schema decode");

    assert_eq!(schema.strings, decoded.strings);
    assert_eq!(schema.types, decoded.types);
    assert_eq!(schema.fields, decoded.fields);
    assert_eq!(schema.procedures, decoded.procedures);

    let opt_def = decoded.types.get(&opt_u64_ty).expect("opt type");
    assert!(matches!(opt_def.kind, TypeKind::Option { .. }));
}

#[test]
fn schema_decode_rejects_unknown_kind_tag_v2() {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"UPISCHM2");
    buf.extend_from_slice(&2u32.to_be_bytes());
    buf.extend_from_slice(&1u32.to_be_bytes()); // strings
    buf.extend_from_slice(&1u32.to_be_bytes()); // types
    buf.extend_from_slice(&0u32.to_be_bytes()); // fields
    buf.extend_from_slice(&0u32.to_be_bytes()); // procs

    // string 1 => "demo::X"
    buf.extend_from_slice(&1u64.to_be_bytes());
    buf.extend_from_slice(&(7u32).to_be_bytes());
    buf.extend_from_slice(b"demo::X");

    // type id 1, name string id 1, kind tag 250 (invalid)
    buf.extend_from_slice(&1u64.to_be_bytes());
    buf.extend_from_slice(&1u64.to_be_bytes());
    buf.push(250u8);

    let err = decode_schema(&buf).expect_err("should error");
    assert!(format!("{err}").contains("type kind"));
}

#[test]
fn schema_decode_rejects_trailing_bytes() {
    let mut b = SchemaBuilder::new();
    let _ = b.register_primitive_type("demo::U", PrimitiveKind::Unsigned { bits: 64 });
    let schema = b.build().expect("schema build");
    let mut encoded = schema.encode().expect("encode");
    encoded.push(0);

    let err = decode_schema(&encoded).expect_err("should error");
    assert!(format!("{err}").contains("trailing"));
}

#[test]
fn schema_decode_rejects_missing_type_ref_option() {
    // v2 schema with one string and one type: Option { some = 999 } (missing type)
    let mut buf = Vec::new();
    buf.extend_from_slice(b"UPISCHM2");
    buf.extend_from_slice(&2u32.to_be_bytes());
    buf.extend_from_slice(&1u32.to_be_bytes()); // strings
    buf.extend_from_slice(&1u32.to_be_bytes()); // types
    buf.extend_from_slice(&0u32.to_be_bytes()); // fields
    buf.extend_from_slice(&0u32.to_be_bytes()); // procs

    buf.extend_from_slice(&1u64.to_be_bytes());
    buf.extend_from_slice(&(7u32).to_be_bytes());
    buf.extend_from_slice(b"demo::O");

    buf.extend_from_slice(&2u64.to_be_bytes()); // TypeId
    buf.extend_from_slice(&1u64.to_be_bytes()); // name StringId
    buf.push(5u8); // TYPE_KIND_OPTION
    buf.extend_from_slice(&999u64.to_be_bytes());

    let err = decode_schema(&buf).expect_err("should error");
    assert!(format!("{err}").contains("option"));
}

#[test]
fn schema_decode_rejects_wrong_magic() {
    let mut bad = Vec::new();
    bad.extend_from_slice(b"NOTSCHEM");
    bad.extend_from_slice(&1u32.to_be_bytes());
    bad.extend_from_slice(&0u32.to_be_bytes());
    bad.extend_from_slice(&0u32.to_be_bytes());
    bad.extend_from_slice(&0u32.to_be_bytes());
    bad.extend_from_slice(&0u32.to_be_bytes());

    let err = decode_schema(&bad).expect_err("should error");
    assert!(format!("{err}").contains("magic"));
}

#[test]
fn schema_decode_rejects_missing_string_ref() {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"UPISCHM2");
    buf.extend_from_slice(&2u32.to_be_bytes());
    buf.extend_from_slice(&0u32.to_be_bytes());
    buf.extend_from_slice(&1u32.to_be_bytes());
    buf.extend_from_slice(&0u32.to_be_bytes());
    buf.extend_from_slice(&0u32.to_be_bytes());

    buf.extend_from_slice(&1u64.to_be_bytes());
    buf.extend_from_slice(&1u64.to_be_bytes());
    buf.push(2u8);

    let err = decode_schema(&buf).expect_err("should error");
    assert!(format!("{err}").contains("StringId"));
}

#[test]
fn schema_builder_build_ok_on_repeated_intern() {
    let mut b = SchemaBuilder::new();
    let _ = b.intern_string("demo::X");
    let _ = b.intern_string("demo::X");
    let _ = b.register_opaque_type("demo::X");
    let schema = b.build().expect("schema build");
    assert!(schema.string(string_id("demo::X")).is_some());
}
