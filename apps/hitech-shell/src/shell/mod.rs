//! Shell chrome boundary.
//!
//! The shell (panels, dock, quick settings, notification centre) is a separate
//! concern from the compositor. Phase 8 keeps this as a trait so the concrete
//! toolkit — Slint for chrome, Iced for settings apps, or an in-process
//! Wayland client — can be selected without touching the layout code.

use crate::config::Config;
use crate::layout::{self, Region};

pub trait ShellSurface {
    fn layout(&self) -> &Config;
    fn regions(&self) -> Vec<Region>;
    fn present(&self);
}

pub struct Shell {
    config: Config,
}

impl Shell {
    pub fn new(config: Config) -> Self {
        Self { config }
    }
}

impl ShellSurface for Shell {
    fn layout(&self) -> &Config {
        &self.config
    }

    fn regions(&self) -> Vec<Region> {
        layout::regions(self.config.layout)
    }

    fn present(&self) {
        println!(
            "shell: present {} regions for layout {}",
            self.regions().len(),
            self.config.layout.as_str()
        );
    }
}
