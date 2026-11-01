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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const ALL_LAYOUTS: [Layout; 3] = [Layout::Hybrid, Layout::Mac, Layout::Windows];

    #[test]
    fn every_layout_exposes_interactive_regions() {
        for layout in ALL_LAYOUTS {
            let regions = regions(layout);
            assert!(!regions.is_empty(), "{layout:?} has no regions");
            assert!(
                regions.iter().all(|region| region.interactive),
                "{layout:?} exposes a non-interactive region"
            );
        }
    }

    #[test]
    fn region_names_are_unique_per_layout() {
        for layout in ALL_LAYOUTS {
            let mut seen = HashSet::new();
            for region in regions(layout) {
                assert!(
                    seen.insert(region.name),
                    "duplicate region {} in {layout:?}",
                    region.name
                );
            }
        }
    }

    #[test]
    fn layout_profiles_have_the_expected_shape() {
        let hybrid = regions(Layout::Hybrid);
        assert!(hybrid
            .iter()
            .any(|r| r.name == "taskbar" && r.anchor == Anchor::Bottom));
        assert!(hybrid
            .iter()
            .any(|r| r.name == "quick-settings" && r.anchor == Anchor::Right));

        let mac = regions(Layout::Mac);
        assert!(mac
            .iter()
            .any(|r| r.name == "menu-bar" && r.anchor == Anchor::Top));
        assert!(mac
            .iter()
            .any(|r| r.name == "dock" && r.anchor == Anchor::Bottom));
        assert!(!mac.iter().any(|r| r.name == "start-menu"));

        let windows = regions(Layout::Windows);
        assert!(windows.iter().any(|r| r.name == "start-menu"));
        assert!(!windows.iter().any(|r| r.anchor == Anchor::Top));
    }

    #[test]
    fn overlay_layers_are_only_used_for_popovers() {
        for layout in ALL_LAYOUTS {
            for region in regions(layout) {
                if region.layer == Layer::Overlay {
                    assert!(
                        matches!(region.anchor, Anchor::Top | Anchor::Bottom | Anchor::Right),
                        "{} uses an unexpected overlay anchor",
                        region.name
                    );
                }
            }
        }
    }

    #[test]
    fn enum_strings_are_stable_contract_values() {
        assert_eq!(Anchor::Top.as_str(), "top");
        assert_eq!(Anchor::Bottom.as_str(), "bottom");
        assert_eq!(Anchor::Left.as_str(), "left");
        assert_eq!(Anchor::Right.as_str(), "right");
        assert_eq!(Anchor::Fullscreen.as_str(), "fullscreen");

        assert_eq!(Layer::Background.as_str(), "background");
        assert_eq!(Layer::Bottom.as_str(), "bottom");
        assert_eq!(Layer::Top.as_str(), "top");
        assert_eq!(Layer::Overlay.as_str(), "overlay");
    }
}
