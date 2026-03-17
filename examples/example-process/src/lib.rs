use awrk_example::{
    ActorId, ActorInfo, CreateActorArgs, CreateActorResult, ListActorsResult, ReferenceEntity,
    ReferenceHealth, ReferenceKind, ReferencePosition, ReferenceVelocity, SetActorVelocityArgs,
};
use awrk_world::core::World;
use awrk_world::{Name, Parent};

pub mod rpc {
    use awrk_example::rpc;
    use awrk_world::Process;

    use super::{create_actor, list_actors, set_actor_velocity};

    pub fn register(process: &mut Process) {
        process.register_rpc(rpc::LIST_ACTORS, |world, ()| list_actors(world));
        process.register_rpc(rpc::CREATE_ACTOR, create_actor);
        process.register_rpc(rpc::SET_ACTOR_VELOCITY, set_actor_velocity);
    }
}

fn list_actors(world: &mut World) -> Result<ListActorsResult, String> {
    Ok(ListActorsResult {
        actors: collect_actor_infos(world),
    })
}

fn create_actor(world: &mut World, args: CreateActorArgs) -> Result<CreateActorResult, String> {
    let parent = find_actors_root(world)?;
    let actor = world.spawn((
        ReferenceEntity,
        Name(args.name.clone()),
        ReferenceKind(args.kind.clone()),
        Parent { parent },
        args.position.clone(),
    ));

    if let Some(velocity) = args.velocity.clone() {
        world.entity_mut(actor)?.insert_one(velocity)?;
    }

    if let Some(health) = args.health.clone() {
        world.entity_mut(actor)?.insert_one(health)?;
    }

    let actor_info = actor_info(world, actor)?;
    Ok(CreateActorResult { actor: actor_info })
}

fn set_actor_velocity(world: &mut World, args: SetActorVelocityArgs) -> Result<(), String> {
    world
        .entity_mut(args.actor.0)?
        .insert_one(args.velocity)
        .map(|_| ())
}

fn find_actors_root(world: &mut World) -> Result<u64, String> {
    let mut actors_root = None;
    world.iter::<(&ReferenceEntity, Option<&ReferenceKind>, Option<&Name>), _>(
        |entity, (_, kind, name)| {
            if kind.is_some_and(|value| value.0 == "Collection")
                && name.is_some_and(|value| value.0 == "Actors")
            {
                actors_root = Some(entity);
            }
        },
    );

    actors_root.ok_or_else(|| "missing Actors collection root".to_string())
}

fn collect_actor_infos(world: &mut World) -> Vec<ActorInfo> {
    let mut actors = Vec::new();
    world.iter::<(
        &ReferenceEntity,
        Option<&Name>,
        Option<&ReferenceKind>,
        Option<&Parent>,
        Option<&ReferencePosition>,
        Option<&ReferenceVelocity>,
        Option<&ReferenceHealth>,
    ), _>(|entity, (_, name, kind, parent, position, velocity, health)| {
        if kind.is_some_and(|value| value.0 == "SceneRoot" || value.0 == "Collection") {
            return;
        }

        let Some(position) = position.cloned() else {
            return;
        };

        actors.push(ActorInfo {
            id: ActorId(entity),
            name: name
                .map(|value| value.0.clone())
                .unwrap_or_else(|| format!("entity-{entity}")),
            kind: kind
                .map(|value| value.0.clone())
                .unwrap_or_else(|| "Unknown".to_string()),
            parent: parent.map(|value| value.parent),
            position,
            velocity: velocity.cloned(),
            health: health.cloned(),
        });
    });

    actors.sort_by_key(|actor| actor.id.0);
    actors
}

fn actor_info(world: &mut World, actor: u64) -> Result<ActorInfo, String> {
    collect_actor_infos(world)
        .into_iter()
        .find(|info| info.id.0 == actor)
        .ok_or_else(|| format!("unknown actor: {actor}"))
}