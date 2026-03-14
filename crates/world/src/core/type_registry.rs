use std::collections::BTreeMap;

use awrk_datex::codec::decode::{DecodeConfig, decode_value_full};
use awrk_datex::codec::encode::{EncodeConfig, Encoder};
use awrk_datex::value::SerializedValueRef;
use awrk_datex::{Decode, Encode, Patch, PatchValidate};
use awrk_datex_schema::{SchemaBuilder, TypeKind, type_id};

use crate::rpc::{ComponentInfo, ListTypesResult, TypeCaps, TypeCapsInfo};

#[derive(Clone, Debug)]
enum ComponentShapeMeta {
    Opaque,
    Struct {
        fields_by_name: BTreeMap<String, u64>,
    },
    Tuple {
        len: usize,
    },
}

struct TypeEntry {
    type_name: String,
    caps: TypeCaps,
    component: Option<ComponentEntry>,
}

struct ComponentEntry {
    shape: ComponentShapeMeta,
    has: Box<dyn Fn(&hecs::World, hecs::Entity) -> bool + Send + Sync>,
    snapshot: Option<
        Box<
            dyn Fn(&hecs::World, hecs::Entity) -> Result<Option<awrk_datex::value::Value>, String>
                + Send
                + Sync,
        >,
    >,
    set: Option<
        Box<
            dyn Fn(&mut hecs::World, hecs::Entity, awrk_datex::value::Value) -> Result<(), String>
                + Send
                + Sync,
        >,
    >,
    patch: Option<
        Box<
            dyn Fn(&mut hecs::World, hecs::Entity, awrk_datex::value::Value) -> Result<(), String>
                + Send
                + Sync,
        >,
    >,
    remove: Box<dyn Fn(&mut hecs::World, hecs::Entity) -> Result<bool, String> + Send + Sync>,
}

#[derive(Default)]
pub struct WorldTypeRegistry {
    by_name: BTreeMap<String, TypeEntry>,
}

impl WorldTypeRegistry {
    pub fn register_schema_root_named(&mut self, type_name: String) {
        let entry = self
            .by_name
            .entry(type_name.clone())
            .or_insert_with(|| TypeEntry {
                type_name,
                caps: TypeCaps::default(),
                component: None,
            });
        entry.caps.is_schema_root = true;
    }

    pub fn list_types(&self) -> ListTypesResult {
        ListTypesResult {
            types: self
                .by_name
                .values()
                .map(|e| TypeCapsInfo {
                    type_name: e.type_name.clone(),
                    caps: e.caps,
                })
                .collect(),
        }
    }

    pub fn register_component_named<T>(&mut self, type_name: String) -> Result<(), String>
    where
        T: hecs::Component
            + Encode
            + for<'a> Decode<'a>
            + Patch
            + PatchValidate
            + awrk_datex_schema::Schema
            + Send
            + Sync
            + 'static,
    {
        self.register_component_base_named::<T>(type_name.clone())?;
        self.enable_component_read::<T>(&type_name)?;
        self.enable_component_write::<T>(&type_name)?;
        self.enable_component_patch::<T>(&type_name)?;
        Ok(())
    }

    pub fn register_component_base_named<T>(&mut self, type_name: String) -> Result<(), String>
    where
        T: hecs::Component + awrk_datex_schema::Schema + Send + Sync + 'static,
    {
        let shape = component_shape_meta::<T>(&type_name)?;

        let entry = self
            .by_name
            .entry(type_name.clone())
            .or_insert_with(|| TypeEntry {
                type_name: type_name.clone(),
                caps: TypeCaps::default(),
                component: None,
            });

        entry.caps.is_component = true;
        entry.caps.can_remove = true;

        let component = ComponentEntry {
            shape,
            has: Box::new(|world, entity| world.get::<&T>(entity).is_ok()),
            snapshot: None,
            set: None,
            patch: None,
            remove: Box::new(|world, entity| {
                if world.get::<&T>(entity).is_ok() {
                    let _ = world.remove_one::<T>(entity).map_err(|e| e.to_string())?;
                    Ok(true)
                } else {
                    Ok(false)
                }
            }),
        };
        entry.component = Some(component);
        Ok(())
    }

    pub fn enable_component_read<T>(&mut self, type_name: &str) -> Result<(), String>
    where
        T: hecs::Component + Encode + Send + Sync + 'static,
    {
        let entry = self
            .by_name
            .get_mut(type_name)
            .ok_or_else(|| format!("Unknown type: {type_name}"))?;

        if !entry.caps.is_component {
            return Err(format!("Type is not a component: {type_name}"));
        }

        let Some(component) = entry.component.as_mut() else {
            return Err("internal error: missing component entry".to_string());
        };

        component.snapshot = Some(Box::new(|world, entity| {
            if world.get::<&T>(entity).is_err() {
                return Ok(None);
            }
            let v = world.get::<&T>(entity).map_err(|e| e.to_string())?;
            let mut enc = Encoder::with_config(EncodeConfig::default());
            v.wire_encode(&mut enc).map_err(|e| format!("{e}"))?;
            let bytes = enc.into_inner();
            let value_ref =
                decode_value_full(&bytes, DecodeConfig::default()).map_err(|e| format!("{e}"))?;
            let owned =
                awrk_datex::value::Value::wire_decode(value_ref).map_err(|e| format!("{e}"))?;
            Ok(Some(owned))
        }));

        entry.caps.can_read = true;
        Ok(())
    }

    pub fn enable_component_write<T>(&mut self, type_name: &str) -> Result<(), String>
    where
        T: hecs::Component + for<'a> Decode<'a> + Send + Sync + 'static,
    {
        let entry = self
            .by_name
            .get_mut(type_name)
            .ok_or_else(|| format!("Unknown type: {type_name}"))?;

        if !entry.caps.is_component {
            return Err(format!("Type is not a component: {type_name}"));
        }

        let Some(component) = entry.component.as_mut() else {
            return Err("internal error: missing component entry".to_string());
        };

        let shape_for_set = component.shape.clone();
        let type_name_for_set = type_name.to_string();
        component.set = Some(Box::new(move |world, entity, value| {
            let value =
                coerce_component_value_for_decode(&type_name_for_set, &shape_for_set, value)?;
            let decoded: T = with_value_ref(&value, |value_ref| {
                T::wire_decode(value_ref).map_err(|e| format!("{e}"))
            })?;

            if world.get::<&T>(entity).is_ok() {
                let _ = world.remove_one::<T>(entity).map_err(|e| e.to_string())?;
            }

            world
                .insert_one(entity, decoded)
                .map_err(|e| e.to_string())?;
            Ok(())
        }));

        entry.caps.can_write = true;
        Ok(())
    }

    pub fn enable_component_patch<T>(&mut self, type_name: &str) -> Result<(), String>
    where
        T: hecs::Component + Patch + PatchValidate + Send + Sync + 'static,
    {
        let entry = self
            .by_name
            .get_mut(type_name)
            .ok_or_else(|| format!("Unknown type: {type_name}"))?;

        if !entry.caps.is_component {
            return Err(format!("Type is not a component: {type_name}"));
        }

        let Some(component) = entry.component.as_mut() else {
            return Err("internal error: missing component entry".to_string());
        };

        let shape_for_patch = component.shape.clone();
        let type_name_for_patch = type_name.to_string();

        component.patch = Some(Box::new(move |world, entity, patch| {
            let patch =
                coerce_component_patch_for_apply(&type_name_for_patch, &shape_for_patch, patch)?;

            with_value_ref(&patch, |patch_ref| {
                if let Ok(existing) = world.get::<&mut T>(entity) {
                    existing
                        .wire_patch_validate(patch_ref.clone())
                        .map_err(|e| format!("{e}"))?;
                }

                let mut existing = world
                    .get::<&mut T>(entity)
                    .map_err(|_| "missing component".to_string())?;
                existing.wire_patch(patch_ref).map_err(|e| format!("{e}"))?;
                Ok(())
            })?;
            Ok(())
        }));

        entry.caps.can_patch = true;
        Ok(())
    }

    pub fn has_component(
        &self,
        world: &hecs::World,
        entity: hecs::Entity,
        type_name: &str,
    ) -> Result<bool, String> {
        let entry = self
            .by_name
            .get(type_name)
            .ok_or_else(|| format!("Unknown type: {type_name}"))?;
        let Some(component) = &entry.component else {
            return Err(format!("Type is not a component: {type_name}"));
        };
        Ok((component.has)(world, entity))
    }

    pub fn type_names(&self) -> impl Iterator<Item = &str> {
        self.by_name.keys().map(String::as_str)
    }

    pub fn is_registered_component_type(&self, type_name: &str) -> bool {
        self.by_name
            .get(type_name)
            .is_some_and(|e| e.caps.is_component)
    }

    pub fn snapshot_entity_components(
        &self,
        world: &hecs::World,
        entity: hecs::Entity,
    ) -> Result<Vec<ComponentInfo>, String> {
        let mut out = Vec::new();
        for (name, entry) in &self.by_name {
            if !entry.caps.is_component {
                continue;
            }

            let value = match entry.component.as_ref().and_then(|c| c.snapshot.as_ref()) {
                Some(snapshot) => snapshot(world, entity)?,
                None => None,
            };

            if let Some(value) = value {
                out.push(ComponentInfo {
                    type_name: name.clone(),
                    value: Some(value),
                });
            }
        }
        Ok(out)
    }

    pub fn snapshot_component(
        &self,
        world: &hecs::World,
        entity: hecs::Entity,
        type_name: &str,
    ) -> Result<Option<ComponentInfo>, String> {
        let entry = self
            .by_name
            .get(type_name)
            .ok_or_else(|| format!("Unknown type: {type_name}"))?;
        if !entry.caps.is_component {
            return Err(format!("Type is not a component: {type_name}"));
        }

        let value = match entry.component.as_ref().and_then(|c| c.snapshot.as_ref()) {
            Some(snapshot) => snapshot(world, entity)?,
            None => None,
        };

        Ok(value.map(|value| ComponentInfo {
            type_name: type_name.to_string(),
            value: Some(value),
        }))
    }

    pub fn set_component(
        &self,
        world: &mut hecs::World,
        entity: hecs::Entity,
        type_name: &str,
        value: awrk_datex::value::Value,
    ) -> Result<(), String> {
        let entry = self
            .by_name
            .get(type_name)
            .ok_or_else(|| format!("Unknown type: {type_name}"))?;
        if !entry.caps.is_component {
            return Err(format!("Type is not a component: {type_name}"));
        }
        if !entry.caps.can_write {
            return Err(format!("Type is not writable: {type_name}"));
        }
        let Some(component) = &entry.component else {
            return Err("internal error: missing component entry".to_string());
        };
        let Some(set) = &component.set else {
            return Err("internal error: missing set handler".to_string());
        };
        set(world, entity, value)
    }

    pub fn patch_component(
        &self,
        world: &mut hecs::World,
        entity: hecs::Entity,
        type_name: &str,
        patch: awrk_datex::value::Value,
    ) -> Result<(), String> {
        let entry = self
            .by_name
            .get(type_name)
            .ok_or_else(|| format!("Unknown type: {type_name}"))?;
        if !entry.caps.is_component {
            return Err(format!("Type is not a component: {type_name}"));
        }
        if !entry.caps.can_patch {
            return Err(format!("Type is not patchable: {type_name}"));
        }
        let Some(component) = &entry.component else {
            return Err("internal error: missing component entry".to_string());
        };
        let Some(patch_fn) = &component.patch else {
            return Err("internal error: missing patch handler".to_string());
        };
        patch_fn(world, entity, patch)
    }

    pub fn remove_component(
        &self,
        world: &mut hecs::World,
        entity: hecs::Entity,
        type_name: &str,
    ) -> Result<bool, String> {
        let entry = self
            .by_name
            .get(type_name)
            .ok_or_else(|| format!("Unknown type: {type_name}"))?;
        if !entry.caps.is_component {
            return Err(format!("Type is not a component: {type_name}"));
        }
        let Some(component) = &entry.component else {
            return Err("internal error: missing component entry".to_string());
        };
        (component.remove)(world, entity)
    }
}

fn with_value_ref<T>(
    value: &awrk_datex::value::Value,
    f: impl for<'a> FnOnce(SerializedValueRef<'a>) -> Result<T, String>,
) -> Result<T, String> {
    let mut enc = Encoder::with_config(EncodeConfig::default());
    value.wire_encode(&mut enc).map_err(|e| format!("{e}"))?;
    let bytes = enc.into_inner();
    let value_ref =
        decode_value_full(&bytes, DecodeConfig::default()).map_err(|e| format!("{e}"))?;
    f(value_ref)
}

fn component_shape_meta<T: awrk_datex_schema::Schema>(
    type_name: &str,
) -> Result<ComponentShapeMeta, String> {
    let mut b = SchemaBuilder::new();
    let ty = <T as awrk_datex_schema::Schema>::wire_schema(&mut b);
    let schema = b.build().map_err(|e| e.to_string())?;

    let def = schema
        .types
        .get(&ty)
        .ok_or_else(|| "type not registered".to_string())?;

    match &def.kind {
        TypeKind::Struct { fields } => {
            let mut fields_by_name = BTreeMap::new();
            for fid in fields {
                let fdef = schema
                    .fields
                    .get(fid)
                    .ok_or_else(|| "field not registered".to_string())?;
                let name = schema
                    .string(fdef.name)
                    .ok_or_else(|| "missing field name".to_string())?
                    .to_string();
                fields_by_name.insert(name, fid.0);
            }
            let _ = type_id(type_name);
            Ok(ComponentShapeMeta::Struct { fields_by_name })
        }
        TypeKind::Tuple { items } => Ok(ComponentShapeMeta::Tuple { len: items.len() }),
        _ => Ok(ComponentShapeMeta::Opaque),
    }
}

fn coerce_component_value_for_decode(
    _type_name: &str,
    shape: &ComponentShapeMeta,
    value: awrk_datex::value::Value,
) -> Result<awrk_datex::value::Value, String> {
    match shape {
        ComponentShapeMeta::Tuple { len } if *len == 1 => match value {
            awrk_datex::value::Value::Array(_) => Ok(value),
            other => Ok(awrk_datex::value::Value::Array(vec![other])),
        },
        ComponentShapeMeta::Struct { fields_by_name, .. } => match value {
            awrk_datex::value::Value::Map(entries) => {
                let mut out = Vec::with_capacity(entries.len());
                for (k, v) in entries {
                    match k {
                        awrk_datex::value::Value::String(name) => {
                            let fid = fields_by_name
                                .get(&name)
                                .ok_or_else(|| format!("unknown field: {name}"))?;
                            out.push((awrk_datex::value::Value::U64(*fid), v));
                        }
                        awrk_datex::value::Value::U64(fid) => {
                            out.push((awrk_datex::value::Value::U64(fid), v));
                        }
                        other => {
                            return Err(format!(
                                "invalid struct key (expected string field name or u64 field id): {other:?}"
                            ));
                        }
                    }
                }
                Ok(awrk_datex::value::Value::Map(out))
            }
            other => Ok(other),
        },
        _ => Ok(value),
    }
}

fn coerce_component_patch_for_apply(
    type_name: &str,
    shape: &ComponentShapeMeta,
    patch: awrk_datex::value::Value,
) -> Result<awrk_datex::value::Value, String> {
    match shape {
        ComponentShapeMeta::Struct { .. } => {
            let coerced = coerce_component_value_for_decode(type_name, shape, patch)?;
            match coerced {
                awrk_datex::value::Value::Map(_) => Ok(coerced),
                _ => Err("expected map patch".to_string()),
            }
        }
        _ => Ok(patch),
    }
}

impl core::fmt::Debug for TypeEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TypeEntry")
            .field("type_name", &self.type_name)
            .finish()
    }
}
