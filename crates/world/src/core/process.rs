use crate::bridge::{WorldBridge, WorldBridgeRemoteConfig};
use crate::core::changes::ChangeLog;
use crate::core::type_registry::WorldTypeRegistry;
use crate::rpc::{EntityInfo, EntityMeta, register_builtin_rpcs};
use crate::transport::{Session, SessionListener, WORLD_MAX_FRAME_SIZE};
use crate::{
    Name, Parent, ProxyAuthority, ProxyAuthorityKind, ProxyEntity, ProxyLifecycle, ProxySpawnError,
    ProxySpawnRequest, ProxyState, RemoteParentRef, RemoteRef, WorldId,
};
use awrk_datex::codec::decode::DecodeConfig;
use awrk_datex::codec::encode::EncodeConfig;
use awrk_datex_rpc::{RpcRegistryWithCtx, decode_envelope};
use clap::Parser;
use hecs::Fetch;

fn entity_parent_bits(world: &hecs::World, entity: hecs::Entity) -> Option<u64> {
    world.get::<&Parent>(entity).ok().map(|p| p.parent)
}

fn query_has_unique_borrows<Q: hecs::Query>() -> bool {
    let mut has_unique = false;
    Q::Fetch::for_each_borrow(|_ty, unique| {
        has_unique |= unique;
    });
    has_unique
}

#[derive(Parser)]
pub struct WorldArgs {
    pub port: Option<u16>,
}

pub struct WorldEntityMut<'w> {
    entity: hecs::Entity,
    world: &'w mut hecs::World,
    changes: &'w mut ChangeLog,
}

impl<'w> WorldEntityMut<'w> {
    pub fn bits(&self) -> u64 {
        self.entity.to_bits().get()
    }

    pub fn get_ref<T: hecs::Component>(&self) -> Option<hecs::Ref<'_, T>> {
        self.world.get::<&T>(self.entity).ok()
    }

    pub fn get_mut<T: hecs::Component>(&mut self) -> Option<hecs::RefMut<'_, T>> {
        if self.world.get::<&T>(self.entity).is_err() {
            return None;
        }

        self.changes.bump_entity_revision(&*self.world, self.entity);
        self.world.get::<&mut T>(self.entity).ok()
    }

    pub fn insert_one<T: hecs::Component>(&mut self, value: T) -> Result<(), String> {
        self.world
            .insert_one(self.entity, value)
            .map_err(|e| e.to_string())?;

        self.changes.bump_entity_revision(&*self.world, self.entity);
        Ok(())
    }

    pub fn remove_one<T: hecs::Component>(&mut self) -> Result<bool, String> {
        if self.world.get::<&T>(self.entity).is_ok() {
            let _ = self
                .world
                .remove_one::<T>(self.entity)
                .map_err(|e| e.to_string())?;

            self.changes.bump_entity_revision(&*self.world, self.entity);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

pub struct World {
    pub(crate) world: hecs::World,
    pub(crate) changes: ChangeLog,
    pub(crate) types: WorldTypeRegistry,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    pub fn new() -> Self {
        Self {
            world: hecs::World::new(),
            changes: ChangeLog::default(),
            types: WorldTypeRegistry::default(),
        }
    }

    pub(crate) fn raw(&self) -> &hecs::World {
        &self.world
    }

    pub(crate) fn types(&self) -> &WorldTypeRegistry {
        &self.types
    }

    pub(crate) fn types_mut(&mut self) -> &mut WorldTypeRegistry {
        &mut self.types
    }

    pub(crate) fn change_seq_now(&self) -> u64 {
        self.changes.now()
    }

    pub(crate) fn poll_changes(
        &self,
        since: u64,
        limit: Option<u32>,
    ) -> Result<crate::rpc::PollChangesResult, String> {
        self.changes.poll(since, limit)
    }

    pub fn spawn(&mut self, components: impl hecs::DynamicBundle) -> u64 {
        let entity = self.world.spawn(components);
        self.bump_entity_revision(entity);
        entity.to_bits().get()
    }

    pub fn spawn_empty(&mut self) -> u64 {
        self.spawn(())
    }

    pub fn despawn(&mut self, entity_bits: u64) -> Result<(), String> {
        let entity = hecs::Entity::from_bits(entity_bits)
            .ok_or_else(|| format!("Unknown entity: {entity_bits}"))?;
        if !self.world.contains(entity) {
            return Err(format!("Unknown entity: {entity_bits}"));
        }

        self.world.despawn(entity).map_err(|e| e.to_string())?;
        self.changes.remove_entity_revision(entity);
        self.changes.log_despawn(entity_bits);
        Ok(())
    }

    pub fn component<'w, T: hecs::Component>(
        &'w self,
        entity_bits: u64,
    ) -> Option<hecs::Ref<'w, T>> {
        let entity = hecs::Entity::from_bits(entity_bits)?;
        self.world.get::<&T>(entity).ok()
    }

    pub fn component_mut<'w, T: hecs::Component>(
        &'w mut self,
        entity_bits: u64,
    ) -> Option<hecs::RefMut<'w, T>> {
        let entity = hecs::Entity::from_bits(entity_bits)?;

        if self.world.get::<&T>(entity).is_err() {
            return None;
        }

        self.bump_entity_revision(entity);
        self.world.get::<&mut T>(entity).ok()
    }

    pub fn contains_entity(&self, entity_bits: u64) -> bool {
        hecs::Entity::from_bits(entity_bits)
            .map(|entity| self.world.contains(entity))
            .unwrap_or(false)
    }

    pub fn snapshot_entity_components(
        &self,
        entity_bits: u64,
    ) -> Result<Vec<crate::rpc::ComponentInfo>, String> {
        let entity = hecs::Entity::from_bits(entity_bits)
            .ok_or_else(|| format!("Unknown entity: {entity_bits}"))?;
        if !self.world.contains(entity) {
            return Err(format!("Unknown entity: {entity_bits}"));
        }

        self.types.snapshot_entity_components(&self.world, entity)
    }

    pub fn snapshot_component(
        &self,
        entity_bits: u64,
        type_name: &str,
    ) -> Result<Option<crate::rpc::ComponentInfo>, String> {
        let entity = hecs::Entity::from_bits(entity_bits)
            .ok_or_else(|| format!("Unknown entity: {entity_bits}"))?;
        if !self.world.contains(entity) {
            return Err(format!("Unknown entity: {entity_bits}"));
        }

        self.types
            .snapshot_component(&self.world, entity, type_name)
    }

    pub fn entity_mut(&mut self, entity_bits: u64) -> Result<WorldEntityMut<'_>, String> {
        let entity = hecs::Entity::from_bits(entity_bits)
            .ok_or_else(|| format!("Unknown entity: {entity_bits}"))?;
        if !self.world.contains(entity) {
            return Err(format!("Unknown entity: {entity_bits}"));
        }

        let world = &mut self.world;
        let changes = &mut self.changes;

        let parent = entity_parent_bits(&*world, entity);
        changes.bump_entity_revision_with_parent(entity, parent);

        Ok(WorldEntityMut {
            entity,
            world,
            changes,
        })
    }

    pub fn iter<Q, F>(&mut self, mut f: F)
    where
        Q: hecs::Query,
        for<'a> F: FnMut(u64, Q::Item<'a>),
    {
        let world = &mut self.world;
        let changes = &mut self.changes;

        let mutating_query = query_has_unique_borrows::<Q>();

        let mut q = world.query::<(hecs::Entity, Q)>();
        for (entity, item) in q.iter() {
            if mutating_query {
                changes.bump_entity_revision_with_parent(entity, None);
            }
            f(entity.to_bits().get(), item);
        }
    }

    pub fn remove_component(&mut self, entity_bits: u64, type_name: &str) -> Result<bool, String> {
        let entity = hecs::Entity::from_bits(entity_bits)
            .ok_or_else(|| format!("Unknown entity: {entity_bits}"))?;
        if !self.world.contains(entity) {
            return Err(format!("Unknown entity: {entity_bits}"));
        }

        let removed = self
            .types
            .remove_component(&mut self.world, entity, type_name)?;
        if removed {
            self.bump_entity_revision(entity);
        }
        Ok(removed)
    }

    pub fn set_component_value(
        &mut self,
        entity_bits: u64,
        type_name: &str,
        value: awrk_datex::value::Value,
    ) -> Result<(), String> {
        let entity = hecs::Entity::from_bits(entity_bits)
            .ok_or_else(|| format!("Unknown entity: {entity_bits}"))?;
        if !self.world.contains(entity) {
            return Err(format!("Unknown entity: {entity_bits}"));
        }

        self.types
            .set_component(&mut self.world, entity, type_name, value)?;
        self.bump_entity_revision(entity);
        Ok(())
    }

    pub(crate) fn set_component_by_entity(
        &mut self,
        entity: hecs::Entity,
        type_name: &str,
        value: awrk_datex::value::Value,
    ) -> Result<(), String> {
        self.types
            .set_component(&mut self.world, entity, type_name, value)
    }

    pub(crate) fn patch_component_by_entity(
        &mut self,
        entity: hecs::Entity,
        type_name: &str,
        patch: awrk_datex::value::Value,
    ) -> Result<(), String> {
        self.types
            .patch_component(&mut self.world, entity, type_name, patch)
    }

    pub fn registered_component_type_names(&self) -> Vec<String> {
        self.types
            .type_names()
            .filter(|type_name| self.types.is_registered_component_type(type_name))
            .map(str::to_string)
            .collect()
    }

    pub(crate) fn bump_entity_revision(&mut self, entity: hecs::Entity) -> u64 {
        self.changes.bump_entity_revision(&self.world, entity)
    }

    pub(crate) fn snapshot_entities_meta(&self) -> Vec<EntityMeta> {
        let mut entities: Vec<hecs::Entity> = self.world.iter().map(|e| e.entity()).collect();
        entities.sort_by_key(|e| e.to_bits().get());

        let mut out = Vec::with_capacity(entities.len());
        for entity in entities {
            let parent = entity_parent_bits(&self.world, entity);

            out.push(EntityMeta {
                entity: entity.to_bits().get(),
                revision: self.changes.entity_revision(entity),
                parent,
            });
        }

        out
    }

    pub(crate) fn snapshot_entity_info(&self, entity: hecs::Entity) -> Result<EntityInfo, String> {
        let bits = entity.to_bits().get();
        let revision = self.changes.entity_revision(entity);
        let components = self.types.snapshot_entity_components(&self.world, entity)?;
        Ok(EntityInfo {
            entity: bits,
            revision,
            components,
        })
    }
}

#[derive(Default)]
pub struct Remotes {
    pub(crate) bridge: WorldBridge,
}

impl Remotes {
    pub fn new() -> Self {
        Self::default()
    }
}

pub struct Rpcs {
    pub(crate) registry: Option<RpcRegistryWithCtx<World>>,
}

impl Default for Rpcs {
    fn default() -> Self {
        Self::new()
    }
}

impl Rpcs {
    pub fn new() -> Self {
        Self {
            registry: Some(RpcRegistryWithCtx::new()),
        }
    }

    fn registry_mut(&mut self) -> &mut RpcRegistryWithCtx<World> {
        self.registry.as_mut().expect("rpcs taken")
    }
}

pub struct Sessions {
    port: Option<u16>,
    listener: Option<SessionListener>,
    sessions: Vec<Session>,
}

impl Default for Sessions {
    fn default() -> Self {
        Self::new()
    }
}

impl Sessions {
    pub fn new() -> Self {
        Self {
            port: None,
            listener: None,
            sessions: Vec::new(),
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    pub fn port(&self) -> Option<u16> {
        self.port
    }

    pub fn start(&mut self, process_name: &str) -> std::io::Result<()> {
        if let Some(port) = self.port {
            self.listener = Some(SessionListener::bind(port)?);
            log::info!("RPC process '{}' listening on port {}", process_name, port);
        } else {
            log::warn!("Can't start sessions: no port configured");
        }
        Ok(())
    }

    pub fn handle(&mut self, world: &mut World, rpcs: &mut Rpcs) {
        self.accept_sessions();
        self.read_sessions(world, rpcs);
    }

    fn handle_message(
        &mut self,
        rpcs: &mut Rpcs,
        world: &mut World,
        session: &mut Session,
        bytes: &[u8],
    ) {
        let env = match decode_envelope(bytes, DecodeConfig::default()) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Invalid IPC envelope: {e}");
                return;
            }
        };

        match env {
            awrk_datex_rpc::RpcEnvelopeRef::Invoke { id, proc_id, args } => {
                if let Some(cached) = session.cloned_cached_invoke_result(id) {
                    let _ = session.write(&cached);
                    return;
                }

                let registry = rpcs.registry.take().expect("rpcs taken");
                let result =
                    registry.handle_invoke(world, id, proc_id, args, EncodeConfig::default());
                rpcs.registry = Some(registry);

                match result {
                    Ok(buf) => {
                        session.cache_invoke_result(id, buf.clone());
                        let _ = session.write(&buf);
                    }
                    Err(e) => {
                        eprintln!("Error handling RPC invoke: {e}");
                    }
                }
            }
            awrk_datex_rpc::RpcEnvelopeRef::InvokeResult { .. } => {}
        }
    }

    fn accept_sessions(&mut self) {
        if let Some(listener) = &self.listener {
            loop {
                match listener.accept() {
                    Ok(stream) => {
                        self.sessions.push(Session::new(stream));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) => {
                        eprintln!("Error accepting RPC session: {}", e);
                        break;
                    }
                }
            }
        }
    }

    fn read_sessions(&mut self, world: &mut World, rpcs: &mut Rpcs) {
        let sessions = std::mem::take(&mut self.sessions);
        let mut kept = Vec::with_capacity(sessions.len());

        for mut session in sessions {
            let mut keep = true;
            let mut eof = false;

            match session.poll_read() {
                Ok(hit_eof) => eof = hit_eof,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::BrokenPipe
                            | std::io::ErrorKind::ConnectionAborted
                            | std::io::ErrorKind::ConnectionReset
                            | std::io::ErrorKind::NotConnected
                            | std::io::ErrorKind::UnexpectedEof
                    ) =>
                {
                    keep = false;
                }
                Err(e) => {
                    eprintln!("Error reading from RPC session: {}", e);
                    keep = false;
                }
            }

            if keep {
                loop {
                    match session.try_read_frame(WORLD_MAX_FRAME_SIZE) {
                        Ok(Some(frame)) => {
                            self.handle_message(rpcs, world, &mut session, &frame);
                        }
                        Ok(None) => break,
                        Err(e)
                            if matches!(
                                e.kind(),
                                std::io::ErrorKind::BrokenPipe
                                    | std::io::ErrorKind::ConnectionAborted
                                    | std::io::ErrorKind::ConnectionReset
                                    | std::io::ErrorKind::NotConnected
                                    | std::io::ErrorKind::UnexpectedEof
                            ) =>
                        {
                            keep = false;
                            break;
                        }
                        Err(e) => {
                            eprintln!("Error decoding RPC frame: {}", e);
                            keep = false;
                            break;
                        }
                    }
                }
            }

            if eof {
                keep = false;
            }

            if keep {
                let _ = session.flush_write();
                kept.push(session);
            }
        }

        self.sessions = kept;
    }
}

pub struct ProcessParts {
    pub name: String,
    pub world: World,
    pub remotes: Remotes,
    pub rpcs: Rpcs,
    pub sessions: Sessions,
}

impl Default for ProcessParts {
    fn default() -> Self {
        Self::new("")
    }
}

impl ProcessParts {
    pub fn new(process_name: impl Into<String>) -> Self {
        Self {
            name: process_name.into(),
            world: World::new(),
            remotes: Remotes::new(),
            rpcs: Rpcs::new(),
            sessions: Sessions::new(),
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.sessions = self.sessions.with_port(port);
        self
    }
}

pub struct Process {
    name: String,
    world: World,
    remotes: Remotes,
    rpcs: Rpcs,
    sessions: Sessions,
}

impl Process {
    pub fn new(process_name: impl Into<String>) -> Self {
        let mut process = Self {
            name: process_name.into(),
            world: World::new(),
            remotes: Remotes::new(),
            rpcs: Rpcs::new(),
            sessions: Sessions::new(),
        };

        process.register_builtin_types();
        register_builtin_rpcs(&mut process);
        crate::registration::register_discovered(&mut process);
        process
    }

    pub fn new_with_sessions(process_name: impl Into<String>, port: u16) -> Self {
        let mut process = Self::new(process_name);
        process.sessions.port = Some(port);
        process
    }

    pub fn from_args(process_name: impl Into<String>) -> Self {
        let args = WorldArgs::parse();
        Self::from_world_args(process_name, args)
    }

    pub fn from_world_args(process_name: impl Into<String>, args: WorldArgs) -> Self {
        match args.port {
            Some(port) => Self::new_with_sessions(process_name, port),
            None => Self::new(process_name),
        }
    }

    pub fn into_parts(self) -> ProcessParts {
        ProcessParts {
            name: self.name,
            world: self.world,
            remotes: self.remotes,
            rpcs: self.rpcs,
            sessions: self.sessions,
        }
    }

    pub(crate) fn register_builtin_types(&mut self) {
        let _ = self.register_component::<Name>();
        let _ = self.register_component::<Parent>();
        let _ = self.register_component::<WorldId>();
        let _ = self.register_type::<RemoteRef>();
        let _ = self.register_type::<ProxyLifecycle>();
        let _ = self.register_type::<ProxyAuthorityKind>();
        let _ = self.register_component::<ProxyEntity>();
        let _ = self.register_component::<ProxyState>();
        let _ = self.register_component::<ProxyAuthority>();
        let _ = self.register_component::<ProxySpawnRequest>();
        let _ = self.register_component::<ProxySpawnError>();
        let _ = self.register_component::<RemoteParentRef>();
    }

    pub fn tick(&mut self) -> Result<(), String> {
        self.sessions.handle(&mut self.world, &mut self.rpcs);
        self.remotes.tick_all(&mut self.world)
    }

    fn rpcs_mut(&mut self) -> &mut RpcRegistryWithCtx<World> {
        self.rpcs.registry_mut()
    }

    pub(crate) fn register_rpc_typed<A, R, F>(
        &mut self,
        name: &str,
        f: F,
    ) -> awrk_datex_rpc::RpcProcId
    where
        for<'a> A: awrk_datex::Decode<'a> + awrk_datex_schema::Schema,
        R: awrk_datex::Encode + awrk_datex_schema::Schema,
        F: Fn(&mut World, A) -> Result<R, String> + Send + Sync + 'static,
    {
        self.rpcs_mut().register_typed::<A, R, F>(name, f)
    }

    pub fn register_type<T>(&mut self) -> awrk_datex_schema::TypeId
    where
        T: awrk_datex_schema::Schema,
    {
        let type_name = core::any::type_name::<T>().to_string();
        self.world.types_mut().register_schema_root_named(type_name);
        self.rpcs_mut().register_type::<T>()
    }

    pub fn register_component<T>(&mut self) -> Result<(), String>
    where
        T: hecs::Component
            + awrk_datex::Encode
            + for<'a> awrk_datex::Decode<'a>
            + awrk_datex::Patch
            + awrk_datex::PatchValidate
            + awrk_datex_schema::Schema
            + Send
            + Sync
            + 'static,
    {
        let type_name = core::any::type_name::<T>().to_string();
        self.register_component_named::<T>(type_name)
    }

    pub fn register_component_named<T>(&mut self, type_name: String) -> Result<(), String>
    where
        T: hecs::Component
            + awrk_datex::Encode
            + for<'a> awrk_datex::Decode<'a>
            + awrk_datex::Patch
            + awrk_datex::PatchValidate
            + awrk_datex_schema::Schema
            + Send
            + Sync
            + 'static,
    {
        self.world
            .types_mut()
            .register_schema_root_named(type_name.clone());
        let _ = self.rpcs_mut().register_type::<T>();
        self.world
            .types_mut()
            .register_component_named::<T>(type_name)
    }

    pub fn register_component_opaque<T>(&mut self) -> Result<(), String>
    where
        T: hecs::Component + awrk_datex_schema::Schema + Send + Sync + 'static,
    {
        let type_name = core::any::type_name::<T>().to_string();
        self.register_component_opaque_named::<T>(type_name)
    }

    pub fn register_component_opaque_named<T>(&mut self, type_name: String) -> Result<(), String>
    where
        T: hecs::Component + awrk_datex_schema::Schema + Send + Sync + 'static,
    {
        self.world
            .types_mut()
            .register_schema_root_named(type_name.clone());
        let _ = self.rpcs_mut().register_type::<T>();
        self.world
            .types_mut()
            .register_component_base_named::<T>(type_name)
    }

    pub fn port(&self) -> Option<u16> {
        self.sessions.port()
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Remotes {
    pub fn add_remote(&mut self, config: WorldBridgeRemoteConfig) -> Result<(), String> {
        self.bridge.add_remote(config)
    }

    pub fn remove_remote(
        &mut self,
        world: &mut World,
        world_id: u64,
        despawn_local_proxies: bool,
    ) -> Result<(), String> {
        let mut bridge = std::mem::take(&mut self.bridge);
        let result = bridge.remove_remote(world, world_id, despawn_local_proxies);
        self.bridge = bridge;
        result
    }

    pub fn clear(&mut self, world: &mut World, despawn_local_proxies: bool) -> Result<(), String> {
        let mut bridge = std::mem::take(&mut self.bridge);
        let result = bridge.clear(world, despawn_local_proxies);
        self.bridge = bridge;
        result
    }

    pub fn tick_all(&mut self, world: &mut World) -> Result<(), String> {
        let mut bridge = std::mem::take(&mut self.bridge);
        let result = bridge.tick_all(world);
        self.bridge = bridge;
        result
    }

    pub fn local_entity_for_remote(&self, remote_ref: RemoteRef) -> Option<u64> {
        self.bridge.local_entity_for_remote(remote_ref)
    }

    pub fn remote_ref_for_local(&self, local_entity: u64) -> Option<RemoteRef> {
        self.bridge.remote_ref_for_local(local_entity)
    }
}
