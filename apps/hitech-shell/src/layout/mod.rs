//! Layout profiles.
//!
//! Every profile is a pure description of screen regions and their anchor
//! semantics. The future Smithay layer-shell code consumes this table; keeping
//! it dependency-free means the NixOS module, tests and the compositor share a
//! single Rust enum with no UI toolkit lock-in.

use crate::config::Layout;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Anchor {
    Top,
    Bottom,
    Left,
    Right,
    Fullscreen,
}

impl Anchor {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Left => "left",
            Self::Right => "right",
            Self::Fullscreen => "fullscreen",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Layer {
    Background,
    Bottom,
    Top,
    Overlay,
}

impl Layer {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::Bottom => "bottom",
            Self::Top => "top",
            Self::Overlay => "overlay",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Region {
    pub name: &'static str,
    pub anchor: Anchor,
    pub layer: Layer,
    pub height: u32,
    pub interactive: bool,
}

pub fn regions(layout: Layout) -> Vec<Region> {
    match layout {
        Layout::Hybrid => vec![
            Region {
                name: "taskbar",
                anchor: Anchor::Bottom,
                layer: Layer::Top,
                height: 48,
                interactive: true,
            },
            Region {
                name: "start-menu",
                anchor: Anchor::Bottom,
                layer: Layer::Overlay,
                height: 560,
                interactive: true,
            },
            Region {
                name: "quick-settings",
                anchor: Anchor::Right,
                layer: Layer::Overlay,
                height: 720,
                interactive: true,
            },
            Region {
                name: "notifications",
                anchor: Anchor::Top,
                layer: Layer::Overlay,
                height: 0,
                interactive: true,
            },
        ],
        Layout::Mac => vec![
            Region {
                name: "menu-bar",
                anchor: Anchor::Top,
                layer: Layer::Top,
                height: 28,
                interactive: true,
            },
            Region {
                name: "dock",
                anchor: Anchor::Bottom,
                layer: Layer::Top,
                height: 64,
                interactive: true,
            },
            Region {
                name: "control-centre",
                anchor: Anchor::Top,
                layer: Layer::Overlay,
                height: 0,
                interactive: true,
            },
        ],
        Layout::Windows => vec![
            Region {
                name: "taskbar",
                anchor: Anchor::Bottom,
                layer: Layer::Top,
                height: 48,
                interactive: true,
            },
            Region {
                name: "start-menu",
                anchor: Anchor::Bottom,
                layer: Layer::Overlay,
                height: 560,
                interactive: true,
            },
            Region {
                name: "system-tray",
                anchor: Anchor::Bottom,
                layer: Layer::Top,
                height: 48,
                interactive: true,
            },
        ],
    }
}
