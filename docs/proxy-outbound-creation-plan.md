# Proxy Outbound Creation Plan

## Goal

Support creating, destroying, and editing remote domain entities through the proxy system without adding domain-specific lifecycle or edit RPCs for state that already has a persistent ECS representation.

The intended model is:

- the consumer creates a local request-driven proxy entity
- the bridge materializes that entity on the remote world using generic `awrk.*` mutation RPCs
- the remote process reacts to the resulting entity state as part of its normal domain logic
- the proxy bridge reconciles the local entity with the remote entity once the remote snapshot appears

Two outbound authority modes are intentionally supported:

- `RequestDriven`: local intent, remote authority after creation
- `Local`: local authority, remote follows local state

For the window domain, a remote `WinWindow` entity should be enough to request native window creation.

## Non-goals

- no generic bidirectional live patch sync in the first phase
- no new domain-specific RPC surface for window creation
- no hidden authority rules inferred from arbitrary local entities
- no duplication of a separate command system beside proxies

## Design Rule

Use bridge-managed entity state for durable domain state.

Use explicit RPC only for operations that are not well represented as persistent ECS state.

This means:

- keep generic world mutation RPCs such as `awrk.spawn`, `awrk.set_component`, `awrk.patch_component`, and `awrk.despawn` as the transport primitives
- remove domain-specific RPCs for entity lifecycle and editable entity state where the domain is already modeled as components
- reserve explicit RPCs for truly procedural or ephemeral operations that do not map cleanly onto durable entities

Repository policy direction:

- built-in `awrk.*` RPCs remain
- custom process-defined RPCs are being removed
- marker/verb components on entities are the default command model
- status/error components on the same entity are the default result model

Examples:

- window creation, title changes, size changes, and destruction should be modeled through entities and components
- a future native file dialog or other transient OS procedure could still be exposed as an explicit RPC

## Current State

The custom window RPC path has been removed.

Today the implemented model is:

1. built-in `awrk.*` RPCs remain as transport/runtime primitives
2. the win server reconciles native windows from ECS state rather than a domain-specific create RPC
3. the bridge mirrors remote entities into local proxy entities and can push local proxy creation upstream through `awrk.spawn`
4. the bridge can propagate remote despawn for locally-originated proxies after local entity removal
5. general outbound component patching is still intentionally out of scope

## Target Model

### Local create intent

A consumer creates a local entity containing:

- `WorldId(remote_world_id)`
- `ProxyAuthority { authority: ProxyAuthorityKind::RequestDriven | ProxyAuthorityKind::Local }`
- a bridge marker indicating that remote materialization is requested
- the domain components to send upstream, such as `WinWindow`, `WinTitle`, `WinInnerSize`, and `Name`

The entity should not have `ProxyEntity` yet.

### Bridge responsibility

The bridge detects local entities that:

- target a known remote world
- are marked as request-driven
- carry the create marker
- do not already have `ProxyEntity`

For each matching entity, the bridge:

1. collects the registered outbound-allowed component set
2. serializes the local component values
3. calls generic `awrk.spawn`
4. records the returned remote entity id
5. attaches `ProxyEntity` locally
6. removes the local create marker
7. waits for normal inbound snapshot reconciliation to move the lifecycle to `Live`

How reconciliation behaves after creation depends on authority:

- `RequestDriven`: the create originates locally, but the remote entity becomes authoritative once materialized
- `Local`: the create originates locally, and later outbound sync may treat local state as canonical

### Remote process responsibility

The remote process treats entity presence as the source of truth.

For the window server:

- if an entity has `WinWindow` and no local runtime binding yet, create the native window
- if a bound entity is despawned, destroy the native window
- if `WinTitle` or `WinInnerSize` changes, update the native window
- runtime-only state should stay server-private and not be part of proxy registration

More generally, domain servers should reconcile runtime objects from ECS state instead of exposing parallel lifecycle RPCs for the same objects.

## Current Supported Scope

The current supported scope is:

- outbound create
- outbound despawn
- inbound reconciliation using the existing bridge snapshot logic

General outbound component patching is still intentionally not implemented.

This keeps the scope small while still enabling proxy-native remote window creation.

## Proposed Components and Markers

### Bridge-owned markers

Introduce one explicit local marker for outbound creation intent.

Chosen name:

- `ProxySpawnRequest`

Meaning:

- the entity should be spawned remotely on the next bridge tick
- after successful remote spawn, the bridge removes this marker
- if spawning fails, the bridge records failure state on the entity and retry remains explicit by re-adding the marker

A matching outbound-despawn marker may not be necessary if local entity removal is treated as the remote-despawn signal once the local entity already has `ProxyEntity`.

### Authority rules

Only outbound-sync entities with explicit local authority.

Recommended outbound-eligible authorities:

- `RequestDriven`
- `Local`

Recommended outbound-ineligible authorities:

- `Remote`
- `ReadOnlyMirror`

This avoids feedback loops where inbound mirrored state is accidentally sent back to the origin.

Authority semantics:

- `RequestDriven`: use local entities to request remote materialization, then accept remote state as authoritative
- `Local`: use local entities as the canonical source, with the remote side acting as an execution target or mirror

For remote windows, `RequestDriven` is still the primary fit because the remote process owns the native platform object. `Local` is kept as a first-class authority mode because other domains may want local ownership after creation.

## Outbound Allowed Components

Outbound creation should not serialize every local component.

Instead, it should use the same explicit registration model already used for inbound proxy subscriptions:

- process/model crates register which domain components belong to a proxy-created entity family
- the bridge uses the registered component set when building the payload for `awrk.spawn`

Current code note:

- the existing `ProxySubscriptionContribution.components` / `register_proxy_subscription!` path is suitable as the registration mechanism to extend
- it should not be reused verbatim as the outbound payload list because some domains currently include remote-owned mirrored components there
- phase 1 should add a distinct outbound-create component list to the same registration flow

For windows, the outbound payload would likely include:

- `WinWindow`
- `WinTitle`
- `WinInnerSize`
- `Name`

`WinFocused` stays remote-owned in phase 1 because it reflects native platform state after the window exists.

## Bridge Algorithm

### Outbound create pass

Add an outbound create pass before or after inbound polling in `WorldBridge::tick_remote`.

Recommended sequence:

1. scan local world for entities targeting this remote world
2. filter to `RequestDriven` or `Local` plus create marker and no `ProxyEntity`
3. collect registered outbound component values
4. call `awrk.spawn`
5. attach `ProxyEntity`
6. set `ProxyState.lifecycle = Creating`
7. remove create marker

### Inbound reconciliation

Keep the existing inbound snapshot logic authoritative for final reconciliation.

That means the bridge should still accept the next remote snapshot as the canonical remote state and apply it to the local proxy entity.

### Outbound despawn

After create works, add a small outbound-despawn rule:

- if a local request-driven proxy entity with `ProxyEntity` is intentionally removed locally, the bridge sends `awrk.despawn` for the remote entity

This likely requires a lightweight tombstone or pending-despawn queue because once the local entity is gone, the bridge still needs the remote id.

## Win Server Changes

The window server should shift from RPC-driven creation to state-driven reconciliation.

### Replace direct domain RPC ownership

Domain-specific lifecycle and edit RPCs for persistent window state have been removed from the window domain.

The primary creation path should be:

- entity appears with `WinWindow`
- server creates native window if not already bound

### Add server-private runtime binding

Add a server-private component or mapping that indicates a native window has already been created for a world entity.

Examples:

- `WindowRuntimeBinding`
- existing map-only bookkeeping if that remains simpler

This must not be part of the proxy registration surface.

## Failure Model

A failed outbound create should be visible locally.

Implemented minimum behavior:

- keep the local entity alive
- remove `ProxySpawnRequest` after the failed attempt
- store the last error string in `ProxySpawnError`
- allow explicit retry by re-adding `ProxySpawnRequest`

The bridge does not auto-retry failed creates every tick.

## Phase Plan

### Phase 1: Core bridge support

- add outbound create marker component
- add outbound create scan in the bridge
- call generic `awrk.spawn`
- attach `ProxyEntity` and transition local state to creating
- keep inbound reconciliation unchanged
- treat `awrk.spawn`, `awrk.set_component`, `awrk.patch_component`, and `awrk.despawn` as the only required transport primitives for persistent remote entity state

### Phase 1 implementation plan

1. Add bridge-owned outbound-create marker/state components in `crates/world-ecs/src/components.rs`.
2. Re-export those components from `crates/world/src/lib.rs` so model crates and consumers can use them directly.
3. Extend `crates/world/src/registration/mod.rs` so a proxy subscription contribution can declare a distinct outbound-create component list in addition to mirrored inbound components.
4. Extend `register_proxy_subscription!` in `crates/world/src/lib.rs` so callers can provide `outbound_create_components: [...]` while preserving the current inbound `components: [...]` behavior.
5. Update model crates that should support outbound creation to declare their outbound-create component set explicitly.
6. In particular, update `crates/win/api/src/lib.rs` so window outbound-create includes `Name`, `WinWindow`, `WinTitle`, and `WinInnerSize`, while leaving `WinFocused` and `WinStatus` inbound-only.
7. Add a bridge helper in `crates/world/src/bridge/mod.rs` to scan the local world for entities targeting a given remote world with `WorldId(remote_world_id)`, `ProxyAuthority(RequestDriven|Local)`, `ProxySpawnRequest`, and no `ProxyEntity`.
8. In that helper, build the `awrk.spawn` payload only from registered outbound-create component types that are actually present on the local entity.
9. Perform the outbound-create pass at the start of `WorldBridge::tick_remote` before polling remote changes so a successful spawn can be observed on the same or next tick through normal inbound reconciliation.
10. After successful `awrk.spawn`, attach `ProxyEntity`, attach or update `ProxyState { lifecycle: Creating, ... }`, record the local/remote mapping in the bridge state, and remove `ProxySpawnRequest`.
11. On spawn failure, keep the local entity alive, remove `ProxySpawnRequest`, and record bridge-visible error state on the entity so retry stays explicit rather than automatic every tick.
12. Keep inbound snapshot reconciliation authoritative: the first matching remote snapshot must still be the step that moves the local lifecycle to `Live` and refreshes mirrored components.
13. Do not implement outbound despawn or outbound patching in this phase.
14. Add bridge-focused tests covering eligibility filtering, payload selection, successful spawn bookkeeping, and failure behavior.

### Phase 1 file touchpoints

- `crates/world-ecs/src/components.rs`: add `ProxySpawnRequest` and minimal generic failure/status component if needed
- `crates/world/src/lib.rs`: export new components and extend `register_proxy_subscription!`
- `crates/world/src/registration/mod.rs`: add outbound-create component contribution support
- `crates/world/src/bridge/mod.rs`: add outbound-create scan, payload building, RPC call, and bookkeeping helpers
- `crates/win/api/src/lib.rs`: declare the window outbound-create component set
- `examples/example-model/src/lib.rs`: optionally declare an outbound-create set if example outbound creation should be supported there too

### Phase 1 acceptance criteria

- a consumer can spawn a local entity with `WorldId`, `ProxyAuthority(RequestDriven|Local)`, `ProxySpawnRequest`, and registered domain components
- one bridge tick issues exactly one `awrk.spawn` for that entity
- on success the local entity gains `ProxyEntity` and enters `Creating`
- the create marker is removed only after successful remote spawn
- the next inbound snapshot drives the entity to `Live`
- window outbound-create does not send `WinFocused` or `WinStatus`
- failed create is visible locally and requires explicit retry

### Phase 2: Win server reconciliation

- make win server create native windows when `WinWindow` entities appear
- keep runtime binding local-only
- update close/destroy behavior to follow entity lifecycle
- keep the window server fully state-driven with no domain-specific lifecycle RPCs

### Phase 3: Outbound despawn

- add bridge support for remote despawn when a local request-driven proxy is removed
- keep enough bridge bookkeeping to send `awrk.despawn` after local removal using the remembered `ProxyEntity.remote` mapping

### Phase 3 implementation plan

1. Extend bridge-internal proxy bookkeeping in `crates/world/src/bridge/mod.rs` so each `ProxyRecord` stores whether the proxy was created locally by the bridge or arrived from inbound remote mirroring.
2. Add two bridge-internal remote-state fields in `RemoteWorldState`:
	- a `local_cursor` for polling the local world's change log
	- a `pending_despawns` set or queue of `RemoteRef`s that still need a remote `awrk.despawn`
3. Initialize `local_cursor` from the local world's current change sequence when the remote is added or bootstrapped so the bridge does not treat pre-existing local history as new intent.
4. Mark proxies created by the outbound-create pass as `origin = Local` when inserting their `ProxyRecord`.
5. Mark proxies first seen through inbound snapshot reconciliation as `origin = Remote`.
6. Add a bridge helper that polls local change events using the existing `World::poll_changes` / local change-log path rather than adding a second ad hoc tombstone system.
7. In that local-change pass, inspect only `ChangeKind::Despawned` events.
8. For each despawned local entity id, check `remote.remote_by_local` to see whether the entity had a known remote mapping.
9. If there is no mapping, ignore the event.
10. If there is a mapping, inspect the corresponding `ProxyRecord.origin` and the last known `ProxyAuthority` policy.
11. Queue outbound remote despawn only for locally-originated proxies. In the minimal phase, this should at least include `RequestDriven`; if `Local` remains a supported local-create authority, include it only for records with `origin = Local`.
12. Do not remote-despawn purely inbound mirrored proxies, even if their local entity disappears because of bridge resync or local cleanup.
13. After the local-change scan, process the `pending_despawns` queue by invoking generic `awrk.despawn` for each queued remote entity id.
14. On successful remote despawn, remove the bridge mappings for that proxy from `remote.remote_by_local` and `remote.local_by_remote`.
15. On remote-despawn failure, keep the `RemoteRef` in `pending_despawns` so the bridge can retry on the next tick without requiring another local event.
16. Do not recreate a local placeholder entity for failures in phase 3. The local entity is already gone; retry should stay bridge-internal.
17. Keep the existing inbound remote poll authoritative for cleanup when a remote despawn notification arrives after the bridge has already dropped local state.
18. Ensure the queue processing is idempotent so duplicate local despawn events or repeated remote confirmations do not cause errors.

### Why this shape

- the existing local change log already records despawns, so Phase 3 does not need a second tombstone component just to notice local removal
- a persistent `origin = Local | Remote` flag in bridge bookkeeping is necessary because `ProxyAuthority` alone is not sufficient to distinguish locally-created proxies from inbound mirrors in all configurations
- a `pending_despawns` queue is still useful because once the local entity is gone, the bridge needs retryable internal state if the remote `awrk.despawn` call fails

### Phase 3 file touchpoints

- `crates/world/src/bridge/mod.rs`: extend `ProxyRecord`, add `local_cursor` and `pending_despawns`, add local-despawn scan and outbound remote-despawn pass
- `crates/world/src/core/process.rs`: expose local change-sequence helpers if the bridge needs a less restrictive public surface for polling local changes
- `docs/proxy-outbound-creation-plan.md`: keep the lifecycle and failure notes aligned with the implementation

### Phase 3 acceptance criteria

- removing a locally-originated proxy entity causes exactly one remote `awrk.despawn` request to be issued
- removing a purely inbound mirrored proxy does not trigger remote despawn
- if the remote `awrk.despawn` call fails, the bridge retries from internal pending state on later ticks
- successful remote despawn clears bridge mappings for that proxy
- later inbound remote despawn notifications do not error if the bridge already processed the removal

### Phase 4: Optional outbound edits

Only if needed later:

- push selected local edits like title or size upstream for local-authority proxies, and selectively for request-driven proxies where that is explicitly allowed
- keep remote-owned state such as focus inbound-only unless a clear use case appears

## Resolved Decisions

1. Use `ProxySpawnRequest` as the outbound creation marker.
2. Phase 1 outbound creation is allowed for `ProxyAuthorityKind::RequestDriven` and `ProxyAuthorityKind::Local`.
3. Failed creates keep local failure state and require explicit retry.
4. Local removal of a locally-originated proxy should imply remote despawn.
5. `WinFocused` stays remote-owned in phase 1.
6. Domain-specific lifecycle/edit RPCs for persistent window state, including `win.create_window`, are removed.
7. `Process` custom RPC registration becomes internal/private.
8. `#[rpc]` is removed immediately rather than deprecated.
9. Inventory registration is narrowed to type/subscription registration only.
10. Marker/verb components on entities are the default command shape.
11. Status/error components on the same entity are the default result shape.
