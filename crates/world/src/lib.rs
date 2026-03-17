extern crate self as awrk_world;

pub use awrk_world_ecs::{Name, Parent};

pub mod core;
pub mod registration;
pub mod rpc;
pub mod transport;

pub use core::{
    Process, ProcessParts, Resources, Rpcs, Sessions, World, WorldArgs, WorldEntityMut,
};
pub use inventory;
pub use rpc::{ProcessClient, ProcessClientError, ProcessClientOptions, Rpc, RpcTrace};
