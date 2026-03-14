use core::mem;

use crate::error::{Result, SchemaError};

use crate::{
    EnumRepr, FieldDef, FieldId, OwnedSchema, PrimitiveKind, ProcDef, ProcId, StringId, TypeDef,
    TypeId, TypeKind,
};

const MAGIC: &[u8; 8] = b"UPISCHM2";
const VERSION: u32 = 2;

const TYPE_KIND_STRUCT: u8 = 0;
const TYPE_KIND_TUPLE: u8 = 1;
const TYPE_KIND_OPAQUE: u8 = 2;
const TYPE_KIND_ENUM: u8 = 3;
const TYPE_KIND_PRIMITIVE: u8 = 4;
const TYPE_KIND_OPTION: u8 = 5;
const TYPE_KIND_ARRAY: u8 = 6;
const TYPE_KIND_MAP: u8 = 7;

const PRIM_NULL: u8 = 0;
const PRIM_BOOL: u8 = 1;
const PRIM_UNIT: u8 = 2;
const PRIM_UNSIGNED: u8 = 3;
const PRIM_SIGNED: u8 = 4;
const PRIM_FLOAT: u8 = 5;
const PRIM_STRING: u8 = 6;
const PRIM_BYTES: u8 = 7;

const ENUM_REPR_INDEX_KEYED_SINGLE_ENTRY_MAP: u8 = 0;

pub fn encode_schema(schema: &OwnedSchema) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(128);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_be_bytes());

    let string_count =
        u32::try_from(schema.strings.len()).map_err(|_| SchemaError::LengthOverflow)?;
    let type_count = u32::try_from(schema.types.len()).map_err(|_| SchemaError::LengthOverflow)?;
    let field_count =
        u32::try_from(schema.fields.len()).map_err(|_| SchemaError::LengthOverflow)?;
    let proc_count =
        u32::try_from(schema.procedures.len()).map_err(|_| SchemaError::LengthOverflow)?;

    out.extend_from_slice(&string_count.to_be_bytes());
    out.extend_from_slice(&type_count.to_be_bytes());
    out.extend_from_slice(&field_count.to_be_bytes());
    out.extend_from_slice(&proc_count.to_be_bytes());

    for (id, s) in &schema.strings {
        out.extend_from_slice(&id.0.to_be_bytes());
        let bytes = s.as_bytes();
        if bytes.len() > u32::MAX as usize {
            return Err(SchemaError::LengthOverflow);
        }
        out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        out.extend_from_slice(bytes);
    }

    for (id, ty) in &schema.types {
        out.extend_from_slice(&id.0.to_be_bytes());
        out.extend_from_slice(&ty.name.0.to_be_bytes());
        match &ty.kind {
            TypeKind::Primitive { prim } => {
                out.push(TYPE_KIND_PRIMITIVE);
                match prim {
                    PrimitiveKind::Null => out.push(PRIM_NULL),
                    PrimitiveKind::Bool => out.push(PRIM_BOOL),
                    PrimitiveKind::Unit => out.push(PRIM_UNIT),
                    PrimitiveKind::Unsigned { bits } => {
                        out.push(PRIM_UNSIGNED);
                        out.push(*bits);
                    }
                    PrimitiveKind::Signed { bits } => {
                        out.push(PRIM_SIGNED);
                        out.push(*bits);
                    }
                    PrimitiveKind::Float { bits } => {
                        out.push(PRIM_FLOAT);
                        out.push(*bits);
                    }
                    PrimitiveKind::String => out.push(PRIM_STRING),
                    PrimitiveKind::Bytes => out.push(PRIM_BYTES),
                }
            }
            TypeKind::Option { some } => {
                out.push(TYPE_KIND_OPTION);
                out.extend_from_slice(&some.0.to_be_bytes());
            }
            TypeKind::Array { items } => {
                out.push(TYPE_KIND_ARRAY);
                out.extend_from_slice(&items.0.to_be_bytes());
            }
            TypeKind::Map { key, value } => {
                out.push(TYPE_KIND_MAP);
                out.extend_from_slice(&key.0.to_be_bytes());
                out.extend_from_slice(&value.0.to_be_bytes());
            }
            TypeKind::Struct { fields } => {
                out.push(TYPE_KIND_STRUCT);
                if fields.len() > u32::MAX as usize {
                    return Err(SchemaError::LengthOverflow);
                }
                out.extend_from_slice(&(fields.len() as u32).to_be_bytes());
                for f in fields {
                    out.extend_from_slice(&f.0.to_be_bytes());
                }
            }
            TypeKind::Tuple { items } => {
                out.push(TYPE_KIND_TUPLE);
                if items.len() > u32::MAX as usize {
                    return Err(SchemaError::LengthOverflow);
                }
                out.extend_from_slice(&(items.len() as u32).to_be_bytes());
                for item in items {
                    out.extend_from_slice(&item.0.to_be_bytes());
                }
            }
            TypeKind::Opaque => {
                out.push(TYPE_KIND_OPAQUE);
            }
            TypeKind::Enum { variants, repr } => {
                out.push(TYPE_KIND_ENUM);
                let repr_tag = match repr {
                    EnumRepr::IndexKeyedSingleEntryMap => ENUM_REPR_INDEX_KEYED_SINGLE_ENTRY_MAP,
                };
                out.push(repr_tag);

                if variants.len() > u32::MAX as usize {
                    return Err(SchemaError::LengthOverflow);
                }
                out.extend_from_slice(&(variants.len() as u32).to_be_bytes());
                for v in variants {
                    out.extend_from_slice(&v.name.0.to_be_bytes());
                    match v.payload {
                        None => out.push(0),
                        Some(t) => {
                            out.push(1);
                            out.extend_from_slice(&t.0.to_be_bytes());
                        }
                    }
                }
            }
        }
    }

    for (id, field) in &schema.fields {
        out.extend_from_slice(&id.0.to_be_bytes());
        out.extend_from_slice(&field.name.0.to_be_bytes());
        out.extend_from_slice(&field.type_id.0.to_be_bytes());
        out.extend_from_slice(&field.flags.to_be_bytes());
    }

    for (id, proc_def) in &schema.procedures {
        out.extend_from_slice(&id.0.to_be_bytes());
        out.extend_from_slice(&proc_def.name.0.to_be_bytes());
        out.extend_from_slice(&proc_def.args_type.0.to_be_bytes());
        out.extend_from_slice(&proc_def.result_type.0.to_be_bytes());
    }

    Ok(out)
}

pub fn decode_schema(buf: &[u8]) -> Result<OwnedSchema> {
    let mut cursor = 0usize;

    let magic = read_bytes(buf, &mut cursor, MAGIC.len())?;
    if magic != MAGIC {
        return Err(SchemaError::Malformed("invalid schema magic"));
    }

    let version = read_u32(buf, &mut cursor)?;
    if version != VERSION {
        return Err(SchemaError::Malformed("unsupported schema version"));
    }

    let string_count = read_u32(buf, &mut cursor)? as usize;
    let type_count = read_u32(buf, &mut cursor)? as usize;
    let field_count = read_u32(buf, &mut cursor)? as usize;
    let proc_count = read_u32(buf, &mut cursor)? as usize;

    const MAX_TABLE_ENTRIES: usize = 1_000_000;
    for (label, count) in [
        ("strings", string_count),
        ("types", type_count),
        ("fields", field_count),
        ("procedures", proc_count),
    ] {
        if count > MAX_TABLE_ENTRIES {
            return Err(SchemaError::Malformed(match label {
                "strings" => "schema string table too large",
                "types" => "schema type table too large",
                "fields" => "schema field table too large",
                _ => "schema procedure table too large",
            }));
        }
    }

    let mut strings = std::collections::BTreeMap::new();
    let mut types = std::collections::BTreeMap::new();
    let mut fields = std::collections::BTreeMap::new();
    let mut procedures = std::collections::BTreeMap::new();

    for _ in 0..string_count {
        let id = StringId(read_u64(buf, &mut cursor)?);
        let len = read_u32(buf, &mut cursor)? as usize;
        let bytes = read_bytes(buf, &mut cursor, len)?;
        let s = core::str::from_utf8(bytes).map_err(|_| SchemaError::InvalidUtf8)?;
        insert_unique_string(&mut strings, id, s)?;
    }

    for _ in 0..type_count {
        let id = TypeId(read_u64(buf, &mut cursor)?);
        let name = StringId(read_u64(buf, &mut cursor)?);
        let kind_tag = read_u8(buf, &mut cursor)?;

        let kind = match kind_tag {
            TYPE_KIND_STRUCT => {
                let count = read_u32(buf, &mut cursor)? as usize;
                let mut field_ids = Vec::with_capacity(count);
                for _ in 0..count {
                    field_ids.push(FieldId(read_u64(buf, &mut cursor)?));
                }
                TypeKind::Struct { fields: field_ids }
            }
            TYPE_KIND_TUPLE => {
                let count = read_u32(buf, &mut cursor)? as usize;
                let mut item_ids = Vec::with_capacity(count);
                for _ in 0..count {
                    item_ids.push(TypeId(read_u64(buf, &mut cursor)?));
                }
                TypeKind::Tuple { items: item_ids }
            }
            TYPE_KIND_OPAQUE => TypeKind::Opaque,
            TYPE_KIND_ENUM => {
                let repr_tag = read_u8(buf, &mut cursor)?;
                let repr = match repr_tag {
                    ENUM_REPR_INDEX_KEYED_SINGLE_ENTRY_MAP => EnumRepr::IndexKeyedSingleEntryMap,
                    _ => return Err(SchemaError::Malformed("invalid enum repr")),
                };

                let count = read_u32(buf, &mut cursor)? as usize;
                let mut variants = Vec::with_capacity(count);
                for _ in 0..count {
                    let name = StringId(read_u64(buf, &mut cursor)?);
                    let has_payload = read_u8(buf, &mut cursor)?;
                    let payload = match has_payload {
                        0 => None,
                        1 => Some(TypeId(read_u64(buf, &mut cursor)?)),
                        _ => {
                            return Err(SchemaError::Malformed(
                                "invalid enum variant payload flag",
                            ));
                        }
                    };
                    variants.push(crate::EnumVariantDef { name, payload });
                }
                TypeKind::Enum { variants, repr }
            }
            TYPE_KIND_PRIMITIVE => {
                let prim_tag = read_u8(buf, &mut cursor)?;
                let prim = match prim_tag {
                    PRIM_NULL => PrimitiveKind::Null,
                    PRIM_BOOL => PrimitiveKind::Bool,
                    PRIM_UNIT => PrimitiveKind::Unit,
                    PRIM_UNSIGNED => PrimitiveKind::Unsigned {
                        bits: read_u8(buf, &mut cursor)?,
                    },
                    PRIM_SIGNED => PrimitiveKind::Signed {
                        bits: read_u8(buf, &mut cursor)?,
                    },
                    PRIM_FLOAT => PrimitiveKind::Float {
                        bits: read_u8(buf, &mut cursor)?,
                    },
                    PRIM_STRING => PrimitiveKind::String,
                    PRIM_BYTES => PrimitiveKind::Bytes,
                    _ => return Err(SchemaError::Malformed("invalid primitive kind")),
                };
                TypeKind::Primitive { prim }
            }
            TYPE_KIND_OPTION => {
                let some = TypeId(read_u64(buf, &mut cursor)?);
                TypeKind::Option { some }
            }
            TYPE_KIND_ARRAY => {
                let items = TypeId(read_u64(buf, &mut cursor)?);
                TypeKind::Array { items }
            }
            TYPE_KIND_MAP => {
                let key = TypeId(read_u64(buf, &mut cursor)?);
                let value = TypeId(read_u64(buf, &mut cursor)?);
                TypeKind::Map { key, value }
            }
            _ => return Err(SchemaError::Malformed("invalid type kind")),
        };

        insert_unique(&mut types, id, TypeDef { name, kind }, "duplicate type id")?;
    }

    for _ in 0..field_count {
        let id = FieldId(read_u64(buf, &mut cursor)?);
        let name = StringId(read_u64(buf, &mut cursor)?);
        let type_id = TypeId(read_u64(buf, &mut cursor)?);
        let flags = read_u32(buf, &mut cursor)?;
        insert_unique(
            &mut fields,
            id,
            FieldDef {
                name,
                type_id,
                flags,
            },
            "duplicate field id",
        )?;
    }

    for _ in 0..proc_count {
        let id = ProcId(read_u64(buf, &mut cursor)?);
        let name = StringId(read_u64(buf, &mut cursor)?);
        let args_type = TypeId(read_u64(buf, &mut cursor)?);
        let result_type = TypeId(read_u64(buf, &mut cursor)?);
        insert_unique(
            &mut procedures,
            id,
            ProcDef {
                name,
                args_type,
                result_type,
            },
            "duplicate procedure id",
        )?;
    }

    if cursor != buf.len() {
        return Err(SchemaError::Malformed("trailing bytes after schema"));
    }

    let schema = OwnedSchema {
        strings,
        types,
        fields,
        procedures,
    };

    validate_schema_references(&schema)?;

    Ok(schema)
}

fn validate_schema_references(schema: &OwnedSchema) -> Result<()> {
    for (_id, s) in &schema.strings {
        let _ = s;
    }

    for (_id, ty) in &schema.types {
        if !schema.strings.contains_key(&ty.name) {
            return Err(SchemaError::Malformed(
                "type name StringId not found in string table",
            ));
        }

        match &ty.kind {
            TypeKind::Primitive { prim: _ } => {}
            TypeKind::Option { some } => {
                if !schema.types.contains_key(some) {
                    return Err(SchemaError::Malformed("option refers to missing TypeId"));
                }
            }
            TypeKind::Array { items } => {
                if !schema.types.contains_key(items) {
                    return Err(SchemaError::Malformed("array refers to missing TypeId"));
                }
            }
            TypeKind::Map { key, value } => {
                if !schema.types.contains_key(key) {
                    return Err(SchemaError::Malformed("map refers to missing key TypeId"));
                }
                if !schema.types.contains_key(value) {
                    return Err(SchemaError::Malformed("map refers to missing value TypeId"));
                }
            }
            TypeKind::Struct { fields } => {
                for f in fields {
                    if !schema.fields.contains_key(f) {
                        return Err(SchemaError::Malformed("type refers to missing FieldId"));
                    }
                }
            }
            TypeKind::Tuple { items } => {
                for t in items {
                    if !schema.types.contains_key(t) {
                        return Err(SchemaError::Malformed("tuple refers to missing TypeId"));
                    }
                }
            }
            TypeKind::Enum { variants, repr: _ } => {
                for v in variants {
                    if !schema.strings.contains_key(&v.name) {
                        return Err(SchemaError::Malformed(
                            "enum variant name StringId not found in string table",
                        ));
                    }
                    if let Some(t) = v.payload {
                        if !schema.types.contains_key(&t) {
                            return Err(SchemaError::Malformed(
                                "enum variant payload refers to missing TypeId",
                            ));
                        }
                    }
                }
            }
            TypeKind::Opaque => {}
        }
    }

    for (_id, field) in &schema.fields {
        if !schema.strings.contains_key(&field.name) {
            return Err(SchemaError::Malformed(
                "field name StringId not found in string table",
            ));
        }
        if !schema.types.contains_key(&field.type_id) {
            return Err(SchemaError::Malformed("field refers to missing TypeId"));
        }
    }

    for (_id, proc_def) in &schema.procedures {
        if !schema.strings.contains_key(&proc_def.name) {
            return Err(SchemaError::Malformed(
                "procedure name StringId not found in string table",
            ));
        }
        if !schema.types.contains_key(&proc_def.args_type) {
            return Err(SchemaError::Malformed(
                "procedure args_type refers to missing TypeId",
            ));
        }
        if !schema.types.contains_key(&proc_def.result_type) {
            return Err(SchemaError::Malformed(
                "procedure result_type refers to missing TypeId",
            ));
        }
    }

    Ok(())
}

fn insert_unique_string(
    map: &mut std::collections::BTreeMap<StringId, String>,
    id: StringId,
    value: &str,
) -> Result<()> {
    if let Some(existing) = map.get(&id) {
        if existing != value {
            return Err(SchemaError::Malformed(
                "duplicate StringId with different value",
            ));
        }
        return Ok(());
    }
    map.insert(id, value.to_owned());
    Ok(())
}

fn insert_unique<K, V>(
    map: &mut std::collections::BTreeMap<K, V>,
    key: K,
    value: V,
    message: &'static str,
) -> Result<()>
where
    K: Ord,
    V: PartialEq,
{
    if let Some(existing) = map.get(&key) {
        if existing != &value {
            return Err(SchemaError::Malformed(message));
        }
        return Ok(());
    }
    map.insert(key, value);
    Ok(())
}

fn read_bytes<'a>(buf: &'a [u8], cursor: &mut usize, len: usize) -> Result<&'a [u8]> {
    let end = cursor.checked_add(len).ok_or(SchemaError::LengthOverflow)?;
    let bytes = buf.get(*cursor..end).ok_or(SchemaError::UnexpectedEof)?;
    *cursor = end;
    Ok(bytes)
}

fn read_u8(buf: &[u8], cursor: &mut usize) -> Result<u8> {
    let bytes = read_bytes(buf, cursor, mem::size_of::<u8>())?;
    Ok(bytes[0])
}

fn read_u32(buf: &[u8], cursor: &mut usize) -> Result<u32> {
    let bytes = read_bytes(buf, cursor, mem::size_of::<u32>())?;
    Ok(u32::from_be_bytes(bytes.try_into().unwrap()))
}

fn read_u64(buf: &[u8], cursor: &mut usize) -> Result<u64> {
    let bytes = read_bytes(buf, cursor, mem::size_of::<u64>())?;
    Ok(u64::from_be_bytes(bytes.try_into().unwrap()))
}
