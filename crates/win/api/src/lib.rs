extern crate awrk_datex_schema;

use awrk_macros::Type;

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

awrk_world::register_proxy_subscription! {
    all_of: [WinWindow],
    any_of: [],
    none_of: [],
    components: [
        awrk_world::Name,
        WinWindow,
        WinTitle,
        WinInnerSize,
        WinFocused,
        WinStatus,
    ],
    outbound_create_components: [
        awrk_world::Name,
        WinWindow,
        WinTitle,
        WinInnerSize,
    ],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outbound_create_components_exclude_remote_owned_window_state() {
        let subscription = awrk_world::ProxySubscription::new();

        assert!(subscription
            .outbound_create_components
            .contains(core::any::type_name::<awrk_world::Name>()));
        assert!(subscription
            .outbound_create_components
            .contains(core::any::type_name::<WinWindow>()));
        assert!(subscription
            .outbound_create_components
            .contains(core::any::type_name::<WinTitle>()));
        assert!(subscription
            .outbound_create_components
            .contains(core::any::type_name::<WinInnerSize>()));
        assert!(!subscription
            .outbound_create_components
            .contains(core::any::type_name::<WinFocused>()));
        assert!(!subscription
            .outbound_create_components
            .contains(core::any::type_name::<WinStatus>()));
        assert!(subscription
            .mirrored_components
            .contains(core::any::type_name::<WinFocused>()));
        assert!(subscription
            .mirrored_components
            .contains(core::any::type_name::<WinStatus>()));
    }
}
