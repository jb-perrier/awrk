#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use awrk_example_model::{
    DEFAULT_EXAMPLE_SERVER_HOST, DEFAULT_EXAMPLE_SERVER_PORT, DEFAULT_EXAMPLE_WORLD_ID,
    ReferenceEntity, ReferenceHealth, ReferenceKind, ReferencePosition, ReferenceVelocity,
};
use awrk_world::core::{Process, ProcessParts, Remotes, Rpcs, Sessions, World};
use awrk_world::{
    ProxyAuthority, ProxyAuthorityKind, ProxyEntity, ProxyLifecycle, ProxySpawnError,
    ProxySpawnRequest, ProxyState, WorldBridgeRemoteConfig, WorldId,
};
use awrk_world_ecs::{Name, Parent};
use std::thread;
use std::time::Duration;

const TICK_SLEEP: Duration = Duration::from_millis(250);
const REMOVE_AFTER_LIVE_TICKS: u32 = 8;

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let ProcessParts {
        name,
        mut world,
        mut remotes,
        mut rpcs,
        mut sessions,
    } = Process::from_args("example-consumer").into_parts();
    sessions.start(&name)?;
    run(&mut world, &mut remotes, &mut rpcs, &mut sessions);
    Ok(())
}

fn run(world: &mut World, remotes: &mut Remotes, rpcs: &mut Rpcs, sessions: &mut Sessions) {
    let host = std::env::var("AWRK_EXAMPLE_SERVER_HOST")
        .unwrap_or_else(|_| DEFAULT_EXAMPLE_SERVER_HOST.to_string());
    let port = std::env::var("AWRK_EXAMPLE_SERVER_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(DEFAULT_EXAMPLE_SERVER_PORT);
    let world_id = std::env::var("AWRK_EXAMPLE_WORLD_ID")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_EXAMPLE_WORLD_ID);

    let mut app = ExampleConsumer::new(host, port, world_id);

    loop {
        app.ensure_remote(remotes);
        sessions.handle(world, rpcs);
        if let Err(err) = remotes.tick_all(world) {
            log::warn!("example-consumer tick failed: {}", err);
        }
        app.tick(world);
        thread::sleep(TICK_SLEEP);
    }
}

struct ExampleConsumer {
    remote: WorldBridgeRemoteConfig,
    remote_registered: bool,
    demo_entity: Option<u64>,
    demo_finished: bool,
    live_ticks: u32,
    last_lifecycle: Option<ProxyLifecycle>,
    last_remote_entity: Option<u64>,
    last_spawn_error: Option<String>,
    last_snapshot: Vec<String>,
}

impl ExampleConsumer {
    fn new(host: String, port: u16, world_id: u64) -> Self {
        Self {
            remote: WorldBridgeRemoteConfig::new(world_id, host, port),
            remote_registered: false,
            demo_entity: None,
            demo_finished: false,
            live_ticks: 0,
            last_lifecycle: None,
            last_remote_entity: None,
            last_spawn_error: None,
            last_snapshot: Vec::new(),
        }
    }

    fn ensure_remote(&mut self, remotes: &mut Remotes) {
        if self.remote_registered {
            return;
        }

        match remotes.add_remote(self.remote.clone()) {
            Ok(()) => {
                log::info!(
                    "example-consumer connected remote to {}:{} (world {})",
                    self.remote.host,
                    self.remote.port,
                    self.remote.world_id
                );
                self.remote_registered = true;
            }
            Err(err) => {
                log::debug!(
                    "example-consumer is waiting for example-server at {}:{}: {}",
                    self.remote.host,
                    self.remote.port,
                    err
                );
            }
        }
    }

    fn tick(&mut self, world: &mut World) {
        self.drive_demo_proxy(world);
        self.log_proxy_snapshot(world);
    }

    fn drive_demo_proxy(&mut self, world: &mut World) {
        if !self.remote_registered || self.demo_finished {
            return;
        }

        let Some(entity) = self.ensure_demo_entity(world) else {
            return;
        };

        if !world.contains_entity(entity) {
            self.demo_entity = None;
            self.demo_finished = true;
            self.live_ticks = 0;
            self.last_lifecycle = None;
            self.last_remote_entity = None;
            self.last_spawn_error = None;
            log::info!(
                "example-consumer removed the local proxy entity; the bridge will propagate remote despawn"
            );
            return;
        }

        if let Some(error) = world
            .component::<ProxySpawnError>(entity)
            .map(|value| value.message.clone())
        {
            if self.last_spawn_error.as_deref() != Some(error.as_str()) {
                log::warn!(
                    "example-consumer outbound proxy creation failed for entity {}: {}",
                    entity,
                    error
                );
                self.last_spawn_error = Some(error);
            }
            return;
        }

        if let Some(proxy) = world.component::<ProxyEntity>(entity)
            && self.last_remote_entity != Some(proxy.remote.entity)
        {
            log::info!(
                "example-consumer materialized local proxy entity {} as remote {}:{}",
                entity,
                proxy.remote.world_id,
                proxy.remote.entity
            );
            self.last_remote_entity = Some(proxy.remote.entity);
        }

        let mut should_remove = false;
        if let Some(state) = world.component::<ProxyState>(entity) {
            if self.last_lifecycle != Some(state.lifecycle) {
                log::info!(
                    "example-consumer proxy entity {} entered lifecycle {:?}",
                    entity,
                    state.lifecycle
                );
                self.last_lifecycle = Some(state.lifecycle);
            }

            if state.lifecycle == ProxyLifecycle::Live {
                self.live_ticks = self.live_ticks.saturating_add(1);
                should_remove = self.live_ticks >= REMOVE_AFTER_LIVE_TICKS;
            } else {
                self.live_ticks = 0;
            }
        }

        if should_remove {
            log::info!(
                "example-consumer is removing local proxy entity {} to demonstrate outbound remote despawn",
                entity
            );
            let _ = world.despawn(entity);
        }
    }

    fn ensure_demo_entity(&mut self, world: &mut World) -> Option<u64> {
        if let Some(entity) = self.demo_entity {
            return Some(entity);
        }

        let entity = world.spawn((
            WorldId(self.remote.world_id),
            ProxyAuthority {
                authority: ProxyAuthorityKind::RequestDriven,
            },
            ProxySpawnRequest,
            ReferenceEntity,
            Name("Requested Actor".to_string()),
            ReferenceKind("RequestedActor".to_string()),
            ReferencePosition::new(3.5, -2.0),
            ReferenceVelocity::new(-0.35, 0.25),
            ReferenceHealth::new(6, 6),
        ));

        log::info!(
            "example-consumer created local proxy intent entity {} targeting remote world {}",
            entity,
            self.remote.world_id
        );

        self.demo_entity = Some(entity);
        Some(entity)
    }

    fn log_proxy_snapshot(&mut self, world: &mut World) {
        let snapshot = collect_proxy_snapshot(world);
        if snapshot != self.last_snapshot {
            if snapshot.is_empty() {
                log::info!("example-consumer has not mirrored any proxy entities yet");
            } else {
                log::info!("example-consumer mirrored proxy snapshot:");
                for line in &snapshot {
                    log::info!("  {}", line);
                }
            }
            self.last_snapshot = snapshot;
        }
    }
}

fn collect_proxy_snapshot(world: &mut World) -> Vec<String> {
    let mut snapshot = Vec::new();

    world.iter::<(
        &ProxyEntity,
        &ReferenceEntity,
        Option<&ProxyState>,
        Option<&Name>,
        Option<&ReferenceKind>,
        Option<&Parent>,
        Option<&ReferencePosition>,
        Option<&ReferenceVelocity>,
        Option<&ReferenceHealth>,
    ), _>(|entity, (proxy, _, state, name, kind, parent, position, velocity, health)| {
        let label = name
            .map(|value| value.0.clone())
            .unwrap_or_else(|| format!("entity-{entity}"));
        let kind = kind
            .map(|value| value.0.clone())
            .unwrap_or_else(|| "Unknown".to_string());
        let parent = parent
            .map(|value| value.parent.to_string())
            .unwrap_or_else(|| "-".to_string());
        let lifecycle = state
            .map(|value| format!("{:?}@{}", value.lifecycle, value.last_remote_revision))
            .unwrap_or_else(|| "Unknown".to_string());
        let position = position
            .map(|value| format!("({:.1}, {:.1})", value.x, value.y))
            .unwrap_or_else(|| "n/a".to_string());
        let velocity = velocity
            .map(|value| format!("({:.2}, {:.2})", value.dx, value.dy))
            .unwrap_or_else(|| "n/a".to_string());
        let health = health
            .map(|value| format!("{}/{}", value.current, value.max))
            .unwrap_or_else(|| "n/a".to_string());

        snapshot.push(format!(
            "local={} remote={}:{} name={label:?} kind={} parent={} lifecycle={} pos={} vel={} health={}",
            entity,
            proxy.remote.world_id,
            proxy.remote.entity,
            kind,
            parent,
            lifecycle,
            position,
            velocity,
            health
        ));
    });

    snapshot.sort();
    snapshot
}
