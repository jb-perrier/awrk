extern crate awrk_datex_schema;

use awrk_macros::Type;
use awrk_world::Name;
use awrk_world::core::World;

pub const DEFAULT_WINDOW_WIDTH: u32 = 1280;
pub const DEFAULT_WINDOW_HEIGHT: u32 = 720;

#[Type]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowHandle {
    id: u64,
}

impl WindowHandle {
    pub const fn new(id: u64) -> Self {
        Self { id }
    }

    pub const fn id(&self) -> u64 {
        self.id
    }
}

#[Type]
#[derive(Clone, Debug)]
pub struct WinWindow;

#[Type]
#[derive(Clone, Debug)]
pub struct WinTitle(pub String);

#[Type]
#[derive(Clone, Debug)]
pub struct WinInnerSize {
    pub width: u32,
    pub height: u32,
}

impl WinInnerSize {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

#[Type]
#[derive(Clone, Debug)]
pub struct WinFocused(pub bool);

#[Type]
#[derive(Clone, Debug)]
pub enum WinStatus {
    Pending,
    Ready,
    CreateFailed { message: String },
}

#[Type]
#[derive(Clone, Debug, Default)]
pub struct WinWindowSpec {
    pub title: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

impl WinWindowSpec {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn with_size(mut self, width: u32, height: u32) -> Self {
        self.width = Some(width);
        self.height = Some(height);
        self
    }

    pub fn title_or_default(&self) -> String {
        self.title.clone().unwrap_or_else(|| "Window".to_string())
    }

    pub fn size_or_default(&self) -> WinInnerSize {
        WinInnerSize {
            width: self.width.unwrap_or(DEFAULT_WINDOW_WIDTH),
            height: self.height.unwrap_or(DEFAULT_WINDOW_HEIGHT),
        }
    }
}

#[Type]
#[derive(Clone, Debug)]
pub struct CreateWindowArgs {
    pub spec: WinWindowSpec,
}

#[Type]
#[derive(Clone, Debug)]
pub struct CreateWindowResult {
    pub handle: WindowHandle,
}

#[Type]
#[derive(Clone, Debug)]
pub struct CloseWindowArgs {
    pub handle: WindowHandle,
}

#[Type]
#[derive(Clone, Debug)]
pub struct WindowInfo {
    pub handle: WindowHandle,
    pub title: String,
    pub size: WinInnerSize,
    pub focused: bool,
    pub status: WinStatus,
}

#[Type]
#[derive(Clone, Debug, Default)]
pub struct ListWindowsResult {
    pub windows: Vec<WindowInfo>,
}

#[Type]
#[derive(Clone, Debug)]
pub struct PollWindowEventsArgs {
    pub since: u64,
    pub limit: Option<u32>,
}

#[Type]
#[derive(Clone, Debug)]
pub struct PollWindowEventsResult {
    pub now: u64,
    pub cursor: u64,
    pub has_more: bool,
    pub events: Vec<WinEvent>,
}

#[Type]
#[derive(Clone, Debug)]
pub struct WinEvent {
    pub seq: u64,
    pub event: WinEventKind,
}

#[Type]
#[derive(Clone, Debug)]
pub enum WinEventKind {
    Created {
        window: WindowInfo,
    },
    Resized {
        handle: WindowHandle,
        size: WinInnerSize,
    },
    Focused {
        handle: WindowHandle,
        focused: bool,
    },
    Closed {
        handle: WindowHandle,
    },
    CreateFailed {
        handle: WindowHandle,
        message: String,
    },
}

#[Type(Opaque)]
#[derive(Debug, Default)]
pub struct WinEventLog {
    next_seq: u64,
    events: Vec<WinEvent>,
}

pub mod rpc {
    use super::{
        CloseWindowArgs, CreateWindowArgs, CreateWindowResult, ListWindowsResult,
        PollWindowEventsArgs, PollWindowEventsResult,
    };
    use awrk_world::Rpc;

    pub const CREATE_WINDOW: Rpc<CreateWindowArgs, CreateWindowResult> =
        Rpc::new("win.create_window");
    pub const CLOSE_WINDOW: Rpc<CloseWindowArgs, ()> = Rpc::new("win.close_window");
    pub const LIST_WINDOWS: Rpc<(), ListWindowsResult> = Rpc::new("win.list_windows");
    pub const POLL_EVENTS: Rpc<PollWindowEventsArgs, PollWindowEventsResult> =
        Rpc::new("win.poll_events");
}

pub fn poll_window_events_since(
    world: &mut World,
    since: u64,
    limit: Option<u32>,
) -> Result<PollWindowEventsResult, String> {
    let log_entity = ensure_win_event_log(world);
    let log = world
        .component_mut::<WinEventLog>(log_entity)
        .ok_or_else(|| "missing window event log".to_string())?;
    let limit = limit.unwrap_or(256).min(10_000) as usize;
    let now = log.next_seq.saturating_sub(1);
    let start_index = log
        .events
        .iter()
        .position(|event| event.seq > since)
        .unwrap_or(log.events.len());
    let remaining = &log.events[start_index..];
    let has_more = remaining.len() > limit;
    let events = remaining.iter().take(limit).cloned().collect::<Vec<_>>();
    let cursor = events.last().map(|event| event.seq).unwrap_or(since);

    Ok(PollWindowEventsResult {
        now,
        cursor,
        has_more,
        events,
    })
}

pub fn record_window_event(world: &mut World, event: WinEventKind) -> Result<(), String> {
    let log_entity = ensure_win_event_log(world);
    let mut log = world
        .component_mut::<WinEventLog>(log_entity)
        .ok_or_else(|| "missing window event log".to_string())?;
    let seq = log.next_seq.saturating_add(1);

    log.next_seq = seq;
    log.events.push(WinEvent { seq, event });
    Ok(())
}

pub fn window_info(world: &mut World, handle: WindowHandle) -> Result<WindowInfo, String> {
    list_window_infos(world)
        .into_iter()
        .find(|window| window.handle == handle)
        .ok_or_else(|| format!("unknown window handle: {}", handle.id()))
}

pub fn list_window_infos(world: &mut World) -> Vec<WindowInfo> {
    let mut windows = Vec::new();

    world.iter::<(
        &WinWindow,
        &WindowHandle,
        Option<&WinTitle>,
        Option<&Name>,
        Option<&WinInnerSize>,
        Option<&WinFocused>,
        Option<&WinStatus>,
    ), _>(|_, (_, handle, title, name, size, focused, status)| {
        let title = title
            .map(|value| value.0.clone())
            .or_else(|| name.map(|value| value.0.clone()))
            .unwrap_or_else(|| "Window".to_string());
        let size = size
            .cloned()
            .unwrap_or_else(|| WinInnerSize::new(DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT));
        let focused = focused.map(|value| value.0).unwrap_or(false);
        let status = status.cloned().unwrap_or(WinStatus::Pending);

        windows.push(WindowInfo {
            handle: *handle,
            title,
            size,
            focused,
            status,
        });
    });

    windows
}

fn ensure_win_event_log(world: &mut World) -> u64 {
    if let Some(entity) = find_win_event_log_entity(world) {
        entity
    } else {
        world.spawn((WinEventLog::default(),))
    }
}

fn find_win_event_log_entity(world: &mut World) -> Option<u64> {
    let mut entity = None;
    world.iter::<&WinEventLog, _>(|current, _| {
        if entity.is_none() {
            entity = Some(current);
        }
    });
    entity
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_window_infos_reads_shared_window_state() {
        let mut world = World::new();
        let entity = world.spawn((
            WinWindow,
            Name("Smoke".to_string()),
            WinTitle("Smoke".to_string()),
            WinInnerSize::new(640, 480),
            WinFocused(false),
            WinStatus::Pending,
        ));
        world
            .entity_mut(entity)
            .expect("window entity")
            .insert_one(WindowHandle::new(entity))
            .expect("insert window handle");

        let listed = list_window_infos(&mut world);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].handle, WindowHandle::new(entity));
        assert_eq!(listed[0].title, "Smoke");
        assert_eq!(listed[0].size.width, 640);
        assert_eq!(listed[0].size.height, 480);
    }

    #[test]
    fn poll_window_events_since_returns_events_after_cursor() {
        let mut world = World::new();
        let entity = world.spawn((
            WinWindow,
            Name("Smoke".to_string()),
            WinTitle("Smoke".to_string()),
            WinInnerSize::new(DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT),
            WinFocused(false),
            WinStatus::Pending,
        ));
        let handle = WindowHandle::new(entity);
        world
            .entity_mut(entity)
            .expect("window entity")
            .insert_one(handle)
            .expect("insert window handle");

        let info = window_info(&mut world, handle).expect("window info");
        record_window_event(
            &mut world,
            WinEventKind::Created {
                window: info.clone(),
            },
        )
        .expect("record created event");
        record_window_event(
            &mut world,
            WinEventKind::Focused {
                handle,
                focused: true,
            },
        )
        .expect("record focused event");

        let result = poll_window_events_since(&mut world, 0, Some(1)).expect("poll events");

        assert_eq!(result.now, 1);
        assert_eq!(result.cursor, 1);
        assert!(result.has_more);
        assert_eq!(result.events.len(), 1);
        assert!(matches!(
            result.events[0].event,
            WinEventKind::Created { .. }
        ));
    }
}
