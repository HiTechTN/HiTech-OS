//! HiTech-Shell compositor.
//!
//! Phase 8.5: a real Smithay compositor with a libseat/udev/DRM backend. It
//! creates a Wayland socket, owns the DRM outputs, dispatches libinput events
//! and keeps running until the session is stopped. The shell chrome (panels,
//! dock) is still a separate layer; this binary is the compositor/session
//! foundation it will attach to.

use std::{
    collections::HashMap,
    error::Error,
    fmt,
    os::fd::OwnedFd,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use smithay::{
    backend::{
        allocator::{
            gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
            Fourcc,
        },
        drm::{
            compositor::FrameFlags,
            exporter::gbm::GbmFramebufferExporter,
            output::{DrmOutput, DrmOutputManager, DrmOutputRenderElements},
            DrmDevice, DrmDeviceFd, DrmEvent, DrmNode, NodeType,
        },
        egl::{EGLContext, EGLDisplay},
        input::{
            AbsolutePositionEvent, Event, InputEvent, KeyboardKeyEvent, PointerAxisEvent,
            PointerButtonEvent, PointerMotionEvent,
        },
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        renderer::{element::surface::WaylandSurfaceRenderElement, gles::GlesRenderer, ImportDma},
        session::{
            libseat::{LibSeatSession, LibSeatSessionNotifier},
            Event as SessionEvent, Session,
        },
        udev::{UdevBackend, UdevEvent},
    },
    desktop::{
        find_popup_root_surface, get_popup_toplevel_coords, space::SpaceRenderElements, PopupKind,
        PopupManager, Space, Window, WindowSurfaceType,
    },
    input::{
        keyboard::FilterResult,
        pointer::{AxisFrame, ButtonEvent, MotionEvent},
        Seat, SeatHandler, SeatState,
    },
    output::{Mode as WlMode, Output, PhysicalProperties},
    reexports::{
        calloop::{
            generic::Generic, EventLoop, Interest, LoopHandle, LoopSignal, Mode, PostAction,
            RegistrationToken,
        },
        drm::control::{connector, crtc, ModeTypeFlags},
        input::{DeviceCapability, Libinput},
        rustix::fs::OFlags,
        wayland_server::{
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::{wl_buffer, wl_seat, wl_surface::WlSurface},
            Client, Display, DisplayHandle, Resource,
        },
    },
    utils::{DeviceFd, Logical, Point, Serial, SERIAL_COUNTER},
    wayland::{
        buffer::BufferHandler,
        compositor::{
            get_parent, is_sync_subsurface, CompositorClientState, CompositorHandler,
            CompositorState,
        },
        output::OutputHandler,
        selection::{
            data_device::{
                set_data_device_focus, ClientDndGrabHandler, DataDeviceHandler, DataDeviceState,
                ServerDndGrabHandler,
            },
            SelectionHandler,
        },
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
            XdgToplevelSurfaceData,
        },
        shm::{ShmHandler, ShmState},
        socket::ListeningSocketSource,
    },
};
use smithay_drm_extras::drm_scanner::{DrmScanEvent, DrmScanner};
use tracing::{debug, error, info, warn};

use crate::config::Config;

type GbmDrmOutput =
    DrmOutput<GbmAllocator<DrmDeviceFd>, GbmFramebufferExporter<DrmDeviceFd>, (), DrmDeviceFd>;
type OutputManager = DrmOutputManager<
    GbmAllocator<DrmDeviceFd>,
    GbmFramebufferExporter<DrmDeviceFd>,
    (),
    DrmDeviceFd,
>;
type ShellRenderElement =
    SpaceRenderElements<GlesRenderer, WaylandSurfaceRenderElement<GlesRenderer>>;

const CLEAR_COLOR: [f32; 4] = [0.04, 0.05, 0.08, 1.0];
const SUPPORTED_FORMATS: &[Fourcc] = &[Fourcc::Argb8888, Fourcc::Xrgb8888];

/// Errors surfaced by the compositor bootstrap or a fatal backend failure.
#[derive(Debug)]
pub enum CompositorError {
    Session(String),
    Display(String),
    Loop(String),
    Backend(String),
}

impl fmt::Display for CompositorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Session(e) => write!(f, "session backend error: {e}"),
            Self::Display(e) => write!(f, "wayland display error: {e}"),
            Self::Loop(e) => write!(f, "event loop error: {e}"),
            Self::Backend(e) => write!(f, "graphics backend error: {e}"),
        }
    }
}

impl Error for CompositorError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct UdevOutputId {
    device_id: DrmNode,
    crtc: crtc::Handle,
}

struct Surface {
    output: Output,
    drm_output: GbmDrmOutput,
}

struct Device {
    manager: OutputManager,
    renderer: GlesRenderer,
    scanner: DrmScanner,
    surfaces: HashMap<crtc::Handle, Surface>,
    notifier_token: RegistrationToken,
}

/// Global compositor state, owned by the calloop event loop.
pub struct HitechShell {
    start_time: Instant,
    socket_name: std::ffi::OsString,
    display_handle: DisplayHandle,
    loop_signal: LoopSignal,
    loop_handle: LoopHandle<'static, HitechShell>,

    space: Space<Window>,
    compositor_state: CompositorState,
    xdg_shell_state: XdgShellState,
    shm_state: ShmState,
    output_manager_state: smithay::wayland::output::OutputManagerState,
    seat_state: SeatState<HitechShell>,
    data_device_state: DataDeviceState,
    popups: PopupManager,
    seat: Seat<HitechShell>,

    session: LibSeatSession,
    devices: HashMap<DrmNode, Device>,

    config: Config,
    paused: bool,
    redraw_queued: bool,
}

#[derive(Default)]
struct ClientState {
    compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

impl HitechShell {
    fn new(
        config: Config,
        display: Display<HitechShell>,
        event_loop: &mut EventLoop<'static, HitechShell>,
        session: LibSeatSession,
        notifier: LibSeatSessionNotifier,
    ) -> Result<Self, CompositorError> {
        let display_handle = display.handle();
        let start_time = Instant::now();

        let compositor_state = CompositorState::new::<Self>(&display_handle);
        let xdg_shell_state = XdgShellState::new::<Self>(&display_handle);
        let shm_state = ShmState::new::<Self>(&display_handle, vec![]);
        let output_manager_state =
            smithay::wayland::output::OutputManagerState::new_with_xdg_output::<Self>(
                &display_handle,
            );
        let mut seat_state = SeatState::new();
        let data_device_state = DataDeviceState::new::<Self>(&display_handle);
        let popups = PopupManager::default();

        let mut seat: Seat<Self> = seat_state.new_wl_seat(&display_handle, "seat0");
        seat.add_keyboard(Default::default(), 200, 25)
            .map_err(|e| CompositorError::Backend(format!("cannot add keyboard: {e}")))?;
        seat.add_pointer();

        let loop_handle = event_loop.handle();
        let loop_signal = event_loop.get_signal();

        let listening_socket = ListeningSocketSource::new_auto()
            .map_err(|e| CompositorError::Display(format!("cannot create wayland socket: {e}")))?;
        let socket_name = listening_socket.socket_name().to_os_string();

        let socket_handle = loop_handle.clone();
        let _ = socket_handle.insert_source(
            listening_socket,
            move |client_stream, _, state: &mut HitechShell| {
                if let Err(e) = state
                    .display_handle
                    .insert_client(client_stream, Arc::new(ClientState::default()))
                {
                    warn!("cannot insert wayland client: {e}");
                }
            },
        );

        let _ = loop_handle.insert_source(
            Generic::new(display, Interest::READ, Mode::Level),
            |_, display, state: &mut HitechShell| {
                // SAFETY: the display lives in this event source until the loop exits.
                unsafe {
                    if let Err(e) = display.get_mut().dispatch_clients(state) {
                        error!("wayland dispatch error: {e}");
                    }
                }
                Ok(PostAction::Continue)
            },
        );

        std::env::set_var("WAYLAND_DISPLAY", &socket_name);
        info!(
            "hitech-shell compositor {} on {}",
            env!("CARGO_PKG_VERSION"),
            socket_name.to_string_lossy()
        );

        let mut state = Self {
            start_time,
            socket_name,
            display_handle,
            loop_signal,
            loop_handle,
            space: Space::default(),
            compositor_state,
            xdg_shell_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            popups,
            seat,
            session,
            devices: HashMap::new(),
            config,
            paused: false,
            redraw_queued: false,
        };

        let _ = state
            .loop_handle
            .insert_source(notifier, |event, _, state: &mut HitechShell| match event {
                SessionEvent::PauseSession => {
                    info!("session paused; suspending rendering");
                    state.paused = true;
                }
                SessionEvent::ActivateSession => {
                    info!("session activated; resuming rendering");
                    state.paused = false;
                    state.queue_redraw();
                }
            });

        state.init_backends();
        Ok(state)
    }

    fn init_backends(&mut self) {
        let seat_name = self.session.seat();

        let _ =
            self.loop_handle.insert_source(
                LibinputInputBackend::new({
                    let mut context = Libinput::new_with_udev::<
                        LibinputSessionInterface<LibSeatSession>,
                    >(self.session.clone().into());
                    if context.udev_assign_seat(&seat_name).is_err() {
                        error!("cannot assign libinput seat {seat_name}");
                    }
                    context
                }),
                |mut event, _, state: &mut HitechShell| {
                    if let InputEvent::DeviceAdded { device } = &mut event {
                        if device.has_capability(DeviceCapability::Keyboard) {
                            if let Some(keyboard) = state.seat.get_keyboard() {
                                device.led_update(keyboard.led_state().into());
                            }
                        }
                    }
                    state.process_input_event(event);
                },
            );

        let backend = match UdevBackend::new(&seat_name) {
            Ok(backend) => backend,
            Err(e) => {
                error!("cannot initialize udev backend: {e}");
                return;
            }
        };

        let devices: Vec<(DrmNode, PathBuf)> = backend
            .device_list()
            .filter_map(|(device_id, path)| {
                DrmNode::from_dev_id(device_id)
                    .ok()
                    .map(|node| (node, path.to_owned()))
            })
            .collect();

        let _ = self
            .loop_handle
            .insert_source(backend, |event, _, state: &mut HitechShell| {
                state.on_udev_event(event)
            });

        for (node, path) in devices {
            self.on_udev_added(node, &path);
        }
    }

    fn on_udev_event(&mut self, event: UdevEvent) {
        match event {
            UdevEvent::Added { device_id, path } => {
                if let Ok(node) = DrmNode::from_dev_id(device_id) {
                    self.on_udev_added(node, &path);
                }
            }
            UdevEvent::Changed { device_id } => {
                if let Ok(node) = DrmNode::from_dev_id(device_id) {
                    self.device_changed(node);
                }
            }
            UdevEvent::Removed { device_id } => {
                if let Ok(node) = DrmNode::from_dev_id(device_id) {
                    self.device_removed(node);
                }
            }
        }
    }

    fn on_udev_added(&mut self, node: DrmNode, path: &Path) {
        if node.ty() != NodeType::Primary || self.devices.contains_key(&node) {
            return;
        }

        let fd: OwnedFd = match self.session.open(
            path,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
        ) {
            Ok(fd) => fd,
            Err(e) => {
                warn!("cannot open {} via libseat: {e}", path.display());
                return;
            }
        };
        let fd = DrmDeviceFd::new(DeviceFd::from(fd));

        let (drm, notifier) = match DrmDevice::new(fd.clone(), true) {
            Ok(pair) => pair,
            Err(e) => {
                warn!("cannot initialize DRM device {}: {e}", path.display());
                return;
            }
        };

        let notifier_token = match self.loop_handle.insert_source(
            notifier,
            move |event, _, state: &mut HitechShell| match event {
                DrmEvent::VBlank(crtc) => state.frame_finish(node, crtc),
                DrmEvent::Error(e) => error!("DRM error on {node}: {e}"),
            },
        ) {
            Ok(token) => token,
            Err(e) => {
                warn!("cannot register DRM notifier: {e}");
                return;
            }
        };

        let gbm = match GbmDevice::new(fd) {
            Ok(gbm) => gbm,
            Err(e) => {
                warn!("cannot initialize GBM device: {e}");
                return;
            }
        };

        let renderer = {
            let egl_display = match unsafe { EGLDisplay::new(gbm.clone()) } {
                Ok(display) => display,
                Err(e) => {
                    warn!("cannot create EGL display: {e}");
                    return;
                }
            };
            let context = match EGLContext::new(&egl_display) {
                Ok(context) => context,
                Err(e) => {
                    warn!("cannot create EGL context: {e}");
                    return;
                }
            };
            match unsafe { GlesRenderer::new(context) } {
                Ok(renderer) => renderer,
                Err(e) => {
                    warn!("cannot create GLES renderer: {e}");
                    return;
                }
            }
        };

        let allocator = GbmAllocator::new(
            gbm.clone(),
            GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
        );
        let exporter = GbmFramebufferExporter::new(gbm.clone(), Some(node));
        let render_formats: Vec<_> = renderer.dmabuf_formats().iter().copied().collect();
        let manager = DrmOutputManager::new(
            drm,
            allocator,
            exporter,
            Some(gbm),
            SUPPORTED_FORMATS.iter().copied(),
            render_formats,
        );

        info!("DRM device {node} ready ({})", path.display());
        self.devices.insert(
            node,
            Device {
                manager,
                renderer,
                scanner: DrmScanner::new(),
                surfaces: HashMap::new(),
                notifier_token,
            },
        );

        self.device_changed(node);
    }

    fn device_changed(&mut self, node: DrmNode) {
        let events = {
            let Some(device) = self.devices.get_mut(&node) else {
                return;
            };
            match device.scanner.scan_connectors(device.manager.device()) {
                Ok(events) => events.into_iter().collect::<Vec<_>>(),
                Err(e) => {
                    warn!("cannot scan connectors on {node}: {e}");
                    return;
                }
            }
        };

        for event in events {
            match event {
                DrmScanEvent::Connected {
                    connector,
                    crtc: Some(crtc),
                } => self.connector_connected(node, connector, crtc),
                DrmScanEvent::Disconnected {
                    connector,
                    crtc: Some(crtc),
                } => self.connector_disconnected(node, &connector, crtc),
                _ => {}
            }
        }
    }

    fn connector_connected(
        &mut self,
        node: DrmNode,
        connector: connector::Info,
        crtc: crtc::Handle,
    ) {
        let Some(device) = self.devices.get_mut(&node) else {
            return;
        };

        let Some(mode) = connector
            .modes()
            .iter()
            .find(|mode| mode.mode_type().contains(ModeTypeFlags::PREFERRED))
            .or_else(|| connector.modes().first())
            .copied()
        else {
            warn!("connector {:?} has no modes", connector.handle());
            return;
        };

        let output_name = format!(
            "{}-{}",
            connector.interface().as_str(),
            connector.interface_id()
        );
        let (phys_w, phys_h) = connector.size().unwrap_or((0, 0));
        let output = Output::new(
            output_name.clone(),
            PhysicalProperties {
                size: (phys_w as i32, phys_h as i32).into(),
                subpixel: connector.subpixel().into(),
                make: "HiTech".to_string(),
                model: "Panel".to_string(),
            },
        );
        let _global = output.create_global::<HitechShell>(&self.display_handle);
        let wl_mode = WlMode::from(mode);

        let x = self.space.outputs().fold(0, |acc, o| {
            acc + self.space.output_geometry(o).map_or(0, |g| g.size.w)
        });
        let position: Point<i32, Logical> = (x, 0).into();

        output.set_preferred(wl_mode);
        output.change_current_state(Some(wl_mode), None, None, Some(position));
        self.space.map_output(&output, position);
        output.user_data().insert_if_missing(|| UdevOutputId {
            device_id: node,
            crtc,
        });

        let planes = device.manager.device().planes(&crtc).ok();
        let render_elements =
            DrmOutputRenderElements::<GlesRenderer, ShellRenderElement>::default();

        let Device {
            manager, renderer, ..
        } = device;
        let drm_output = match manager.initialize_output::<_, ShellRenderElement>(
            crtc,
            mode,
            &[connector.handle()],
            &output,
            planes,
            renderer,
            &render_elements,
        ) {
            Ok(drm_output) => drm_output,
            Err(e) => {
                warn!("cannot initialize output {output_name}: {e}");
                self.space.unmap_output(&output);
                return;
            }
        };

        device.surfaces.insert(crtc, Surface { output, drm_output });
        info!("output {output_name} enabled on {node}/{crtc:?}");
        self.queue_redraw();
    }

    fn connector_disconnected(
        &mut self,
        node: DrmNode,
        connector: &connector::Info,
        crtc: crtc::Handle,
    ) {
        let Some(device) = self.devices.get_mut(&node) else {
            return;
        };
        if let Some(surface) = device.surfaces.remove(&crtc) {
            self.space.unmap_output(&surface.output);
            info!(
                "output {} disconnected ({:?})",
                surface.output.name(),
                connector.handle()
            );
            self.queue_redraw();
        }
    }

    fn device_removed(&mut self, node: DrmNode) {
        if let Some(device) = self.devices.remove(&node) {
            self.loop_handle.remove(device.notifier_token);
            let outputs: Vec<Output> = device
                .surfaces
                .values()
                .map(|surface| surface.output.clone())
                .collect();
            for output in outputs {
                self.space.unmap_output(&output);
            }
            info!("DRM device {node} removed");
        }
    }

    fn frame_finish(&mut self, node: DrmNode, crtc: crtc::Handle) {
        if let Some(device) = self.devices.get_mut(&node) {
            if let Some(surface) = device.surfaces.get_mut(&crtc) {
                if let Err(e) = surface.drm_output.frame_submitted() {
                    warn!("frame_submitted failed on {node}/{crtc:?}: {e}");
                }
            }
        }
    }

    fn queue_redraw(&mut self) {
        if self.redraw_queued {
            return;
        }
        self.redraw_queued = true;
        let _ = self.loop_handle.insert_idle(|state: &mut HitechShell| {
            state.redraw_queued = false;
            state.render_all();
        });
    }

    fn render_all(&mut self) {
        if self.paused {
            return;
        }
        let nodes: Vec<DrmNode> = self.devices.keys().copied().collect();
        for node in nodes {
            let crtcs: Vec<crtc::Handle> = self
                .devices
                .get(&node)
                .map(|device| device.surfaces.keys().copied().collect())
                .unwrap_or_default();
            for crtc in crtcs {
                self.render_output(node, crtc);
            }
        }
    }

    fn render_output(&mut self, node: DrmNode, crtc: crtc::Handle) {
        let Self {
            space,
            devices,
            popups,
            display_handle,
            start_time,
            paused,
            ..
        } = self;
        if *paused {
            return;
        }

        let Some(device) = devices.get_mut(&node) else {
            return;
        };
        let Some(surface) = device.surfaces.get_mut(&crtc) else {
            return;
        };
        let output = surface.output.clone();

        let elements = match space.render_elements_for_output(&mut device.renderer, &output, 1.0) {
            Ok(elements) => elements,
            Err(e) => {
                warn!("cannot build render elements for {}: {e}", output.name());
                return;
            }
        };

        let frame = match surface.drm_output.render_frame(
            &mut device.renderer,
            &elements,
            CLEAR_COLOR,
            FrameFlags::DEFAULT,
        ) {
            Ok(frame) => frame,
            Err(e) => {
                warn!("render failed on {}: {e:?}", output.name());
                return;
            }
        };

        if !frame.is_empty {
            if let Err(e) = surface.drm_output.queue_frame(()) {
                warn!("queue_frame failed on {}: {e}", output.name());
                return;
            }
            let now = start_time.elapsed();
            space.elements().for_each(|window| {
                window.send_frame(&output, now, Some(Duration::ZERO), |_, _| {
                    Some(output.clone())
                })
            });
        }

        space.refresh();
        popups.cleanup();
        let _ = display_handle.flush_clients();
    }

    fn surface_under(&self, pos: Point<f64, Logical>) -> Option<(WlSurface, Point<f64, Logical>)> {
        self.space
            .element_under(pos)
            .and_then(|(window, location)| {
                window
                    .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                    .map(|(surface, point)| (surface, (point + location).to_f64()))
            })
    }

    fn unconstrain_popup(&self, popup: &PopupSurface) {
        let Ok(root) = find_popup_root_surface(&PopupKind::Xdg(popup.clone())) else {
            return;
        };
        let Some(window) = self
            .space
            .elements()
            .find(|window| window.toplevel().map(|t| t.wl_surface()) == Some(&root))
        else {
            return;
        };
        let Some(output) = self.space.outputs().next() else {
            return;
        };
        let Some(output_geo) = self.space.output_geometry(output) else {
            return;
        };
        let Some(window_geo) = self.space.element_geometry(window) else {
            return;
        };

        let mut target = output_geo;
        target.loc -= get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()));
        target.loc -= window_geo.loc;

        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }

    fn process_input_event(&mut self, event: InputEvent<LibinputInputBackend>) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time_msec(&event);
                if let Some(keyboard) = self.seat.get_keyboard() {
                    keyboard.input::<(), _>(
                        self,
                        event.key_code(),
                        event.state(),
                        serial,
                        time,
                        |_, _, _| FilterResult::Forward,
                    );
                }
            }
            InputEvent::PointerMotion { event, .. } => {
                let Some(pointer) = self.seat.get_pointer() else {
                    return;
                };
                let mut location = pointer.current_location() + event.delta();
                if let Some(output) = self.space.outputs().next() {
                    if let Some(geo) = self.space.output_geometry(output) {
                        location.x = location.x.clamp(0.0, geo.size.w as f64 - 1.0);
                        location.y = location.y.clamp(0.0, geo.size.h as f64 - 1.0);
                    }
                }
                let under = self.surface_under(location);
                pointer.motion(
                    self,
                    under,
                    &MotionEvent {
                        location,
                        serial: SERIAL_COUNTER.next_serial(),
                        time: event.time_msec(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let Some(pointer) = self.seat.get_pointer() else {
                    return;
                };
                let Some(output) = self.space.outputs().next() else {
                    return;
                };
                let Some(geo) = self.space.output_geometry(output) else {
                    return;
                };
                let location = event.position_transformed(geo.size) + geo.loc.to_f64();
                let under = self.surface_under(location);
                pointer.motion(
                    self,
                    under,
                    &MotionEvent {
                        location,
                        serial: SERIAL_COUNTER.next_serial(),
                        time: event.time_msec(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerButton { event, .. } => {
                let Some(pointer) = self.seat.get_pointer() else {
                    return;
                };
                let serial = SERIAL_COUNTER.next_serial();
                if event.state() == smithay::backend::input::ButtonState::Pressed
                    && !pointer.is_grabbed()
                {
                    if let Some((window, _)) = self
                        .space
                        .element_under(pointer.current_location())
                        .map(|(window, location)| (window.clone(), location))
                    {
                        self.space.raise_element(&window, true);
                        if let Some(toplevel) = window.toplevel() {
                            if let Some(keyboard) = self.seat.get_keyboard() {
                                keyboard.set_focus(
                                    self,
                                    Some(toplevel.wl_surface().clone()),
                                    serial,
                                );
                            }
                            toplevel.send_pending_configure();
                        }
                    } else {
                        for window in self.space.elements() {
                            window.set_activated(false);
                            if let Some(toplevel) = window.toplevel() {
                                toplevel.send_pending_configure();
                            }
                        }
                        if let Some(keyboard) = self.seat.get_keyboard() {
                            keyboard.set_focus(self, Option::<WlSurface>::None, serial);
                        }
                    }
                }
                pointer.button(
                    self,
                    &ButtonEvent {
                        button: event.button_code(),
                        state: event.state(),
                        serial,
                        time: event.time_msec(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerAxis { event, .. } => {
                let Some(pointer) = self.seat.get_pointer() else {
                    return;
                };
                let source = event.source();
                let horizontal = event
                    .amount(smithay::backend::input::Axis::Horizontal)
                    .unwrap_or(0.0);
                let vertical = event
                    .amount(smithay::backend::input::Axis::Vertical)
                    .unwrap_or(0.0);
                let mut frame = AxisFrame::new(event.time_msec()).source(source);
                if horizontal != 0.0 {
                    frame = frame.value(smithay::backend::input::Axis::Horizontal, horizontal);
                }
                if vertical != 0.0 {
                    frame = frame.value(smithay::backend::input::Axis::Vertical, vertical);
                }
                pointer.axis(self, frame);
                pointer.frame(self);
            }
            _ => {}
        }
    }
}

impl CompositorHandler for HitechShell {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client
            .get_data::<ClientState>()
            .expect("client without compositor state")
            .compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        smithay::backend::renderer::utils::on_commit_buffer_handler::<Self>(surface);
        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            if let Some(window) = self
                .space
                .elements()
                .find(|window| window.toplevel().map(|t| t.wl_surface()) == Some(&root))
            {
                window.on_commit();
            }
        }

        if let Some(window) = self
            .space
            .elements()
            .find(|window| window.toplevel().map(|t| t.wl_surface()) == Some(surface))
            .cloned()
        {
            let initial_configure_sent =
                smithay::wayland::compositor::with_states(surface, |states| {
                    states
                        .data_map
                        .get::<XdgToplevelSurfaceData>()
                        .expect("xdg toplevel without data")
                        .lock()
                        .expect("poisoned xdg toplevel data")
                        .initial_configure_sent
                });
            if !initial_configure_sent {
                if let Some(toplevel) = window.toplevel() {
                    let _ = toplevel.send_configure();
                }
            }
        }

        self.popups.commit(surface);
        if let Some(PopupKind::Xdg(popup)) = self.popups.find_popup(surface) {
            if !popup.is_initial_configure_sent() {
                let _ = popup.send_configure();
            }
        }

        self.queue_redraw();
    }
}

impl BufferHandler for HitechShell {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl ShmHandler for HitechShell {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

impl SeatHandler for HitechShell {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<HitechShell> {
        &mut self.seat_state
    }

    fn cursor_image(
        &mut self,
        _seat: &Seat<Self>,
        _image: smithay::input::pointer::CursorImageStatus,
    ) {
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        let client = focused.and_then(|surface| self.display_handle.get_client(surface.id()).ok());
        set_data_device_focus(&self.display_handle, seat, client);
    }
}

impl SelectionHandler for HitechShell {
    type SelectionUserData = ();
}

impl DataDeviceHandler for HitechShell {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl ClientDndGrabHandler for HitechShell {}
impl ServerDndGrabHandler for HitechShell {}
impl OutputHandler for HitechShell {}

impl XdgShellHandler for HitechShell {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let window = Window::new_wayland_window(surface.clone());
        self.space.map_element(window, (0, 0), false);
        let serial = SERIAL_COUNTER.next_serial();
        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.set_focus(self, Some(surface.wl_surface().clone()), serial);
        }
        let _ = surface.send_configure();
        self.queue_redraw();
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        self.unconstrain_popup(&surface);
        let _ = self.popups.track_popup(PopupKind::Xdg(surface));
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        self.unconstrain_popup(&surface);
        surface.send_repositioned(token);
    }

    fn move_request(&mut self, _surface: ToplevelSurface, _seat: wl_seat::WlSeat, _serial: Serial) {
        debug!("toplevel move request (interactive move is handled by the shell layer)");
    }

    fn resize_request(
        &mut self,
        _surface: ToplevelSurface,
        _seat: wl_seat::WlSeat,
        _serial: Serial,
        _edges: smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::ResizeEdge,
    ) {
        debug!("toplevel resize request (interactive resize is handled by the shell layer)");
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {}
}

smithay::delegate_compositor!(HitechShell);
smithay::delegate_shm!(HitechShell);
smithay::delegate_seat!(HitechShell);
smithay::delegate_data_device!(HitechShell);
smithay::delegate_output!(HitechShell);
smithay::delegate_xdg_shell!(HitechShell);

/// Build and run the compositor until the event loop is stopped.
pub fn run_compositor(config: &Config) -> Result<(), CompositorError> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    let mut event_loop: EventLoop<'static, HitechShell> =
        EventLoop::try_new().map_err(|e| CompositorError::Loop(e.to_string()))?;

    let display: Display<HitechShell> =
        Display::new().map_err(|e| CompositorError::Display(e.to_string()))?;

    let (session, notifier) =
        LibSeatSession::new().map_err(|e| CompositorError::Session(e.to_string()))?;

    let mut state = HitechShell::new(config.clone(), display, &mut event_loop, session, notifier)?;
    event_loop
        .run(None, &mut state, |_| {
            // The compositor runs until LoopSignal::stop() is called.
        })
        .map_err(|e| CompositorError::Loop(e.to_string()))?;

    Ok(())
}

/// Public compositor handle used by `main.rs`.
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

    pub fn run(&self) {
        if let Err(e) = run_compositor(&self.config) {
            error!("compositor fatal: {e}");
            std::process::exit(1);
        }
    }
}
