use awrk_macros::Type;

pub const DEFAULT_EXAMPLE_SERVER_HOST: &str = "127.0.0.1";
pub const DEFAULT_EXAMPLE_SERVER_PORT: u16 = 7780;
pub const DEFAULT_EXAMPLE_WORLD_ID: u64 = 100;

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

awrk_world::register_proxy_subscription! {
    all_of: [ReferenceEntity],
    any_of: [],
    none_of: [],
    components: [
        awrk_world::Name,
        ReferenceEntity,
        ReferenceKind,
        ReferencePosition,
        ReferenceVelocity,
        ReferenceHealth,
    ],
    outbound_create_components: [
        awrk_world::Name,
        ReferenceEntity,
        ReferenceKind,
        ReferencePosition,
        ReferenceVelocity,
        ReferenceHealth,
    ],
}
