//! Config contract shared by the NixOS module and the Wayland shell.
//!
//! The NixOS module renders `/etc/hitechos/hitech-shell.toml` (or sets
//! `HITECH_SHELL_LAYOUT`) from the same enum below. The shell loads the same
//! value; `services.hitech-shell.layout` is therefore the single source of
//! truth in the installed system.

use std::env;

/// Layout profile exposed as the NixOS option
/// `services.hitech-shell.layout`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Layout {
    /// Windows-style bottom taskbar + start menu, plus an Android-style
    /// quick-settings panel.
    Hybrid,
    /// macOS-style bottom dock + top menu bar.
    Mac,
    /// Windows-style taskbar and start menu only.
    Windows,
}

impl Layout {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "hybrid" => Some(Self::Hybrid),
            "mac" | "macos" => Some(Self::Mac),
            "windows" | "win" => Some(Self::Windows),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hybrid => "hybrid",
            Self::Mac => "mac",
            Self::Windows => "windows",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub layout: Layout,
    pub outputs: Vec<String>,
    pub source: String,
}

impl Config {
    /// Reads the environment set by the NixOS module/session.
    ///
    /// `HITECH_SHELL_LAYOUT` is authoritative; `HITECH_SHELL_CONFIG` points at
    /// the generated TOML file for later phases (multi-output settings,
    /// theme, widget allow-list). No parser dependency is pulled in yet.
    pub fn from_env() -> Self {
        let source =
            env::var("HITECH_SHELL_CONFIG").unwrap_or_else(|_| "<environment>".to_string());

        let layout = parse_layout(env::var("HITECH_SHELL_LAYOUT").ok().as_deref());
        let outputs = parse_outputs(env::var("HITECH_SHELL_OUTPUTS").ok().as_deref());

        Self {
            layout,
            outputs,
            source,
        }
    }
}

/// Parses `HITECH_SHELL_LAYOUT`, defaulting to the hybrid profile.
pub fn parse_layout(value: Option<&str>) -> Layout {
    value.and_then(Layout::parse).unwrap_or(Layout::Hybrid)
}

/// Parses `HITECH_SHELL_OUTPUTS` (comma-separated), defaulting to all outputs.
///
/// Empty entries are dropped so `"HDMI-A-1,,DP-1,"` yields two outputs and an
/// empty string falls back to `*`.
pub fn parse_outputs(value: Option<&str>) -> Vec<String> {
    let outputs: Vec<String> = value
        .unwrap_or("*")
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect();

    if outputs.is_empty() {
        vec!["*".to_string()]
    } else {
        outputs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_parse_accepts_aliases_case_insensitively() {
        assert_eq!(Layout::parse("HYBRID"), Some(Layout::Hybrid));
        assert_eq!(Layout::parse(" mac "), Some(Layout::Mac));
        assert_eq!(Layout::parse("macos"), Some(Layout::Mac));
        assert_eq!(Layout::parse("Windows"), Some(Layout::Windows));
        assert_eq!(Layout::parse("win"), Some(Layout::Windows));
        assert_eq!(Layout::parse("gnome"), None);
    }

    #[test]
    fn layout_round_trips_through_as_str() {
        for layout in [Layout::Hybrid, Layout::Mac, Layout::Windows] {
            assert_eq!(Layout::parse(layout.as_str()), Some(layout));
        }
    }

    #[test]
    fn missing_or_invalid_layout_defaults_to_hybrid() {
        assert_eq!(parse_layout(None), Layout::Hybrid);
        assert_eq!(parse_layout(Some("unknown")), Layout::Hybrid);
    }

    #[test]
    fn outputs_default_to_all() {
        assert_eq!(parse_outputs(None), vec!["*".to_string()]);
        assert_eq!(parse_outputs(Some("")), vec!["*".to_string()]);
        assert_eq!(parse_outputs(Some(" , ,")), vec!["*".to_string()]);
    }

    #[test]
    fn outputs_are_split_trimmed_and_filtered() {
        assert_eq!(
            parse_outputs(Some(" HDMI-A-1, ,DP-1,")),
            vec!["HDMI-A-1".to_string(), "DP-1".to_string()]
        );
    }
}
