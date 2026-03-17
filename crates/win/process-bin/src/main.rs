#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::collections::HashMap;

use awrk_win::{
    WinEventKind, WinFocused, WinInnerSize, WinStatus, WinTitle, WinWindow, WindowHandle,
    record_window_event, window_info,
};
use awrk_world::core::{Process, ProcessParts, Rpcs, Sessions, World};
use awrk_world_ecs::Name;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let mut process = Process::from_args("win-process");
    awrk_win_process::rpc::register(&mut process);
    let ProcessParts {
        name,
        mut world,
        rpcs,
        mut sessions,
        ..
    } = process.into_parts();
    init(&mut world);
    sessions.start(&name)?;
    run(world, rpcs, sessions)?;
    Ok(())
}

fn init(world: &mut World) {
    let _ = world.spawn((Name("Root".to_string()),));
}

fn run(world: World, rpcs: Rpcs, sessions: Sessions) -> Result<(), winit::error::EventLoopError> {
    let event_loop = EventLoop::new().expect("create event loop");
    let mut app = App::new(world, rpcs, sessions);

    event_loop.run_app(&mut app)
}

struct App {
    world: World,
    rpcs: Rpcs,
    sessions: Sessions,
    windows_by_entity: HashMap<u64, Window>,
    entity_by_window: HashMap<WindowId, u64>,
}

impl App {
    fn new(world: World, rpcs: Rpcs, sessions: Sessions) -> Self {
        Self {
            world,
            rpcs,
            sessions,
            windows_by_entity: HashMap::new(),
            entity_by_window: HashMap::new(),
        }
    }

    fn tick(&mut self, elwt: &winit::event_loop::ActiveEventLoop) {
        let desired = collect_desired_windows(&mut self.world);
        let desired_ids: std::collections::BTreeSet<u64> =
            desired.iter().map(|window| window.entity).collect();
        let stale: Vec<u64> = self
            .windows_by_entity
            .keys()
            .copied()
            .filter(|entity| !desired_ids.contains(entity))
            .collect();

        for entity in stale {
            if let Some(window) = self.windows_by_entity.remove(&entity) {
                self.entity_by_window.remove(&window.id());
                let _ = record_window_event(
                    &mut self.world,
                    WinEventKind::Closed {
                        handle: WindowHandle::new(entity),
                    },
                );
            }
        }

        for window in desired {
            if let Some(native) = self.windows_by_entity.get(&window.entity) {
                native.set_title(&window.title);
                let current = native.inner_size();
                if current.width != window.size.width || current.height != window.size.height {
                    let _ = native.request_inner_size(PhysicalSize::new(
                        window.size.width,
                        window.size.height,
                    ));
                }

                if !matches!(window.status, Some(WinStatus::Ready)) {
                    let _ = set_window_status(&mut self.world, window.entity, WinStatus::Ready);
                }
                continue;
            }

            if matches!(window.status, Some(WinStatus::CreateFailed { .. })) {
                continue;
            }

            let attrs = Window::default_attributes()
                .with_title(window.title)
                .with_inner_size(PhysicalSize::new(window.size.width, window.size.height));

            match elwt.create_window(attrs) {
                Ok(native) => {
                    let id = native.id();
                    self.entity_by_window.insert(id, window.entity);
                    self.windows_by_entity.insert(window.entity, native);
                    let _ = set_window_status(&mut self.world, window.entity, WinStatus::Ready);
                    let handle = WindowHandle::new(window.entity);
                    if let Ok(info) = window_info(&mut self.world, handle) {
                        let _ = record_window_event(
                            &mut self.world,
                            WinEventKind::Created { window: info },
                        );
                    }
                }
                Err(error) => {
                    let _ = set_window_status(
                        &mut self.world,
                        window.entity,
                        WinStatus::CreateFailed {
                            message: error.to_string(),
                        },
                    );
                    let _ = record_window_event(
                        &mut self.world,
                        WinEventKind::CreateFailed {
                            handle: WindowHandle::new(window.entity),
                            message: error.to_string(),
                        },
                    );
                }
            }
        }
    }

    fn on_window_event(&mut self, window_id: WindowId, event: WindowEvent) {
        let Some(&entity) = self.entity_by_window.get(&window_id) else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => {
                let _ = record_window_event(
                    &mut self.world,
                    WinEventKind::Closed {
                        handle: WindowHandle::new(entity),
                    },
                );
                self.entity_by_window.remove(&window_id);
                self.windows_by_entity.remove(&entity);
                let _ = self.world.despawn(entity);
            }
            WindowEvent::Resized(size) => {
                let inner_size = WinInnerSize::new(size.width, size.height);
                if let Ok(mut e) = self.world.entity_mut(entity) {
                    let _ = e.insert_one(inner_size.clone());
                }
                let _ = record_window_event(
                    &mut self.world,
                    WinEventKind::Resized {
                        handle: WindowHandle::new(entity),
                        size: inner_size,
                    },
                );
            }
            WindowEvent::Focused(focused) => {
                if let Ok(mut e) = self.world.entity_mut(entity) {
                    let _ = e.insert_one(WinFocused(focused));
                }
                let _ = record_window_event(
                    &mut self.world,
                    WinEventKind::Focused {
                        handle: WindowHandle::new(entity),
                        focused,
                    },
                );
            }
            _ => {}
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Poll);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Poll);
        self.sessions.handle(&mut self.world, &mut self.rpcs);
        self.tick(event_loop);
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        self.on_window_event(window_id, event);
    }
}

#[derive(Clone)]
struct DesiredWindow {
    entity: u64,
    title: String,
    size: WinInnerSize,
    status: Option<WinStatus>,
}

fn collect_desired_windows(world: &mut World) -> Vec<DesiredWindow> {
    let mut windows = Vec::new();

    world.iter::<(
        &WinWindow,
        Option<&Name>,
        Option<&WinTitle>,
        Option<&WinInnerSize>,
        Option<&WinStatus>,
    ), _>(|entity, (_, name, title, size, status)| {
        let title = title
            .map(|value| value.0.clone())
            .or_else(|| name.map(|value| value.0.clone()))
            .unwrap_or_else(|| "Window".to_string());
        let size = size
            .cloned()
            .unwrap_or_else(|| WinInnerSize::new(1280, 720));
        windows.push(DesiredWindow {
            entity,
            title,
            size,
            status: status.cloned(),
        });
    });

    windows
}

fn set_window_status(world: &mut World, entity: u64, status: WinStatus) -> Result<(), String> {
    let mut entity_mut = world.entity_mut(entity)?;
    entity_mut.insert_one(status)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_desired_windows_tracks_state_driven_create_and_teardown() {
        let mut world = World::new();
        let entity = world.spawn((
            WinWindow,
            Name("Smoke Window".to_string()),
            WinTitle("Smoke".to_string()),
            WinInnerSize::new(640, 480),
        ));

        let desired = collect_desired_windows(&mut world);
        assert_eq!(desired.len(), 1);
        assert_eq!(desired[0].entity, entity);
        assert_eq!(desired[0].title, "Smoke");
        assert_eq!(desired[0].size.width, 640);
        assert_eq!(desired[0].size.height, 480);

        world
            .despawn(entity)
            .expect("despawn desired window entity");
        assert!(collect_desired_windows(&mut world).is_empty());
    }

    #[test]
    fn set_window_status_updates_entity_state() {
        let mut world = World::new();
        let entity = world.spawn((WinWindow, Name("Status Window".to_string())));

        set_window_status(&mut world, entity, WinStatus::Ready).expect("set window status");

        assert!(matches!(
            world.component::<WinStatus>(entity).as_deref(),
            Some(WinStatus::Ready)
        ));
    }
}
