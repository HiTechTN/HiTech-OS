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
        let source = env::var("HITECH_SHELL_CONFIG")
            .unwrap_or_else(|_| "<environment>".to_string());

        let layout = env::var("HITECH_SHELL_LAYOUT")
            .ok()
            .and_then(|value| Layout::parse(&value))
            .unwrap_or(Layout::Hybrid);

        let outputs = env::var("HITECH_SHELL_OUTPUTS")
            .map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_else(|_| vec!["*".to_string()]);

        Self {
            layout,
            outputs,
            source,
        }
    }
}
