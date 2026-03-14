# Process

`Process` is the main entry point for an `awrk-world` runtime.

It contains:

- `World`: the local ECS world
- `Rpcs`: the built-in runtime RPC registry
- `Sessions`: the local TCP session listener and active clients
- `Remotes`: remote-world mirroring through `WorldBridge`
- a process name used for logs and startup

When you create a `Process`, it already wires in the built-in world types, the built-in `awrk.*` RPCs, and inventory-discovered ECS registrations such as `#[Type]` and proxy subscription declarations.

## Basic process

1. Create a `Process`
2. Register your own components or types if needed
3. Split it into `ProcessParts`
4. Start sessions
5. Run your loop

```rust
use awrk_world::{Name, Process, ProcessParts};

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ProcessParts {
        name,
        mut world,
        mut remotes,
        mut rpcs,
        mut sessions,
    } = Process::new_with_sessions("my-process", 7780).into_parts();

    sessions.start(&name)?;

    let _root = world.spawn((Name("Root".to_string()),));

    loop {
        sessions.handle(&mut world, &mut rpcs);
        remotes.tick_all(&mut world).map_err(std::io::Error::other)?;
    }
}
```

If you want the port to come from the CLI, use `Process::from_args("my-process")` instead. It understands the `--port` world argument.

`into_parts()` returns:

- `world` for ECS reads and writes
- `sessions` for incoming local RPC traffic
- `rpcs` for the registered procedure set used by sessions
- `rpcs` only contains the built-in runtime procedures in the `awrk.*` namespace
- `remotes` for remote-world mirroring

`Process::tick()` runs `sessions.handle(...)` and `remotes.tick_all(...)`.

## ECS features

Main operations:

- spawn entities with `world.spawn(...)` or `world.spawn_empty()`
- remove entities with `world.despawn(...)`
- iterate with `world.iter(...)`
- read and write components with `component`, `component_mut`, and `entity_mut`
- apply dynamic component writes and patches through the registered type system

```rust
let player = world.spawn((Name("Player".to_string()),));

world.entity_mut(player)?.insert_one(Health { current: 10, max: 10 })?;
```

Register components before calling `into_parts()`:

```rust
let mut process = Process::new("my-process");
process.register_component::<Health>()?;
```

If a type should exist in schema/runtime space but is not a hecs component, use `register_type::<T>()` instead.

## Remote worlds

`Remotes` lets one local process mirror one or more remote worlds into its own local ECS world.

1. Build a `WorldBridgeRemoteConfig`
2. Add it with `remotes.add_remote(...)`
3. Call `remotes.tick_all(&mut world)` every loop

```rust
use awrk_world::WorldBridgeRemoteConfig;

let config = WorldBridgeRemoteConfig::new(1, "127.0.0.1", 7780);

remotes.add_remote(config)?;
```

Mirrored entities use proxy metadata such as `ProxyEntity`, `ProxyState`, `ProxyAuthority`, and `RemoteParentRef`.

`ProxySpawnRequest` and `ProxySpawnError` are also public, tooling-visible bridge command/status components for bridge-managed proxy creation flows.

## Introspection and built-in RPCs

Introspection RPCs:

- `awrk.list_entities`: snapshot entity ids, revisions, and parent links
- `awrk.list_types`: discover registered types and capabilities
- `awrk.query_entities`: filter entities by component type names
- `awrk.get_entities`: fetch full entity/component payloads

Mutation RPCs:

- `awrk.spawn`
- `awrk.spawn_empty`
- `awrk.despawn`
- `awrk.remove_component`
- `awrk.set_component`
- `awrk.patch_component`
- `awrk.poll_changes`

These are low-level transport/runtime primitives. Tooling and debugging code may call them directly.

App-facing domain workflows should prefer entities, components, bridge-managed proxies, and command/status components such as `ProxySpawnRequest` and `ProxySpawnError` rather than app-facing RPC procedures.

## Registration Model

Process crates are expected to contribute:

- component and type definitions
- proxy subscription declarations
- ECS systems and loops
- command and status components for domain behavior

Process crates are not expected to expose custom network procedures.

Built-in `awrk.*` RPCs remain as the runtime transport and introspection substrate.

## Examples

- [examples/example-server/src/main.rs](../examples/example-server/src/main.rs): a local process exposing a reference ECS world over RPC
- [examples/example-consumer/src/main.rs](../examples/example-consumer/src/main.rs): the canonical bridge example, showing remote mirroring plus local proxy-intent creation and local removal driving remote despawn