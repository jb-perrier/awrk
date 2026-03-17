# Process

`Process` is the host object for an `awrk-world` runtime.

It contains:

- `World`: the local ECS world
- `Rpcs`: the runtime RPC registry
- `Sessions`: the local TCP session listener and active clients
- `Resources`: a typed host-side resource bag for clients, services, caches, and config
- a process name used for logs and startup

When you create a `Process`, it wires in built-in world types, built-in `awrk.*` procedures, and inventory-discovered `#[Type]` registrations.

Custom domain RPCs are not auto-registered. Serving processes must register them explicitly during bootstrap.

## Crate naming

When implementing a domain around `Process`, use this crate split:

- `<crate_name>`: shared domain crate for types, components, schema-facing definitions, and typed `Rpc<Args, Result>` descriptors
- `<crate_name>-process`: reusable process-side implementation for `register(process)`, RPC handlers, process-local resources, and domain systems built on top of `Process`
- `<crate_name>-process-bin`: concrete executable wrapper for CLI, logging, ports, event loops, and other binary-specific startup glue

The shared `<crate_name>` crate should not own `register(process)`.

## Basic process

1. Create a `Process`
2. Register domain RPC sets, extra components, or types if needed
3. Start sessions if the process should accept inbound RPC
4. Run your loop

```rust
use awrk_world::{Name, Process};

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut process = Process::new_with_sessions("my-process", 7780);
    my_domain::rpc::register(&mut process);
    process.sessions_mut().start(process.name())?;

    let _root = process.world_mut().spawn((Name("Root".to_string()),));

    loop {
        process.tick()?;
    }
}
```

If you want the port to come from the CLI, use `Process::from_args("my-process")`. In the current version, it reads the port as a positional world argument, for example `cargo run -p awrk-example-process-bin -- 7780`.

`Process::tick()` only runs `sessions.handle(&mut world, &mut rpcs)`.

Use `into_parts()` when an external event loop needs to own the runtime state directly, such as a `winit` application.

## World access

Main operations:

- spawn entities with `world.spawn(...)` or `world.spawn_empty()`
- remove entities with `world.despawn(...)`
- iterate with `world.iter(...)`
- read and write components with `component`, `component_mut`, and `entity_mut`
- apply dynamic component writes and patches through the registered type system

```rust
let player = process.world_mut().spawn((Name("Player".to_string()),));

process
    .world_mut()
    .entity_mut(player)?
    .insert_one(Health { current: 10, max: 10 })?;
```

Register components before serving traffic:

```rust
let mut process = Process::new("my-process");
process.register_component::<Health>()?;
```

If a type should exist in schema/runtime space but is not a hecs component, use `register_type::<T>()` instead.

## Resources

`Resources` is the host-side storage mechanism for outbound clients and app services.

Typical uses:

- shared `ProcessClient` connections
- typed RPC descriptors from shared domain crates such as `awrk_win::rpc` or `awrk_example::rpc`
- local projection caches
- configuration
- app-owned services

```rust
let mut process = Process::new("consumer");
process
    .resources_mut()
    .insert(ProcessClient::connect("127.0.0.1", 7780, ProcessClientOptions::default())?);

let actors = process
    .resource_mut::<ProcessClient>()?
    .invoke(awrk_example::rpc::LIST_ACTORS, ())?;
```

Resources are keyed by concrete type. Missing lookups return a clear `Result<_, String>` error from `Process::resource()` and `Process::resource_mut()`.

## RPC model

There are two RPC layers.

- `awrk.*` procedures are low-level runtime/tooling primitives.
- custom domain procedures are the app-facing integration surface and must be explicitly registered by serving processes.

Built-in introspection/runtime procedures include:

- `awrk.list_entities`
- `awrk.list_types`
- `awrk.list_procedures`
- `awrk.query_entities`
- `awrk.get_entities`
- `awrk.spawn`
- `awrk.spawn_empty`
- `awrk.despawn`
- `awrk.remove_component`
- `awrk.set_component`
- `awrk.patch_component`
- `awrk.poll_changes`

Tooling and debugging code may call these directly. Domain behavior should usually live behind explicit namespaced procedures such as `win.create_window` or `example.list_actors`.

## Explicit domain registration

Shared domain crates should own typed RPC descriptors. Process crates should own the server-side registration list and handler bodies.

```rust
// awrk-example
pub mod rpc {
    use awrk_world::Rpc;

    pub const LIST_ACTORS: Rpc<(), ListActorsResult> = Rpc::new("example.list_actors");
}
```

```rust
// awrk-example-process
pub mod rpc {
    use awrk_example::rpc;
    use awrk_world::Process;

    pub fn register(process: &mut Process) {
        process.register_rpc(rpc::LIST_ACTORS, |world, ()| list_actors(world));
        process.register_rpc(rpc::CREATE_ACTOR, create_actor);
    }
}
```

Serving binaries must call that registration during bootstrap before they start serving traffic.

```rust
let mut process = Process::from_args("example-process");
awrk_example_process::rpc::register(&mut process);
process.sessions_mut().start(process.name())?;
```

Clients use the same typed descriptors with `ProcessClient::invoke(...)`, but they do not need to register handlers locally.

Duplicate procedure names and invalid schema registrations still fail fast during runtime startup or explicit registration.

## Examples

- [examples/example-process-bin/src/main.rs](../examples/example-process-bin/src/main.rs): a local process exposing a reference ECS world plus explicit example-domain RPCs
- [examples/example-consumer/src/main.rs](../examples/example-consumer/src/main.rs): a `ProcessClient` stored in `Process` resources and used with typed example-domain RPC descriptors
- [crates/win/process-bin/src/main.rs](../crates/win/process-bin/src/main.rs): a `winit` host using explicit window-domain RPCs over the runtime session layer