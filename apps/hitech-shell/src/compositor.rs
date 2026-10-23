//! Compositor boundary.
//!
//! Phase 8 scaffold: this type defines the seam where Smithay will live.
//! Keeping `Compositor` independent from the shell UI lets the project ship a
//! headless compositor first, then add the Slint/Iced shell as a layer-shell
//! client or an in-process surface.

use crate::config::Config;

pub struct Compositor {
    config: Config,
}

impl Compositor {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Placeholder for the Smithay `calloop` event loop.
    ///
    /// Implementation plan (Phase 8):
    ///   - `smithay::wayland::compositor::CompositorState`
    ///   - `smithay::wayland::output::OutputManagerState` (multi-monitor)
    ///   - `smithay::wayland::shell::wlr_layer::WlrLayerShellState`
    ///   - `smithay::xwayland::XWayland` for legacy apps
    ///   - DRM/KMS backend for bare metal; winit backend for nested tests
    pub fn run(&self) {
        println!();
        println!(
            "compositor: Smithay scaffold ready (layout={}, outputs={})",
            self.config.layout.as_str(),
            self.config.outputs.join(", ")
        );
        println!("compositor: replace this placeholder with the calloop event loop");
    }
}
