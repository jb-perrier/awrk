use crate::schema_types::{ProcInfo, TypeInfo, procs_from_schema, types_from_schema};
use awrk_datex::codec::decode::DecodeConfig;
use awrk_datex::codec::encode::Encoder;
use awrk_datex::value::Value;
use awrk_datex::{Decode, Encode};
use awrk_datex_schema::OwnedSchema;
use awrk_world::rpc::{
    ChangeKind, ComponentInfo, DespawnArgs, EntityInfo, EntityMeta, GetEntitiesArgs,
    GetEntitiesResult, ListEntitiesResult, ListTypesResult, PatchComponentArgs, PollChangesArgs,
    PollChangesResult, RemoveComponentArgs, SetComponentArgs, SpawnArgs, SpawnResult,
};
use awrk_world::rpc::{RpcTrace, TypeCaps, WorldClient, WorldClientOptions};
use awrk_world_ecs::{Name, Parent};
use std::{sync::mpsc, thread};

type Target = (String, u16);

#[derive(Debug)]
pub(crate) enum WorkerRequest {
    RefreshAll {
        host: String,
        port: u16,
        visible_entities: Vec<u64>,
        expanded_entities: Vec<u64>,
        selected_entity: Option<u64>,
    },
    SpawnEmpty {
        host: String,
        port: u16,
    },
    SpawnEntity {
        host: String,
        port: u16,
        parent: Option<u64>,
        name: Option<String>,
    },
    Despawn {
        host: String,
        port: u16,
        entity: u64,
    },
    SetComponent {
        host: String,
        port: u16,
        entity: u64,
        type_name: String,
        json: Value,
    },
    RemoveComponent {
        host: String,
        port: u16,
        entity: u64,
        type_name: String,
    },
    PatchComponent {
        host: String,
        port: u16,
        entity: u64,
        type_name: String,
        patch: Value,
    },
    Invoke {
        host: String,
        port: u16,
        proc: String,
        args: Value,
    },
    Quit,
}

#[derive(Debug)]
pub(crate) enum WorkerResponse {
    Status(String),
    Trace(RpcTrace),
    Refreshed {
        entities: Vec<EntityInfo>,
        types: Vec<TypeInfo>,
        procs: Vec<ProcInfo>,
    },
    /// Sent after a refresh attempt finishes (success or failure).
    RefreshDone,
    Spawned {
        entity: u64,
        parent: Option<u64>,
    },
    Invoked(Value),
}

struct WorkerState {
    client: Option<WorldClient>,
    target: Option<Target>,
    schema: Option<OwnedSchema>,
    types: Option<Vec<TypeInfo>>,
    procs: Option<Vec<ProcInfo>>,
    sent_trace_count: usize,

    // Incremental refresh state
    change_cursor: Option<u64>,
    metas_by_entity: std::collections::HashMap<u64, EntityMeta>,
    kids_by_parent: std::collections::HashMap<u64, Vec<u64>>,
    roots: std::collections::BTreeSet<u64>,
}

impl WorkerState {
    fn new() -> Self {
        Self {
            client: None,
            target: None,
            schema: None,
            types: None,
            procs: None,
            sent_trace_count: 0,

            change_cursor: None,
            metas_by_entity: std::collections::HashMap::new(),
            kids_by_parent: std::collections::HashMap::new(),
            roots: std::collections::BTreeSet::new(),
        }
    }

    fn reset_connection_state(&mut self) {
        self.client = None;
        self.schema = None;
        self.types = None;
        self.procs = None;
        self.sent_trace_count = 0;

        self.change_cursor = None;
        self.metas_by_entity.clear();
        self.kids_by_parent.clear();
        self.roots.clear();
    }

    fn ensure_client(
        &mut self,
        host: &str,
        port: u16,
        opts: WorldClientOptions,
    ) -> Result<(), String> {
        let needs_reconnect = match &self.target {
            Some((h, p)) => h != host || *p != port,
            None => true,
        };

        if needs_reconnect {
            self.reset_connection_state();
            self.target = Some((host.to_string(), port));
        }

        if self.client.is_none() {
            self.client = Some(WorldClient::connect(host, port, opts)?);
        }

        Ok(())
    }

    fn client_mut(
        &mut self,
        host: &str,
        port: u16,
        opts: WorldClientOptions,
    ) -> Result<&mut WorldClient, String> {
        self.ensure_client(host, port, opts)?;
        Ok(self.client.as_mut().expect("client is set"))
    }

    fn ensure_schema_cached(&mut self) -> Result<(), String> {
        if self.schema.is_some() {
            return Ok(());
        }
        let bytes = self
            .client
            .as_mut()
            .expect("client is set")
            .get_schema_bytes()
            .map_err(|e| e.to_string())?;
        let schema = awrk_datex_schema::decode_schema(&bytes).map_err(|e| e.to_string())?;
        self.schema = Some(schema);
        Ok(())
    }

    fn ensure_metadata_cached(&mut self) -> Result<(), String> {
        self.ensure_schema_cached()?;

        if self.types.is_some() && self.procs.is_some() {
            return Ok(());
        }

        let type_caps: ListTypesResult = {
            let client = self.client.as_mut().expect("client is set");
            client.list_types().map_err(|e| e.to_string())?
        };
        let caps_by_name: std::collections::HashMap<String, TypeCaps> = type_caps
            .types
            .into_iter()
            .map(|t| (t.type_name, t.caps))
            .collect();

        let schema = self.schema.as_ref().expect("schema is set");
        let mut types = types_from_schema(schema);
        for t in &mut types {
            if let Some(caps) = caps_by_name.get(&t.type_name) {
                t.caps = *caps;
            }
        }
        let procs = procs_from_schema(schema);

        self.types = Some(types);
        self.procs = Some(procs);
        Ok(())
    }
}

pub(crate) fn start_worker() -> (mpsc::Sender<WorkerRequest>, mpsc::Receiver<WorkerResponse>) {
    let (req_tx, req_rx) = mpsc::channel::<WorkerRequest>();
    let (resp_tx, resp_rx) = mpsc::channel::<WorkerResponse>();

    thread::spawn(move || {
        let mut state = WorkerState::new();
        let opts = WorldClientOptions::default();

        loop {
            let req = match req_rx.recv() {
                Ok(r) => r,
                Err(_) => break,
            };

            if matches!(&req, WorkerRequest::Quit) {
                break;
            }

            match req {
                WorkerRequest::Quit => unreachable!(),

                WorkerRequest::RefreshAll {
                    host,
                    port,
                    visible_entities,
                    expanded_entities,
                    selected_entity,
                } => {
                    let _ = resp_tx.send(WorkerResponse::Status("Refreshing...".to_string()));
                    let refresh_result = refresh_all_wire(
                        &mut state,
                        &host,
                        port,
                        opts,
                        &visible_entities,
                        &expanded_entities,
                        selected_entity,
                    );

                    match refresh_result {
                        Ok((entities, types, procs)) => {
                            let _ = resp_tx.send(WorkerResponse::Refreshed {
                                entities,
                                types,
                                procs,
                            });
                            let _ = resp_tx.send(WorkerResponse::Status("Refreshed".to_string()));
                        }
                        Err(e) => {
                            let _ = resp_tx
                                .send(WorkerResponse::Status(format!("Refresh failed: {e}")));
                        }
                    }

                    let _ = resp_tx.send(WorkerResponse::RefreshDone);
                }

                WorkerRequest::SpawnEmpty { host, port } => {
                    let res: Result<(), String> = (|| {
                        let client = state.client_mut(&host, port, opts)?;
                        let r: SpawnResult = client
                            .invoke_typed("awrk.spawn_empty", ())
                            .map_err(|e| e.to_string())?;
                        let _ = resp_tx.send(WorkerResponse::Spawned {
                            entity: r.entity,
                            parent: None,
                        });
                        let _ = resp_tx.send(WorkerResponse::Status(format!(
                            "Spawned entity {}",
                            r.entity
                        )));
                        Ok(())
                    })();

                    if let Err(e) = res {
                        let _ = resp_tx.send(WorkerResponse::Status(format!("Spawn failed: {e}")));
                    }
                }

                WorkerRequest::SpawnEntity {
                    host,
                    port,
                    parent,
                    name,
                } => {
                    let res: Result<(), String> = (|| {
                        let mut components = Vec::new();

                        if let Some(name) = name {
                            components.push(ComponentInfo {
                                type_name: std::any::type_name::<Name>().to_string(),
                                value: Some(typed_to_value(&Name(name))?),
                            });
                        }

                        if let Some(parent) = parent {
                            components.push(ComponentInfo {
                                type_name: std::any::type_name::<Parent>().to_string(),
                                value: Some(typed_to_value(&Parent { parent })?),
                            });
                        }

                        let client = state.client_mut(&host, port, opts)?;
                        let r: SpawnResult = client
                            .invoke_typed("awrk.spawn", SpawnArgs { components })
                            .map_err(|e| e.to_string())?;
                        let _ = resp_tx.send(WorkerResponse::Spawned {
                            entity: r.entity,
                            parent,
                        });
                        let _ = resp_tx.send(WorkerResponse::Status(format!(
                            "Spawned entity {}",
                            r.entity
                        )));
                        Ok(())
                    })();

                    if let Err(e) = res {
                        let _ = resp_tx.send(WorkerResponse::Status(format!("Spawn failed: {e}")));
                    }
                }

                WorkerRequest::Despawn { host, port, entity } => {
                    let res: Result<(), String> = (|| {
                        let client = state.client_mut(&host, port, opts)?;
                        client
                            .invoke_typed::<DespawnArgs, ()>("awrk.despawn", DespawnArgs { entity })
                            .map_err(|e| e.to_string())?;
                        let _ = resp_tx
                            .send(WorkerResponse::Status(format!("Despawned entity {entity}")));
                        Ok(())
                    })();

                    if let Err(e) = res {
                        let _ =
                            resp_tx.send(WorkerResponse::Status(format!("Despawn failed: {e}")));
                    }
                }

                WorkerRequest::SetComponent {
                    host,
                    port,
                    entity,
                    type_name,
                    json,
                } => {
                    let res: Result<(), String> = (|| {
                        let client = state.client_mut(&host, port, opts)?;
                        client
                            .invoke_typed::<SetComponentArgs, ()>(
                                "awrk.set_component",
                                SetComponentArgs {
                                    entity,
                                    type_name: type_name.clone(),
                                    value: json,
                                },
                            )
                            .map_err(|e| e.to_string())?;
                        let _ = resp_tx
                            .send(WorkerResponse::Status(format!("Set component {type_name}")));
                        Ok(())
                    })();

                    if let Err(e) = res {
                        let _ = resp_tx.send(WorkerResponse::Status(format!("Set failed: {e}")));
                    }
                }

                WorkerRequest::RemoveComponent {
                    host,
                    port,
                    entity,
                    type_name,
                } => {
                    let res: Result<(), String> = (|| {
                        let client = state.client_mut(&host, port, opts)?;
                        client
                            .invoke_typed::<RemoveComponentArgs, ()>(
                                "awrk.remove_component",
                                RemoveComponentArgs {
                                    entity,
                                    type_name: type_name.clone(),
                                },
                            )
                            .map_err(|e| e.to_string())?;
                        let _ = resp_tx.send(WorkerResponse::Status(format!(
                            "Removed component {type_name}"
                        )));
                        Ok(())
                    })();

                    if let Err(e) = res {
                        let _ = resp_tx.send(WorkerResponse::Status(format!("Remove failed: {e}")));
                    }
                }

                WorkerRequest::PatchComponent {
                    host,
                    port,
                    entity,
                    type_name,
                    patch,
                } => {
                    let res: Result<(), String> = (|| {
                        let client = state.client_mut(&host, port, opts)?;
                        client
                            .invoke_typed::<PatchComponentArgs, ()>(
                                "awrk.patch_component",
                                PatchComponentArgs {
                                    entity,
                                    type_name: type_name.clone(),
                                    patch,
                                },
                            )
                            .map_err(|e| e.to_string())?;
                        let _ = resp_tx.send(WorkerResponse::Status(format!(
                            "Patched component {type_name}"
                        )));
                        Ok(())
                    })();

                    if let Err(e) = res {
                        let _ = resp_tx.send(WorkerResponse::Status(format!("Patch failed: {e}")));
                    }
                }

                WorkerRequest::Invoke {
                    host,
                    port,
                    proc,
                    args,
                } => {
                    let invoke_result = (|| {
                        let client = state.client_mut(&host, port, opts)?;
                        client.invoke_value(&proc, args).map_err(|e| e.to_string())
                    })();

                    match invoke_result {
                        Ok(v) => {
                            let _ = resp_tx.send(WorkerResponse::Invoked(v));
                            let _ = resp_tx.send(WorkerResponse::Status("RPC ok".to_string()));
                        }
                        Err(e) => {
                            let _ =
                                resp_tx.send(WorkerResponse::Status(format!("RPC failed: {e}")));
                        }
                    }
                }
            }

            flush_traces(&mut state, &resp_tx);
        }
    });

    (req_tx, resp_rx)
}

fn refresh_all_wire(
    state: &mut WorkerState,
    host: &str,
    port: u16,
    opts: WorldClientOptions,
    visible_entities: &[u64],
    expanded_entities: &[u64],
    selected_entity: Option<u64>,
) -> Result<(Vec<EntityInfo>, Vec<TypeInfo>, Vec<ProcInfo>), String> {
    state.ensure_client(host, port, opts)?;
    state.ensure_metadata_cached()?;

    // Initial sync (or resync) via full entity meta snapshot.
    if state.change_cursor.is_none() {
        let metas: ListEntitiesResult = {
            let client = state.client.as_mut().expect("client is set");
            client
                .invoke_typed("awrk.list_entities", ())
                .map_err(|e| e.to_string())?
        };
        rebuild_meta_state(state, metas.now, metas.entities);
    } else {
        // Incremental sync: apply change events.
        let mut cursor = state.change_cursor.expect("cursor is set");
        loop {
            let polled: PollChangesResult = {
                let client = state.client.as_mut().expect("client is set");
                client
                    .invoke_typed(
                        "awrk.poll_changes",
                        PollChangesArgs {
                            since: cursor,
                            limit: Some(2048),
                        },
                    )
                    .map_err(|e| e.to_string())?
            };

            if polled.needs_resync {
                // Fall back to a full meta snapshot.
                let metas: ListEntitiesResult = {
                    let client = state.client.as_mut().expect("client is set");
                    client
                        .invoke_typed("awrk.list_entities", ())
                        .map_err(|e| e.to_string())?
                };
                rebuild_meta_state(state, metas.now, metas.entities);
                break;
            }

            for ev in &polled.events {
                apply_change_event(state, ev);
            }

            cursor = polled.cursor;
            state.change_cursor = Some(cursor);

            if !polled.has_more {
                break;
            }
        }
    }

    let mut wanted: Vec<u64> = Vec::new();
    wanted.extend_from_slice(visible_entities);
    wanted.extend_from_slice(expanded_entities);

    // Always include current roots so newly spawned root entities discovered via
    // `awrk.poll_changes` can appear in the scene tree immediately.
    wanted.extend(state.roots.iter().copied());

    // If nothing is visible yet (fresh UI state), fetching roots is sufficient to bootstrap
    // the tree; the call above already ensured roots are included.

    // If a node is expanded, fetch its direct children (derived from metas).
    for &expanded in expanded_entities {
        if let Some(kids) = state.kids_by_parent.get(&expanded) {
            wanted.extend(kids.iter().copied());
        }
    }
    if let Some(sel) = selected_entity {
        wanted.push(sel);
    }
    wanted.sort_unstable();
    wanted.dedup();

    let entities: Vec<EntityInfo> = if wanted.is_empty() {
        Vec::new()
    } else {
        let full: GetEntitiesResult = {
            let client = state.client.as_mut().expect("client is set");
            client
                .invoke_typed(
                    "awrk.get_entities",
                    GetEntitiesArgs {
                        entities: wanted.clone(),
                    },
                )
                .map_err(|e| e.to_string())?
        };
        full.entities
    };

    let mut entities = entities;
    inject_parent_from_meta_map(&state.metas_by_entity, &mut entities)?;

    let types = state.types.clone().expect("types are cached");
    let procs = state.procs.clone().expect("procs are cached");

    Ok((entities, types, procs))
}

fn rebuild_meta_state(state: &mut WorkerState, cursor: u64, metas: Vec<EntityMeta>) {
    state.metas_by_entity.clear();
    state.kids_by_parent.clear();
    state.roots.clear();

    for meta in metas {
        if let Some(p) = meta.parent {
            state.kids_by_parent.entry(p).or_default().push(meta.entity);
        }
        state.metas_by_entity.insert(meta.entity, meta);
    }

    // Compute roots once; after this, roots are updated incrementally.
    for (&entity, meta) in state.metas_by_entity.iter() {
        if is_root_entity(entity, meta.parent, &state.metas_by_entity) {
            state.roots.insert(entity);
        }
    }

    state.change_cursor = Some(cursor);
}

fn is_root_entity(
    entity: u64,
    parent: Option<u64>,
    metas_by_entity: &std::collections::HashMap<u64, EntityMeta>,
) -> bool {
    match parent {
        None => true,
        Some(p) => p == entity || !metas_by_entity.contains_key(&p),
    }
}

fn typed_to_value<T>(value: &T) -> Result<Value, String>
where
    T: Encode,
{
    let mut encoder = Encoder::default();
    value.wire_encode(&mut encoder).map_err(|e| e.to_string())?;
    let bytes = encoder.into_inner();
    let value_ref = awrk_datex::codec::decode::decode_value_full(&bytes, DecodeConfig::default())
        .map_err(|e| e.to_string())?;
    Value::wire_decode(value_ref).map_err(|e| e.to_string())
}

fn remove_child(children: &mut Vec<u64>, child: u64) {
    if let Some(i) = children.iter().position(|&c| c == child) {
        children.swap_remove(i);
    }
}

fn apply_change_event(state: &mut WorkerState, ev: &awrk_world::rpc::ChangeEvent) {
    match ev.kind {
        ChangeKind::Upserted => {
            let entity = ev.entity;
            let new_parent = ev.parent;

            let old_parent = state.metas_by_entity.get(&entity).and_then(|m| m.parent);

            state.metas_by_entity.insert(
                entity,
                EntityMeta {
                    entity,
                    revision: ev.revision,
                    parent: new_parent,
                },
            );

            if old_parent != new_parent {
                if let Some(op) = old_parent {
                    if let Some(kids) = state.kids_by_parent.get_mut(&op) {
                        remove_child(kids, entity);
                        if kids.is_empty() {
                            state.kids_by_parent.remove(&op);
                        }
                    }
                }
                if let Some(np) = new_parent {
                    let kids = state.kids_by_parent.entry(np).or_default();
                    if !kids.contains(&entity) {
                        kids.push(entity);
                    }
                }
            }

            state.roots.remove(&entity);
            if is_root_entity(entity, new_parent, &state.metas_by_entity) {
                state.roots.insert(entity);
            }
        }
        ChangeKind::Despawned => {
            let entity = ev.entity;
            let old_parent = state.metas_by_entity.remove(&entity).and_then(|m| m.parent);

            if let Some(op) = old_parent {
                if let Some(kids) = state.kids_by_parent.get_mut(&op) {
                    remove_child(kids, entity);
                    if kids.is_empty() {
                        state.kids_by_parent.remove(&op);
                    }
                }
            }

            state.roots.remove(&entity);

            if let Some(kids) = state.kids_by_parent.remove(&entity) {
                for kid in kids {
                    if let Some(meta) = state.metas_by_entity.get(&kid) {
                        if is_root_entity(kid, meta.parent, &state.metas_by_entity) {
                            state.roots.insert(kid);
                        }
                    }
                }
            }
        }
    }
}

fn flush_traces(state: &mut WorkerState, resp_tx: &mpsc::Sender<WorkerResponse>) {
    let Some(c) = state.client.as_ref() else {
        return;
    };
    for trace in c.traces().iter().skip(state.sent_trace_count).cloned() {
        let _ = resp_tx.send(WorkerResponse::Trace(trace));
        state.sent_trace_count += 1;
    }
}

fn set_component_value(
    components: &mut Vec<awrk_world::rpc::ComponentInfo>,
    type_name: &str,
    value: Option<Value>,
) {
    if let Some(i) = components.iter().position(|c| c.type_name == type_name) {
        if let Some(v) = value {
            components[i].value = Some(v);
        } else {
            components.remove(i);
        }
        return;
    }

    if let Some(v) = value {
        components.push(awrk_world::rpc::ComponentInfo {
            type_name: type_name.to_string(),
            value: Some(v),
        });
    }
}

fn encode_typed_to_value<A: Encode>(args: &A) -> Result<Value, String> {
    let mut enc = awrk_datex::codec::encode::Encoder::default();
    args.wire_encode(&mut enc).map_err(|e| e.to_string())?;
    let buf = enc.into_inner();
    let value_ref = awrk_datex::codec::decode::decode_value_full(&buf, DecodeConfig::default())
        .map_err(|e| e.to_string())?;
    Value::wire_decode(value_ref).map_err(|e| e.to_string())
}

fn inject_parent_from_meta_map(
    metas_by_entity: &std::collections::HashMap<u64, EntityMeta>,
    entities: &mut [EntityInfo],
) -> Result<(), String> {
    let parent_type = std::any::type_name::<Parent>();

    for e in entities {
        let parent = metas_by_entity.get(&e.entity).and_then(|m| m.parent);
        match parent {
            Some(parent) => {
                let v = encode_typed_to_value(&Parent { parent })?;
                set_component_value(&mut e.components, parent_type, Some(v));
            }
            None => set_component_value(&mut e.components, parent_type, None),
        }
    }

    Ok(())
}

fn _is_transport(e: &str) -> bool {
    let _ = e;
    false
}
