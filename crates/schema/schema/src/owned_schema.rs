use std::collections::BTreeMap;

use crate::error::Result;

use crate::{FieldId, ProcId, StringId, TypeId};

#[derive(Debug, Clone)]
pub struct OwnedSchema {
    pub strings: BTreeMap<StringId, String>,
    pub types: BTreeMap<TypeId, TypeDef>,
    pub fields: BTreeMap<FieldId, FieldDef>,
    pub procedures: BTreeMap<ProcId, ProcDef>,
}

impl OwnedSchema {
    pub fn string(&self, id: StringId) -> Option<&str> {
        self.strings.get(&id).map(String::as_str)
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        super::codec::encode_schema(self)
    }

    pub fn decode(buf: &[u8]) -> Result<Self> {
        super::codec::decode_schema(buf)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDef {
    pub name: StringId,
    pub kind: TypeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeKind {
    Primitive {
        prim: PrimitiveKind,
    },
    Option {
        some: TypeId,
    },
    Array {
        items: TypeId,
    },
    Map {
        key: TypeId,
        value: TypeId,
    },
    Struct {
        fields: Vec<FieldId>,
    },
    Tuple {
        items: Vec<TypeId>,
    },
    Enum {
        variants: Vec<EnumVariantDef>,
        repr: EnumRepr,
    },
    Opaque,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveKind {
    Null,
    Bool,
    Unit,
    Unsigned { bits: u8 },
    Signed { bits: u8 },
    Float { bits: u8 },
    String,
    Bytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnumRepr {
    /// Current `upi-wire-macros` encoding:
    /// - Value is a map with exactly one entry
    /// - Key is `u64(variant_index)`
    /// - Payload is `bool(true)` for unit variants
    /// - Payload is the encoded field value for single-field tuple variants
    /// - Payload is an encoded array for multi-field tuple variants
    /// - Payload is an encoded struct/map for named-field variants
    IndexKeyedSingleEntryMap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumVariantDef {
    pub name: StringId,
    pub payload: Option<TypeId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDef {
    pub name: StringId,
    pub type_id: TypeId,
    pub flags: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcDef {
    pub name: StringId,
    pub args_type: TypeId,
    pub result_type: TypeId,
}
