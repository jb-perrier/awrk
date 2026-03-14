use std::collections::BTreeMap;

use crate::{
    EnumRepr, EnumVariantDef, FieldDef, FieldId, OwnedSchema, PrimitiveKind, ProcDef, ProcId,
    StringId, TypeDef, TypeId, TypeKind, field_id, proc_id, string_id, type_id,
};

pub struct SchemaBuilder {
    errors: Vec<String>,
    strings: BTreeMap<StringId, String>,
    types: BTreeMap<TypeId, TypeDef>,
    fields: BTreeMap<FieldId, FieldDef>,
    procedures: BTreeMap<ProcId, ProcDef>,
}

impl SchemaBuilder {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            strings: BTreeMap::new(),
            types: BTreeMap::new(),
            fields: BTreeMap::new(),
            procedures: BTreeMap::new(),
        }
    }

    pub fn intern_string(&mut self, value: &str) -> StringId {
        let id = string_id(value);
        if let Some(existing) = self.strings.get(&id) {
            if existing != value {
                self.errors.push(format!(
                    "StringId collision: id={} existing={:?} new={:?}",
                    id.0, existing, value
                ));
            }
        } else {
            self.strings.insert(id, value.to_owned());
        }
        id
    }

    pub fn register_opaque_type(&mut self, type_name: &str) -> TypeId {
        let id = type_id(type_name);
        let name = self.intern_string(type_name);
        let def = TypeDef {
            name,
            kind: TypeKind::Opaque,
        };
        self.insert_type(id, def);
        id
    }

    pub fn register_primitive_type(&mut self, type_name: &str, prim: PrimitiveKind) -> TypeId {
        let id = type_id(type_name);
        let name = self.intern_string(type_name);
        let def = TypeDef {
            name,
            kind: TypeKind::Primitive { prim },
        };
        self.insert_type(id, def);
        id
    }

    pub fn register_tuple_type(&mut self, type_name: &str, items: Vec<TypeId>) -> TypeId {
        let id = type_id(type_name);
        let name = self.intern_string(type_name);
        let def = TypeDef {
            name,
            kind: TypeKind::Tuple { items },
        };
        self.insert_type(id, def);
        id
    }

    pub fn register_enum_type<'a>(
        &mut self,
        type_name: &str,
        variants: impl IntoIterator<Item = (&'a str, Option<TypeId>)>,
    ) -> TypeId {
        self.register_enum_type_with_repr(type_name, EnumRepr::IndexKeyedSingleEntryMap, variants)
    }

    pub fn register_enum_type_with_repr<'a>(
        &mut self,
        type_name: &str,
        repr: EnumRepr,
        variants: impl IntoIterator<Item = (&'a str, Option<TypeId>)>,
    ) -> TypeId {
        let id = type_id(type_name);
        let name = self.intern_string(type_name);

        let mut out_variants = Vec::new();
        for (variant_name, payload) in variants {
            let v_name = self.intern_string(variant_name);
            out_variants.push(EnumVariantDef {
                name: v_name,
                payload,
            });
        }

        let def = TypeDef {
            name,
            kind: TypeKind::Enum {
                variants: out_variants,
                repr,
            },
        };
        self.insert_type(id, def);
        id
    }

    pub fn register_option_type(&mut self, item: TypeId) -> TypeId {
        let item_name = self.type_name_of(item).unwrap_or_else(|| {
            self.errors
                .push("register_option_type: item TypeId not registered".into());
            "<unknown>".into()
        });

        let type_name = format!("core::option::Option<{item_name}>");
        let id = type_id(&type_name);
        let name = self.intern_string(&type_name);
        let def = TypeDef {
            name,
            kind: TypeKind::Option { some: item },
        };
        self.insert_type(id, def);
        id
    }

    pub fn register_vec_type(&mut self, item: TypeId) -> TypeId {
        let item_name = self.type_name_of(item).unwrap_or_else(|| {
            self.errors
                .push("register_vec_type: item TypeId not registered".into());
            "<unknown>".into()
        });

        let type_name = format!("alloc::vec::Vec<{item_name}>");
        let id = type_id(&type_name);
        let name = self.intern_string(&type_name);
        let def = TypeDef {
            name,
            kind: TypeKind::Array { items: item },
        };
        self.insert_type(id, def);
        id
    }

    pub fn register_map_type(&mut self, key: TypeId, value: TypeId) -> TypeId {
        let key_name = self.type_name_of(key).unwrap_or_else(|| {
            self.errors
                .push("register_map_type: key TypeId not registered".into());
            "<unknown>".into()
        });
        let value_name = self.type_name_of(value).unwrap_or_else(|| {
            self.errors
                .push("register_map_type: value TypeId not registered".into());
            "<unknown>".into()
        });

        let type_name = format!("std::collections::BTreeMap<{key_name},{value_name}>");
        let id = type_id(&type_name);
        let name = self.intern_string(&type_name);
        let def = TypeDef {
            name,
            kind: TypeKind::Map { key, value },
        };
        self.insert_type(id, def);
        id
    }

    pub fn register_struct_type<'a>(
        &mut self,
        type_name: &str,
        fields: impl IntoIterator<Item = (&'a str, TypeId, u32)>,
    ) -> TypeId {
        let type_id_value = type_id(type_name);
        let type_name_id = self.intern_string(type_name);

        let mut field_ids = Vec::new();
        for (name, value_type, flags) in fields {
            let f_id = field_id(type_id_value, name);
            let name_id = self.intern_string(name);
            let def = FieldDef {
                name: name_id,
                type_id: value_type,
                flags,
            };
            self.insert_field(f_id, def);
            field_ids.push(f_id);
        }

        field_ids.sort_unstable_by_key(|f| f.0);
        let def = TypeDef {
            name: type_name_id,
            kind: TypeKind::Struct { fields: field_ids },
        };
        self.insert_type(type_id_value, def);

        type_id_value
    }

    pub fn register_proc(
        &mut self,
        proc_name: &str,
        args_type: TypeId,
        result_type: TypeId,
    ) -> ProcId {
        let id = proc_id(proc_name);
        let name = self.intern_string(proc_name);
        let def = ProcDef {
            name,
            args_type,
            result_type,
        };
        self.insert_proc(id, def);
        id
    }

    pub fn register_proc_with_id(
        &mut self,
        id: ProcId,
        proc_name: &str,
        args_type: TypeId,
        result_type: TypeId,
    ) -> ProcId {
        let name = self.intern_string(proc_name);
        let def = ProcDef {
            name,
            args_type,
            result_type,
        };
        self.insert_proc(id, def);
        id
    }

    pub fn build(self) -> core::result::Result<OwnedSchema, String> {
        if !self.errors.is_empty() {
            return Err(self.errors.join("\n"));
        }

        Ok(OwnedSchema {
            strings: self.strings,
            types: self.types,
            fields: self.fields,
            procedures: self.procedures,
        })
    }

    /// Builds a schema snapshot without consuming the builder.
    pub fn build_clone(&self) -> core::result::Result<OwnedSchema, String> {
        if !self.errors.is_empty() {
            return Err(self.errors.join("\n"));
        }

        Ok(OwnedSchema {
            strings: self.strings.clone(),
            types: self.types.clone(),
            fields: self.fields.clone(),
            procedures: self.procedures.clone(),
        })
    }

    fn type_name_of(&self, id: TypeId) -> Option<String> {
        let def = self.types.get(&id)?;
        Some(self.strings.get(&def.name)?.clone())
    }

    fn insert_type(&mut self, id: TypeId, def: TypeDef) {
        if let Some(existing) = self.types.get(&id) {
            if existing != &def {
                self.errors.push(format!(
                    "TypeId collision: id={} existing={:?} new={:?}",
                    id.0, existing, def
                ));
            }
            return;
        }
        self.types.insert(id, def);
    }

    fn insert_field(&mut self, id: FieldId, def: FieldDef) {
        if let Some(existing) = self.fields.get(&id) {
            if existing != &def {
                self.errors.push(format!(
                    "FieldId collision: id={} existing={:?} new={:?}",
                    id.0, existing, def
                ));
            }
            return;
        }
        self.fields.insert(id, def);
    }

    fn insert_proc(&mut self, id: ProcId, def: ProcDef) {
        if let Some(existing) = self.procedures.get(&id) {
            if existing != &def {
                self.errors.push(format!(
                    "ProcId collision: id={} existing={:?} new={:?}",
                    id.0, existing, def
                ));
            }
            return;
        }
        self.procedures.insert(id, def);
    }
}
