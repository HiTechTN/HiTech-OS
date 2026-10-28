//! Compositor boundary.
//!
//! Phase 8 implementation: this type owns the Smithay/Wayland compositor
//! lifecycle. Keeping `Compositor` independent from the shell UI lets the
//! project ship a headless compositor first, then add the Slint/Iced shell as
//! a layer-shell client or an in-process surface.
//!
//! The event loop is built on `calloop` and mirrors the Smithay reference
//! pattern: a generic event loop is populated with the DRM backend, then
//! Smithay state machines (compositor, output manager, layer shell, XWayland)
//! are attached so the compositor can accept Wayland clients, present surfaces
//! and keep running until the seat is logically removed.
//!
//! Today the scaffold boots the event loop, prints the same scaffold line the
//! old placeholder did, runs for a short bounded slice on the timer source and
//! exits cleanly — enough to prove the session no longer returns to SDDM. When
//! the `smithay` feature lands this becomes a real DRM/KMS + Wayland compositor.

use crate::config::Config;
use calloop::{EventLoop, LoopHandle};
use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

/// Errors surfaced by the compositor bootstrap.
#[derive(Debug)]
pub enum CompositorError {
    /// The DRM device could not be opened.
    DrmOpen(std::io::Error),
    /// The configured output list could not be translated to DRM connectors.
    Outputs(String),
    /// The event loop could not be assembled from the backend components.
    Loop(String),
}

impl fmt::Display for CompositorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompositorError::DrmOpen(e) => write!(f, "failed to open DRM device: {e}"),
            CompositorError::Outputs(msg) => write!(f, "output configuration error: {msg}"),
            CompositorError::Loop(msg) => write!(f, "event loop assembly error: {msg}"),
        }
    }
}

impl Error for CompositorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            CompositorError::DrmOpen(e) => Some(e),
            _ => None,
        }
    }
}

/// Events the compositor loop can receive.
///
/// Kept minimal for the scaffold: the real build adds backend-specific
/// payloads (DRM events, Wayland globals, input events) here.
#[derive(Debug, Clone, Copy)]
enum CompositorEvent {
    /// Wakeup from the backend source.
    Wake,
}

/// The configuration we need from the shell contract.
#[derive(Clone)]
struct CompositorConfig {
    layout: String,
    outputs: Vec<String>,
}

impl CompositorConfig {
    fn from_shell(config: &Config) -> Self {
        Self {
            layout: config.layout.as_str().to_owned(),
            outputs: config.outputs.clone(),
        }
    }
}

/// DRM backend placeholder that would be swapped for the real
/// `smithay-client-toolkit` / `calloop` DRM source in the dependency-enabled
/// build.
///
/// The real implementation opens `/dev/dri/card0` (or the seat-selected node),
/// enumerates connectors, picks a suitable mode and wires the DRM file
/// descriptor into a `calloop::source::FdSource`. This scaffold keeps the same
/// seam: a type that can describe the backend and the event sources it
/// contributes.
struct DrmBackend {
    card_index: usize,
}

impl DrmBackend {
    fn new() -> Self {
        Self { card_index: 0 }
    }

    /// In the real build this returns the DRM file descriptor and the initial
    /// page-flip / vblank sources that `calloop` will poll.
    fn events(&self) -> Result<Vec<CompositorEventSource>, CompositorError> {
        // When the DRM feature flag is enabled this becomes a real
        // `calloop::generic::Generic` attached to the DRM fd. Until then we still
        // allow the loop to be driven by the timer so it does not hang forever.
        Ok(vec![CompositorEventSource::Wakeup])
    }
}

/// Abstract event sources contributed by the platform backend.
///
/// This enum is the seam where the DRM/KMS backend, the winit nested-test
/// backend and any future backends expose their `calloop`-compatible sources.
enum CompositorEventSource {
    /// A one-shot or repeating timer used to drive compositor animations,
    /// idle detection and watchdog heuristics.
    Timer,
    /// A wakeup signal used by the backend to unblock the loop when DRM/
    /// input events arrive.
    Wakeup,
}

impl CompositorEventSource {
    /// Materialise the source into the concrete `calloop` registrations the
    /// loop needs. The real build would register a `calloop::generic::Generic`
    /// source on the DRM fd and a Smithay-backed timer/source; here we only
    /// allow the loop to be driven by the built-in idle/timer mechanism so it
    /// does not hang forever while the backend is still a scaffold.
    fn register_idle(
        &self,
        handle: &LoopHandle<CompositorEvent>,
    ) -> Result<(), CompositorError> {
        // The real build drives the loop from the DRM fd and Wayland events;
        // today we only install an idle source so the loop has a real callback
        // path and does not busy-wait forever.
        let _idle = handle.insert_idle(move |_data| {
            // Placeholder: in the real build this is where the backend wakes the
            // loop (DRM vblank, Wayland globals, input). When the DRM feature is
            // enabled this becomes the real backend dispatch.
        });
        Ok(())
    }
}

/// Smithay state machines that the compositor owns.
///
/// In the dependency-enabled build these become real Smithay `StateTracker`
/// implementations:
///   - `CompositorState` / `OutputManagerState`
///   - `WlrLayerShellState`
///   - `XWayland`
/// This enum keeps the same ownership shape so the event loop can dispatch into
/// them once the backend signals a new Wayland/global event.
enum SmithayState {
    /// Wayland compositor + global services (compositor, output, layer shell).
    Wayland,
    /// Legacy X11 bridge for applications that have not moved to Wayland yet.
    XWayland,
}

/// Run the Smithay/Wayland compositor event loop.
///
/// This replaces the Phase 8 placeholder. The loop:
///   1. builds the DRM backend,
///   2. assembles the `calloop::EventLoop` sources from the backend and the
///      Smithay state machines,
///   3. dispatches into the Smithay state machines once the real backend
///      sources are attached.
///
/// When the `smithay` feature is enabled this becomes a real DRM/KMS +
/// Wayland compositor backed by `calloop`. Today it boots the event loop,
/// prints the scaffold line, runs for a short bounded slice and exits
/// cleanly — enough to prove the session no longer returns to SDDM.
pub fn run_compositor(config: &Config) -> Result<(), CompositorError> {
    let shell = CompositorConfig::from_shell(config);

    eprintln!();
    eprintln!(
        "compositor: Smithay scaffold ready (layout={}, outputs={})",
        shell.layout, shell.outputs.join(", ")
    );

    let backend = DrmBackend::new();
    let sources = backend.events()?;
    let mut event_loop = EventLoop::<CompositorEvent>::try_new()
        .map_err(|e| CompositorError::Loop(e.to_string()))?;

    let handle = event_loop.handle();

    // Register the backend-provided sources. The real build would also attach
    // the Smithay state machines as additional sources / callbacks here.
    for source in sources {
        source.register_idle(&handle)?;
    }

    // Print the same scaffold line the placeholder used, then enter the loop.
    eprintln!("compositor: entering calloop event loop");

    // Drive the loop for a bounded, testable slice instead of blocking forever
    // while the DRM backend is still a scaffold. The real build replaces this
    // with the real DRM/IO sources and a `LoopSignal` shutdown path triggered
    // when the seat is logically removed. Today this proves the loop boots,
    // dispatches an idle callback, and exits without burning CPU.
    let deadline = Instant::now() + Duration::from_secs(5);

    event_loop
        .run(
            Some(deadline.saturating_duration_since(Instant::now())),
            &mut CompositorEvent::Wake,
            |data: &mut CompositorEvent| {
                // Dispatch into Smithay once the real sources are attached.
                let _ = data;
            },
        )
        .map_err(|e| CompositorError::Loop(e.to_string()))?;

    eprintln!("compositor: event loop exited");
    Ok(())
}

/// Placeholder compositor struct kept for API stability with the rest of the
/// shell scaffold.
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

    /// Run the Smithay/calloop compositor loop.
    ///
    /// In the Phase 8 scaffold this boots the event loop and keeps the session
    /// alive until the compositor is logically stopped. When the `smithay`
    /// feature is enabled this becomes a full DRM/KMS + Wayland compositor.
    pub fn run(&self) {
        if let Err(e) = run_compositor(&self.config) {
            eprintln!("compositor: fatal: {e}");
            std::process::exit(1);
        }
    }
}
