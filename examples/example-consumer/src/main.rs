#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use awrk_example::{
    CreateActorArgs, DEFAULT_EXAMPLE_PROCESS_HOST, DEFAULT_EXAMPLE_PROCESS_PORT, ReferenceHealth,
    ReferencePosition, ReferenceVelocity, SetActorVelocityArgs, rpc as example_rpc,
};
use awrk_world::{ProcessClient, ProcessClientOptions, core::Process};
use std::thread;
use std::time::Duration;

const TICK_SLEEP: Duration = Duration::from_millis(10);

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let mut process = Process::new("example-consumer");
    run(&mut process)?;
    Ok(())
}

fn run(process: &mut Process) -> Result<(), String> {
    let host = std::env::var("AWRK_EXAMPLE_PROCESS_HOST")
        .unwrap_or_else(|_| DEFAULT_EXAMPLE_PROCESS_HOST.to_string());
    let port = std::env::var("AWRK_EXAMPLE_PROCESS_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(DEFAULT_EXAMPLE_PROCESS_PORT);

    let client = ProcessClient::connect(&host, port, ProcessClientOptions::default())?;
    process.resources_mut().insert(client);
    process.resources_mut().insert(ExampleConsumerState::new());

    loop {
        tick_example_consumer(process)?;
        thread::sleep(TICK_SLEEP);
    }
}

struct ExampleConsumerState {
    created_actor: bool,
    velocity_updated: bool,
    last_snapshot: Vec<String>,
}

impl ExampleConsumerState {
    fn new() -> Self {
        Self {
            created_actor: false,
            velocity_updated: false,
            last_snapshot: Vec::new(),
        }
    }
}

fn tick_example_consumer(process: &mut Process) -> Result<(), String> {
    let created_actor = process.resource::<ExampleConsumerState>()?.created_actor;
    if !created_actor {
        let created = process.resource_mut::<ProcessClient>()?.invoke(
            example_rpc::CREATE_ACTOR,
            CreateActorArgs {
                name: "Requested Actor".to_string(),
                kind: "RequestedActor".to_string(),
                position: ReferencePosition::new(3.5, -2.0),
                velocity: Some(ReferenceVelocity::new(-0.35, 0.25)),
                health: Some(ReferenceHealth::new(6, 6)),
            },
        );

        match created {
            Ok(result) => {
                log::info!(
                    "example-consumer created remote actor {} named {:?}",
                    result.actor.id.0,
                    result.actor.name
                );
                process
                    .resource_mut::<ExampleConsumerState>()?
                    .created_actor = true;
            }
            Err(error) if error.is_transport() => {
                log::debug!("example-consumer is waiting for example-process: {}", error);
                return Ok(());
            }
            Err(error) => return Err(error.to_string()),
        }
    }

    let snapshot = process
        .resource_mut::<ProcessClient>()?
        .invoke(example_rpc::LIST_ACTORS, ());
    let snapshot = match snapshot {
        Ok(result) => result.actors,
        Err(error) if error.is_transport() => {
            log::debug!("example-consumer lost server connection: {}", error);
            return Ok(());
        }
        Err(error) => return Err(error.to_string()),
    };

    let should_update_velocity = {
        let state = process.resource::<ExampleConsumerState>()?;
        state.created_actor && !state.velocity_updated
    };
    if should_update_velocity
        && let Some(actor) = snapshot.iter().find(|actor| actor.kind == "RequestedActor")
    {
        process
            .resource_mut::<ProcessClient>()?
            .invoke(
                example_rpc::SET_ACTOR_VELOCITY,
                SetActorVelocityArgs {
                    actor: actor.id,
                    velocity: ReferenceVelocity::new(0.5, 0.0),
                },
            )
            .map_err(|error| error.to_string())?;
        log::info!(
            "example-consumer updated actor {} velocity through RPC",
            actor.id.0
        );
        process
            .resource_mut::<ExampleConsumerState>()?
            .velocity_updated = true;
    }

    let snapshot = snapshot
        .into_iter()
        .map(|actor| {
            format!(
                "actor={} name={:?} kind={} parent={} pos=({:.1}, {:.1}) vel={} health={}",
                actor.id.0,
                actor.name,
                actor.kind,
                actor
                    .parent
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                actor.position.x,
                actor.position.y,
                actor
                    .velocity
                    .map(|value| format!("({:.2}, {:.2})", value.dx, value.dy))
                    .unwrap_or_else(|| "n/a".to_string()),
                actor
                    .health
                    .map(|value| format!("{}/{}", value.current, value.max))
                    .unwrap_or_else(|| "n/a".to_string())
            )
        })
        .collect::<Vec<_>>();

    let snapshot_changed = {
        let state = process.resource::<ExampleConsumerState>()?;
        snapshot != state.last_snapshot
    };
    if snapshot_changed {
        log::info!("example-consumer actor snapshot:");
        for line in &snapshot {
            log::info!("  {}", line);
        }
        process
            .resource_mut::<ExampleConsumerState>()?
            .last_snapshot = snapshot;
    }

    Ok(())
}
