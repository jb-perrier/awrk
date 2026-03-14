use std::collections::VecDeque;

use crate::{PrimitiveKind, SchemaBuilder, TypeId};

/// Registers a Rust type into a `SchemaBuilder`.
///
/// This is intended to be consistent with `upi-wire`'s on-wire semantics.
pub trait Schema {
    fn wire_schema(builder: &mut SchemaBuilder) -> TypeId;
}

impl Schema for () {
    fn wire_schema(builder: &mut SchemaBuilder) -> TypeId {
        builder.register_primitive_type(core::any::type_name::<Self>(), PrimitiveKind::Unit)
    }
}

impl Schema for bool {
    fn wire_schema(builder: &mut SchemaBuilder) -> TypeId {
        builder.register_primitive_type(core::any::type_name::<Self>(), PrimitiveKind::Bool)
    }
}

impl Schema for u64 {
    fn wire_schema(builder: &mut SchemaBuilder) -> TypeId {
        builder.register_primitive_type(
            core::any::type_name::<Self>(),
            PrimitiveKind::Unsigned { bits: 64 },
        )
    }
}

impl Schema for u32 {
    fn wire_schema(builder: &mut SchemaBuilder) -> TypeId {
        builder.register_primitive_type(
            core::any::type_name::<Self>(),
            PrimitiveKind::Unsigned { bits: 32 },
        )
    }
}

impl Schema for u16 {
    fn wire_schema(builder: &mut SchemaBuilder) -> TypeId {
        builder.register_primitive_type(
            core::any::type_name::<Self>(),
            PrimitiveKind::Unsigned { bits: 16 },
        )
    }
}

impl Schema for u8 {
    fn wire_schema(builder: &mut SchemaBuilder) -> TypeId {
        builder.register_primitive_type(
            core::any::type_name::<Self>(),
            PrimitiveKind::Unsigned { bits: 8 },
        )
    }
}

impl Schema for i64 {
    fn wire_schema(builder: &mut SchemaBuilder) -> TypeId {
        builder.register_primitive_type(
            core::any::type_name::<Self>(),
            PrimitiveKind::Signed { bits: 64 },
        )
    }
}

impl Schema for i32 {
    fn wire_schema(builder: &mut SchemaBuilder) -> TypeId {
        builder.register_primitive_type(
            core::any::type_name::<Self>(),
            PrimitiveKind::Signed { bits: 32 },
        )
    }
}

impl Schema for i16 {
    fn wire_schema(builder: &mut SchemaBuilder) -> TypeId {
        builder.register_primitive_type(
            core::any::type_name::<Self>(),
            PrimitiveKind::Signed { bits: 16 },
        )
    }
}

impl Schema for i8 {
    fn wire_schema(builder: &mut SchemaBuilder) -> TypeId {
        builder.register_primitive_type(
            core::any::type_name::<Self>(),
            PrimitiveKind::Signed { bits: 8 },
        )
    }
}

impl Schema for f32 {
    fn wire_schema(builder: &mut SchemaBuilder) -> TypeId {
        builder.register_primitive_type(
            core::any::type_name::<Self>(),
            PrimitiveKind::Float { bits: 32 },
        )
    }
}

impl Schema for f64 {
    fn wire_schema(builder: &mut SchemaBuilder) -> TypeId {
        builder.register_primitive_type(
            core::any::type_name::<Self>(),
            PrimitiveKind::Float { bits: 64 },
        )
    }
}

impl Schema for String {
    fn wire_schema(builder: &mut SchemaBuilder) -> TypeId {
        builder.register_primitive_type(core::any::type_name::<Self>(), PrimitiveKind::String)
    }
}

impl<T> Schema for Option<T>
where
    T: Schema,
{
    fn wire_schema(builder: &mut SchemaBuilder) -> TypeId {
        let some = T::wire_schema(builder);
        builder.register_option_type(some)
    }
}

impl<T> Schema for Vec<T>
where
    T: Schema + 'static,
{
    fn wire_schema(builder: &mut SchemaBuilder) -> TypeId {
        if core::any::TypeId::of::<T>() == core::any::TypeId::of::<u8>() {
            return builder
                .register_primitive_type(core::any::type_name::<Self>(), PrimitiveKind::Bytes);
        }

        let item = T::wire_schema(builder);
        builder.register_vec_type(item)
    }
}

impl<T> Schema for VecDeque<T>
where
    T: Schema + 'static,
{
    fn wire_schema(builder: &mut SchemaBuilder) -> TypeId {
        if core::any::TypeId::of::<T>() == core::any::TypeId::of::<u8>() {
            return builder
                .register_primitive_type(core::any::type_name::<Self>(), PrimitiveKind::Bytes);
        }

        let item = T::wire_schema(builder);
        builder.register_vec_type(item)
    }
}

impl<K, V> Schema for std::collections::BTreeMap<K, V>
where
    K: Schema,
    V: Schema,
{
    fn wire_schema(builder: &mut SchemaBuilder) -> TypeId {
        let key = K::wire_schema(builder);
        let value = V::wire_schema(builder);
        builder.register_map_type(key, value)
    }
}
