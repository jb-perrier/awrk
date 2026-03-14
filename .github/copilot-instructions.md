# Copilot instructions (wpi / awrk)

## Goal
This repo implements a local-only ECS/world runtime around `hecs::World`, an explicit TCP RPC protocol for inspecting and mutating worlds, shared DATEX/schema crates, and a few reference/demo applications.

The transport is intended for **local machine tooling, debugging, and process-to-process integration only**. Do **not** design or modify it for Internet/LAN exposure or untrusted peers.

When making changes, optimize for:
- small, explicit runtime behavior
- focused diffs
- clear `Result<_, String>` error propagation across RPC boundaries
- keeping the RPC/wire format and bridge semantics stable unless a deliberate migration is being made

## Workspace layout
- `crates/world`
  - world runtime, TCP sessions, built-in RPCs, `Process`, `WorldClient`, `WorldBridge`
  - key files:
    - `src/core/process.rs`
    - `src/rpc/builtins.rs`
    - `src/rpc/api.rs`
    - `src/rpc/client.rs`
    - `src/transport/session.rs`
    - `src/bridge/mod.rs`
- `crates/world-ecs`
  - shared ECS-facing components such as `Name`, `Parent`, proxy metadata, and cross-world references
- `crates/datex/datex`
  - DATEX value encoding/decoding and patching
- `crates/datex/rpc`
  - RPC envelope and invocation/result protocol types
- `crates/datex/macros`
  - DATEX encode/decode/patch derives
- `crates/datex/viz`
  - visualization helpers for DATEX values
- `crates/schema/schema`
  - schema model and runtime registration support
- `crates/schema/macros`
  - schema derive proc macros
- `crates/macros`
  - `#[Type]`
- `crates/core`
  - small shared utilities such as semantic UUID support
- `crates/explorer-ui`
  - egui client for browsing a remote world
- `crates/win/api`, `crates/win/server`
  - shared window-domain types plus a concrete server example
- `examples/example-model`, `examples/example-server`, `examples/example-consumer`
  - canonical examples showing a server world and a `WorldBridge`-based proxy consumer, including proxy-intent create/despawn flow

## Wire/runtime rules
- Keep the transport **RPC-oriented and explicit**.
- Keep framing and session behavior stable unless intentionally changing the protocol.
- Built-in world RPCs are registered in `crates/world/src/rpc/builtins.rs` and use argument/result types from `crates/world/src/rpc/api.rs`.
- World snapshots and component payloads use `awrk_datex::value::Value`.
- Request/response failures should surface as RPC errors where possible instead of being hidden locally.

## Built-in world RPCs
`Process::new` registers built-in procedures before discovered type/subscription registrations.

Current built-ins include:
- introspection
  - `awrk.list_entities`
  - `awrk.list_types`
  - `awrk.query_entities`
  - `awrk.get_entities`
- mutations
  - `awrk.spawn`
  - `awrk.spawn_empty`
  - `awrk.despawn`
  - `awrk.remove_component`
  - `awrk.set_component`
  - `awrk.patch_component`
  - `awrk.poll_changes`

If you add or change a built-in RPC:
- update protocol structs in `crates/world/src/rpc/api.rs`
- register it in `register_builtin_rpcs` in `crates/world/src/rpc/builtins.rs`
- ensure `Process::new` still wires built-ins before inventory-discovered registrations in `crates/world/src/core/process.rs`
- preserve naming consistency with the existing `awrk.*` namespace
- keep all shipped clients/examples aligned

## Type registration and macros
- `#[Type]` generates DATEX/schema derives and auto-registers the type into `Process` via inventory.
- Prefer using the existing registration flow rather than hand-rolling duplicate registries.
- When working with bridge mirroring, remember that not every registered type is a component and not every component should be bridge-managed.

Macro split to keep in mind:
- `awrk-macros`: `#[Type]`
- `awrk-datex-macros`: DATEX encode/decode/patch derives
- `awrk-schema-macros`: schema derives

## World bridge expectations
- `WorldBridge` mirrors remote entities into normal local ECS entities with proxy metadata.
- Prefer transparent local reads of mirrored state.
- Keep bridge-managed metadata explicit:
  - `ProxyEntity`
  - `ProxyState`
  - `ProxyAuthority`
  - `RemoteParentRef`
- Avoid mixing bridge bookkeeping with domain components unless there is a clear reason.
- If changing proxy semantics, also review:
  - `crates/world/src/bridge/mod.rs`
  - `docs/world-proxy-entities-architecture.md`
  - the example crates under `examples/` as `example-model`, `example-server`, and `example-consumer`

## Coding guidelines
- Prefer small, direct abstractions over generic framework layers.
- Prefer using existing helpers like `WorldClient`, `Process`, and `WorldBridge` instead of duplicating transport logic.
- Keep session loops predictable.
- Avoid casual wire-format or schema changes.
- Preserve current naming and code style.
- When changing a feature used by both a server and a client, update both sides together.

## Testing workflow
Use small manual checks with separate terminals.

### Multi-process runbook
- For server/client or server/UI checks, start the server first and keep it running in its own terminal or background task.
- Wait until the server has completed startup and bound its TCP port before starting the dependent process.
- Use the documented ports for the example being exercised unless the task requires a different port.
- If a launch fails because the port is already in use, stop the old process or choose an explicit alternate port and keep both commands aligned.
- When finished, stop long-running server processes explicitly so later checks do not accidentally attach to stale state.

### Reference example
1. Start the reference server:
  - `cargo run -p awrk-example-server -- 7780`
2. Start the reference consumer:
  - `cargo run -p awrk-example-consumer`

Expected flow:
- wait for the server to finish startup before launching the consumer
- keep the server on port `7780` unless the consumer is also updated to use a different port

This demonstrates:
- world/entity exposure from a server
- proxy mirroring through `WorldBridge`
- local reads of mirrored entities/components

## Notes
- Keep long-running servers in separate terminals or background tasks.
- On Windows PowerShell, `Start-Process` can be useful for detached runs.
- If a change touches user-visible examples, keep docs and example paths current.
