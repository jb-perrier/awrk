pub mod builder;
pub mod codec;
pub mod error;
pub mod text;
pub mod traits;
pub mod value;

pub use error::{Result, WireError};
pub use traits::{Decode, Encode, Patch, PatchValidate};

pub use awrk_datex_macros::{Decode, Encode, Patch};

impl ::awrk_datex_schema::Schema for crate::value::Value {
    fn wire_schema(
        builder: &mut ::awrk_datex_schema::SchemaBuilder,
    ) -> ::awrk_datex_schema::TypeId {
        builder.register_opaque_type(core::any::type_name::<Self>())
    }
}
