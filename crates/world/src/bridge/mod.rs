use crate::core::World;
use crate::rpc::{
    ChangeEvent, ChangeKind, ComponentInfo, DespawnArgs, EntityInfo, EntityMeta, GetEntitiesArgs,
    GetEntitiesResult, ListEntitiesResult, QueryEntitiesArgs, QueryEntitiesResult, SpawnArgs,
    SpawnResult, WorldClient, WorldClientError, WorldClientOptions,
};
use crate::{
    Parent, ProxyAuthority, ProxyAuthorityKind, ProxyEntity, ProxyLifecycle, ProxySpawnError,
    ProxySpawnRequest, ProxyState, RemoteParentRef, RemoteRef, WorldId,
};
use std::collections::{BTreeMap, BTreeSet};

const QUERY_PAGE_LIMIT: u32 = 2048;
const GET_ENTITIES_BATCH_SIZE: usize = 256;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProxyQueryClause {
    pub all_of: Vec<String>,
    pub any_of: Vec<String>,
    pub none_of: Vec<String>,
}

impl ProxyQueryClause {
    fn is_empty(&self) -> bool {
        self.all_of.is_empty() && self.any_of.is_empty() && self.none_of.is_empty()
    }

    fn to_query_args(&self) -> QueryEntitiesArgs {
        QueryEntitiesArgs {
            all_of: self.all_of.clone(),
            any_of: self.any_of.clone(),
            none_of: self.none_of.clone(),
            after: None,
            limit: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProxySubscription {
    pub queries: Vec<ProxyQueryClause>,
    pub mirrored_components: BTreeSet<String>,
    pub outbound_create_components: BTreeSet<String>,
}

impl Default for ProxySubscription {
    fn default() -> Self {
        Self::new()
    }
}

impl ProxySubscription {
    pub fn new() -> Self {
        let mut queries = Vec::new();
        let mut mirrored_components = BTreeSet::new();
        let mut outbound_create_components = BTreeSet::new();

        for registration in
            crate::inventory::iter::<crate::registration::ProxySubscriptionRegistration>
        {
            let contribution = (registration.build)();
            let query = ProxyQueryClause {
                all_of: contribution.all_of,
                any_of: contribution.any_of,
                none_of: contribution.none_of,
            };

            if !query.is_empty() {
                queries.push(query);
            }

            mirrored_components.extend(contribution.components);
            outbound_create_components.extend(contribution.outbound_create_components);
        }

        Self {
            queries,
            mirrored_components,
            outbound_create_components,
        }
    }
}

#[derive(Clone, Debug)]
pub struct WorldBridgeRemoteConfig {
    pub world_id: u64,
    pub host: String,
    pub port: u16,
    pub client_options: WorldClientOptions,
    pub subscription: ProxySubscription,
    pub authority: ProxyAuthorityKind,
}

impl WorldBridgeRemoteConfig {
    pub fn new(world_id: u64, host: impl Into<String>, port: u16) -> Self {
        Self {
            world_id,
            host: host.into(),
            port,
            client_options: WorldClientOptions::default(),
            subscription: ProxySubscription::default(),
            authority: ProxyAuthorityKind::Remote,
        }
    }

    pub fn with_client_options(mut self, client_options: WorldClientOptions) -> Self {
        self.client_options = client_options;
        self
    }

    pub fn with_authority(mut self, authority: ProxyAuthorityKind) -> Self {
        self.authority = authority;
        self
    }
}

#[derive(Clone, Debug)]
enum ProxyOrigin {
    Local,
    Remote,
}

#[derive(Clone, Debug)]
struct ProxyRecord {
    local_entity: u64,
    remote_revision: u64,
    remote_parent: Option<u64>,
    origin: ProxyOrigin,
}

struct RemoteWorldState {
    config: WorldBridgeRemoteConfig,
    client: WorldClient,
    cursor: Option<u64>,
    local_cursor: u64,
    local_by_remote: BTreeMap<RemoteRef, ProxyRecord>,
    remote_by_local: BTreeMap<u64, RemoteRef>,
    pending_despawns: BTreeSet<RemoteRef>,
}

impl RemoteWorldState {
    fn new(config: WorldBridgeRemoteConfig, client: WorldClient) -> Self {
        Self {
            config,
            client,
            cursor: None,
            local_cursor: 0,
            local_by_remote: BTreeMap::new(),
            remote_by_local: BTreeMap::new(),
            pending_despawns: BTreeSet::new(),
        }
    }
}

#[derive(Default)]
pub struct WorldBridge {
    remotes: BTreeMap<u64, RemoteWorldState>,
}

impl WorldBridge {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_remote(&mut self, config: WorldBridgeRemoteConfig) -> Result<(), String> {
        if self.remotes.contains_key(&config.world_id) {
            return Err(format!(
                "Remote world {} is already registered",
                config.world_id
            ));
        }

        let client = WorldClient::connect(&config.host, config.port, config.client_options)?;
        self.remotes
            .insert(config.world_id, RemoteWorldState::new(config, client));
        Ok(())
    }

    pub fn bootstrap_remote(&mut self, world: &mut World, world_id: u64) -> Result<(), String> {
        let remote = self
            .remotes
            .get_mut(&world_id)
            .ok_or_else(|| format!("Unknown remote world: {world_id}"))?;

        let current_now = call_remote(world, remote, |client| {
            client.invoke_typed::<(), ListEntitiesResult>("awrk.list_entities", ())
        })?
        .now;
        let metas = fetch_subscription_metas(world, remote)?;
        apply_full_resync(world, remote, current_now, metas)?;
        mark_all_proxy_states(world, remote, ProxyLifecycle::Live, current_now)?;
        remote.cursor = Some(current_now);
        queue_missing_local_despawn_candidates(world, remote);
        remote.local_cursor = world.change_seq_now();
        Ok(())
    }

    pub fn tick_remote(&mut self, world: &mut World, world_id: u64) -> Result<(), String> {
        let needs_bootstrap = self
            .remotes
            .get(&world_id)
            .ok_or_else(|| format!("Unknown remote world: {world_id}"))?
            .cursor
            .is_none();

        if needs_bootstrap {
            self.bootstrap_remote(world, world_id)?;
        }

        let remote = self
            .remotes
            .get_mut(&world_id)
            .ok_or_else(|| format!("Unknown remote world: {world_id}"))?;

        let mut pre_poll_errors = Vec::new();
        if let Err(err) = run_outbound_despawn_pass(world, remote) {
            pre_poll_errors.push(err);
        }
        run_outbound_create_pass(world, remote)?;

        let Some(mut cursor) = remote.cursor else {
            return Err(format!(
                "Remote world {world_id} has no sync cursor after bootstrap"
            ));
        };

        loop {
            let polled = call_remote(world, remote, |client| {
                client.poll_changes(cursor, Some(QUERY_PAGE_LIMIT))
            })?;

            if polled.needs_resync {
                mark_all_proxy_states(world, remote, ProxyLifecycle::Stale, polled.now)?;
                return self.bootstrap_remote(world, world_id);
            }

            let last_events = coalesce_change_events(polled.events);
            let mut upserted = Vec::new();
            let mut despawned = Vec::new();

            for event in last_events.into_values() {
                match event.kind {
                    ChangeKind::Upserted => upserted.push(event),
                    ChangeKind::Despawned => despawned.push(event.entity),
                }
            }

            if !upserted.is_empty() {
                let ids: Vec<u64> = upserted.iter().map(|event| event.entity).collect();
                let infos = fetch_entity_infos(world, remote, &ids)?;
                let info_by_entity: BTreeMap<u64, EntityInfo> =
                    infos.into_iter().map(|info| (info.entity, info)).collect();

                for event in upserted {
                    let remote_ref = RemoteRef {
                        world_id,
                        entity: event.entity,
                    };

                    let Some(info) = info_by_entity.get(&event.entity) else {
                        remove_proxy(world, remote, &remote_ref)?;
                        continue;
                    };

                    if !matches_subscription(&remote.config.subscription, info) {
                        remove_proxy(world, remote, &remote_ref)?;
                        continue;
                    }

                    let meta = EntityMeta {
                        entity: event.entity,
                        revision: event.revision,
                        parent: event.parent,
                    };
                    apply_entity_snapshot(world, remote, polled.now, &meta, info)?;
                }
            }

            for entity in despawned {
                remove_proxy(world, remote, &RemoteRef { world_id, entity })?;
            }

            repair_parent_links(world, remote)?;
            cursor = polled.cursor;
            remote.cursor = Some(cursor);

            if !polled.has_more {
                mark_all_proxy_states(world, remote, ProxyLifecycle::Live, polled.now)?;
                break;
            }
        }

        if pre_poll_errors.is_empty() {
            Ok(())
        } else {
            Err(pre_poll_errors.join("; "))
        }
    }

    pub fn tick_all(&mut self, world: &mut World) -> Result<(), String> {
        let world_ids: Vec<u64> = self.remotes.keys().copied().collect();
        let mut errors = Vec::new();

        for world_id in world_ids {
            if let Err(err) = self.tick_remote(world, world_id) {
                errors.push(format!("remote {world_id}: {err}"));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    pub fn local_entity_for_remote(&self, remote_ref: RemoteRef) -> Option<u64> {
        self.remotes
            .get(&remote_ref.world_id)
            .and_then(|remote| remote.local_by_remote.get(&remote_ref))
            .map(|record| record.local_entity)
    }

    pub fn remote_ref_for_local(&self, local_entity: u64) -> Option<RemoteRef> {
        self.remotes
            .values()
            .find_map(|remote| remote.remote_by_local.get(&local_entity).copied())
    }

    pub fn remove_remote(
        &mut self,
        world: &mut World,
        world_id: u64,
        despawn_local_proxies: bool,
    ) -> Result<(), String> {
        let Some(mut remote) = self.remotes.remove(&world_id) else {
            return Ok(());
        };

        if despawn_local_proxies {
            let remote_refs: Vec<RemoteRef> = remote.local_by_remote.keys().copied().collect();
            for remote_ref in remote_refs {
                remove_proxy(world, &mut remote, &remote_ref)?;
            }
        }

        Ok(())
    }

    pub fn clear(&mut self, world: &mut World, despawn_local_proxies: bool) -> Result<(), String> {
        let world_ids: Vec<u64> = self.remotes.keys().copied().collect();
        for world_id in world_ids {
            self.remove_remote(world, world_id, despawn_local_proxies)?;
        }
        Ok(())
    }
}

fn call_remote<T>(
    world: &mut World,
    remote: &mut RemoteWorldState,
    f: impl FnOnce(&mut WorldClient) -> Result<T, WorldClientError>,
) -> Result<T, String> {
    match f(&mut remote.client) {
        Ok(value) => Ok(value),
        Err(error) => {
            if error.is_transport() {
                let _ = mark_all_proxy_states(
                    world,
                    remote,
                    ProxyLifecycle::Disconnected,
                    remote.cursor.unwrap_or(0),
                );
            }
            Err(error.to_string())
        }
    }
}

fn fetch_subscription_metas(
    world: &mut World,
    remote: &mut RemoteWorldState,
) -> Result<Vec<EntityMeta>, String> {
    let mut metas = BTreeMap::new();
    let queries = remote.config.subscription.queries.clone();

    for clause in queries {
        let mut query = clause.to_query_args();
        query.limit = Some(QUERY_PAGE_LIMIT);

        loop {
            let page: QueryEntitiesResult =
                call_remote(world, remote, |client| client.query_entities(query.clone()))?;
            let has_more = page.has_more;
            let next_after = page.next_after;

            for meta in page.entities {
                metas.insert(meta.entity, meta);
            }

            if !has_more {
                break;
            }

            query.after = next_after;
        }
    }

    Ok(metas.into_values().collect())
}

fn fetch_entity_infos(
    world: &mut World,
    remote: &mut RemoteWorldState,
    entity_ids: &[u64],
) -> Result<Vec<EntityInfo>, String> {
    let mut out = Vec::new();

    for chunk in entity_ids.chunks(GET_ENTITIES_BATCH_SIZE) {
        let result = call_remote(world, remote, |client| {
            client.invoke_typed::<GetEntitiesArgs, GetEntitiesResult>(
                "awrk.get_entities",
                GetEntitiesArgs {
                    entities: chunk.to_vec(),
                },
            )
        })?;
        out.extend(result.entities);
    }

    Ok(out)
}

fn run_outbound_despawn_pass(
    world: &mut World,
    remote: &mut RemoteWorldState,
) -> Result<(), String> {
    collect_pending_outbound_despawns(world, remote)?;
    flush_pending_outbound_despawns(world, remote)
}

fn collect_pending_outbound_despawns(
    world: &mut World,
    remote: &mut RemoteWorldState,
) -> Result<(), String> {
    let mut cursor = remote.local_cursor;

    loop {
        let polled = world.poll_changes(cursor, Some(QUERY_PAGE_LIMIT))?;

        if polled.needs_resync {
            queue_missing_local_despawn_candidates(world, remote);
            remote.local_cursor = polled.now;
            break;
        }

        let last_events = coalesce_change_events(polled.events);
        for event in last_events.into_values() {
            if event.kind != ChangeKind::Despawned {
                continue;
            }

            if let Some(remote_ref) = outbound_despawn_candidate_for_local_entity(
                &remote.local_by_remote,
                &remote.remote_by_local,
                event.entity,
            ) {
                remote.pending_despawns.insert(remote_ref);
            }
        }

        cursor = polled.cursor;
        remote.local_cursor = cursor;

        if !polled.has_more {
            break;
        }
    }

    Ok(())
}

fn flush_pending_outbound_despawns(
    world: &mut World,
    remote: &mut RemoteWorldState,
) -> Result<(), String> {
    let pending: Vec<RemoteRef> = remote.pending_despawns.iter().copied().collect();
    let mut errors = Vec::new();

    for remote_ref in pending {
        let result = call_remote(world, remote, |client| {
            client.invoke_typed::<DespawnArgs, ()>(
                "awrk.despawn",
                DespawnArgs {
                    entity: remote_ref.entity,
                },
            )
        });

        match result {
            Ok(()) => {
                let _ = drop_proxy_mapping(remote, &remote_ref);
            }
            Err(error) => errors.push(format!(
                "remote despawn {}:{} failed: {error}",
                remote_ref.world_id, remote_ref.entity
            )),
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn run_outbound_create_pass(
    world: &mut World,
    remote: &mut RemoteWorldState,
) -> Result<(), String> {
    let local_entities = collect_outbound_create_candidates(world, remote.config.world_id);

    for local_entity in local_entities {
        if !world.contains_entity(local_entity) {
            continue;
        }

        let components =
            collect_outbound_spawn_components(world, &remote.config.subscription, local_entity)?;
        let spawn_result = call_remote(world, remote, |client| {
            client.invoke_typed::<SpawnArgs, SpawnResult>("awrk.spawn", SpawnArgs { components })
        });

        match spawn_result {
            Ok(result) => {
                let remote_ref = RemoteRef {
                    world_id: remote.config.world_id,
                    entity: result.entity,
                };

                remote.local_by_remote.insert(
                    remote_ref,
                    ProxyRecord {
                        local_entity,
                        remote_revision: 0,
                        remote_parent: None,
                        origin: ProxyOrigin::Local,
                    },
                );
                remote.remote_by_local.insert(local_entity, remote_ref);

                upsert_local_component(world, local_entity, ProxyEntity { remote: remote_ref })?;
                upsert_local_component(
                    world,
                    local_entity,
                    ProxyState {
                        last_remote_revision: 0,
                        lifecycle: ProxyLifecycle::Creating,
                        last_seen_at: remote.cursor.unwrap_or(0),
                    },
                )?;
                remove_local_component::<ProxySpawnRequest>(world, local_entity)?;
                remove_local_component::<ProxySpawnError>(world, local_entity)?;
            }
            Err(error) => {
                remove_local_component::<ProxySpawnRequest>(world, local_entity)?;
                upsert_local_component(world, local_entity, ProxySpawnError { message: error })?;
            }
        }
    }

    Ok(())
}

fn collect_outbound_create_candidates(world: &mut World, world_id: u64) -> Vec<u64> {
    let mut candidates = Vec::new();

    world.iter::<(
        &WorldId,
        &ProxyAuthority,
        &ProxySpawnRequest,
        Option<&ProxyEntity>,
    ), _>(|entity, (entity_world, authority, _, proxy_entity)| {
        if entity_world.0 != world_id {
            return;
        }

        if proxy_entity.is_some() {
            return;
        }

        if !matches!(
            authority.authority,
            ProxyAuthorityKind::RequestDriven | ProxyAuthorityKind::Local
        ) {
            return;
        }

        candidates.push(entity);
    });

    candidates
}

fn collect_outbound_spawn_components(
    world: &World,
    subscription: &ProxySubscription,
    entity: u64,
) -> Result<Vec<ComponentInfo>, String> {
    let mut components = Vec::new();

    for type_name in &subscription.outbound_create_components {
        if is_bridge_managed_component(type_name) {
            continue;
        }

        if let Some(component) = world.snapshot_component(entity, type_name)? {
            components.push(component);
        }
    }

    Ok(components)
}

fn outbound_despawn_candidate_for_local_entity(
    local_by_remote: &BTreeMap<RemoteRef, ProxyRecord>,
    remote_by_local: &BTreeMap<u64, RemoteRef>,
    local_entity: u64,
) -> Option<RemoteRef> {
    let remote_ref = remote_by_local.get(&local_entity).copied()?;
    let record = local_by_remote.get(&remote_ref)?;

    matches!(record.origin, ProxyOrigin::Local).then_some(remote_ref)
}

fn queue_missing_local_despawn_candidates(world: &World, remote: &mut RemoteWorldState) {
    let pending = collect_missing_local_despawn_candidates(world, &remote.local_by_remote);

    remote.pending_despawns.extend(pending);
}

fn collect_missing_local_despawn_candidates(
    world: &World,
    local_by_remote: &BTreeMap<RemoteRef, ProxyRecord>,
) -> Vec<RemoteRef> {
    local_by_remote
        .iter()
        .filter_map(|(remote_ref, record)| {
            matches!(record.origin, ProxyOrigin::Local)
                .then_some(record.local_entity)
                .filter(|local_entity| !world.contains_entity(*local_entity))
                .map(|_| *remote_ref)
        })
        .collect()
}

fn apply_full_resync(
    world: &mut World,
    remote: &mut RemoteWorldState,
    seen_at: u64,
    metas: Vec<EntityMeta>,
) -> Result<(), String> {
    let mut ids: Vec<u64> = metas.iter().map(|meta| meta.entity).collect();
    ids.sort_unstable();
    ids.dedup();

    let infos = fetch_entity_infos(world, remote, &ids)?;
    let info_by_entity: BTreeMap<u64, EntityInfo> =
        infos.into_iter().map(|info| (info.entity, info)).collect();
    let mut live_remote_refs = BTreeSet::new();

    for meta in metas {
        let remote_ref = RemoteRef {
            world_id: remote.config.world_id,
            entity: meta.entity,
        };

        let Some(info) = info_by_entity.get(&meta.entity) else {
            remove_proxy(world, remote, &remote_ref)?;
            continue;
        };

        if !matches_subscription(&remote.config.subscription, info) {
            remove_proxy(world, remote, &remote_ref)?;
            continue;
        }

        apply_entity_snapshot(world, remote, seen_at, &meta, info)?;
        live_remote_refs.insert(remote_ref);
    }

    let stale: Vec<RemoteRef> = remote
        .local_by_remote
        .keys()
        .filter(|remote_ref| !live_remote_refs.contains(remote_ref))
        .copied()
        .collect();

    for remote_ref in stale {
        remove_proxy(world, remote, &remote_ref)?;
    }

    repair_parent_links(world, remote)?;
    Ok(())
}

fn apply_entity_snapshot(
    world: &mut World,
    remote: &mut RemoteWorldState,
    seen_at: u64,
    meta: &EntityMeta,
    info: &EntityInfo,
) -> Result<(), String> {
    let remote_ref = RemoteRef {
        world_id: remote.config.world_id,
        entity: meta.entity,
    };

    let local_entity = if let Some(record) = remote.local_by_remote.get(&remote_ref) {
        record.local_entity
    } else {
        let local_entity = world.spawn((
            WorldId(remote.config.world_id),
            ProxyEntity { remote: remote_ref },
            ProxyState {
                last_remote_revision: meta.revision,
                lifecycle: ProxyLifecycle::Creating,
                last_seen_at: seen_at,
            },
            ProxyAuthority {
                authority: remote.config.authority,
            },
        ));

        remote.local_by_remote.insert(
            remote_ref,
            ProxyRecord {
                local_entity,
                remote_revision: meta.revision,
                remote_parent: meta.parent,
                origin: ProxyOrigin::Remote,
            },
        );
        remote.remote_by_local.insert(local_entity, remote_ref);
        local_entity
    };

    upsert_local_component(world, local_entity, WorldId(remote.config.world_id))?;
    upsert_local_component(world, local_entity, ProxyEntity { remote: remote_ref })?;
    upsert_local_component(
        world,
        local_entity,
        world
            .component::<ProxyAuthority>(local_entity)
            .map(|authority| *authority)
            .unwrap_or(ProxyAuthority {
                authority: remote.config.authority,
            }),
    )?;
    upsert_local_component(
        world,
        local_entity,
        ProxyState {
            last_remote_revision: meta.revision,
            lifecycle: ProxyLifecycle::Live,
            last_seen_at: seen_at,
        },
    )?;

    if let Some(remote_parent) = meta.parent {
        upsert_local_component(
            world,
            local_entity,
            RemoteParentRef {
                remote: RemoteRef {
                    world_id: remote.config.world_id,
                    entity: remote_parent,
                },
            },
        )?;
    } else {
        remove_local_component::<RemoteParentRef>(world, local_entity)?;
    }

    let snapshot_values: BTreeMap<&str, &awrk_datex::value::Value> = info
        .components
        .iter()
        .filter_map(|component| {
            component
                .value
                .as_ref()
                .map(|value| (component.type_name.as_str(), value))
        })
        .collect();

    let mirrored_types: Vec<String> = remote
        .config
        .subscription
        .mirrored_components
        .iter()
        .filter(|type_name| !is_bridge_managed_component(type_name))
        .cloned()
        .collect();

    for type_name in &mirrored_types {
        if let Some(value) = snapshot_values.get(type_name.as_str()) {
            world.set_component_value(local_entity, type_name, (*value).clone())?;
        } else {
            let _ = world.remove_component(local_entity, type_name)?;
        }
    }

    let record = remote
        .local_by_remote
        .get_mut(&remote_ref)
        .expect("proxy record exists after snapshot apply");
    record.remote_revision = meta.revision;
    record.remote_parent = meta.parent;

    Ok(())
}

fn is_bridge_managed_component(type_name: &str) -> bool {
    type_name == core::any::type_name::<Parent>()
        || type_name == core::any::type_name::<ProxyEntity>()
        || type_name == core::any::type_name::<ProxyState>()
        || type_name == core::any::type_name::<ProxyAuthority>()
        || type_name == core::any::type_name::<ProxySpawnRequest>()
        || type_name == core::any::type_name::<ProxySpawnError>()
        || type_name == core::any::type_name::<RemoteParentRef>()
}

fn repair_parent_links(world: &mut World, remote: &mut RemoteWorldState) -> Result<(), String> {
    let repairs: Vec<(u64, Option<u64>)> = remote
        .local_by_remote
        .values()
        .map(|record| {
            let local_parent = record.remote_parent.and_then(|remote_parent| {
                remote
                    .local_by_remote
                    .get(&RemoteRef {
                        world_id: remote.config.world_id,
                        entity: remote_parent,
                    })
                    .map(|parent_record| parent_record.local_entity)
            });
            (record.local_entity, local_parent)
        })
        .collect();

    for (local_entity, local_parent) in repairs {
        if let Some(local_parent) = local_parent {
            upsert_local_component(
                world,
                local_entity,
                Parent {
                    parent: local_parent,
                },
            )?;
        } else {
            remove_local_component::<Parent>(world, local_entity)?;
        }
    }

    Ok(())
}

fn remove_proxy(
    world: &mut World,
    remote: &mut RemoteWorldState,
    remote_ref: &RemoteRef,
) -> Result<(), String> {
    let Some(record) = drop_proxy_mapping(remote, remote_ref) else {
        return Ok(());
    };

    if world.contains_entity(record.local_entity) {
        world.despawn(record.local_entity)?;
    }
    Ok(())
}

fn drop_proxy_mapping(
    remote: &mut RemoteWorldState,
    remote_ref: &RemoteRef,
) -> Option<ProxyRecord> {
    let record = remote.local_by_remote.remove(remote_ref)?;
    remote.remote_by_local.remove(&record.local_entity);
    remote.pending_despawns.remove(remote_ref);
    Some(record)
}

fn mark_all_proxy_states(
    world: &mut World,
    remote: &RemoteWorldState,
    lifecycle: ProxyLifecycle,
    seen_at: u64,
) -> Result<(), String> {
    let proxies: Vec<(u64, u64)> = remote
        .local_by_remote
        .values()
        .map(|record| (record.local_entity, record.remote_revision))
        .collect();

    for (local_entity, last_remote_revision) in proxies {
        if !world.contains_entity(local_entity) {
            continue;
        }

        upsert_local_component(
            world,
            local_entity,
            ProxyState {
                last_remote_revision,
                lifecycle,
                last_seen_at: seen_at,
            },
        )?;
    }

    Ok(())
}

fn matches_query_clause(query: &ProxyQueryClause, info: &EntityInfo) -> bool {
    let component_names: BTreeSet<&str> = info
        .components
        .iter()
        .map(|component| component.type_name.as_str())
        .collect();

    if query
        .all_of
        .iter()
        .any(|name| !component_names.contains(name.as_str()))
    {
        return false;
    }

    if !query.any_of.is_empty()
        && !query
            .any_of
            .iter()
            .any(|name| component_names.contains(name.as_str()))
    {
        return false;
    }

    if query
        .none_of
        .iter()
        .any(|name| component_names.contains(name.as_str()))
    {
        return false;
    }

    true
}

fn matches_subscription(subscription: &ProxySubscription, info: &EntityInfo) -> bool {
    subscription
        .queries
        .iter()
        .any(|query| matches_query_clause(query, info))
}

fn coalesce_change_events(events: Vec<ChangeEvent>) -> BTreeMap<u64, ChangeEvent> {
    let mut last_by_entity = BTreeMap::new();
    for event in events {
        last_by_entity.insert(event.entity, event);
    }
    last_by_entity
}

fn upsert_local_component<T>(world: &mut World, entity: u64, value: T) -> Result<(), String>
where
    T: hecs::Component + Send + Sync + 'static,
{
    let mut entity_mut = world.entity_mut(entity)?;
    if let Some(mut existing) = entity_mut.get_mut::<T>() {
        *existing = value;
    } else {
        entity_mut.insert_one(value)?;
    }
    Ok(())
}

fn remove_local_component<T>(world: &mut World, entity: u64) -> Result<(), String>
where
    T: hecs::Component + Send + Sync + 'static,
{
    if !world.contains_entity(entity) {
        return Ok(());
    }

    let mut entity_mut = world.entity_mut(entity)?;
    let _ = entity_mut.remove_one::<T>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use awrk_datex::value::Value;
    use std::collections::BTreeSet;
    use std::net::TcpListener;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::thread;
    use std::time::{Duration, Instant};

    struct TestQueryA;
    struct TestQueryB;
    struct TestMirroredA;
    struct TestMirroredB;
    #[derive(
        Debug,
        Clone,
        PartialEq,
        awrk_datex::Encode,
        awrk_datex::Decode,
        awrk_datex::Patch,
        awrk_schema_macros::Schema,
    )]
    struct TestOutboundComponent(pub u32);

    #[derive(
        Debug,
        Clone,
        PartialEq,
        awrk_datex::Encode,
        awrk_datex::Decode,
        awrk_datex::Patch,
        awrk_schema_macros::Schema,
    )]
    struct TestInboundOnlyComponent(pub u32);

    crate::register_proxy_subscription! {
        all_of: [TestQueryA],
        any_of: [],
        none_of: [],
        components: [TestMirroredA],
        outbound_create_components: [TestMirroredA],
    }

    crate::register_proxy_subscription! {
        all_of: [TestQueryB],
        any_of: [],
        none_of: [],
        components: [TestMirroredB],
        outbound_create_components: [TestMirroredB],
    }

    fn info_with_components(names: &[&str]) -> EntityInfo {
        EntityInfo {
            entity: 1,
            revision: 1,
            components: names
                .iter()
                .map(|name| crate::rpc::ComponentInfo {
                    type_name: (*name).to_string(),
                    value: Some(Value::Unit),
                })
                .collect(),
        }
    }

    #[test]
    fn subscription_matching_respects_component_filters() {
        let subscription = ProxySubscription {
            queries: vec![ProxyQueryClause {
                all_of: vec!["a".to_string(), "b".to_string()],
                any_of: vec!["c".to_string(), "d".to_string()],
                none_of: vec!["z".to_string()],
            }],
            mirrored_components: BTreeSet::new(),
            outbound_create_components: BTreeSet::new(),
        };

        assert!(matches_subscription(
            &subscription,
            &info_with_components(&["a", "b", "d"])
        ));
        assert!(!matches_subscription(
            &subscription,
            &info_with_components(&["a", "d"])
        ));
        assert!(!matches_subscription(
            &subscription,
            &info_with_components(&["a", "b", "z"])
        ));
        assert!(!matches_subscription(
            &subscription,
            &info_with_components(&["a", "b"])
        ));
    }

    #[test]
    fn subscription_matching_unions_registered_queries() {
        let subscription = ProxySubscription::new();

        assert!(matches_subscription(
            &subscription,
            &info_with_components(&[core::any::type_name::<TestQueryA>()])
        ));
        assert!(matches_subscription(
            &subscription,
            &info_with_components(&[core::any::type_name::<TestQueryB>()])
        ));
        assert!(!matches_subscription(
            &subscription,
            &info_with_components(&["other"])
        ));
        assert!(
            subscription
                .mirrored_components
                .contains(core::any::type_name::<TestMirroredA>())
        );
        assert!(
            subscription
                .mirrored_components
                .contains(core::any::type_name::<TestMirroredB>())
        );
        assert!(
            subscription
                .outbound_create_components
                .contains(core::any::type_name::<TestMirroredA>())
        );
        assert!(
            subscription
                .outbound_create_components
                .contains(core::any::type_name::<TestMirroredB>())
        );
    }

    #[test]
    fn coalesce_change_events_keeps_last_event_per_entity() {
        let events = vec![
            ChangeEvent {
                seq: 1,
                entity: 10,
                revision: 1,
                parent: None,
                kind: ChangeKind::Upserted,
            },
            ChangeEvent {
                seq: 2,
                entity: 20,
                revision: 1,
                parent: None,
                kind: ChangeKind::Upserted,
            },
            ChangeEvent {
                seq: 3,
                entity: 10,
                revision: 2,
                parent: Some(20),
                kind: ChangeKind::Despawned,
            },
        ];

        let out = coalesce_change_events(events);
        assert_eq!(out.len(), 2);
        assert_eq!(out.get(&10).expect("entity 10").seq, 3);
        assert_eq!(out.get(&20).expect("entity 20").seq, 2);
    }

    #[test]
    fn outbound_create_candidates_require_matching_world_authority_and_marker() {
        let mut world = World::new();

        let request_driven = world.spawn((
            WorldId(7),
            ProxyAuthority {
                authority: ProxyAuthorityKind::RequestDriven,
            },
            ProxySpawnRequest,
        ));
        let local = world.spawn((
            WorldId(7),
            ProxyAuthority {
                authority: ProxyAuthorityKind::Local,
            },
            ProxySpawnRequest,
        ));
        let _wrong_world = world.spawn((
            WorldId(8),
            ProxyAuthority {
                authority: ProxyAuthorityKind::RequestDriven,
            },
            ProxySpawnRequest,
        ));
        let _wrong_authority = world.spawn((
            WorldId(7),
            ProxyAuthority {
                authority: ProxyAuthorityKind::Remote,
            },
            ProxySpawnRequest,
        ));
        let _already_bound = world.spawn((
            WorldId(7),
            ProxyAuthority {
                authority: ProxyAuthorityKind::RequestDriven,
            },
            ProxySpawnRequest,
            ProxyEntity {
                remote: RemoteRef {
                    world_id: 7,
                    entity: 99,
                },
            },
        ));

        let mut candidates = collect_outbound_create_candidates(&mut world, 7);
        candidates.sort_unstable();
        let mut expected = vec![request_driven, local];
        expected.sort_unstable();

        assert_eq!(candidates, expected);
    }

    #[test]
    fn outbound_spawn_components_only_include_registered_outbound_types_present_on_entity() {
        let mut world = World::new();
        world
            .types_mut()
            .register_component_named::<TestOutboundComponent>(
                core::any::type_name::<TestOutboundComponent>().to_string(),
            )
            .expect("register outbound component");
        world
            .types_mut()
            .register_component_named::<TestInboundOnlyComponent>(
                core::any::type_name::<TestInboundOnlyComponent>().to_string(),
            )
            .expect("register inbound-only component");

        let entity = world.spawn((TestOutboundComponent(5), TestInboundOnlyComponent(9)));
        let subscription = ProxySubscription {
            queries: Vec::new(),
            mirrored_components: BTreeSet::from([
                core::any::type_name::<TestOutboundComponent>().to_string(),
                core::any::type_name::<TestInboundOnlyComponent>().to_string(),
            ]),
            outbound_create_components: BTreeSet::from([core::any::type_name::<
                TestOutboundComponent,
            >()
            .to_string()]),
        };

        let components = collect_outbound_spawn_components(&world, &subscription, entity)
            .expect("collect outbound components");

        assert_eq!(components.len(), 1);
        assert_eq!(
            components[0].type_name,
            core::any::type_name::<TestOutboundComponent>()
        );
    }

    #[test]
    fn outbound_despawn_candidate_only_returns_locally_originated_proxy() {
        let local_remote_ref = RemoteRef {
            world_id: 7,
            entity: 10,
        };
        let mirrored_remote_ref = RemoteRef {
            world_id: 7,
            entity: 11,
        };
        let local_by_remote = BTreeMap::from([
            (
                local_remote_ref,
                ProxyRecord {
                    local_entity: 100,
                    remote_revision: 0,
                    remote_parent: None,
                    origin: ProxyOrigin::Local,
                },
            ),
            (
                mirrored_remote_ref,
                ProxyRecord {
                    local_entity: 101,
                    remote_revision: 0,
                    remote_parent: None,
                    origin: ProxyOrigin::Remote,
                },
            ),
        ]);
        let remote_by_local = BTreeMap::from([(100, local_remote_ref), (101, mirrored_remote_ref)]);

        assert_eq!(
            outbound_despawn_candidate_for_local_entity(&local_by_remote, &remote_by_local, 100),
            Some(local_remote_ref)
        );
        assert_eq!(
            outbound_despawn_candidate_for_local_entity(&local_by_remote, &remote_by_local, 101),
            None
        );
        assert_eq!(
            outbound_despawn_candidate_for_local_entity(&local_by_remote, &remote_by_local, 999),
            None
        );
    }

    #[test]
    fn collect_missing_local_despawn_candidates_only_returns_missing_local_origins() {
        let mut world = World::new();
        let still_alive = world.spawn(());
        let removed_local = world.spawn(());
        let removed_remote = world.spawn(());
        world
            .despawn(removed_local)
            .expect("despawn local-origin proxy");
        world
            .despawn(removed_remote)
            .expect("despawn mirrored proxy");

        let alive_remote_ref = RemoteRef {
            world_id: 7,
            entity: 20,
        };
        let removed_local_remote_ref = RemoteRef {
            world_id: 7,
            entity: 21,
        };
        let removed_remote_remote_ref = RemoteRef {
            world_id: 7,
            entity: 22,
        };

        let local_by_remote = BTreeMap::from([
            (
                alive_remote_ref,
                ProxyRecord {
                    local_entity: still_alive,
                    remote_revision: 0,
                    remote_parent: None,
                    origin: ProxyOrigin::Local,
                },
            ),
            (
                removed_local_remote_ref,
                ProxyRecord {
                    local_entity: removed_local,
                    remote_revision: 0,
                    remote_parent: None,
                    origin: ProxyOrigin::Local,
                },
            ),
            (
                removed_remote_remote_ref,
                ProxyRecord {
                    local_entity: removed_remote,
                    remote_revision: 0,
                    remote_parent: None,
                    origin: ProxyOrigin::Remote,
                },
            ),
        ]);

        let pending = collect_missing_local_despawn_candidates(&world, &local_by_remote);

        assert!(pending.contains(&removed_local_remote_ref));
        assert!(!pending.contains(&alive_remote_ref));
        assert!(!pending.contains(&removed_remote_remote_ref));
    }

    fn next_test_port() -> u16 {
        TcpListener::bind(("127.0.0.1", 0))
            .expect("bind ephemeral port")
            .local_addr()
            .expect("get local addr")
            .port()
    }

    fn start_bridge_test_server<F>(port: u16, setup: F) -> Arc<AtomicBool>
    where
        F: FnOnce(&mut crate::core::World) + Send + 'static,
    {
        let running = Arc::new(AtomicBool::new(true));
        let running_flag = Arc::clone(&running);

        thread::spawn(move || {
            let crate::core::ProcessParts {
                name,
                mut world,
                mut rpcs,
                mut sessions,
                ..
            } = crate::core::Process::new_with_sessions("bridge-test-server", port).into_parts();

            setup(&mut world);
            sessions.start(&name).expect("start test server sessions");

            while running_flag.load(Ordering::Relaxed) {
                sessions.handle(&mut world, &mut rpcs);
                thread::sleep(Duration::from_millis(5));
            }
        });

        running
    }

    fn bridge_test_subscription() -> ProxySubscription {
        ProxySubscription {
            queries: vec![ProxyQueryClause {
                all_of: vec![core::any::type_name::<crate::Name>().to_string()],
                any_of: Vec::new(),
                none_of: Vec::new(),
            }],
            mirrored_components: BTreeSet::from([core::any::type_name::<crate::Name>().to_string()]),
            outbound_create_components: BTreeSet::from([core::any::type_name::<crate::Name>().to_string()]),
        }
    }

    fn wait_until(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if condition() {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        condition()
    }

    #[test]
    fn bridge_integration_supports_outbound_create_and_remote_despawn() {
        let port = next_test_port();
        let running = start_bridge_test_server(port, |_| {});
        thread::sleep(Duration::from_millis(30));

        let crate::core::ProcessParts {
            mut world,
            mut remotes,
            ..
        } = crate::core::Process::new("bridge-test-client").into_parts();

        remotes
            .add_remote(WorldBridgeRemoteConfig {
                world_id: 1,
                host: "127.0.0.1".to_string(),
                port,
                client_options: WorldClientOptions::default(),
                subscription: bridge_test_subscription(),
                authority: ProxyAuthorityKind::Remote,
            })
            .expect("add remote world");

        let local_entity = world.spawn((
            WorldId(1),
            ProxyAuthority {
                authority: ProxyAuthorityKind::RequestDriven,
            },
            ProxySpawnRequest,
            crate::Name("bridge-create".to_string()),
        ));

        assert!(wait_until(Duration::from_secs(3), || {
            remotes.tick_all(&mut world).is_ok()
                && world.component::<ProxyEntity>(local_entity).is_some()
                && matches!(
                    world.component::<ProxyState>(local_entity)
                        .map(|state| state.lifecycle),
                    Some(ProxyLifecycle::Live)
                )
        }));

        let remote_ref = world
            .component::<ProxyEntity>(local_entity)
            .map(|value| value.remote)
            .expect("local proxy remote ref");

        let mut client = WorldClient::connect("127.0.0.1", port, WorldClientOptions::default())
            .expect("connect verification client");
        assert!(wait_until(Duration::from_secs(2), || {
            client
                .list_entities()
                .map(|result| result.entities.iter().any(|meta| meta.entity == remote_ref.entity))
                .unwrap_or(false)
        }));

        world.despawn(local_entity).expect("despawn local proxy");

        assert!(wait_until(Duration::from_secs(3), || {
            remotes.tick_all(&mut world).is_ok()
                && client
                    .list_entities()
                    .map(|result| result.entities.iter().all(|meta| meta.entity != remote_ref.entity))
                    .unwrap_or(false)
        }));

        running.store(false, Ordering::Relaxed);
    }

    #[test]
    fn bridge_integration_does_not_remote_despawn_inbound_only_mirrors() {
        let port = next_test_port();
        let running = start_bridge_test_server(port, |world| {
            world.spawn((crate::Name("remote-only".to_string()),));
        });
        thread::sleep(Duration::from_millis(30));

        let crate::core::ProcessParts {
            mut world,
            mut remotes,
            ..
        } = crate::core::Process::new("bridge-test-client").into_parts();

        remotes
            .add_remote(WorldBridgeRemoteConfig {
                world_id: 1,
                host: "127.0.0.1".to_string(),
                port,
                client_options: WorldClientOptions::default(),
                subscription: bridge_test_subscription(),
                authority: ProxyAuthorityKind::Remote,
            })
            .expect("add remote world");

        let mut client = WorldClient::connect("127.0.0.1", port, WorldClientOptions::default())
            .expect("connect verification client");
        let remote_entity = client
            .list_entities()
            .expect("list remote entities")
            .entities
            .into_iter()
            .next()
            .expect("remote test entity")
            .entity;

        let local_proxy = {
            let remote_ref = RemoteRef {
                world_id: 1,
                entity: remote_entity,
            };

            let mut local_entity = None;
            assert!(wait_until(Duration::from_secs(3), || {
                if remotes.tick_all(&mut world).is_err() {
                    return false;
                }
                local_entity = remotes.local_entity_for_remote(remote_ref);
                local_entity.is_some()
            }));
            local_entity.expect("local mirrored proxy")
        };

        world
            .despawn(local_proxy)
            .expect("despawn inbound-only mirrored proxy locally");

        assert!(wait_until(Duration::from_secs(2), || {
            remotes.tick_all(&mut world).is_ok()
                && client
                    .list_entities()
                    .map(|result| result.entities.iter().any(|meta| meta.entity == remote_entity))
                    .unwrap_or(false)
        }));

        running.store(false, Ordering::Relaxed);
    }
}
