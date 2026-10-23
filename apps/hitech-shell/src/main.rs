//! HiTech-Shell — entry point.
#![allow(dead_code)] // scaffold keeps the compositor/shell seams visible before they are wired.
//!
//! Phase 8 scaffold. This binary currently validates the NixOS configuration
//! contract and prints the compositor plan; the Smithay compositor and
//! Slint/Iced shell processes are added in Phase 8 implementation.
//!
//! See `docs/audit/PHASE8-NETINSTALLER-RUST-DE.md` for the architecture.

mod compositor;
mod config;
mod layout;
mod shell;

use shell::ShellSurface;

fn main() {
    let config = config::Config::from_env();

    println!("hitech-shell {}", option_env!("CARGO_PKG_VERSION").unwrap_or("0.1.0"));
    println!("layout profile : {}", config.layout.as_str());
    println!("config source  : {}", config.source);
    println!("outputs        : {}", config.outputs.join(", "));
    println!();
    println!("layout regions :");

    for region in layout::regions(config.layout) {
        println!(
            "  {:<14} anchor={:<10} layer={:<7} input={}",
            region.name,
            region.anchor.as_str(),
            region.layer.as_str(),
            if region.interactive { "yes" } else { "no" }
        );
    }

    let shell = shell::Shell::new(config.clone());
    shell.present();

    let compositor = compositor::Compositor::new(config);
    compositor.run();
}
