#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use awrk_example_model::{
    ReferenceEntity, ReferenceHealth, ReferenceKind, ReferencePosition, ReferenceVelocity,
};
use awrk_world::core::{Process, ProcessParts, Rpcs, Sessions, World};
use awrk_world_ecs::{Name, Parent};
use std::collections::VecDeque;
use std::thread;
use std::time::Duration;

const TICK_SLEEP: Duration = Duration::from_millis(1);
const LOG_EVERY_TICKS: u64 = 4;
const MAX_PULSES: usize = 2;

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let ProcessParts {
        name,
        mut world,
        mut rpcs,
        mut sessions,
        ..
    } = Process::from_args("example-server").into_parts();
    sessions.start(&name)?;
    run(&mut world, &mut rpcs, &mut sessions);
    Ok(())
}

fn run(world: &mut World, rpcs: &mut Rpcs, sessions: &mut Sessions) {
    let mut app = ExampleServerApp::new(world);

    loop {
        sessions.handle(world, rpcs);
        app.tick(world);
        thread::sleep(TICK_SLEEP);
    }
}

struct ExampleServerApp {
    actors_root: u64,
    tick_counter: u64,
    next_pulse_index: u64,
    pulse_entities: VecDeque<u64>,
    last_snapshot: Vec<String>,
}

impl ExampleServerApp {
    fn new(world: &mut World) -> Self {
        let scene_root = world.spawn((
            ReferenceEntity,
            Name("Reference Scene".to_string()),
            ReferenceKind("SceneRoot".to_string()),
            ReferencePosition::new(0.0, 0.0),
        ));

        let actors_root = world.spawn((
            ReferenceEntity,
            Name("Actors".to_string()),
            ReferenceKind("Collection".to_string()),
            Parent { parent: scene_root },
            ReferencePosition::new(0.0, 0.0),
        ));

        spawn_actor(
            world,
            actors_root,
            "Scout",
            ReferencePosition::new(-6.0, 1.0),
            ReferenceVelocity::new(0.75, 0.15),
            ReferenceHealth::new(8, 10),
        );
        spawn_actor(
            world,
            actors_root,
            "Tank",
            ReferencePosition::new(2.0, -1.5),
            ReferenceVelocity::new(0.2, 0.05),
            ReferenceHealth::new(20, 20),
        );
        spawn_actor(
            world,
            actors_root,
            "Support",
            ReferencePosition::new(5.0, 2.0),
            ReferenceVelocity::new(-0.45, -0.1),
            ReferenceHealth::new(12, 12),
        );

        log::info!(
            "example-server bootstrapped a sample world with entity/component data ready for mirroring"
        );

        Self {
            actors_root,
            tick_counter: 0,
            next_pulse_index: 0,
            pulse_entities: VecDeque::new(),
            last_snapshot: Vec::new(),
        }
    }

    fn tick(&mut self, world: &mut World) {
        self.tick_counter = self.tick_counter.saturating_add(1);
        self.advance_entities(world);

        if self.tick_counter.is_multiple_of(8) {
            self.spawn_pulse(world);
        }

        if self.pulse_entities.len() > MAX_PULSES
            && let Some(entity) = self.pulse_entities.pop_front()
        {
            let _ = world.despawn(entity);
        }

        if self.tick_counter.is_multiple_of(LOG_EVERY_TICKS) {
            self.log_snapshot(world);
        }
    }

    fn advance_entities(&mut self, world: &mut World) {
        let wave = (self.tick_counter % 6) as f32 - 3.0;

        world.iter::<(
            &ReferenceEntity,
            Option<&ReferenceKind>,
            Option<&mut ReferencePosition>,
            Option<&mut ReferenceVelocity>,
            Option<&mut ReferenceHealth>,
        ), _>(|_, (_, kind, position, velocity, health)| {
            if let (Some(position), Some(velocity)) = (position, velocity) {
                position.x += velocity.dx;
                position.y += velocity.dy;

                if position.x.abs() > 8.0 {
                    velocity.dx = -velocity.dx;
                    position.x += velocity.dx;
                }

                if position.y.abs() > 4.0 {
                    velocity.dy = -velocity.dy;
                    position.y += velocity.dy;
                }
            }

            if let (Some(kind), Some(health)) = (kind, health) {
                if kind.0 == "Pulse" {
                    health.current = ((wave + 4.0) as u32).min(health.max).max(1);
                } else if self.tick_counter.is_multiple_of(6) {
                    health.current = if health.current <= 1 {
                        health.max
                    } else {
                        health.current - 1
                    };
                }
            }
        });
    }

    fn spawn_pulse(&mut self, world: &mut World) {
        self.next_pulse_index = self.next_pulse_index.saturating_add(1);
        let offset = self.next_pulse_index as f32;

        let entity = world.spawn((
            ReferenceEntity,
            Name(format!("Pulse {}", self.next_pulse_index)),
            ReferenceKind("Pulse".to_string()),
            Parent {
                parent: self.actors_root,
            },
            ReferencePosition::new(-4.0 + offset, 2.5 - (offset * 0.3)),
            ReferenceVelocity::new(0.3, -0.2),
            ReferenceHealth::new(4, 4),
        ));

        self.pulse_entities.push_back(entity);
    }

    fn log_snapshot(&mut self, world: &mut World) {
        let snapshot = collect_example_snapshot(world);
        if snapshot != self.last_snapshot {
            log::info!("example-server world snapshot:");
            for line in &snapshot {
                log::info!("  {}", line);
            }
            self.last_snapshot = snapshot;
        }
    }
}

fn spawn_actor(
    world: &mut World,
    parent: u64,
    name: &str,
    position: ReferencePosition,
    velocity: ReferenceVelocity,
    health: ReferenceHealth,
) {
    let _ = world.spawn((
        ReferenceEntity,
        Name(name.to_string()),
        ReferenceKind("Actor".to_string()),
        Parent { parent },
        position,
        velocity,
        health,
    ));
}

fn collect_example_snapshot(world: &mut World) -> Vec<String> {
    let mut snapshot = Vec::new();

    world.iter::<(
        &ReferenceEntity,
        Option<&Name>,
        Option<&ReferenceKind>,
        Option<&Parent>,
        Option<&ReferencePosition>,
        Option<&ReferenceVelocity>,
        Option<&ReferenceHealth>,
    ), _>(
        |entity, (_, name, kind, parent, position, velocity, health)| {
            let label = name
                .map(|value| value.0.clone())
                .unwrap_or_else(|| format!("entity-{entity}"));
            let kind = kind
                .map(|value| value.0.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            let parent = parent
                .map(|value| value.parent.to_string())
                .unwrap_or_else(|| "-".to_string());
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
                "entity={} name={label:?} kind={kind} parent={} pos={} vel={} health={}",
                entity, parent, position, velocity, health
            ));
        },
    );

    snapshot.sort();
    snapshot
}
