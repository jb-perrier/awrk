use awrk_macros::Type;

pub const DEFAULT_EXAMPLE_PROCESS_HOST: &str = "127.0.0.1";
pub const DEFAULT_EXAMPLE_PROCESS_PORT: u16 = 7780;

#[Type]
#[derive(Clone, Debug)]
pub struct ReferenceEntity;

#[Type]
#[derive(Clone, Debug)]
pub struct ReferenceKind(pub String);

#[Type]
#[derive(Clone, Debug)]
pub struct ReferencePosition {
    pub x: f32,
    pub y: f32,
}

impl ReferencePosition {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[Type]
#[derive(Clone, Debug)]
pub struct ReferenceVelocity {
    pub dx: f32,
    pub dy: f32,
}

impl ReferenceVelocity {
    pub const fn new(dx: f32, dy: f32) -> Self {
        Self { dx, dy }
    }
}

#[Type]
#[derive(Clone, Debug)]
pub struct ReferenceHealth {
    pub current: u32,
    pub max: u32,
}

impl ReferenceHealth {
    pub const fn new(current: u32, max: u32) -> Self {
        Self { current, max }
    }
}

#[Type]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ActorId(pub u64);

#[Type]
#[derive(Clone, Debug)]
pub struct ActorInfo {
    pub id: ActorId,
    pub name: String,
    pub kind: String,
    pub parent: Option<u64>,
    pub position: ReferencePosition,
    pub velocity: Option<ReferenceVelocity>,
    pub health: Option<ReferenceHealth>,
}

#[Type]
#[derive(Clone, Debug)]
pub struct CreateActorArgs {
    pub name: String,
    pub kind: String,
    pub position: ReferencePosition,
    pub velocity: Option<ReferenceVelocity>,
    pub health: Option<ReferenceHealth>,
}

#[Type]
#[derive(Clone, Debug)]
pub struct CreateActorResult {
    pub actor: ActorInfo,
}

#[Type]
#[derive(Clone, Debug, Default)]
pub struct ListActorsResult {
    pub actors: Vec<ActorInfo>,
}

#[Type]
#[derive(Clone, Debug)]
pub struct SetActorVelocityArgs {
    pub actor: ActorId,
    pub velocity: ReferenceVelocity,
}

pub mod rpc {
    use super::{CreateActorArgs, CreateActorResult, ListActorsResult, SetActorVelocityArgs};
    use awrk_world::Rpc;

    pub const LIST_ACTORS: Rpc<(), ListActorsResult> = Rpc::new("example.list_actors");
    pub const CREATE_ACTOR: Rpc<CreateActorArgs, CreateActorResult> =
        Rpc::new("example.create_actor");
    pub const SET_ACTOR_VELOCITY: Rpc<SetActorVelocityArgs, ()> =
        Rpc::new("example.set_actor_velocity");
}
