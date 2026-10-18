//! Declarative top-bar layout.
//!
//! Move an action id between the lists below to move a button in the top bar.
//! Sizes come from [`crate::ui::tokens::Metrics`]; section order and panel tabs
//! live in `ui/layout.rs`.

use crate::ui::icons::Icon;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ActionId {
    OpenFolder,
    RotateLeft,
    RotateRight,
    FlipV,
    FlipH,
    Crop,
    Sliders,
    CopyPath,
    Info,
    Reveal,
    Export,
    Theme,
    Refresh,
    WindowMinimize,
    WindowMaximize,
    WindowClose,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonStyle {
    /// shadcn `default` (filled primary).
    Primary,
    /// shadcn `ghost` (`icon` size).
    Ghost,
    /// Text-only shadcn `ghost` / `outline`.
    Text,
    /// Header-bar window control (minimize/maximize).
    Window,
    /// Header-bar close control (destructive hover).
    WindowClose,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Always,
    ViewerOpen,
}

#[derive(Clone, Copy)]
pub struct ActionSpec {
    pub id: ActionId,
    pub icon: Option<Icon>,
    /// Tooltip for icon buttons; the visible label for text buttons.
    pub label: &'static str,
    pub style: ButtonStyle,
    pub visible: Visibility,
}

const fn icon(
    id: ActionId,
    icon: Icon,
    label: &'static str,
    style: ButtonStyle,
    visible: Visibility,
) -> ActionSpec {
    ActionSpec {
        id,
        icon: Some(icon),
        label,
        style,
        visible,
    }
}

/// Left group, after the app mark.
pub const LEFT_ACTIONS: &[ActionSpec] = &[ActionSpec {
    id: ActionId::OpenFolder,
    icon: None,
    label: "Open Folder",
    style: ButtonStyle::Text,
    visible: Visibility::Always,
}];

/// Middle group (image tools).
pub const MIDDLE_ACTIONS: &[ActionSpec] = &[
    icon(
        ActionId::RotateRight,
        Icon::RotateRight,
        "Rotate 90° clockwise",
        ButtonStyle::Ghost,
        Visibility::ViewerOpen,
    ),
    icon(
        ActionId::RotateLeft,
        Icon::RotateLeft,
        "Rotate 90° counter-clockwise",
        ButtonStyle::Ghost,
        Visibility::ViewerOpen,
    ),
    icon(
        ActionId::FlipV,
        Icon::FlipV,
        "Flip vertically",
        ButtonStyle::Ghost,
        Visibility::ViewerOpen,
    ),
    icon(
        ActionId::FlipH,
        Icon::FlipH,
        "Flip horizontally",
        ButtonStyle::Ghost,
        Visibility::ViewerOpen,
    ),
    icon(
        ActionId::Crop,
        Icon::Crop,
        "Crop",
        ButtonStyle::Ghost,
        Visibility::ViewerOpen,
    ),
    icon(
        ActionId::Sliders,
        Icon::Sliders,
        "Adjust panel (E)",
        ButtonStyle::Ghost,
        Visibility::ViewerOpen,
    ),
    icon(
        ActionId::CopyPath,
        Icon::Copy,
        "Copy image path",
        ButtonStyle::Ghost,
        Visibility::ViewerOpen,
    ),
];

/// Right group, visual left-to-right. Rendered right-to-left.
pub const RIGHT_ACTIONS: &[ActionSpec] = &[
    icon(
        ActionId::Info,
        Icon::Info,
        "Image info",
        ButtonStyle::Ghost,
        Visibility::ViewerOpen,
    ),
    icon(
        ActionId::Reveal,
        Icon::FolderOpen,
        "Open in file manager",
        ButtonStyle::Ghost,
        Visibility::ViewerOpen,
    ),
    icon(
        ActionId::Export,
        Icon::Download,
        "Export the edited image",
        ButtonStyle::Primary,
        Visibility::ViewerOpen,
    ),
    ActionSpec {
        id: ActionId::Theme,
        icon: None,
        label: "Theme",
        style: ButtonStyle::Text,
        visible: Visibility::Always,
    },
    icon(
        ActionId::Refresh,
        Icon::RefreshCw,
        "Refresh",
        ButtonStyle::Ghost,
        Visibility::Always,
    ),
    // GNOME-style window controls at the far right.
    icon(
        ActionId::WindowMinimize,
        Icon::Minimize,
        "Minimize",
        ButtonStyle::Window,
        Visibility::Always,
    ),
    icon(
        ActionId::WindowMaximize,
        Icon::Maximize,
        "Maximize",
        ButtonStyle::Window,
        Visibility::Always,
    ),
    icon(
        ActionId::WindowClose,
        Icon::Close,
        "Close",
        ButtonStyle::WindowClose,
        Visibility::Always,
    ),
];
