mod builder;
mod codec;
pub mod error;
mod ids;
mod owned_schema;
mod schema;

pub use builder::SchemaBuilder;
pub use codec::{decode_schema, encode_schema};
pub use error::{Result, SchemaError};
pub use ids::{FieldId, ProcId, StringId, TypeId, field_id, proc_id, string_id, type_id};
pub use owned_schema::{
    EnumRepr, EnumVariantDef, FieldDef, OwnedSchema, PrimitiveKind, ProcDef, TypeDef, TypeKind,
};
pub use schema::Schema;

pub const PROC_ID_GET_SCHEMA: ProcId = ProcId(0);

#[cfg(test)]
mod tests;
