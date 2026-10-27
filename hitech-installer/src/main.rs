//! HiTech-OS // Cybernetic OS Studio v2.0
//!
//! A dependency-free (beyond eframe/egui) graphical NetInstaller for the
//! HiTech-OS live ISO. The installer is intentionally anonymous: the target
//! identity is entirely supplied by the operator, never baked into the binary.
//!
//! Installation model:
//!   1. partition a GPT disk (512 MiB ESP + ext4 root),
//!   2. mount the target under /mnt,
//!   3. generate the hardware configuration with nixos-generate-config,
//!   4. fetch the pinned HiTech-OS profile (when configured by the ISO),
//!   5. write an identity-only configuration.nix,
//!   6. run nixos-install, preferring the HiTech-Store binary cache.
//!
//! Every privileged operation is executed through `sudo`. The live ISO grants
//! the `nixos` wheel user passwordless sudo; this is scoped to the ephemeral
//! medium and is not copied into the target configuration.

use std::{
    collections::BTreeMap,
    env, fs,
    io::{BufRead, BufReader, Read},
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use eframe::egui::{
    self, Align, Color32, FontFamily, FontId, Frame, Layout, Margin, RichText, Rounding,
    ScrollArea, Stroke, TextStyle,
};

// ---------------------------------------------------------------------------
// Cyberpunk palette
// ---------------------------------------------------------------------------

const BG: Color32 = Color32::from_rgb(0x0B, 0x0F, 0x19);
const BG_DEEP: Color32 = Color32::from_rgb(0x05, 0x08, 0x10);
const CARD: Color32 = Color32::from_rgb(0x10, 0x16, 0x24);
const CARD_HOVER: Color32 = Color32::from_rgb(0x16, 0x1F, 0x31);
const CYAN: Color32 = Color32::from_rgb(0x00, 0xE6, 0xFF);
const CYAN_DIM: Color32 = Color32::from_rgb(0x00, 0x6B, 0x7A);
const PURPLE: Color32 = Color32::from_rgb(0xBE, 0x78, 0xFF);
const PURPLE_DIM: Color32 = Color32::from_rgb(0x4A, 0x2D, 0x66);
const TEXT: Color32 = Color32::from_rgb(0xE6, 0xF4, 0xFF);
const MUTED: Color32 = Color32::from_rgb(0x7D, 0x8C, 0xA6);
const GREEN: Color32 = Color32::from_rgb(0x38, 0xE8, 0xA0);
const RED: Color32 = Color32::from_rgb(0xFF, 0x4D, 0x6A);
const AMBER: Color32 = Color32::from_rgb(0xFF, 0xC2, 0x45);

// ---------------------------------------------------------------------------
// Cinematic storyboard
// ---------------------------------------------------------------------------

struct Slide {
    chapter: &'static str,
    title: &'static str,
    body: &'static str,
}

const CINEMATIC_SLIDES: [Slide; 4] = [
    Slide {
        chapter: "CHAPTER 01 // GENESIS",
        title: "Zero-Bloat Genesis",
        body: "HiTech-OS is born from a simple refusal: an operating system should \
               serve the operator, not the vendor. No telemetry, no bundled bloat, \
               no hidden accounts. The live medium is a clean room; the installed \
               system is assembled from the choices you make here.",
    },
    Slide {
        chapter: "CHAPTER 02 // FORTRESS",
        title: "NixOS Immutable Fortress",
        body: "Every generation is atomic. Every upgrade is reversible. The Nix store \
               keeps the old system alive until you decide to garbage-collect it, so \
               a failed update is never a dead machine. Roll back from the boot menu \
               and keep working.",
    },
    Slide {
        chapter: "CHAPTER 03 // COGNITION",
        title: "AI-Native Ecosystem",
        body: "Built for local inference: Genio, AgentOS and on-device LLMs are first-class \
               citizens. Your prompts, documents and model weights stay on your hardware. \
               GPU acceleration is a configuration choice, not a cloud subscription.",
    },
    Slide {
        chapter: "CHAPTER 04 // IGNITION",
        title: "Ready for Ignition",
        body: "The target disk is prepared, the identity is yours, and the closure is \
               waiting in the HiTech-Store cache. When the progress bar reaches 100%, \
               remove the live medium and boot into a system that belongs to you.",
    },
];

// ---------------------------------------------------------------------------
// Hardware / runtime data
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct DiskInfo {
    name: String,
    size: String,
    model: String,
}

impl DiskInfo {
    fn label(&self) -> String {
        format!("/dev/{} ({} - {})", self.name, self.size, self.model)
    }
}

enum InstallerMessage {
    Log(String),
    Progress(u16, String),
    Finished(bool),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Welcome,
    Configuration,
    Installing,
    Done(bool),
}

// ---------------------------------------------------------------------------
// Application
// ---------------------------------------------------------------------------

struct App {
    screen: Screen,
    slide_index: usize,
    last_slide_tick: Instant,

    disks: Vec<DiskInfo>,
    selected_disk: Option<usize>,

    ssids: Vec<String>,
    selected_ssid: Option<usize>,
    wifi_password: String,
    wifi_status: String,

    hostname: String,
    username: String,
    password: String,
    confirm_password: String,
    show_password: bool,
    validation_errors: Vec<String>,

    logs: Vec<String>,
    progress: u16,
    status_text: String,
    rx: Option<mpsc::Receiver<InstallerMessage>>,

    network_online: bool,
    last_network_check: Instant,
    notice: Option<String>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_style(&cc.egui_ctx);

        let mut app = Self {
            screen: Screen::Welcome,
            slide_index: 0,
            last_slide_tick: Instant::now(),
            disks: Vec::new(),
            selected_disk: None,
            ssids: Vec::new(),
            selected_ssid: None,
            wifi_password: String::new(),
            wifi_status: String::new(),
            hostname: "hitech-os".to_string(),
            username: String::new(),
            password: String::new(),
            confirm_password: String::new(),
            show_password: false,
            validation_errors: Vec::new(),
            logs: Vec::new(),
            progress: 0,
            status_text: "Waiting for operator input.".to_string(),
            rx: None,
            network_online: false,
            last_network_check: Instant::now(),
            notice: None,
        };

        app.refresh_disks();
        app.network_online = check_network();
        app
    }

    fn refresh_disks(&mut self) {
        self.disks = scan_disks();
        if let Some(index) = self.selected_disk {
            if index >= self.disks.len() {
                self.selected_disk = None;
            }
        }
    }

    fn scan_wifi(&mut self) {
        self.ssids = scan_wifi_networks();
        self.selected_ssid = None;
        if self.ssids.is_empty() {
            self.wifi_status = "No Wi-Fi network detected.".to_string();
        } else {
            self.wifi_status = format!("{} network(s) detected.", self.ssids.len());
        }
    }

    fn connect_wifi(&mut self) {
        let Some(index) = self.selected_ssid else {
            self.wifi_status = "Select a Wi-Fi network first.".to_string();
            return;
        };
        let Some(ssid) = self.ssids.get(index).cloned() else {
            self.wifi_status = "Invalid Wi-Fi selection.".to_string();
            return;
        };

        let mut command = Command::new("nmcli");
        command.args(["dev", "wifi", "connect", &ssid]);
        if !self.wifi_password.is_empty() {
            command.args(["password", &self.wifi_password]);
        }

        match command.output() {
            Ok(output) if output.status.success() => {
                self.wifi_status = format!("Connected to {ssid}.");
                self.network_online = check_network();
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                self.wifi_status = if stderr.is_empty() {
                    format!("Could not connect to {ssid}.")
                } else {
                    format!("Wi-Fi error: {stderr}")
                };
            }
            Err(error) => {
                self.wifi_status = format!("nmcli could not be launched: {error}");
            }
        }
    }

    fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        if self.selected_disk.is_none() {
            errors.push("Select a target disk.".to_string());
        }

        let hostname = self.hostname.trim();
        if hostname.is_empty() {
            errors.push("Hostname is required.".to_string());
        } else if !Self::valid_hostname(hostname) {
            errors.push("Hostname must be 1-63 letters, digits or hyphens.".to_string());
        }

        let username = self.username.trim();
        if username.is_empty() {
            errors.push("Username is required.".to_string());
        } else if !Self::valid_username(username) {
            errors.push(
                "Username must start with a-z/_ and contain only a-z, 0-9, _ or -.".to_string(),
            );
        } else if matches!(username, "root" | "hitechos") {
            errors.push("That username is reserved by the target profile.".to_string());
        }

        if self.password.len() < 4 {
            errors.push("Password must contain at least 4 characters.".to_string());
        }
        if self.password != self.confirm_password {
            errors.push("Passwords do not match.".to_string());
        }

        errors
    }

    fn valid_hostname(value: &str) -> bool {
        !value.is_empty()
            && value.len() <= 63
            && !value.starts_with('-')
            && !value.ends_with('-')
            && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    }

    fn valid_username(value: &str) -> bool {
        let mut chars = value.chars();
        match chars.next() {
            Some(c) if c.is_ascii_lowercase() || c == '_' => {}
            _ => return false,
        }
        value.len() <= 32
            && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    }

    fn try_start_installation(&mut self) {
        let errors = self.validate();
        if !errors.is_empty() {
            self.validation_errors = errors;
            return;
        }

        let Some(index) = self.selected_disk else {
            self.validation_errors = vec!["Select a target disk.".to_string()];
            return;
        };
        let Some(disk_info) = self.disks.get(index).cloned() else {
            self.validation_errors =
                vec!["The selected disk disappeared. Refresh the list.".to_string()];
            return;
        };

        let disk = format!("/dev/{}", disk_info.name);
        let hostname = self.hostname.trim().to_string();
        let username = self.username.trim().to_string();
        let password = self.password.clone();

        self.validation_errors.clear();
        self.logs.clear();
        self.progress = 0;
        self.status_text = "Preparing installation pipeline...".to_string();
        self.screen = Screen::Installing;
        self.last_slide_tick = Instant::now();

        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        thread::spawn(move || run_install_pipeline(disk, hostname, username, password, tx));
    }

    fn drain_messages(&mut self) {
        let mut messages = Vec::new();
        if let Some(rx) = &self.rx {
            while let Ok(message) = rx.try_recv() {
                messages.push(message);
            }
        }

        let mut finished = false;
        for message in messages {
            match message {
                InstallerMessage::Log(line) => {
                    self.logs.push(line);
                    if self.logs.len() > 2_000 {
                        self.logs.drain(0..500);
                    }
                }
                InstallerMessage::Progress(percent, status) => {
                    self.progress = percent.min(100);
                    self.status_text = status;
                }
                InstallerMessage::Finished(success) => {
                    self.progress = 100;
                    self.screen = Screen::Done(success);
                    finished = true;
                }
            }
        }

        if finished {
            self.rx = None;
        }
    }

    fn terminal_notice(&mut self) {
        self.notice = match spawn_live_terminal() {
            Ok(()) => None,
            Err(error) => Some(error),
        };
    }

    // -- Header -------------------------------------------------------------

    fn ui_header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("hitechos_header")
            .frame(
                Frame::none()
                    .fill(BG_DEEP)
                    .inner_margin(Margin::symmetric(20.0, 12.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("HITECH-OS // CYBERNETIC OS STUDIO v2.0")
                            .color(CYAN)
                            .strong()
                            .size(19.0),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Live Terminal").color(BG).strong(),
                                )
                                .fill(CYAN)
                                .rounding(Rounding::same(8.0))
                                .stroke(Stroke::NONE),
                            )
                            .clicked()
                        {
                            self.terminal_notice();
                        }

                        let (dot, label, color) = if self.network_online {
                            ("●", "ONLINE", GREEN)
                        } else {
                            ("●", "OFFLINE", AMBER)
                        };
                        ui.label(RichText::new(dot).color(color).size(14.0));
                        ui.label(RichText::new(label).color(color).strong().size(12.0));
                        ui.label(RichText::new("NETWORK").color(MUTED).size(11.0));
                    });
                });

                if let Some(notice) = &self.notice {
                    ui.add_space(4.0);
                    ui.label(RichText::new(notice).color(RED).size(12.0));
                }

                ui.add_space(6.0);
                ui.add(egui::Separator::default().spacing(1.0));
            });
    }

    // -- Welcome ------------------------------------------------------------

    fn ui_welcome(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(Frame::none().fill(BG).inner_margin(Margin::same(24.0)))
            .show(ctx, |ui| {
                ui.columns(2, |columns| {
                    card().show(&mut columns[0], |ui| {
                        ui.label(
                            RichText::new("WELCOME // BIENVENUE")
                                .color(PURPLE)
                                .strong()
                                .size(12.0),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("Install HiTech-OS")
                                .color(TEXT)
                                .strong()
                                .size(32.0),
                        );
                        ui.label(
                            RichText::new(
                                "An anonymous, operator-owned system. No account, no telemetry, no vendor cloud.",
                            )
                            .color(MUTED)
                            .size(15.0),
                        );

                        ui.add_space(20.0);
                        for benefit in [
                            "Atomic NixOS generations with boot-menu rollback",
                            "Zero-bloat live medium; only the tools needed to install",
                            "Local-first AI ecosystem on your own GPU",
                            "Your identity is created here, never pre-seeded",
                        ] {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("▸").color(CYAN).strong());
                                ui.label(RichText::new(benefit).color(TEXT).size(14.0));
                            });
                            ui.add_space(4.0);
                        }

                        ui.add_space(24.0);
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Commencer l'installation")
                                        .color(BG)
                                        .strong(),
                                )
                                .fill(CYAN)
                                .rounding(Rounding::same(8.0))
                                .stroke(Stroke::NONE)
                                .min_size(egui::vec2(260.0, 44.0)),
                            )
                            .clicked()
                        {
                            self.screen = Screen::Configuration;
                            self.refresh_disks();
                        }

                        ui.add_space(10.0);
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Explorer le Systeme Live (Terminal)")
                                        .color(CYAN),
                                )
                                .fill(Color32::TRANSPARENT)
                                .rounding(Rounding::same(8.0))
                                .stroke(Stroke::new(1.0_f32, CYAN_DIM))
                                .min_size(egui::vec2(260.0, 40.0)),
                            )
                            .clicked()
                        {
                            self.terminal_notice();
                        }

                        ui.add_space(18.0);
                        ui.label(
                            RichText::new(
                                "The live medium is read-only. Nothing is written until you start the installation.",
                            )
                            .color(MUTED)
                            .size(12.0),
                        );
                    });

                    card().show(&mut columns[1], |ui| {
                        self.storyboard(ui, true);
                    });
                });
            });
    }

    fn storyboard(&mut self, ui: &mut egui::Ui, controls: bool) {
        let slide = &CINEMATIC_SLIDES[self.slide_index];

        ui.label(
            RichText::new(slide.chapter)
                .color(PURPLE)
                .strong()
                .size(12.0),
        );
        ui.add_space(10.0);
        ui.label(RichText::new(slide.title).color(CYAN).strong().size(26.0));
        ui.add_space(14.0);
        ui.add(egui::Label::new(RichText::new(slide.body).color(TEXT).size(15.0)).wrap());

        ui.add_space(18.0);
        ui.label(
            RichText::new(format!(
                "{:02} / {:02}",
                self.slide_index + 1,
                CINEMATIC_SLIDES.len()
            ))
            .color(MUTED)
            .monospace()
            .size(12.0),
        );

        if controls {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(RichText::new("Precedent").color(CYAN))
                            .fill(Color32::TRANSPARENT)
                            .rounding(Rounding::same(8.0))
                            .stroke(Stroke::new(1.0_f32, CYAN_DIM)),
                    )
                    .clicked()
                {
                    self.slide_index = self
                        .slide_index
                        .checked_sub(1)
                        .unwrap_or(CINEMATIC_SLIDES.len() - 1);
                    self.last_slide_tick = Instant::now();
                }

                if ui
                    .add(
                        egui::Button::new(RichText::new("Suivant").color(BG).strong())
                            .fill(PURPLE)
                            .rounding(Rounding::same(8.0))
                            .stroke(Stroke::NONE),
                    )
                    .clicked()
                {
                    self.slide_index = (self.slide_index + 1) % CINEMATIC_SLIDES.len();
                    self.last_slide_tick = Instant::now();
                }
            });
        }
    }

    // -- Configuration ------------------------------------------------------

    fn ui_configuration(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(Frame::none().fill(BG).inner_margin(Margin::same(24.0)))
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("CONFIGURATION // ANONYMOUS IDENTITY")
                        .color(PURPLE)
                        .strong()
                        .size(12.0),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Prepare the target")
                        .color(TEXT)
                        .strong()
                        .size(28.0),
                );
                ui.label(
                    RichText::new(
                        "Network, target disk and account are chosen locally. No value is pre-filled from the live system.",
                    )
                    .color(MUTED)
                    .size(14.0),
                );
                ui.add_space(16.0);

                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.columns(2, |columns| {
                            card().show(&mut columns[0], |ui| {
                                ui.label(
                                    RichText::new("01 // WI-FI MANAGER")
                                        .color(CYAN)
                                        .strong()
                                        .size(13.0),
                                );
                                ui.add_space(8.0);
                                ui.horizontal(|ui| {
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                RichText::new("Scan Wi-Fi").color(BG).strong(),
                                            )
                                            .fill(CYAN)
                                            .rounding(Rounding::same(8.0))
                                            .stroke(Stroke::NONE),
                                        )
                                        .clicked()
                                    {
                                        self.scan_wifi();
                                    }
                                    if !self.wifi_status.is_empty() {
                                        ui.label(
                                            RichText::new(&self.wifi_status)
                                                .color(MUTED)
                                                .size(12.0),
                                        );
                                    }
                                });

                                ui.add_space(8.0);
                                let selected_ssid = self
                                    .selected_ssid
                                    .and_then(|index| self.ssids.get(index))
                                    .cloned()
                                    .unwrap_or_else(|| "Select a network".to_string());
                                egui::ComboBox::from_id_source("wifi_ssid")
                                    .selected_text(selected_ssid)
                                    .width(ui.available_width())
                                    .show_ui(ui, |ui| {
                                        for (index, ssid) in self.ssids.iter().enumerate() {
                                            ui.selectable_value(
                                                &mut self.selected_ssid,
                                                Some(index),
                                                ssid.as_str(),
                                            );
                                        }
                                    });

                                ui.add_space(8.0);
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.wifi_password)
                                        .password(true)
                                        .hint_text("Wi-Fi password (leave empty for open networks)")
                                        .desired_width(f32::INFINITY),
                                );
                                ui.add_space(8.0);
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new("Se connecter au Wi-Fi")
                                                .color(CYAN),
                                        )
                                        .fill(Color32::TRANSPARENT)
                                        .rounding(Rounding::same(8.0))
                                        .stroke(Stroke::new(1.0_f32, CYAN_DIM))
                                        .min_size(egui::vec2(220.0, 36.0)),
                                    )
                                    .clicked()
                                {
                                    self.connect_wifi();
                                }

                                ui.add_space(18.0);
                                ui.add(egui::Separator::default().spacing(1.0));
                                ui.add_space(14.0);

                                ui.label(
                                    RichText::new("02 // TARGET DISK")
                                        .color(CYAN)
                                        .strong()
                                        .size(13.0),
                                );
                                ui.add_space(8.0);

                                if self.disks.is_empty() {
                                    ui.label(
                                        RichText::new(
                                            "No installable disk detected. Check the live medium and refresh.",
                                        )
                                        .color(AMBER)
                                        .size(12.0),
                                    );
                                } else {
                                    for (index, disk) in self.disks.iter().enumerate() {
                                        ui.radio_value(
                                            &mut self.selected_disk,
                                            Some(index),
                                            disk.label(),
                                        );
                                    }
                                }

                                ui.add_space(8.0);
                                if ui
                                    .add(
                                        egui::Button::new(RichText::new("Rafraichir").color(CYAN))
                                            .fill(Color32::TRANSPARENT)
                                            .rounding(Rounding::same(8.0))
                                            .stroke(Stroke::new(1.0_f32, CYAN_DIM)),
                                    )
                                    .clicked()
                                {
                                    self.refresh_disks();
                                }

                                ui.add_space(12.0);
                                ui.label(
                                    RichText::new(
                                        "WARNING: the selected disk is fully erased (GPT, ESP + ext4).",
                                    )
                                    .color(RED)
                                    .size(12.0),
                                );
                            });

                            card().show(&mut columns[1], |ui| {
                                ui.label(
                                    RichText::new("03 // OPERATOR IDENTITY")
                                        .color(CYAN)
                                        .strong()
                                        .size(13.0),
                                );
                                ui.add_space(8.0);
                                ui.label(
                                    RichText::new("No default account. Choose your own.")
                                        .color(MUTED)
                                        .size(12.0),
                                );
                                ui.add_space(10.0);

                                ui.label(RichText::new("Hostname").color(MUTED).size(12.0));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.hostname)
                                        .hint_text("hitech-os")
                                        .desired_width(f32::INFINITY),
                                );

                                ui.add_space(8.0);
                                ui.label(RichText::new("Username").color(MUTED).size(12.0));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.username)
                                        .hint_text("operator")
                                        .desired_width(f32::INFINITY),
                                );

                                ui.add_space(8.0);
                                ui.label(RichText::new("Password").color(MUTED).size(12.0));
                                let show_password = self.show_password;
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.password)
                                        .password(!show_password)
                                        .desired_width(f32::INFINITY),
                                );

                                ui.add_space(8.0);
                                ui.label(
                                    RichText::new("Confirm Password").color(MUTED).size(12.0),
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.confirm_password)
                                        .password(!show_password)
                                        .desired_width(f32::INFINITY),
                                );

                                ui.add_space(6.0);
                                ui.checkbox(&mut self.show_password, "Show Password");

                                ui.add_space(10.0);
                                for error in &self.validation_errors {
                                    ui.label(RichText::new(error).color(RED).size(12.0));
                                }

                                ui.add_space(12.0);
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new("Start Installation")
                                                .color(BG)
                                                .strong(),
                                        )
                                        .fill(GREEN)
                                        .rounding(Rounding::same(8.0))
                                        .stroke(Stroke::NONE)
                                        .min_size(egui::vec2(220.0, 44.0)),
                                    )
                                    .clicked()
                                {
                                    self.try_start_installation();
                                }

                                ui.add_space(8.0);
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new("Retour").color(CYAN),
                                        )
                                        .fill(Color32::TRANSPARENT)
                                        .rounding(Rounding::same(8.0))
                                        .stroke(Stroke::new(1.0_f32, CYAN_DIM)),
                                    )
                                    .clicked()
                                {
                                    self.screen = Screen::Welcome;
                                }
                            });
                        });
                    });
            });
    }

    // -- Installing / Done --------------------------------------------------

    fn ui_installation(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(Frame::none().fill(BG).inner_margin(Margin::same(24.0)))
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(match self.screen {
                        Screen::Done(true) => "MISSION COMPLETE // SYSTEM READY",
                        Screen::Done(false) => "MISSION ABORTED // CHECK TELEMETRY",
                        _ => "INSTALLATION IN PROGRESS // DO NOT POWER OFF",
                    })
                    .color(match self.screen {
                        Screen::Done(true) => GREEN,
                        Screen::Done(false) => RED,
                        _ => CYAN,
                    })
                    .strong()
                    .size(13.0),
                );
                ui.add_space(12.0);

                ui.columns(2, |columns| {
                    card().show(&mut columns[0], |ui| {
                        ui.label(
                            RichText::new("CINEMATIC FEED")
                                .color(PURPLE)
                                .strong()
                                .size(12.0),
                        );
                        ui.add_space(8.0);
                        self.storyboard(ui, false);
                        ui.add_space(18.0);
                        ui.add(egui::Separator::default().spacing(1.0));
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new(
                                "The storyboard rotates every 8 seconds while the target is assembled.",
                            )
                            .color(MUTED)
                            .size(12.0),
                        );
                    });

                    card().show(&mut columns[1], |ui| {
                        ui.label(
                            RichText::new("INSTALLATION TELEMETRY")
                                .color(CYAN)
                                .strong()
                                .size(12.0),
                        );
                        ui.add_space(10.0);
                        ui.add(
                            egui::ProgressBar::new(self.progress as f32 / 100.0)
                                .text(format!("{}% // {}", self.progress, self.status_text))
                                .animate(self.screen == Screen::Installing),
                        );
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new(&self.status_text).color(TEXT).size(14.0),
                        );

                        ui.add_space(12.0);
                        ui.add(egui::Separator::default().spacing(1.0));
                        ui.add_space(8.0);
                        ui.label(RichText::new("LIVE KERNEL / INSTALL LOG").color(MUTED).size(11.0));
                        ui.add_space(4.0);

                        ScrollArea::vertical()
                            .stick_to_bottom(true)
                            .auto_shrink([false, false])
                            .max_height(320.0)
                            .show(ui, |ui| {
                                if self.logs.is_empty() {
                                    ui.label(
                                        RichText::new("Waiting for the first pipeline event...")
                                            .color(MUTED)
                                            .monospace()
                                            .size(12.0),
                                    );
                                }
                                for line in &self.logs {
                                    let color = if line.contains("ERROR")
                                        || line.to_lowercase().contains("error:")
                                    {
                                        RED
                                    } else if line.starts_with(">>>") {
                                        CYAN
                                    } else {
                                        TEXT
                                    };
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(line)
                                                .color(color)
                                                .monospace()
                                                .size(11.0),
                                        )
                                        .wrap(),
                                    );
                                }
                            });

                        if matches!(self.screen, Screen::Done(true)) {
                            ui.add_space(14.0);
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("Redemarrer maintenant (Reboot)")
                                            .color(BG)
                                            .strong(),
                                    )
                                    .fill(GREEN)
                                    .rounding(Rounding::same(8.0))
                                    .stroke(Stroke::NONE)
                                    .min_size(egui::vec2(260.0, 44.0)),
                                )
                                .clicked()
                            {
                                let _ = Command::new("reboot").status();
                            }
                        }

                        if matches!(self.screen, Screen::Done(false)) {
                            ui.add_space(14.0);
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("Retour a la configuration")
                                            .color(CYAN),
                                    )
                                    .fill(Color32::TRANSPARENT)
                                    .rounding(Rounding::same(8.0))
                                    .stroke(Stroke::new(1.0_f32, CYAN_DIM)),
                                )
                                .clicked()
                            {
                                self.screen = Screen::Configuration;
                                self.validation_errors.clear();
                                self.refresh_disks();
                            }
                        }
                    });
                });
            });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_messages();

        if matches!(self.screen, Screen::Installing | Screen::Done(_))
            && self.last_slide_tick.elapsed() >= Duration::from_secs(8)
        {
            self.slide_index = (self.slide_index + 1) % CINEMATIC_SLIDES.len();
            self.last_slide_tick = Instant::now();
        }

        if self.last_network_check.elapsed() >= Duration::from_secs(3) {
            self.network_online = check_network();
            self.last_network_check = Instant::now();
        }

        if matches!(self.screen, Screen::Installing) {
            ctx.request_repaint_after(Duration::from_millis(120));
        }

        self.ui_header(ctx);
        match self.screen {
            Screen::Welcome => self.ui_welcome(ctx),
            Screen::Configuration => self.ui_configuration(ctx),
            Screen::Installing | Screen::Done(_) => self.ui_installation(ctx),
        }
    }
}

// ---------------------------------------------------------------------------
// Styling
// ---------------------------------------------------------------------------

fn cyber_visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::dark();

    visuals.panel_fill = BG;
    visuals.window_fill = BG;
    visuals.extreme_bg_color = BG_DEEP;
    visuals.faint_bg_color = CARD;
    visuals.code_bg_color = BG_DEEP;
    visuals.override_text_color = Some(TEXT);
    visuals.hyperlink_color = CYAN;
    visuals.warn_fg_color = AMBER;
    visuals.error_fg_color = RED;
    visuals.window_rounding = Rounding::same(8.0);
    visuals.menu_rounding = Rounding::same(8.0);
    visuals.window_shadow = egui::epaint::Shadow {
        offset: egui::vec2(0.0, 8.0),
        blur: 18.0,
        spread: 0.0,
        color: Color32::from_black_alpha(120),
    };
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: egui::vec2(0.0, 6.0),
        blur: 12.0,
        spread: 0.0,
        color: Color32::from_black_alpha(120),
    };
    visuals.selection.bg_fill = PURPLE_DIM;
    visuals.selection.stroke = Stroke::new(1.0_f32, CYAN);

    visuals.widgets.noninteractive.bg_fill = BG;
    visuals.widgets.noninteractive.weak_bg_fill = BG;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, CYAN_DIM);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.widgets.noninteractive.rounding = Rounding::same(8.0);

    visuals.widgets.inactive.bg_fill = CARD;
    visuals.widgets.inactive.weak_bg_fill = CARD;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, PURPLE_DIM);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.widgets.inactive.rounding = Rounding::same(8.0);

    visuals.widgets.hovered.bg_fill = CARD_HOVER;
    visuals.widgets.hovered.weak_bg_fill = CARD_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, CYAN);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, CYAN);
    visuals.widgets.hovered.rounding = Rounding::same(8.0);

    visuals.widgets.active.bg_fill = CYAN;
    visuals.widgets.active.weak_bg_fill = CYAN;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, CYAN);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, BG);
    visuals.widgets.active.rounding = Rounding::same(8.0);

    visuals.widgets.open.bg_fill = CARD_HOVER;
    visuals.widgets.open.weak_bg_fill = CARD_HOVER;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, CYAN);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, CYAN);
    visuals.widgets.open.rounding = Rounding::same(8.0);

    visuals
}

fn configure_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 8.0);
    style.spacing.interact_size = egui::vec2(44.0, 30.0);
    style.visuals = cyber_visuals();

    let mut text_styles = BTreeMap::new();
    text_styles.insert(
        TextStyle::Heading,
        FontId::new(26.0, FontFamily::Proportional),
    );
    text_styles.insert(TextStyle::Body, FontId::new(15.0, FontFamily::Proportional));
    text_styles.insert(
        TextStyle::Button,
        FontId::new(15.0, FontFamily::Proportional),
    );
    text_styles.insert(
        TextStyle::Small,
        FontId::new(12.0, FontFamily::Proportional),
    );
    text_styles.insert(
        TextStyle::Monospace,
        FontId::new(12.0, FontFamily::Monospace),
    );
    style.text_styles = text_styles;

    ctx.set_style(style);
}

fn card() -> Frame {
    Frame::none()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, PURPLE_DIM))
        .rounding(Rounding::same(8.0))
        .inner_margin(Margin::same(18.0))
}

// ---------------------------------------------------------------------------
// Host probing
// ---------------------------------------------------------------------------

fn scan_disks() -> Vec<DiskInfo> {
    let mut disks = Vec::new();
    let Ok(output) = Command::new("lsblk")
        .args(["-d", "-n", "-o", "NAME,SIZE,MODEL,TYPE"])
        .output()
    else {
        return disks;
    };

    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let name = parts[0];
        let dev_type = parts.last().copied().unwrap_or_default();
        if dev_type != "disk"
            || name.starts_with("loop")
            || name.starts_with("sr")
            || name.starts_with("zram")
        {
            continue;
        }

        let size = parts[1].to_string();
        let model = if parts.len() > 3 {
            parts[2..parts.len() - 1].join(" ")
        } else {
            "Generic Drive".to_string()
        };

        disks.push(DiskInfo {
            name: name.to_string(),
            size,
            model,
        });
    }

    disks
}

fn scan_wifi_networks() -> Vec<String> {
    let Ok(output) = Command::new("nmcli")
        .args(["-t", "-f", "SSID", "dev", "wifi", "list"])
        .output()
    else {
        return Vec::new();
    };

    let mut networks: Vec<String> = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let ssid = line.replace("\\:", ":").trim().to_string();
        if !ssid.is_empty() && !networks.contains(&ssid) {
            networks.push(ssid);
        }
    }
    networks
}

fn check_network() -> bool {
    Command::new("ip")
        .args(["route", "get", "1.1.1.1"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn spawn_live_terminal() -> Result<(), String> {
    const TERMINALS: [&str; 6] = [
        "foot",
        "konsole",
        "xfce4-terminal",
        "alacritty",
        "kitty",
        "xterm",
    ];

    for terminal in TERMINALS {
        if find_in_path(terminal).is_none() {
            continue;
        }
        match Command::new(terminal).spawn() {
            Ok(_) => return Ok(()),
            Err(error) => {
                return Err(format!("Could not launch {terminal}: {error}"));
            }
        }
    }

    Err(
        "No terminal emulator found (foot, konsole, xfce4-terminal, alacritty, kitty, xterm)."
            .to_string(),
    )
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|directory| directory.join(name))
            .find(|candidate| candidate.is_file())
    })
}

// ---------------------------------------------------------------------------
// Installation pipeline
// ---------------------------------------------------------------------------

fn run_install_pipeline(
    disk: String,
    hostname: String,
    username: String,
    password: String,
    tx: mpsc::Sender<InstallerMessage>,
) {
    let result = install_steps(&disk, &hostname, &username, &password, &tx);

    match result {
        Ok(()) => {
            let _ = tx.send(InstallerMessage::Progress(
                100,
                "Installation complete. Remove the live medium and reboot.".to_string(),
            ));
            let _ = tx.send(InstallerMessage::Finished(true));
        }
        Err(error) => {
            let _ = tx.send(InstallerMessage::Log(format!("ERROR: {error}")));
            let _ = tx.send(InstallerMessage::Progress(
                100,
                "Installation failed. Review the live log.".to_string(),
            ));
            let _ = tx.send(InstallerMessage::Finished(false));
        }
    }
}

fn install_steps(
    disk: &str,
    hostname: &str,
    username: &str,
    password: &str,
    tx: &mpsc::Sender<InstallerMessage>,
) -> Result<(), String> {
    let (boot_partition, root_partition) = partition_names(disk);

    send_progress(tx, 2, "Unmounting any previous /mnt target...");
    run_step(tx, "umount", &arg_list(&["-R", "/mnt"]), true, &[])?;
    run_step(tx, "umount", &arg_list(&["-l", "/mnt/boot"]), true, &[])?;

    send_progress(tx, 6, &format!("Wiping signatures on {disk}..."));
    run_step(tx, "wipefs", &arg_list(&["-af", disk]), false, &[])?;

    send_progress(tx, 12, &format!("Partitioning {disk} (GPT / UEFI)..."));
    run_step(
        tx,
        "parted",
        &arg_list(&["-s", disk, "--", "mklabel", "gpt"]),
        false,
        &[],
    )?;
    run_step(
        tx,
        "parted",
        &arg_list(&["-s", disk, "--", "mkpart", "ESP", "fat32", "1MiB", "512MiB"]),
        false,
        &[],
    )?;
    run_step(
        tx,
        "parted",
        &arg_list(&["-s", disk, "--", "set", "1", "esp", "on"]),
        false,
        &[],
    )?;
    run_step(
        tx,
        "parted",
        &arg_list(&[
            "-s", disk, "--", "mkpart", "primary", "ext4", "512MiB", "100%",
        ]),
        false,
        &[],
    )?;
    run_step(tx, "partprobe", &arg_list(&[disk]), true, &[])?;
    run_step(tx, "udevadm", &arg_list(&["settle"]), true, &[])?;
    thread::sleep(Duration::from_secs(2));

    send_progress(tx, 22, "Formatting ESP and root filesystems...");
    run_step(
        tx,
        "mkfs.fat",
        &arg_list(&["-F", "32", "-n", "BOOT", boot_partition.as_str()]),
        false,
        &[],
    )?;
    run_step(
        tx,
        "mkfs.ext4",
        &arg_list(&["-F", "-L", "nixos", root_partition.as_str()]),
        false,
        &[],
    )?;
    run_step(tx, "udevadm", &arg_list(&["settle"]), true, &[])?;

    send_progress(tx, 30, "Mounting the target under /mnt...");
    run_step(
        tx,
        "mount",
        &arg_list(&[root_partition.as_str(), "/mnt"]),
        false,
        &[],
    )?;
    run_step(tx, "mkdir", &arg_list(&["-p", "/mnt/boot"]), false, &[])?;
    run_step(
        tx,
        "mount",
        &arg_list(&[boot_partition.as_str(), "/mnt/boot"]),
        false,
        &[],
    )?;

    send_progress(tx, 38, "Generating the hardware configuration...");
    run_step(tx, "rm", &arg_list(&["-rf", "/mnt/etc/nixos"]), true, &[])?;
    run_step(
        tx,
        "mkdir",
        &arg_list(&["-p", "/mnt/etc/nixos"]),
        false,
        &[],
    )?;
    run_step(
        tx,
        "nixos-generate-config",
        &arg_list(&["--root", "/mnt"]),
        false,
        &[],
    )?;

    send_progress(tx, 43, "Fetching the HiTech-OS profile...");
    let repo_url = env::var("HITECH_REPO_URL")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let repo_ref = env::var("HITECH_REPO_REF")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "main".to_string());
    let mut imports = String::from("./hardware-configuration.nix");
    if let Some(repo_url) = repo_url.as_deref() {
        fetch_hitechos_profile(tx, repo_url, &repo_ref)?;
        imports.push_str(" ./hitech-os/os/installation.nix");
    }

    send_progress(tx, 46, "Writing the anonymous identity configuration...");
    let state_version = env::var("HITECH_STATE_VERSION").unwrap_or_else(|_| "24.05".to_string());
    let configuration = build_configuration(
        hostname,
        username,
        password,
        &state_version,
        &imports,
        repo_url.is_none(),
    );
    let temporary = env::temp_dir().join("hitechos-configuration.nix");
    fs::write(&temporary, configuration)
        .map_err(|error| format!("could not write temporary configuration: {error}"))?;
    let _ = fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600));
    let temporary_display = temporary.display().to_string();
    run_step(
        tx,
        "install",
        &arg_list(&[
            "-m",
            "0600",
            temporary_display.as_str(),
            "/mnt/etc/nixos/configuration.nix",
        ]),
        false,
        &[],
    )?;
    let _ = fs::remove_file(&temporary);

    send_progress(tx, 55, "Installing the NixOS closure...");
    let mut install_args = vec![
        "--root".to_string(),
        "/mnt".to_string(),
        "--no-root-passwd".to_string(),
    ];

    if let Some(cache_url) = env::var("HITECH_CACHE_URL")
        .ok()
        .filter(|value| !value.is_empty())
    {
        install_args.extend([
            "--option".to_string(),
            "substituters".to_string(),
            format!("{cache_url} https://cache.nixos.org"),
        ]);
    }
    if let Some(cache_key) = env::var("HITECH_CACHE_KEY")
        .ok()
        .filter(|value| !value.is_empty())
    {
        install_args.extend([
            "--option".to_string(),
            "trusted-public-keys".to_string(),
            format!("{cache_key} cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY="),
        ]);
    }
    let allow_source_fallback = env::var("HITECH_ALLOW_SOURCE_FALLBACK")
        .map(|value| value != "false")
        .unwrap_or(true);
    install_args.extend([
        "--option".to_string(),
        "fallback".to_string(),
        if allow_source_fallback {
            "true"
        } else {
            "false"
        }
        .to_string(),
        "--option".to_string(),
        "extra-experimental-features".to_string(),
        "nix-command flakes".to_string(),
    ]);

    let mut install_envs: Vec<(&str, String)> = Vec::new();
    if let Ok(archive) = env::var("HITECH_NIXPKGS_ARCHIVE") {
        if !archive.is_empty() {
            install_envs.push(("NIX_PATH", format!("nixpkgs={archive}")));
        }
    }

    run_nixos_install(tx, &install_args, &install_envs)?;

    send_progress(tx, 98, "Synchronizing the target filesystem...");
    run_step(tx, "sync", &[], true, &[])?;
    Ok(())
}

fn fetch_hitechos_profile(
    tx: &mpsc::Sender<InstallerMessage>,
    repo_url: &str,
    repo_ref: &str,
) -> Result<(), String> {
    let repo = "/mnt/etc/nixos/hitech-os";
    run_step(tx, "mkdir", &arg_list(&["-p", repo]), false, &[])?;
    run_step(tx, "git", &arg_list(&["-C", repo, "init", "-q"]), true, &[])?;
    run_step(
        tx,
        "git",
        &arg_list(&["-C", repo, "remote", "add", "origin", repo_url]),
        true,
        &[],
    )?;
    run_step(
        tx,
        "git",
        &arg_list(&["-C", repo, "remote", "set-url", "origin", repo_url]),
        false,
        &[],
    )?;
    run_step(
        tx,
        "git",
        &arg_list(&["-C", repo, "fetch", "--depth", "1", "origin", repo_ref]),
        false,
        &[],
    )?;
    run_step(
        tx,
        "git",
        &arg_list(&["-C", repo, "checkout", "--detach", "--force", "FETCH_HEAD"]),
        false,
        &[],
    )?;
    Ok(())
}

fn partition_names(disk: &str) -> (String, String) {
    if disk.contains("nvme") || disk.contains("mmcblk") {
        (format!("{disk}p1"), format!("{disk}p2"))
    } else {
        (format!("{disk}1"), format!("{disk}2"))
    }
}

fn arg_list(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn send_progress(tx: &mpsc::Sender<InstallerMessage>, percent: u16, status: &str) {
    let _ = tx.send(InstallerMessage::Progress(percent, status.to_string()));
}

fn run_step(
    tx: &mpsc::Sender<InstallerMessage>,
    program: &str,
    args: &[String],
    allow_failure: bool,
    envs: &[(&str, String)],
) -> Result<(), String> {
    let _ = tx.send(InstallerMessage::Log(format!(
        "$ sudo {program} {}",
        args.join(" ")
    )));

    let mut command = privileged_command(program, args, envs);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|error| format!("could not launch {program}: {error}"))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let tx_out = tx.clone();
    let tx_err = tx.clone();
    let stdout_thread = stdout.map(|stream| thread::spawn(move || stream_lines(stream, &tx_out)));
    let stderr_thread = stderr.map(|stream| thread::spawn(move || stream_lines(stream, &tx_err)));

    let status = child
        .wait()
        .map_err(|error| format!("could not wait for {program}: {error}"))?;

    if let Some(handle) = stdout_thread {
        let _ = handle.join();
    }
    if let Some(handle) = stderr_thread {
        let _ = handle.join();
    }

    if !status.success() && !allow_failure {
        return Err(format!("{program} exited with status {status}"));
    }

    Ok(())
}

fn run_nixos_install(
    tx: &mpsc::Sender<InstallerMessage>,
    args: &[String],
    envs: &[(&str, String)],
) -> Result<(), String> {
    let _ = tx.send(InstallerMessage::Log(format!(
        "$ sudo nixos-install {}",
        args.join(" ")
    )));

    let mut command = privileged_command("nixos-install", args, envs);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|error| format!("could not launch nixos-install: {error}"))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let tx_out = tx.clone();
    let tx_err = tx.clone();
    let stdout_thread =
        stdout.map(|stream| thread::spawn(move || stream_install_lines(stream, &tx_out)));
    let stderr_thread =
        stderr.map(|stream| thread::spawn(move || stream_install_lines(stream, &tx_err)));

    let status = child
        .wait()
        .map_err(|error| format!("could not wait for nixos-install: {error}"))?;

    if let Some(handle) = stdout_thread {
        let _ = handle.join();
    }
    if let Some(handle) = stderr_thread {
        let _ = handle.join();
    }

    if !status.success() {
        return Err(format!("nixos-install exited with status {status}"));
    }

    Ok(())
}

fn stream_lines<R: Read>(stream: R, tx: &mpsc::Sender<InstallerMessage>) {
    for line in BufReader::new(stream).lines().map_while(Result::ok) {
        if !line.trim().is_empty() {
            let _ = tx.send(InstallerMessage::Log(line));
        }
    }
}

fn stream_install_lines<R: Read>(stream: R, tx: &mpsc::Sender<InstallerMessage>) {
    for line in BufReader::new(stream).lines().map_while(Result::ok) {
        let lower = line.to_lowercase();
        if lower.contains("copying path") {
            let _ = tx.send(InstallerMessage::Progress(
                72,
                "Substituting the HiTech-OS closure from cache...".to_string(),
            ));
        } else if lower.contains("installing the boot loader") {
            let _ = tx.send(InstallerMessage::Progress(
                92,
                "Installing the systemd-boot loader...".to_string(),
            ));
        } else if lower.contains("installing the system") {
            let _ = tx.send(InstallerMessage::Progress(
                58,
                "Installing the NixOS system profile...".to_string(),
            ));
        }

        if !line.trim().is_empty() {
            let _ = tx.send(InstallerMessage::Log(line));
        }
    }
}

fn privileged_command(program: &str, args: &[String], envs: &[(&str, String)]) -> Command {
    let program_path = find_in_path(program).unwrap_or_else(|| PathBuf::from(program));

    match find_in_path("sudo") {
        Some(sudo) => {
            let mut command = Command::new(sudo);
            if envs.is_empty() {
                command.arg(program_path);
            } else {
                command.arg(find_in_path("env").unwrap_or_else(|| PathBuf::from("env")));
                for (key, value) in envs {
                    command.arg(format!("{key}={value}"));
                }
                command.arg(program_path);
            }
            command.args(args);
            command
        }
        None => {
            let mut command = Command::new(program_path);
            for (key, value) in envs {
                command.env(key, value);
            }
            command.args(args);
            command
        }
    }
}

// ---------------------------------------------------------------------------
// Target configuration
// ---------------------------------------------------------------------------

fn build_configuration(
    hostname: &str,
    username: &str,
    password: &str,
    state_version: &str,
    imports: &str,
    include_state_version: bool,
) -> String {
    let state_version_line = if include_state_version {
        format!("  system.stateVersion = \"{}\";", nix_escape(state_version))
    } else {
        String::new()
    };
    format!(
        r#"{{ config, pkgs, ... }}:
{{
  imports = [ {imports} ];

  boot.loader.systemd-boot.enable = true;
  boot.loader.efi.canTouchEfiVariables = true;

  networking.hostName = "{hostname}";
  networking.networkmanager.enable = true;

  users.users."{username}" = {{
    isNormalUser = true;
    extraGroups = [ "wheel" "networkmanager" "video" "audio" ];
    initialPassword = "{password}";
  }};

  nix.settings.experimental-features = [ "nix-command" "flakes" ];
{state_version_line}
}}
"#,
        imports = imports,
        hostname = nix_escape(hostname),
        username = nix_escape(username),
        password = nix_escape(password),
        state_version_line = state_version_line,
    )
}

fn nix_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace("${", "\\${")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([1024.0, 700.0])
            .with_title("HiTech-OS // Cybernetic OS Studio"),
        vsync: true,
        multisampling: 0,
        depth_buffer: 0,
        stencil_buffer: 0,
        ..Default::default()
    };

    eframe::run_native(
        "HiTech-OS // Cybernetic OS Studio",
        native_options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
