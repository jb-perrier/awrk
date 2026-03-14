# Architecture Remediation Plan

Status: draft remediation plan derived from the current architecture review.

## Goal

Resolve the current architecture inconsistencies across code, docs, examples, and lifecycle semantics without changing the core direction of the repository.

The target steady state is:

- ECS-first domain behavior
- built-in `awrk.*` RPCs used as transport/runtime primitives only
- bridge-managed proxy creation and destruction as the canonical remote-entity workflow
- examples and docs teaching the same model as the runtime implementation

## Review Findings To Resolve

1. The bridge now supports outbound create and outbound despawn, but docs still describe it as inbound-only.
2. The demoserver still demonstrates direct raw `WorldClient` spawning for the window case instead of the bridge-managed proxy flow.
3. `ProxySpawnRequest` and `ProxySpawnError` are now part of the bridge command/status model, but their built-in registration and intended tooling visibility are not yet explicit.
4. `ProxyLifecycle::Tombstoned` exists in the public API, but there is no implementation path that enters it.
5. Validation is strong at the unit level, but weak at the live bridge integration boundary.

## Decisions Needed

1. Should `ProxySpawnRequest` and `ProxySpawnError` be part of the public/tooling-visible schema surface?
   Decision: Yes. They should be public and tooling-visible.
2. Should `crates/demoserver` become the canonical proxy-based example, or remain a low-level raw-RPC demo?
   Decision: Remove `crates/demoserver` in favor of the example crates.

These decisions are now fixed inputs to the remediation work:

- `ProxySpawnRequest` and `ProxySpawnError` should be treated as part of the public, tooling-visible schema surface.
- `crates/demoserver` should be removed rather than repurposed as a canonical example.

## Execution Plan

### Phase A: Normalize the Public Story

Objective:
Make docs, examples, and code describe the same architecture.

Steps:

1. Update `docs/proxy-outbound-creation-plan.md` so the `Current State` section reflects that outbound create and outbound despawn now exist in the bridge.
2. Update `docs/process.md` to distinguish clearly between:
   - low-level raw transport usage through `WorldClient`
   - canonical domain workflow through `WorldBridge` and proxy entities
3. Update `docs/process.md` and `docs/proxy-outbound-creation-plan.md` to state that direct built-in `awrk.*` RPC calls are allowed for tooling and debugging, but app-facing domain workflows should prefer ECS plus bridge state.
4. Remove references that position `crates/demoserver` as either a canonical or low-level example, and point readers to the example crates instead.

Success criteria:

- no doc states that the bridge is inbound-only
- docs clearly separate low-level transport usage from the preferred domain workflow
- docs consistently point to the example crates as the supported architecture examples

### Phase B: Make the Proxy Command Model Explicit

Objective:
Remove ambiguity around bridge-owned command/status components.

Steps:

1. Register `ProxySpawnRequest` and `ProxySpawnError` in `crates/world/src/core/process.rs` alongside the rest of the built-in proxy types.
2. Document them as public/tooling-visible bridge command/status components in:
   - `docs/process.md`
   - `docs/proxy-outbound-creation-plan.md`
   - any example code that constructs them
3. Ensure the chosen policy is reflected consistently in:
   - `crates/world-ecs/src/components.rs`
   - `crates/world/src/lib.rs`
   - `crates/world/src/core/process.rs`

Success criteria:

- there is no accidental or ambiguous visibility of bridge command/status components
- tooling/schema behavior matches the documented design

### Phase C: Clean Up Proxy Lifecycle Semantics

Objective:
Make the public `ProxyLifecycle` enum reflect actual implemented states.

Steps:

1. Audit all runtime transitions involving `ProxyLifecycle`.
2. Either remove `Tombstoned` or implement a concrete state transition that uses it.
3. If `Tombstoned` is kept, define:
   - when a proxy enters it
   - whether it is observable before cleanup
   - whether it is local-only or mirrored
   - what valid transitions exist out of it
4. Update the docs to match the final state machine.
5. Add tests covering every supported nontrivial lifecycle state.

Success criteria:

- every public lifecycle state has a real semantic meaning
- there are no dead public states in the bridge API

### Phase D: Unify Examples With The Intended Architecture

Objective:
Ensure example applications reinforce the intended model instead of bypassing it.

Steps:

1. Remove `crates/demoserver` from the workspace by:
   - removing `crates/demoserver` from the workspace members list in `Cargo.toml`
   - deleting the `crates/demoserver` crate directory
   - removing or rewriting references in `.github/copilot-instructions.md` and any other docs that still point to it
2. Treat `examples/example-server` and `examples/example-consumer` as the canonical architecture examples.
3. Extend `examples/example-server` and `examples/example-consumer` so that together they demonstrate the full proxy flow through a local proxy-intent entity with:
   - `WorldId`
   - `ProxyAuthority`
   - `ProxySpawnRequest`
   - example-domain components from `examples/example-model` plus `Name`
4. Remove or isolate any direct `WorldClient.invoke_typed("awrk.spawn", ...)` path from the main example flow in the canonical examples.
5. If a raw transport example is still desired later, add it as a clearly named separate example rather than leaving it in the main teaching path.
6. Ensure the canonical example pair demonstrates the full proxy flow:
   - local create intent
   - remote materialization
   - inbound reconciliation
   - local removal
   - remote despawn

Success criteria:

- examples teach one consistent architecture story
- the example crates, not `crates/demoserver`, are the supported teaching surface
- the canonical example crates do not depend on `win-server` or the `win/*` domain types
- the workspace manifest and docs no longer reference `crates/demoserver`
- raw transport usage, if retained, is clearly marked as low-level

### Phase E: Strengthen Integration Validation

Objective:
Test the architecture where subsystem boundaries actually meet.

Steps:

1. Add an end-to-end bridge integration test that boots a real local world server and exercises:
   - outbound create
   - inbound mirror reconciliation
   - outbound despawn
2. Add a test proving that inbound-only mirrored proxies do not cause outbound remote despawn.
3. Add a test proving that `outbound_create_components` excludes remote-owned state such as `WinFocused` and `WinStatus`.
4. Add a win-domain smoke test covering state-driven creation and teardown behavior.
5. Keep `cargo test` as the repository-wide baseline validation gate.

Success criteria:

- the create/mirror/delete flow is validated across a real RPC boundary
- the bridge behavior is verified beyond helper-level unit tests

### Phase F: Optional Process Surface Cleanup

Objective:
Improve long-term maintainability without changing behavior.

Steps:

1. Evaluate whether `Process` should remain a single aggregation type for:
   - world state
   - RPC/session hosting
   - bridge management
2. If needed, gradually refactor toward clearer layering after the documentation and example consistency work is complete.
3. Do not prioritize this ahead of normalization, lifecycle cleanup, and integration coverage.

Success criteria:

- the public API surface is easier to understand and maintain
- no behavior or wire format changes are introduced accidentally

## Recommended Order

1. Normalize docs around the decided `demoserver` removal and the example-crate-first story.
2. Implement the visibility policy for `ProxySpawnRequest` and `ProxySpawnError`.
3. Clean up `ProxyLifecycle`, especially `Tombstoned`.
4. Remove `crates/demoserver` from the workspace and update the `example-server` and `example-consumer` pair to match the chosen policy.
5. Add end-to-end bridge integration coverage.
6. Consider optional `Process` surface cleanup only after the above are stable.

## Success Criteria For The Whole Remediation

- docs, examples, and runtime code all describe the same remote-entity workflow
- there is one clearly canonical way to model domain-level remote creation/destruction
- public lifecycle states have implemented semantics
- command/status component visibility is an explicit design choice
- integration coverage exists for bridge-managed create and destroy flows

## Notes

- This plan intentionally preserves the existing core design direction rather than proposing a new architecture.
- The current architecture is broadly coherent; the remediation work is about eliminating contradictory teaching surfaces and unfinished public semantics.