use awrk_datex_schema::{EnumRepr, PrimitiveKind, SchemaBuilder, TypeKind, field_id, type_id};
use awrk_schema_macros::Schema;

#[allow(dead_code)]
#[derive(Schema)]
struct DemoStruct {
    a: u64,
    b: Option<String>,
}

#[allow(dead_code)]
#[derive(Schema)]
struct DemoTupleStruct(u32, i32);

#[allow(dead_code)]
#[derive(Schema)]
struct DemoNewtype(u32);

#[allow(dead_code)]
#[derive(Schema)]
struct DemoRenamedField {
    #[awrk_datex(rename = "renamed")]
    original: u64,
}

#[allow(dead_code)]
#[derive(Schema)]
enum DemoEnum {
    A,
    B(u64),
}

#[allow(dead_code)]
#[derive(Schema)]
enum DemoEnumStructVariant {
    Create { entity: u64, title: String },
    Quit,
}

#[allow(dead_code)]
#[derive(Schema)]
enum DemoEnumTupleVariant {
    Move(u32, i32),
    Quit,
}

#[test]
fn derive_wire_schema_struct_fields() {
    let mut b = SchemaBuilder::new();
    let ty = <DemoStruct as awrk_datex_schema::Schema>::wire_schema(&mut b);
    let schema = b.build().expect("schema build");

    let def = schema.types.get(&ty).expect("struct typedef");
    let TypeKind::Struct { fields } = &def.kind else {
        panic!("expected struct kind");
    };

    let parent = type_id(core::any::type_name::<DemoStruct>());
    let mut expected = vec![field_id(parent, "a"), field_id(parent, "b")];
    expected.sort_unstable_by_key(|f| f.0);

    assert_eq!(fields, &expected);

    let b_field = schema
        .fields
        .get(&field_id(parent, "b"))
        .expect("field def");
    let b_ty = schema.types.get(&b_field.type_id).expect("field type def");

    assert!(matches!(b_ty.kind, TypeKind::Option { .. }));
}

#[test]
fn derive_wire_schema_enum_repr_and_order() {
    let mut b = SchemaBuilder::new();
    let ty = <DemoEnum as awrk_datex_schema::Schema>::wire_schema(&mut b);
    let schema = b.build().expect("schema build");

    let def = schema.types.get(&ty).expect("enum typedef");
    let TypeKind::Enum { variants, repr } = &def.kind else {
        panic!("expected enum kind");
    };

    assert_eq!(*repr, EnumRepr::IndexKeyedSingleEntryMap);
    assert_eq!(
        variants
            .iter()
            .map(|v| schema.string(v.name).unwrap())
            .collect::<Vec<_>>(),
        vec!["A", "B"]
    );

    let b_variant = &variants[1];
    let payload = b_variant.payload.expect("payload type");
    let payload_def = schema.types.get(&payload).expect("payload typedef");
    assert!(matches!(
        payload_def.kind,
        TypeKind::Primitive {
            prim: PrimitiveKind::Unsigned { bits: 64 }
        }
    ));

    let enum_name = schema.string(def.name).unwrap();
    assert_eq!(enum_name, core::any::type_name::<DemoEnum>());
}

#[test]
fn derive_wire_schema_enum_struct_variant_payload_is_struct_type() {
    let mut b = SchemaBuilder::new();
    let ty = <DemoEnumStructVariant as awrk_datex_schema::Schema>::wire_schema(&mut b);
    let schema = b.build().expect("schema build");

    let def = schema.types.get(&ty).expect("enum typedef");
    let TypeKind::Enum { variants, .. } = &def.kind else {
        panic!("expected enum kind");
    };

    assert_eq!(
        variants
            .iter()
            .map(|v| schema.string(v.name).unwrap())
            .collect::<Vec<_>>(),
        vec!["Create", "Quit"]
    );

    let create_variant = &variants[0];
    let payload = create_variant.payload.expect("payload type");
    let payload_def = schema.types.get(&payload).expect("payload typedef");

    let payload_name = schema.string(payload_def.name).unwrap();
    let expected_name = format!(
        "{}::{}",
        core::any::type_name::<DemoEnumStructVariant>(),
        "Create"
    );
    assert_eq!(payload_name, expected_name);

    let TypeKind::Struct { fields } = &payload_def.kind else {
        panic!("expected struct payload kind");
    };

    let parent = type_id(&payload_name);
    let mut expected = vec![field_id(parent, "entity"), field_id(parent, "title")];
    expected.sort_unstable_by_key(|f| f.0);
    assert_eq!(fields, &expected);
}

#[test]
fn derive_wire_schema_enum_tuple_variant_payload_is_tuple_type() {
    let mut b = SchemaBuilder::new();
    let ty = <DemoEnumTupleVariant as awrk_datex_schema::Schema>::wire_schema(&mut b);
    let schema = b.build().expect("schema build");

    let def = schema.types.get(&ty).expect("enum typedef");
    let TypeKind::Enum { variants, .. } = &def.kind else {
        panic!("expected enum kind");
    };

    assert_eq!(
        variants
            .iter()
            .map(|v| schema.string(v.name).unwrap())
            .collect::<Vec<_>>(),
        vec!["Move", "Quit"]
    );

    let move_variant = &variants[0];
    let payload = move_variant.payload.expect("payload type");
    let payload_def = schema.types.get(&payload).expect("payload typedef");

    let payload_name = schema.string(payload_def.name).unwrap();
    let expected_name = format!(
        "{}::{}",
        core::any::type_name::<DemoEnumTupleVariant>(),
        "Move"
    );
    assert_eq!(payload_name, expected_name);

    let TypeKind::Tuple { items } = &payload_def.kind else {
        panic!("expected tuple payload kind");
    };

    assert_eq!(items.len(), 2);
}

#[test]
fn derive_wire_schema_tuple_struct_kind_and_items() {
    let mut b = SchemaBuilder::new();
    let ty = <DemoTupleStruct as awrk_datex_schema::Schema>::wire_schema(&mut b);
    let schema = b.build().expect("schema build");

    let def = schema.types.get(&ty).expect("tuple typedef");
    let TypeKind::Tuple { items } = &def.kind else {
        panic!("expected tuple kind");
    };
    assert_eq!(items.len(), 2);

    let item0 = schema.types.get(&items[0]).expect("item0 typedef");
    assert!(matches!(
        item0.kind,
        TypeKind::Primitive {
            prim: PrimitiveKind::Unsigned { bits: 32 }
        }
    ));

    let item1 = schema.types.get(&items[1]).expect("item1 typedef");
    assert!(matches!(
        item1.kind,
        TypeKind::Primitive {
            prim: PrimitiveKind::Signed { bits: 32 }
        }
    ));
}

#[test]
fn derive_wire_schema_newtype_is_tuple_len_1() {
    let mut b = SchemaBuilder::new();
    let ty = <DemoNewtype as awrk_datex_schema::Schema>::wire_schema(&mut b);
    let schema = b.build().expect("schema build");

    let def = schema.types.get(&ty).expect("newtype typedef");
    let TypeKind::Tuple { items } = &def.kind else {
        panic!("expected tuple kind");
    };
    assert_eq!(items.len(), 1);
}

#[test]
fn derive_wire_schema_field_rename_affects_field_id_and_name() {
    let mut b = SchemaBuilder::new();
    let ty = <DemoRenamedField as awrk_datex_schema::Schema>::wire_schema(&mut b);
    let schema = b.build().expect("schema build");

    let def = schema.types.get(&ty).expect("struct typedef");
    let TypeKind::Struct { fields } = &def.kind else {
        panic!("expected struct kind");
    };

    let parent = type_id(core::any::type_name::<DemoRenamedField>());
    let renamed_id = field_id(parent, "renamed");
    let original_id = field_id(parent, "original");

    assert!(schema.fields.contains_key(&renamed_id));
    assert!(!schema.fields.contains_key(&original_id));
    assert_eq!(fields, &vec![renamed_id]);

    let fdef = schema.fields.get(&renamed_id).expect("field def");
    assert_eq!(schema.string(fdef.name).unwrap(), "renamed");
}
