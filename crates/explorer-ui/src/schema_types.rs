use awrk_datex_schema::{
    EnumRepr, OwnedSchema, PrimitiveKind, ProcDef, TypeDef, TypeId, TypeKind as SchemaTypeKind,
};
use awrk_world::rpc::TypeCaps;

#[derive(Clone, Debug)]
pub struct TypeInfo {
    pub type_name: String,
    pub kind: TypeKind,
    pub caps: TypeCaps,
}

#[derive(Clone, Debug)]
pub struct ProcInfo {
    pub name: String,
    pub args: TypeKind,
    pub result: TypeKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeKind {
    Unit,
    Primitive {
        prim: PrimitiveKind,
    },
    Struct {
        fields: Vec<FieldInfo>,
    },
    Tuple {
        items: Vec<TupleItemInfo>,
    },
    Enum {
        variants: Vec<EnumVariantInfo>,
        repr: EnumRepr,
    },
    Other {
        kind: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumVariantInfo {
    pub index: u32,
    pub name: String,
    pub payload_type_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldInfo {
    pub name: String,
    pub type_name: String,
    pub field_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TupleItemInfo {
    pub index: u32,
    pub type_name: String,
}

pub fn schema_type_name(schema: &OwnedSchema, type_id: TypeId) -> String {
    schema
        .types
        .get(&type_id)
        .and_then(|t| schema.string(t.name))
        .unwrap_or("<unknown>")
        .to_string()
}

fn type_kind_from_type_def(schema: &OwnedSchema, def: &TypeDef) -> TypeKind {
    match &def.kind {
        SchemaTypeKind::Primitive { prim } => {
            if matches!(prim, PrimitiveKind::Unit) {
                TypeKind::Unit
            } else {
                TypeKind::Primitive { prim: *prim }
            }
        }
        SchemaTypeKind::Struct { fields } => {
            let mut out = Vec::with_capacity(fields.len());
            for field_id in fields {
                let Some(field_def) = schema.fields.get(field_id) else {
                    continue;
                };

                let name = schema
                    .string(field_def.name)
                    .unwrap_or("<unnamed>")
                    .to_string();
                let type_name = schema_type_name(schema, field_def.type_id);

                out.push(FieldInfo {
                    name,
                    type_name,
                    field_id: field_id.0,
                });
            }
            TypeKind::Struct { fields: out }
        }
        SchemaTypeKind::Tuple { items } => {
            let mut out = Vec::with_capacity(items.len());
            for (idx, item_ty) in items.iter().enumerate() {
                out.push(TupleItemInfo {
                    index: idx as u32,
                    type_name: schema_type_name(schema, *item_ty),
                });
            }
            TypeKind::Tuple { items: out }
        }
        SchemaTypeKind::Enum { variants, repr } => {
            let mut out = Vec::with_capacity(variants.len());
            for (idx, v) in variants.iter().enumerate() {
                let name = schema.string(v.name).unwrap_or("<unnamed>").to_string();
                let payload_type_name = v.payload.map(|ty| schema_type_name(schema, ty));
                out.push(EnumVariantInfo {
                    index: idx as u32,
                    name,
                    payload_type_name,
                });
            }
            TypeKind::Enum {
                variants: out,
                repr: *repr,
            }
        }
        other => TypeKind::Other {
            kind: format!("{other:?}"),
        },
    }
}

pub fn types_from_schema(schema: &OwnedSchema) -> Vec<TypeInfo> {
    let mut out: Vec<TypeInfo> = schema
        .types
        .iter()
        .map(|(type_id, def)| TypeInfo {
            type_name: schema_type_name(schema, *type_id),
            kind: type_kind_from_type_def(schema, def),
            caps: TypeCaps::default(),
        })
        .collect();

    out.sort_by(|a, b| a.type_name.cmp(&b.type_name));
    out
}

fn proc_info_from_def(schema: &OwnedSchema, def: &ProcDef) -> Option<ProcInfo> {
    let name = schema.string(def.name)?.to_string();

    let args_def = schema.types.get(&def.args_type)?;
    let res_def = schema.types.get(&def.result_type)?;

    Some(ProcInfo {
        name,
        args: type_kind_from_type_def(schema, args_def),
        result: type_kind_from_type_def(schema, res_def),
    })
}

pub fn procs_from_schema(schema: &OwnedSchema) -> Vec<ProcInfo> {
    let mut out: Vec<ProcInfo> = schema
        .procedures
        .values()
        .filter_map(|p| proc_info_from_def(schema, p))
        .collect();

    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn field_id_for_name(kind: &TypeKind, field_name: &str) -> Option<u64> {
    let TypeKind::Struct { fields } = kind else {
        return None;
    };

    fields
        .iter()
        .find(|f| f.name == field_name)
        .map(|f| f.field_id)
}
