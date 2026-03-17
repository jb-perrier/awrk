# Remote Bridge Design

This note proposes a generic pattern for exposing a remote `Process` through local ECS components and explicit sync systems, while keeping RPC transport details hidden from most application code.

The intent is to move from:

- application systems calling typed clients directly
- transport-aware code spread across app logic

to:

- local projected entities and components for confirmed remote state
- one-shot intent components for remote writes
- explicit bridge sync functions that translate ECS <-> RPC

This is a design proposal, not an implemented runtime feature.

## Goals

- keep the wire protocol explicit and domain-specific
- hide `ProcessClient` and per-domain client calls from most app systems
- let app code read remote state through normal ECS queries
- keep remote processes authoritative for their own state
- avoid reviving implicit proxy or bridge semantics inside `awrk-world`
- keep sync execution explicit through normal tick functions

## Non-Goals

- no hidden background replication in `Process`
- no generic automatic world mirroring in `awrk-world`
- no transport writes through generic `awrk.set_component` for app behavior
- no optimistic local mutation pretending remote writes already succeeded

## Current Baseline

Today the codebase mostly uses `ProcessClient` plus typed domain RPC descriptors stored in `Process` resources.

Examples:

- `ProcessClient` with `example_model::rpc::*` in [examples/example-consumer/src/main.rs](../examples/example-consumer/src/main.rs)
- `awrk_win::rpc::*` descriptors and explicit `win.*` RPCs in [crates/win/shared/src/lib.rs](../crates/win/shared/src/lib.rs)

Serving processes are expected to opt into those domain RPCs explicitly by calling the domain crate's `rpc::register(&mut Process)` function during bootstrap.

That baseline keeps transport explicit while avoiding a wrapper type per domain.

## Proposed Model

Each remote domain exposes a bridge layer with four parts:

1. projected ECS components for confirmed remote state
2. one-shot intent components for requested remote mutations
3. a bridge resource that owns the client and sync cursors
4. explicit sync functions that run inside the app tick

The important separation is:

- projected components are confirmed remote state
- intent components are requested actions
- bridge resources hold transport state and retry bookkeeping

This keeps domain state clean and avoids mixing sync flags into application-facing components.

## Why Not Dirty Flags In Components

The tempting model is a component like:

```rust
struct Window {
    changed: bool,
    desired_width: u32,
    width: u32,
}
```

That shape has several problems:

- it mixes domain state with transport bookkeeping
- it is unclear who clears `changed` and when
- failed RPCs leave local state in an awkward half-applied state
- inbound remote updates compete with local dirty tracking
- every domain starts reinventing per-component sync state machines

Instead, local components should describe domain state, while the bridge resource tracks what has been sent, confirmed, or needs retry.

For projected state components, the normal ownership split should be:

- app systems read them
- `sync_in` or reconciliation code writes them
- app systems request changes by adding intent components instead of mutating projected state directly

## Core Design Rules

### 1. Remote Process Is Authoritative

The remote process owns lifecycle and current truth.

Local ECS is a projection of that truth plus a queue of user intent.

### 2. Confirmed State And Requested Actions Are Separate

Use normal components for confirmed projected state.

In practice those projected state components should usually be treated as read-only by app systems. The bridge updates them in `sync_in` when confirmed state arrives from the remote process.

Use separate intent components for writes such as:

- create
- update size
- update title
- close

### 3. Writes Are Explicit And Domain-Specific

Bridge systems should call explicit domain RPCs such as:

- `win.create_window`
- `win.set_inner_size`
- `win.set_title`
- `win.close_window`

This is preferred over generic built-in mutation RPCs for app-facing behavior.

### 4. Sync Is Explicit In The App Tick

The bridge does not run on its own.

Applications call explicit functions like:

```rust
fn tick(process: &mut Process) -> Result<(), String> {
    process.tick()?;
    remote_bridge_sync_out(process)?;
    remote_bridge_sync_in(process)?;
    Ok(())
}
```

This keeps ordering predictable and matches the current `Process` model.

### 5. Stale Projections Are Acceptable During Disconnects

If the transport is down:

- keep the last confirmed projection in ECS
- keep or retry pending intents according to bridge policy
- mark bridge connectivity separately if needed

Do not silently clear projected entities just because the remote is temporarily unavailable.

## Bridge Resource Shape

In practice each domain should own a concrete bridge resource instead of trying to fit everything into a reusable generic struct.

Mappings, retry policy, cursors, connectivity state, and temporary reconciliation data are domain-specific enough that a custom resource is the better default.

For example:

```rust
pub struct WinBridge {
    pub client: ProcessClient,
    pub event_cursor: u64,
    pub connected: bool,
}
```

The reusable part should probably be conventions and helper traits, not a deep generic framework.

## Suggested Bridge Contract

Each remote domain helper should expose something close to:

```rust
pub fn sync_out(process: &mut Process) -> Result<(), String>;
pub fn sync_in(process: &mut Process) -> Result<(), String>;
```

or:

```rust
pub fn tick_bridge(process: &mut Process) -> Result<(), String> {
    sync_out(process)?;
    sync_in(process)?;
    Ok(())
}
```

This keeps the bridge easy to call from examples and applications without forcing runtime changes into `Process` itself.

## ECS Surface Area

The bridge should expose only the minimum ECS surface needed by app code.

In practice, it is usually better for that surface to be the root-level API of the bridge helper, with transport-specific details moved into a dedicated nested module. That keeps the ownership boundary obvious:

- app-facing projected state and intent components should usually be the root-level API of the bridge helper
- transport-facing types, RPC adapters, and other protocol-specific details should live in a dedicated nested module

Recommended categories:

- remote identity or binding
- projected state
- write intents
- optional bridge-local sync status for diagnostics

Avoid adding transport-oriented flags like `changed`, `pending_send`, or `last_sent` directly to the main domain components.

## `win` Domain Example

### Current State

The `win` domain already has:

- explicit create, close, list, and poll RPCs in [crates/win/shared/src/lib.rs](../crates/win/shared/src/lib.rs)
- a process-side registration crate in [crates/win/process/src/lib.rs](../crates/win/process/src/lib.rs)
- a concrete `winit` host in [crates/win/process-bin/src/main.rs](../crates/win/process-bin/src/main.rs)

Current RPCs are enough for:

- remote creation
- remote close
- reading remote snapshots
- reading remote event feed

Current RPCs are not enough for a full desired-state bridge, because there is no explicit update RPC for changing title or size after creation.

### Proposed Bridge-Owned Components

The app-facing ECS types should probably be the root-level API of the bridge helper.

Then the more transport-specific pieces can live in a dedicated nested module. That keeps the ownership model clearer:

- root-level bridge types expose the projection and intents used by app systems
- a nested module can hold adapters to `awrk_win` payloads, reconciliation helpers, and other protocol-facing details

That means the ECS shape should probably look more like this:

```rust
use awrk_win::WindowHandle;

pub struct Window {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub focused: bool,
    pub status: WindowStatus,
}

pub struct WindowHandleBinding(pub WindowHandle);

pub enum WindowStatus {
    Pending,
    Ready,
    CreateFailed { message: String },
}

pub struct CreateWindow {
    pub title: String,
    pub width: u32,
    pub height: u32,
}

pub struct SetWindowSize {
    pub width: u32,
    pub height: u32,
}

pub struct SetWindowTitle {
    pub title: String,
}

pub struct CloseWindow;

pub mod protocol {
    pub use awrk_win::{WinEventKind, WindowInfo};

    // Conversion and reconciliation helpers can translate between
    // bridge-local Window / CreateWindow values and awrk_win payloads.
}
```

Notes:

- bridge-owned entities carry confirmed state, and the app-facing projection types stay at the root where app systems can discover them easily
- the projected `Window` component should usually be updated only by `sync_in` or reconciliation code
- write requests are one-shot components that the bridge consumes
- the bridge removes an intent only after the corresponding RPC succeeds

This is a better default than either:

- reusing the authoritative shared domain components directly in app-facing ECS projections
- introducing a synthetic aggregate observed-state struct

Reasons:

- `WindowInfo` already exists as the RPC snapshot shape
- `WinEventKind` already carries `WindowInfo` and `WinInnerSize`
- the nested protocol module remains the place for transport/domain types instead of bridge-owned ECS state
- the app-facing surface stays compact and easier to reason about as a confirmed projection model

`WindowInfo` is still useful as a shared payload type, but the projected ECS components and create intent do not need to be those exact types.

If a domain later proves that aggregate snapshots are materially easier to work with, that can still be a deliberate domain choice. It should not be the default pattern in this document.

### Proposed `win` Bridge Resource

```rust
pub struct WinBridge {
    pub client: ProcessClient,
    pub event_cursor: u64,
    pub connected: bool,
}
```

If entity-to-handle mapping is not kept entirely in ECS, the bridge resource may also keep temporary lookup maps.

### Proposed `win_sync_out`

`win_sync_out(process)` should:

1. find `CreateWindow` intents and call `win.create_window`
2. write `WindowHandleBinding` on success
3. remove create intents only on success
4. find update intents like `SetWindowSize`
5. call explicit update RPCs
6. remove update intents only on success
7. find `CloseWindow` intents
8. call `win.close_window`
9. remove or despawn local bridge-owned entities only after confirmed success

### Proposed `win_sync_in`

`win_sync_in(process)` should:

1. call `win.poll_events(cursor, ...)` when event-based sync is enough
2. optionally call `win.list_windows()` for periodic reconciliation
3. update projected `Window` components
4. create missing local bridge-owned entities for newly discovered remote windows
5. remove or mark bridge-owned entities when remote close is confirmed
6. advance the cursor only after successful processing

### Missing `win` RPCs

To support reconciliation cleanly, `awrk-win` likely needs explicit update procedures such as:

- `win.set_title`
- `win.set_inner_size`

An alternative is a single coarse-grained RPC such as:

- `win.update_window_spec`

The explicit split is easier to reason about and better matches the current RPC style.

## Generic Framework Guidance

The request here is to design a reusable pattern now, not just a `win`-only helper.

The safest reusable layer is probably:

- a naming convention for projected components and intent components
- root-level app-facing projection and intent types, with transport-facing details in a dedicated nested module
- a small bridge resource convention
- a standard `sync_out` / `sync_in` contract
- a shared recommendation that remote domains use explicit RPCs for writes

The risky part would be introducing a big generic framework in `awrk-world` too early.

Recommendation:

- put the pattern in docs first
- implement it in one domain such as `win`
- only extract helpers after a second domain confirms the same shape

That keeps the design generic without forcing premature abstractions into core runtime crates.

## Example Tick Shape

For an app process:

```rust
fn tick(process: &mut Process) -> Result<(), String> {
    process.tick()?;
    win_sync_out(process)?;
    win_sync_in(process)?;
    Ok(())
}
```

For an event-loop-driven host that owns `World`, `Rpcs`, and `Sessions` directly, the same bridge calls can happen alongside the normal loop, just as the current win server already drives `sessions.handle(...)` explicitly.

## Open Design Questions

These points still need implementation decisions:

- should `sync_out` run before `sync_in` or vice versa for each bridge
- should local projected entities be created eagerly from `list_*` snapshots or lazily from events only
- how much retry metadata belongs in ECS vs bridge resources
- whether every bridge should expose a domain-specific `tick_bridge` helper
- whether a local projected entity should survive a permanent remote deletion as tombstone state or be removed immediately

## Recommendation

The proposed direction is:

1. keep `Process` and the RPC layer explicit
2. keep typed clients internal to bridge resources
3. expose confirmed remote state as projected ECS components
4. expose writes as one-shot intent components
5. run explicit `sync_out` and `sync_in` functions in the app tick
6. add explicit domain RPCs for updates where a bridge needs them

This gives ECS ergonomics to app code without turning the core runtime back into an implicit projection system hidden inside `awrk-world`.