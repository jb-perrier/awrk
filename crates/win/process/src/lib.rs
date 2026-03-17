use awrk_win::{
    CloseWindowArgs, CreateWindowArgs, CreateWindowResult, ListWindowsResult, PollWindowEventsArgs,
    PollWindowEventsResult, WinFocused, WinStatus, WinTitle, WinWindow, WindowHandle,
    list_window_infos, poll_window_events_since,
};
use awrk_world::Name;
use awrk_world::core::World;

pub mod rpc {
    use awrk_world::Process;

    use super::{close_window, create_window, list_windows, poll_events};

    pub fn register(process: &mut Process) {
        process.register_rpc(awrk_win::rpc::CREATE_WINDOW, create_window);
        process.register_rpc(awrk_win::rpc::CLOSE_WINDOW, close_window);
        process.register_rpc(awrk_win::rpc::LIST_WINDOWS, |world, ()| list_windows(world));
        process.register_rpc(awrk_win::rpc::POLL_EVENTS, poll_events);
    }
}

fn create_window(world: &mut World, args: CreateWindowArgs) -> Result<CreateWindowResult, String> {
    let title = args.spec.title_or_default();
    let size = args.spec.size_or_default();
    let entity = world.spawn((
        WinWindow,
        Name(title.clone()),
        WinTitle(title),
        size,
        WinFocused(false),
        WinStatus::Pending,
    ));

    let handle = WindowHandle::new(entity);
    world.entity_mut(entity)?.insert_one(handle)?;
    Ok(CreateWindowResult { handle })
}

fn close_window(world: &mut World, args: CloseWindowArgs) -> Result<(), String> {
    world.despawn(args.handle.id())
}

fn list_windows(world: &mut World) -> Result<ListWindowsResult, String> {
    let mut windows = list_window_infos(world);
    windows.sort_by_key(|window| window.handle.id());
    Ok(ListWindowsResult { windows })
}

fn poll_events(
    world: &mut World,
    args: PollWindowEventsArgs,
) -> Result<PollWindowEventsResult, String> {
    poll_window_events_since(world, args.since, args.limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use awrk_win::{WinEventKind, WinWindowSpec, record_window_event, window_info};

    #[test]
    fn create_list_and_close_window_rpc_flow_uses_explicit_domain_types() {
        let mut world = World::new();

        let created = create_window(
            &mut world,
            CreateWindowArgs {
                spec: WinWindowSpec::new().with_title("Smoke").with_size(640, 480),
            },
        )
        .expect("create window");

        let listed = list_windows(&mut world).expect("list windows");
        assert_eq!(listed.windows.len(), 1);
        assert_eq!(listed.windows[0].handle, created.handle);
        assert_eq!(listed.windows[0].title, "Smoke");
        assert_eq!(listed.windows[0].size.width, 640);
        assert_eq!(listed.windows[0].size.height, 480);

        close_window(
            &mut world,
            CloseWindowArgs {
                handle: created.handle,
            },
        )
        .expect("close window");

        assert!(
            list_windows(&mut world)
                .expect("list windows after close")
                .windows
                .is_empty()
        );
    }

    #[test]
    fn poll_events_returns_events_after_cursor() {
        let mut world = World::new();
        let created = create_window(
            &mut world,
            CreateWindowArgs {
                spec: WinWindowSpec::new().with_title("Smoke"),
            },
        )
        .expect("create window");

        let info = window_info(&mut world, created.handle).expect("window info");
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
                handle: created.handle,
                focused: true,
            },
        )
        .expect("record focused event");

        let result = poll_events(
            &mut world,
            PollWindowEventsArgs {
                since: 0,
                limit: Some(1),
            },
        )
        .expect("poll events");

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