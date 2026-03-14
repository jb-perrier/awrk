use crate::core::Process;
use crate::rpc::{
    DespawnArgs, EntityMeta, GetEntitiesArgs, GetEntitiesResult, ListEntitiesResult,
    ListTypesResult, PatchComponentArgs, PollChangesArgs, PollChangesResult, QueryEntitiesArgs,
    QueryEntitiesResult, RemoveComponentArgs, RemoveComponentResult, SetComponentArgs, SpawnArgs,
    SpawnResult,
};

pub fn register_builtin_rpcs(process: &mut Process) {
    process.register_rpc_typed::<(), ListEntitiesResult, _>("awrk.list_entities", |p, _| {
        let entities = p.snapshot_entities_meta();
        Ok(ListEntitiesResult {
            now: p.change_seq_now(),
            entities,
        })
    });

    process.register_rpc_typed::<(), ListTypesResult, _>("awrk.list_types", |p, _| {
        Ok(p.types().list_types())
    });

    process.register_rpc_typed::<QueryEntitiesArgs, QueryEntitiesResult, _>(
        "awrk.query_entities",
        |p, args| {
            use std::collections::BTreeSet;

            let limit = args.limit.unwrap_or(1024).min(100_000) as usize;
            let after = args.after.unwrap_or(0);

            let all_of: BTreeSet<String> = args.all_of.into_iter().collect();
            let any_of: BTreeSet<String> = args.any_of.into_iter().collect();
            let none_of: BTreeSet<String> = args.none_of.into_iter().collect();

            for t in all_of.iter().chain(any_of.iter()).chain(none_of.iter()) {
                if !p.types().is_registered_component_type(t.as_str()) {
                    return Err(format!("Unknown type: {t}"));
                }
            }

            let mut entities: Vec<hecs::Entity> = p.raw().iter().map(|e| e.entity()).collect();
            entities.sort_by_key(|e| e.to_bits().get());

            let mut out: Vec<EntityMeta> = Vec::new();
            let mut has_more = false;

            'entity_loop: for entity in entities {
                let bits = entity.to_bits().get();
                if bits <= after {
                    continue;
                }

                for t in &all_of {
                    if !p.types().has_component(p.raw(), entity, t)? {
                        continue 'entity_loop;
                    }
                }

                if !any_of.is_empty() {
                    let mut any_hit = false;
                    for t in &any_of {
                        if p.types().has_component(p.raw(), entity, t)? {
                            any_hit = true;
                            break;
                        }
                    }
                    if !any_hit {
                        continue 'entity_loop;
                    }
                }

                for t in &none_of {
                    if p.types().has_component(p.raw(), entity, t)? {
                        continue 'entity_loop;
                    }
                }

                out.push(EntityMeta {
                    entity: bits,
                    revision: p.snapshot_entity_info(entity)?.revision,
                    parent: p.raw().get::<&crate::Parent>(entity).ok().map(|p| p.parent),
                });

                if out.len() >= limit {
                    has_more = true;
                    break;
                }
            }

            let next_after = out.last().map(|m| m.entity);
            Ok(QueryEntitiesResult {
                entities: out,
                has_more,
                next_after,
            })
        },
    );

    process.register_rpc_typed::<GetEntitiesArgs, GetEntitiesResult, _>(
        "awrk.get_entities",
        |p, args| {
            let mut out = Vec::with_capacity(args.entities.len());
            for bits in args.entities {
                let Some(entity) = hecs::Entity::from_bits(bits) else {
                    continue;
                };
                if !p.raw().contains(entity) {
                    continue;
                }
                out.push(p.snapshot_entity_info(entity)?);
            }
            Ok(GetEntitiesResult { entities: out })
        },
    );

    process.register_rpc_typed::<SpawnArgs, SpawnResult, _>("awrk.spawn", |p, args| {
        let entity_bits = p.spawn_empty();
        let entity = hecs::Entity::from_bits(entity_bits).expect("spawned entity");

        for c in args.components {
            let value = c.value.unwrap_or(awrk_datex::value::Value::Unit);
            if let Err(e) = p.set_component_by_entity(entity, &c.type_name, value) {
                let _ = p.despawn(entity_bits);
                return Err(e);
            }
        }

        Ok(SpawnResult {
            entity: entity_bits,
        })
    });

    process.register_rpc_typed::<(), SpawnResult, _>("awrk.spawn_empty", |p, _| {
        Ok(SpawnResult {
            entity: p.spawn_empty(),
        })
    });

    process.register_rpc_typed::<DespawnArgs, (), _>("awrk.despawn", |p, args| {
        p.despawn(args.entity)?;
        Ok(())
    });

    process.register_rpc_typed::<RemoveComponentArgs, RemoveComponentResult, _>(
        "awrk.remove_component",
        |p, args| {
            let removed = p.remove_component(args.entity, &args.type_name)?;
            Ok(RemoveComponentResult { removed })
        },
    );

    process.register_rpc_typed::<SetComponentArgs, (), _>("awrk.set_component", |p, args| {
        let entity = hecs::Entity::from_bits(args.entity)
            .ok_or_else(|| format!("Unknown entity: {}", args.entity))?;
        if !p.raw().contains(entity) {
            return Err(format!("Unknown entity: {}", args.entity));
        }

        p.set_component_by_entity(entity, &args.type_name, args.value)?;
        p.bump_entity_revision(entity);
        Ok(())
    });

    process.register_rpc_typed::<PatchComponentArgs, (), _>("awrk.patch_component", |p, args| {
        let entity = hecs::Entity::from_bits(args.entity)
            .ok_or_else(|| format!("Unknown entity: {}", args.entity))?;
        if !p.raw().contains(entity) {
            return Err(format!("Unknown entity: {}", args.entity));
        }

        p.patch_component_by_entity(entity, &args.type_name, args.patch)?;
        p.bump_entity_revision(entity);
        Ok(())
    });

    process.register_rpc_typed::<PollChangesArgs, PollChangesResult, _>(
        "awrk.poll_changes",
        |p, args| p.poll_changes(args.since, args.limit),
    );
}
