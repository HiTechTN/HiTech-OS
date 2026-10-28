// HiTech-OS ORION-X installer core.
//
// A small, dependency-free Rust HTTP/API server. The operator-facing UI is a
// TypeScript/HTML/SVG cockpit served from HITECH_WEB_ROOT (installed by the Nix
// package). Privileged work stays in this process: disks are inspected with
// lsblk, partitions are mounted explicitly, and the target is installed with
// nixos-install. Every mutating operation is triggered by an explicit POST.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const MIN_DISK_BYTES: u64 = 1024 * 1024 * 1024;
const INSTALL_LOG_LIMIT: usize = 48_000;

#[derive(Clone, Default)]
struct InstallStatus {
    running: bool,
    success: bool,
    progress: u16,
    status: String,
    logs: String,
    network_online: bool,
}

#[derive(Clone)]
struct InstallRequest {
    mode: String,
    hostname: String,
    username: String,
    password: String,
    desktop: String,
    disk: String,
    root_partition: String,
    efi_partition: String,
}

#[derive(Clone)]
struct Partition {
    name: String,
    path: String,
    size_bytes: u64,
    size_human: String,
    fstype: String,
    mountpoint: String,
    label: String,
    partlabel: String,
    uuid: String,
    parttype: String,
    is_efi: bool,
}

#[derive(Clone)]
struct Disk {
    name: String,
    path: String,
    size_bytes: u64,
    size_human: String,
    model: String,
    partitions: Vec<Partition>,
}

fn main() {
    let port: u16 = env::var("HITECH_INSTALLER_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(17870);
    let web_root = web_root();
    let state = Arc::new(Mutex::new(InstallStatus {
        status: "Flight control online. Waiting for operator.".to_string(),
        ..InstallStatus::default()
    }));

    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cannot bind 127.0.0.1:{port}: {e}");
            std::process::exit(1);
        }
    };
    let url = format!("http://127.0.0.1:{port}/");
    println!("ORION-X cockpit listening on {url}");
    println!("web root: {}", web_root.display());

    // Browser launch is best-effort: the API remains reachable even if the
    // kiosk browser is missing or crashes.
    thread::spawn(move || {
        if let Err(e) = launch_browser(&url) {
            eprintln!("browser launch failed: {e}");
            eprintln!("open {url} manually");
        }
    });

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let state = state.clone();
                let root = web_root.clone();
                thread::spawn(move || handle_client(stream, state, root));
            }
            Err(e) => eprintln!("connection error: {e}"),
        }
    }
}

fn web_root() -> PathBuf {
    if let Ok(p) = env::var("HITECH_WEB_ROOT") {
        return PathBuf::from(p);
    }
    if Path::new("frontend/index.html").exists() {
        return PathBuf::from("frontend");
    }
    PathBuf::from("share/hitech-installer/www")
}

fn handle_client(mut stream: TcpStream, state: Arc<Mutex<InstallStatus>>, root: PathBuf) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(8)));
    let request = match read_request(&mut stream) {
        Ok(r) => r,
        Err(e) => {
            let _ = send_text(&mut stream, 400, "bad request", &e);
            return;
        }
    };

    if request.path == "/api/status" && request.method == "GET" {
        let body = status_json(&state);
        let _ = send_json(&mut stream, 200, &body);
        return;
    }
    if request.path == "/api/network" && request.method == "GET" {
        let online = network_online();
        let _ = send_json(&mut stream, 200, &format!("{{\"online\":{}}}", online));
        return;
    }
    if request.path == "/api/disks" && request.method == "GET" {
        match scan_disks() {
            Ok(disks) => {
                let body = format!("{{\"disks\":[{}]}}", disks.iter().map(disk_to_json).collect::<Vec<_>>().join(","));
                let _ = send_json(&mut stream, 200, &body);
            }
            Err(e) => send_api_error(&mut stream, &e),
        }
        return;
    }
    if request.path == "/api/wifi/scan" && request.method == "POST" {
        match scan_wifi() {
            Ok(networks) => {
                let body = format!("{{\"networks\":[{}]}}", networks.iter().map(wifi_to_json).collect::<Vec<_>>().join(","));
                let _ = send_json(&mut stream, 200, &body);
            }
            Err(e) => send_api_error(&mut stream, &e),
        }
        return;
    }
    if request.path == "/api/wifi/connect" && request.method == "POST" {
        let form = parse_form(&request.body);
        let ssid = form.get("ssid").cloned().unwrap_or_default();
        let password = form.get("password").cloned().unwrap_or_default();
        if ssid.is_empty() {
            send_api_error(&mut stream, "missing SSID");
            return;
        }
        match connect_wifi(&ssid, &password) {
            Ok(()) => { let _ = send_json(&mut stream, 200, "{\"ok\":true}"); }
            Err(e) => send_api_error(&mut stream, &e),
        }
        return;
    }
    if request.path == "/api/gparted" && request.method == "POST" {
        match launch_detached("sudo", &["-E", "gparted"]) {
            Ok(()) => { let _ = send_json(&mut stream, 200, "{\"ok\":true}"); }
            Err(e) => send_api_error(&mut stream, &e),
        }
        return;
    }
    if request.path == "/api/terminal" && request.method == "POST" {
        match launch_terminal() {
            Ok(()) => { let _ = send_json(&mut stream, 200, "{\"ok\":true}"); }
            Err(e) => send_api_error(&mut stream, &e),
        }
        return;
    }
    if request.path == "/api/reboot" && request.method == "POST" {
        match launch_detached("sudo", &["reboot"]) {
            Ok(()) => { let _ = send_json(&mut stream, 200, "{\"ok\":true}"); }
            Err(e) => send_api_error(&mut stream, &e),
        }
        return;
    }
    if request.path == "/api/install" && request.method == "POST" {
        let form = parse_form(&request.body);
        let req = InstallRequest {
            mode: form.get("mode").cloned().unwrap_or_default(),
            hostname: form.get("hostname").cloned().unwrap_or_default(),
            username: form.get("username").cloned().unwrap_or_default(),
            password: form.get("password").cloned().unwrap_or_default(),
            desktop: form.get("desktop").cloned().unwrap_or_default(),
            disk: form.get("disk").cloned().unwrap_or_default(),
            root_partition: form.get("root_partition").cloned().unwrap_or_default(),
            efi_partition: form.get("efi_partition").cloned().unwrap_or_default(),
        };
        if let Err(e) = validate_request(&req) {
            send_api_error(&mut stream, &e);
            return;
        }
        {
            let mut st = state.lock().unwrap();
            if st.running {
                send_api_error(&mut stream, "installation already running");
                return;
            }
            st.running = true;
            st.success = false;
            st.progress = 0;
            st.status = "Ignition sequence started.".to_string();
            st.logs.clear();
        }
        let state2 = state.clone();
        thread::spawn(move || {
            let result = install_steps(&req, &state2);
            let mut st = state2.lock().unwrap();
            st.running = false;
            st.success = result.is_ok();
            match result {
                Ok(()) => {
                    st.progress = 100;
                    st.status = "Installation complete. Target is ready to reboot.".to_string();
                    append_log_locked(&mut st, "== installation complete ==");
                }
                Err(e) => {
                    st.status = format!("Installation failed: {e}");
                    append_log_locked(&mut st, &format!("ERROR: {e}"));
                }
            }
        });
        let _ = send_json(&mut stream, 200, "{\"ok\":true}");
        return;
    }

    if request.path == "/" || request.path == "/index.html" {
        serve_static(&mut stream, &root, "index.html");
    } else if request.path == "/favicon.ico" {
        let _ = send_text(&mut stream, 204, "image/x-icon", "");
    } else {
        let rel = request.path.trim_start_matches('/');
        serve_static(&mut stream, &root, rel);
    }
}

fn read_request(stream: &mut TcpStream) -> Result<Request, String> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end;
    loop {
        let n = stream.read(&mut chunk).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("empty request".to_string());
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = find_bytes(&buf, b"\r\n\r\n") {
            header_end = pos + 4;
            break;
        }
        if buf.len() > 64 * 1024 {
            return Err("headers too large".to_string());
        }
    }
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("/").to_string();
    let path = target.split('?').next().unwrap_or("/").to_string();
    let mut content_length = 0usize;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                content_length = v.trim().parse().unwrap_or(0);
            }
        }
    }
    if content_length > 2 * 1024 * 1024 {
        return Err("body too large".to_string());
    }
    let mut body = buf[header_end..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut chunk).map_err(|e| e.to_string())?;
        if n == 0 { break; }
        body.extend_from_slice(&chunk[..n]);
    }
    if body.len() > content_length { body.truncate(content_length); }
    Ok(Request {
        method,
        path,
        body: String::from_utf8_lossy(&body).to_string(),
    })
}

struct Request { method: String, path: String, body: String }

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn send_response(stream: &mut TcpStream, status: u16, content_type: &str, body: &[u8]) -> std::io::Result<()> {
    let reason = match status { 200 => "OK", 204 => "No Content", 400 => "Bad Request", 404 => "Not Found", 500 => "Internal Server Error", _ => "OK" };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

fn send_json(stream: &mut TcpStream, status: u16, body: &str) -> std::io::Result<()> {
    send_response(stream, status, "application/json; charset=utf-8", body.as_bytes())
}

fn send_text(stream: &mut TcpStream, status: u16, content_type: &str, body: &str) -> std::io::Result<()> {
    send_response(stream, status, content_type, body.as_bytes())
}

fn send_api_error(stream: &mut TcpStream, message: &str) {
    let body = format!("{{\"error\":\"{}\"}}", json_escape(message));
    let _ = send_json(stream, 400, &body);
}

fn serve_static(stream: &mut TcpStream, root: &Path, rel: &str) {
    if rel.contains("..") {
        let _ = send_text(stream, 404, "text/plain; charset=utf-8", "not found");
        return;
    }
    let path = root.join(rel);
    let canonical_root = match root.canonicalize() { Ok(p) => p, Err(_) => root.to_path_buf() };
    let canonical_path = match path.canonicalize() {
        Ok(p) if p.starts_with(&canonical_root) => p,
        _ => {
            let _ = send_text(stream, 404, "text/plain; charset=utf-8", "not found");
            return;
        }
    };
    match fs::read(&canonical_path) {
        Ok(bytes) => {
            let mime = match canonical_path.extension().and_then(|e| e.to_str()).unwrap_or("") {
                "html" => "text/html; charset=utf-8",
                "css" => "text/css; charset=utf-8",
                "js" => "text/javascript; charset=utf-8",
                "svg" => "image/svg+xml",
                "png" => "image/png",
                _ => "application/octet-stream",
            };
            let _ = send_response(stream, 200, mime, &bytes);
        }
        Err(_) => { let _ = send_text(stream, 404, "text/plain; charset=utf-8", "not found"); }
    }
}

fn status_json(state: &Arc<Mutex<InstallStatus>>) -> String {
    let mut st = state.lock().unwrap();
    st.network_online = network_online();
    format!(
        "{{\"running\":{},\"success\":{},\"progress\":{},\"status\":\"{}\",\"logs\":\"{}\",\"network_online\":{}}}",
        st.running, st.success, st.progress, json_escape(&st.status), json_escape(&st.logs), st.network_online
    )
}

fn set_status(state: &Arc<Mutex<InstallStatus>>, progress: u16, status: &str) {
    let mut st = state.lock().unwrap();
    st.progress = progress.min(100);
    st.status = status.to_string();
    append_log_locked(&mut st, &format!("[{progress:>3}%] {status}"));
}

fn append_log(state: &Arc<Mutex<InstallStatus>>, line: &str) {
    let mut st = state.lock().unwrap();
    append_log_locked(&mut st, line);
}

fn append_log_locked(st: &mut InstallStatus, line: &str) {
    if !st.logs.is_empty() { st.logs.push('\n'); }
    st.logs.push_str(line);
    if st.logs.len() > INSTALL_LOG_LIMIT {
        let cut = st.logs.len() - INSTALL_LOG_LIMIT;
        let keep = st.logs[cut..].find('\n').map(|i| cut + i + 1).unwrap_or(cut);
        st.logs = st.logs[keep..].to_string();
    }
}

fn validate_request(req: &InstallRequest) -> Result<(), String> {
    if !["replace", "erase"].contains(&req.mode.as_str()) { return Err("invalid mode".to_string()); }
    if !["orion", "plasma6", "gnome", "xfce"].contains(&req.desktop.as_str()) { return Err("invalid desktop".to_string()); }
    if !valid_hostname(&req.hostname) { return Err("invalid hostname".to_string()); }
    if !valid_username(&req.username) { return Err("invalid username".to_string()); }
    if req.password.chars().count() < 4 { return Err("password must contain at least 4 characters".to_string()); }
    if req.mode == "replace" {
        if !req.root_partition.starts_with("/dev/") { return Err("invalid root partition".to_string()); }
        if !req.efi_partition.starts_with("/dev/") { return Err("invalid EFI partition".to_string()); }
        if req.root_partition == req.efi_partition { return Err("root and EFI partitions must differ".to_string()); }
    } else if !req.disk.starts_with("/dev/") {
        return Err("invalid target disk".to_string());
    }
    Ok(())
}

fn valid_hostname(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphanumeric() => {}
        _ => return false,
    }
    s.len() <= 63 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn valid_username(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c == '_' => {}
        _ => return false,
    }
    s.len() <= 32 && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

fn nix_escape(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"").replace("${", "\\${"))
}

fn install_steps(req: &InstallRequest, state: &Arc<Mutex<InstallStatus>>) -> Result<(), String> {
    set_status(state, 2, "Preparing target storage…");
    if req.mode == "replace" {
        prepare_existing(&req.root_partition, &req.efi_partition, state)?;
    } else {
        prepare_erase(&req.disk, state)?;
    }

    set_status(state, 24, "Generating hardware configuration…");
    run_checked(state, "nixos-generate-config", &["--root", "/mnt"])?;

    set_status(state, 28, "Fetching the pinned HiTech-OS profile…");
    fetch_profile(state)?;

    set_status(state, 34, "Writing target identity and desktop configuration…");
    let hash = hash_password(&req.password)?;
    let config = target_config(req, &hash);
    root_mkdir(state, "/mnt/etc/nixos")?;
    root_write(state, "/mnt/etc/nixos/configuration.nix", &config)?;

    let fallback = env::var("HITECH_ALLOW_SOURCE_FALLBACK").map(|v| v != "false" && v != "0").unwrap_or(true);
    let mut last_error = String::new();
    for attempt in 1..=3 {
        set_status(state, 38 + (attempt as u16 - 1) * 4, &format!("Installing system closure (attempt {attempt}/3)…"));
        append_log(state, &format!("== nixos-install attempt {attempt}/3 =="));
        let args = [
            "--root", "/mnt",
            "--no-root-passwd",
            "--option", "fallback", if fallback { "true" } else { "false" },
            "--option", "substitute", "true",
        ];
        match run_stream(state, "nixos-install", &args) {
            Ok(()) => {
                last_error.clear();
                break;
            }
            Err(e) => {
                last_error = e.clone();
                append_log(state, &format!("attempt {attempt} failed: {e}"));
                if attempt < 3 {
                    set_status(state, 40, &format!("Installer I/O/network retry {}/3…", attempt + 1));
                    thread::sleep(Duration::from_secs(4));
                }
            }
        }
    }
    if !last_error.is_empty() {
        return Err(format!("nixos-install failed after 3 attempts: {last_error}"));
    }

    set_status(state, 96, "Flushing caches and unmounting target…");
    let _ = run_checked(state, "sync", &[]);
    let _ = run_checked(state, "umount", &["-R", "/mnt"]);
    Ok(())
}

/// /mnt is root-owned once the target is mounted, so even plain file writes
/// must be performed through the privileged path.
fn root_mkdir(state: &Arc<Mutex<InstallStatus>>, path: &str) -> Result<(), String> {
    run_checked(state, "mkdir", &["-p", path])
}

fn root_write(state: &Arc<Mutex<InstallStatus>>, path: &str, contents: &str) -> Result<(), String> {
    let tmp = format!("/tmp/hitech-target-{}.tmp", std::process::id());
    fs::write(&tmp, contents).map_err(|e| format!("cannot stage {path}: {e}"))?;
    let result = run_checked(state, "install", &["-m", "0644", &tmp, path]);
    let _ = fs::remove_file(&tmp);
    result
}

fn prepare_existing(root: &str, efi: &str, state: &Arc<Mutex<InstallStatus>>) -> Result<(), String> {
    set_status(state, 5, "Unmounting any stale target mounts…");
    unmount_path(root, state);
    unmount_path(efi, state);
    let _ = run_checked(state, "udevadm", &["settle"]);

    set_status(state, 10, "Formatting the selected root partition…");
    run_checked(state, "wipefs", &["-a", root])?;
    run_checked(state, "mkfs.ext4", &["-F", "-L", "nixos", root])?;

    set_status(state, 14, "Mounting root and preserving the existing EFI partition…");
    run_checked(state, "mkdir", &["-p", "/mnt"])?;
    run_checked(state, "mount", &[root, "/mnt"])?;
    run_checked(state, "mkdir", &["-p", "/mnt/boot"])?;
    run_checked(state, "mount", &[efi, "/mnt/boot"])?;
    Ok(())
}

fn prepare_erase(disk: &str, state: &Arc<Mutex<InstallStatus>>) -> Result<(), String> {
    set_status(state, 5, "Unmounting all partitions on the target disk…");
    if let Ok(disks) = scan_disks() {
        if let Some(d) = disks.iter().find(|d| d.path == disk) {
            for p in &d.partitions { unmount_path(&p.path, state); }
        }
    }
    unmount_path(disk, state);
    let _ = run_checked(state, "udevadm", &["settle"]);

    set_status(state, 8, "Erasing disk and writing GPT…");
    run_checked(state, "wipefs", &["-af", disk])?;
    run_checked(state, "parted", &["--script", disk, "mklabel", "gpt"])?;
    run_checked(state, "parted", &["--script", disk, "mkpart", "ESP", "fat32", "1MiB", "513MiB"])?;
    run_checked(state, "parted", &["--script", disk, "set", "1", "esp", "on"])?;
    run_checked(state, "parted", &["--script", disk, "mkpart", "root", "ext4", "513MiB", "100%"])?;
    let _ = run_checked(state, "partprobe", &[disk]);
    let _ = run_checked(state, "udevadm", &["settle"]);
    thread::sleep(Duration::from_millis(500));

    let efi = partition_path(disk, 1);
    let root = partition_path(disk, 2);
    set_status(state, 12, "Formatting EFI and root…");
    run_checked(state, "mkfs.fat", &["-F32", "-n", "HITECH-EFI", &efi])?;
    run_checked(state, "mkfs.ext4", &["-F", "-L", "nixos", &root])?;
    set_status(state, 16, "Mounting target…");
    run_checked(state, "mkdir", &["-p", "/mnt"])?;
    run_checked(state, "mount", &[&root, "/mnt"])?;
    run_checked(state, "mkdir", &["-p", "/mnt/boot"])?;
    run_checked(state, "mount", &[&efi, "/mnt/boot"])?;
    Ok(())
}

fn partition_path(disk: &str, n: u32) -> String {
    if disk.chars().last().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        format!("{disk}p{n}")
    } else {
        format!("{disk}{n}")
    }
}

fn unmount_path(path: &str, state: &Arc<Mutex<InstallStatus>>) {
    let output = Command::new("findmnt").args(["-rn", "-S", path, "-o", "TARGET"]).output();
    let Ok(out) = output else { return; };
    for target in String::from_utf8_lossy(&out.stdout).lines().map(str::trim).filter(|s| !s.is_empty()) {
        if target == "/" || target == "/nix" || target == "/nix/store" || target.starts_with("/sys") || target.starts_with("/proc") || target == "/boot" {
            append_log(state, &format!("WARN: refusing to unmount live mount {target} ({path})"));
            continue;
        }
        if target.starts_with("/run/media/") || target.starts_with("/media/") || target == "/mnt" || target.starts_with("/mnt/") {
            append_log(state, &format!("unmount {target} ({path})"));
            let _ = privileged("umount").args(["-l", target]).output();
        }
    }
}

fn fetch_profile(state: &Arc<Mutex<InstallStatus>>) -> Result<(), String> {
    let url = env::var("HITECH_REPO_URL").unwrap_or_default();
    if url.trim().is_empty() {
        append_log(state, "HITECH_REPO_URL is empty; installing a generic NixOS profile.");
        return Ok(());
    }
    let dest = "/mnt/etc/nixos/hitech-os";
    if Path::new(dest).exists() {
        run_checked(state, "rm", &["-rf", dest])?;
    }
    let refname = env::var("HITECH_REPO_REF").unwrap_or_else(|_| "main".to_string());
    append_log(state, &format!("git clone {url} ({refname}) -> {dest}"));
    let mut args = vec!["clone", "--depth", "1"];
    if !refname.trim().is_empty() {
        args.push("--branch");
        args.push(refname.trim());
    }
    args.push(url.trim());
    args.push(dest);
    run_checked(state, "git", &args)?;
    Ok(())
}

fn hash_password(password: &str) -> Result<String, String> {
    if let Ok(out) = Command::new("mkpasswd").args(["-m", "sha-512", password]).output() {
        if out.status.success() {
            let hash = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if hash.starts_with("$6$") { return Ok(hash); }
        }
    }
    let out = Command::new("openssl").args(["passwd", "-6", password]).output().map_err(|e| format!("cannot hash password: {e}"))?;
    if !out.status.success() { return Err("password hashing failed".to_string()); }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn target_config(req: &InstallRequest, password_hash: &str) -> String {
    let mut s = String::new();
    s.push_str("{ config, pkgs, lib, ... }:\n\n{\n  imports = [\n    ./hardware-configuration.nix\n    ./hitech-os/os/installation.nix\n  ];\n\n");
    s.push_str(&format!("  networking.hostName = {};\n", nix_escape(&req.hostname)));
    s.push_str("  networking.networkmanager.enable = true;\n");
    s.push_str("  time.timeZone = \"UTC\";\n");
    s.push_str(&format!("  users.users.{} = {{\n", nix_escape(&req.username)));
    s.push_str("    isNormalUser = true;\n");
    s.push_str(&format!("    description = {};\n", nix_escape(&req.username)));    s.push_str("    extraGroups = [ \"wheel\" \"networkmanager\" \"video\" \"render\" \"input\" ];\n");
    s.push_str(&format!("    initialHashedPassword = {};\n", nix_escape(password_hash)));
            s.push_str("  };\n");
            s.push_str("  security.sudo.wheelNeedsPassword = true;\n");
            s.push_str("  security.polkit.enable = true;\n");
            s.push_str("  services.udisks2.enable = true;\n");
            s.push_str("  services.libinput.enable = true;\n");
            s.push_str("  services.xserver.enable = true;\n");
            s.push_str("  boot.loader.systemd-boot.enable = true;\n");
            s.push_str("  boot.loader.efi.canTouchEfiVariables = true;\n");
            s.push_str(&format!("  system.stateVersion = {};\n", nix_escape(&env::var("HITECH_STATE_VERSION").unwrap_or_else(|_| "25.11".to_string()))));
            s.push_str("  nix.settings.experimental-features = [ \"nix-command\" \"flakes\" ];\n");
            s.push_str("\n  # HiTech-Shell: always enabled when Orion desktop is selected.\n");
            s.push_str("  services.hitech-shell.enable = true;\n");
            match req.desktop.as_str() {
                "orion" => {
                    s.push_str("  services.displayManager.sddm.enable = true;\n");
                    s.push_str("  services.displayManager.sddm.wayland.enable = true;\n");
                }
                "plasma6" => {
                    s.push_str("  services.desktopManager.plasma6.enable = true;\n");
                    s.push_str("  services.displayManager.sddm.enable = true;\n");
                }
                "gnome" => {
                    s.push_str("  services.desktopManager.gnome.enable = true;\n");
                    s.push_str("  services.displayManager.gdm.enable = true;\n");
                }
                "xfce" => {
                    s.push_str("  services.xserver.desktopManager.xfce.enable = true;\n");
                    s.push_str("  services.xserver.displayManager.lightdm.enable = true;\n");
                }
                _ => {}
            }
    s.push_str("}\n");
    s
}

/// The installer runs as the unprivileged live user (Cage kiosk and the Live
/// GNOME session both start it without root), while partitioning, mounting and
/// nixos-install all need root. Privileged work goes through the live images'
/// NOPASSWD wheel policy; when the process is already root it runs directly.
fn is_root() -> bool {
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("Uid:"))
                .and_then(|l| l.split_whitespace().nth(1).and_then(|v| v.parse::<u32>().ok()))
        })
        .map(|uid| uid == 0)
        .unwrap_or(false)
}

fn privileged(program: &str) -> Command {
    if is_root() {
        Command::new(program)
    } else {
        let mut cmd = Command::new("sudo");
        cmd.args(["-n", "--"]).arg(program);
        cmd
    }
}

fn run_checked(state: &Arc<Mutex<InstallStatus>>, program: &str, args: &[&str]) -> Result<(), String> {
    let shown = format!("{program} {}", args.join(" "));
    append_log(state, &format!("$ {shown}"));
    let out = privileged(program).args(args).output().map_err(|e| format!("{program}: {e}"))?;
    if !out.stdout.is_empty() {
        for line in String::from_utf8_lossy(&out.stdout).lines().filter(|l| !l.trim().is_empty()) {
            append_log(state, line);
        }
    }
    if !out.stderr.is_empty() {
        for line in String::from_utf8_lossy(&out.stderr).lines().filter(|l| !l.trim().is_empty()) {
            append_log(state, line);
        }
    }
    if out.status.success() { Ok(()) } else { Err(format!("{shown} exited with {}", out.status)) }
}

fn run_stream(state: &Arc<Mutex<InstallStatus>>, program: &str, args: &[&str]) -> Result<(), String> {
    append_log(state, &format!("$ {program} {}", args.join(" ")));
    let mut child = privileged(program).args(args).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| format!("{program}: {e}"))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let s1 = state.clone();
    let s2 = state.clone();
    let h1 = stdout.map(|out| thread::spawn(move || {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            if !line.trim().is_empty() { append_log(&s1, &line); }
        }
    }));
    let h2 = stderr.map(|err| thread::spawn(move || {
        for line in BufReader::new(err).lines().map_while(Result::ok) {
            if !line.trim().is_empty() { append_log(&s2, &line); }
        }
    }));
    let status = child.wait().map_err(|e| e.to_string())?;
    if let Some(h) = h1 { let _ = h.join(); }
    if let Some(h) = h2 { let _ = h.join(); }
    if status.success() { Ok(()) } else { Err(format!("{program} exited with {status}")) }
}

fn run_capture(program: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program).args(args).output().map_err(|e| format!("{program}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

fn network_online() -> bool {
    if let Ok(out) = Command::new("ip").args(["route", "show", "default"]).output() {
        if out.status.success() && !out.stdout.is_empty() {
            return true;
        }
    }
    if let Ok(out) = Command::new("nmcli").args(["-t", "-f", "STATE", "general"]).output() {
        if out.status.success() && String::from_utf8_lossy(&out.stdout).contains("connected") {
            return true;
        }
    }
    false
}

fn scan_wifi() -> Result<Vec<(String, String, String)>, String> {
    let output = run_capture("nmcli", &["-t", "-f", "SSID,SIGNAL,SECURITY", "dev", "wifi", "list"])?;
    let mut seen = Vec::new();
    let mut result = Vec::new();
    for line in output.lines() {
        let mut parts = line.splitn(3, ':');
        let ssid = parts.next().unwrap_or("").trim().to_string();
        let signal = parts.next().unwrap_or("").trim().to_string();
        let security = parts.next().unwrap_or("").trim().to_string();
        if ssid.is_empty() || ssid == "--" || seen.contains(&ssid) { continue; }
        seen.push(ssid.clone());
        result.push((ssid, signal, security));
        if result.len() >= 40 { break; }
    }
    Ok(result)
}

fn connect_wifi(ssid: &str, password: &str) -> Result<(), String> {
    if password.is_empty() {
        run_capture("nmcli", &["dev", "wifi", "connect", ssid])?;
    } else {
        run_capture("nmcli", &["dev", "wifi", "connect", ssid, "password", password])?;
    }
    Ok(())
}

fn launch_detached(program: &str, args: &[&str]) -> Result<(), String> {
    Command::new(program).args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map(|_| ()).map_err(|e| format!("{program}: {e}"))
}

fn launch_terminal() -> Result<(), String> {
    for (program, args) in [
        ("foot", vec!["-e", "bash", "-l"]),
        ("kgx", vec!["--", "bash", "-l"]),
        ("gnome-terminal", vec!["--", "bash", "-l"]),
        ("xterm", vec!["-e", "bash", "-l"]),
    ] {
        if launch_detached(program, &args).is_ok() { return Ok(()); }
    }
    Err("no terminal emulator found".to_string())
}

fn launch_browser(url: &str) -> Result<(), String> {
    let candidates = env::var("HITECH_BROWSER").ok().map(|b| vec![b]).unwrap_or_else(|| {
        vec!["chromium".to_string(), "chromium-browser".to_string(), "google-chrome".to_string(), "firefox".to_string()]
    });
    let wayland = env::var("WAYLAND_DISPLAY").is_ok();
    // A desktop session (GNOME on the Live ISO) has a window manager: an app
    // window that starts fullscreen gives the immersive cockpit while Alt+Tab
    // still reaches the rest of the desktop. With no session shell (Cage on the
    // NetInstaller) we take the whole seat in real kiosk mode instead.
    let desktop_session = env::var("XDG_CURRENT_DESKTOP")
        .map(|d| !d.trim().is_empty())
        .unwrap_or(false)
        && env::var("HITECH_FORCE_KIOSK").map(|v| v == "0").unwrap_or(true);
    let mut last = String::new();
    for name in candidates {
        let lower = name.to_lowercase();
        let mut cmd = Command::new(&name);
        if lower.contains("firefox") {
            cmd.args(["--kiosk", url]);
        } else {
            if desktop_session {
                // Fullscreen maximized window with --app for kiosk-style immersive,
                // but window decorations remain via the session shell so Alt+Tab works.
                cmd.arg("--app").arg(url)
                    .arg("--start-fullscreen")
                    .arg("--start-maximized");
            } else {
                cmd.arg("--kiosk").arg("--start-fullscreen").arg(url);
            }
            cmd.arg("--password-store=basic")
                .arg("--no-first-run")
                .arg("--no-default-browser-check")
                .arg("--disable-gpu")
                .env("LIBGL_ALWAYS_SOFTWARE", "1")
                .arg("--disable-dev-shm-usage")
                .arg("--window-size=1920,1080");
            if wayland { cmd.arg("--ozone-platform=wayland"); }
        }
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        match cmd.spawn() {
            Ok(_) => return Ok(()),
            Err(e) => last = format!("{name}: {e}"),
        }
    }
    Err(if last.is_empty() { "no browser found".to_string() } else { last })
}

fn parse_form(body: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    for pair in body.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        result.insert(percent_decode(k), percent_decode(v));
    }
    result
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => { out.push(b' '); i += 1; }
            b'%' if i + 2 < bytes.len() => {
                let hex = &s[i + 1..i + 3];
                if let Ok(v) = u8::from_str_radix(hex, 16) { out.push(v); i += 3; }
                else { out.push(bytes[i]); i += 1; }
            }
            b => { out.push(b); i += 1; }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

fn disk_to_json(d: &Disk) -> String {
    format!(
        "{{\"name\":\"{}\",\"path\":\"{}\",\"size_bytes\":{},\"size_human\":\"{}\",\"model\":\"{}\",\"partitions\":[{}]}}",
        json_escape(&d.name), json_escape(&d.path), d.size_bytes, json_escape(&d.size_human), json_escape(&d.model),
        d.partitions.iter().map(partition_to_json).collect::<Vec<_>>().join(",")
    )
}

fn partition_to_json(p: &Partition) -> String {
    format!(
        "{{\"name\":\"{}\",\"path\":\"{}\",\"size_bytes\":{},\"size_human\":\"{}\",\"fstype\":\"{}\",\"mountpoint\":\"{}\",\"label\":\"{}\",\"partlabel\":\"{}\",\"uuid\":\"{}\",\"parttype\":\"{}\",\"is_efi\":{}}}",
        json_escape(&p.name), json_escape(&p.path), p.size_bytes, json_escape(&p.size_human), json_escape(&p.fstype),
        json_escape(&p.mountpoint), json_escape(&p.label), json_escape(&p.partlabel), json_escape(&p.uuid),
        json_escape(&p.parttype), p.is_efi
    )
}

fn wifi_to_json(w: &(String, String, String)) -> String {
    format!("{{\"ssid\":\"{}\",\"signal\":\"{}\",\"security\":\"{}\"}}", json_escape(&w.0), json_escape(&w.1), json_escape(&w.2))
}

fn scan_disks() -> Result<Vec<Disk>, String> {
    let out = Command::new("lsblk")
        .args(["-J", "-b", "-o", "NAME,PATH,SIZE,TYPE,FSTYPE,MOUNTPOINT,LABEL,PARTLABEL,MODEL,PARTTYPE,UUID"])
        .output().map_err(|e| format!("lsblk: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let root = JsonParser::new(&out.stdout).parse()?;
    let arr = match root { Json::Obj(_) => json_get(&root, "blockdevices").cloned().unwrap_or(Json::Arr(vec![])), other => other };
    let items = match arr { Json::Arr(v) => v, _ => return Err("unexpected lsblk JSON".to_string()) };
    let mut disks = Vec::new();
    for item in items {
        if json_str(&item, "type") != "disk" { continue; }
        let path = json_str(&item, "path");
        if path.starts_with("/dev/loop") || path.starts_with("/dev/sr") || path.starts_with("/dev/zram") || path.starts_with("/dev/ram") || path.starts_with("/dev/fd") { continue; }
        let size = json_u64(&item, "size");
        if size < MIN_DISK_BYTES { continue; }
        let mut partitions = Vec::new();
        if let Some(Json::Arr(children)) = json_get(&item, "children") {
            for c in children {
                let psize = json_u64(c, "size");
                if psize == 0 { continue; }
                let fstype = json_str(c, "fstype");
                let mountpoint = json_str(c, "mountpoint");
                let parttype = json_str(c, "parttype");
                let parttype_l = parttype.to_lowercase();
                let fstype_l = fstype.to_lowercase();
                let is_efi = fstype_l == "vfat" || fstype_l == "fat" || fstype_l == "fat32" || parttype_l.contains("c12a7328-f81f-11d2-ba4b-00a0c93ec93b") || mountpoint == "/boot" || mountpoint == "/efi" || mountpoint == "/boot/efi";
                partitions.push(Partition {
                    name: json_str(c, "name"),
                    path: json_str(c, "path"),
                    size_bytes: psize,
                    size_human: human_size(psize),
                    fstype,
                    mountpoint,
                    label: json_str(c, "label"),
                    partlabel: json_str(c, "partlabel"),
                    uuid: json_str(c, "uuid"),
                    parttype,
                    is_efi,
                });
            }
        }
        disks.push(Disk {
            name: json_str(&item, "name"),
            path,
            size_bytes: size,
            size_human: human_size(size),
            model: json_str(&item, "model").trim().to_string(),
            partitions,
        });
    }
    disks.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(disks)
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 { v /= 1024.0; i += 1; }
    if i == 0 { format!("{} {}", bytes, UNITS[i]) } else { format!("{v:.1} {}", UNITS[i]) }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

// A minimal JSON reader for lsblk -J. Keeping it local avoids a dependency on
// serde in the installer initramfs and keeps the server small.
#[derive(Debug, Clone)]
enum Json { Null, Bool(bool), Num(f64), Str(String), Arr(Vec<Json>), Obj(Vec<(String, Json)>) }

struct JsonParser<'a> { data: &'a [u8], pos: usize }

impl<'a> JsonParser<'a> {
    fn new(data: &'a [u8]) -> Self { Self { data, pos: 0 } }
    fn parse(&mut self) -> Result<Json, String> {
        self.ws();
        let v = self.value()?;
        self.ws();
        Ok(v)
    }
    fn ws(&mut self) {
        while self.pos < self.data.len() && matches!(self.data[self.pos], b' ' | b'\n' | b'\r' | b'\t') { self.pos += 1; }
    }
    fn value(&mut self) -> Result<Json, String> {
        self.ws();
        match self.data.get(self.pos).copied() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b't') => { self.literal("true")?; Ok(Json::Bool(true)) }
            Some(b'f') => { self.literal("false")?; Ok(Json::Bool(false)) }
            Some(b'n') => { self.literal("null")?; Ok(Json::Null) }
            Some(b'-') | Some(b'0'..=b'9') => self.number(),
            _ => Err(format!("unexpected JSON at {}", self.pos)),
        }
    }
    fn object(&mut self) -> Result<Json, String> {
        self.pos += 1;
        let mut out = Vec::new();
        self.ws();
        if self.data.get(self.pos) == Some(&b'}') { self.pos += 1; return Ok(Json::Obj(out)); }
        loop {
            self.ws();
            let key = self.string()?;
            self.ws();
            if self.data.get(self.pos) != Some(&b':') { return Err("expected ':'".to_string()); }
            self.pos += 1;
            let val = self.value()?;
            out.push((key, val));
            self.ws();
            match self.data.get(self.pos) {
                Some(b',') => { self.pos += 1; }
                Some(b'}') => { self.pos += 1; break; }
                _ => return Err("expected ',' or '}'".to_string()),
            }
        }
        Ok(Json::Obj(out))
    }
    fn array(&mut self) -> Result<Json, String> {
        self.pos += 1;
        let mut out = Vec::new();
        self.ws();
        if self.data.get(self.pos) == Some(&b']') { self.pos += 1; return Ok(Json::Arr(out)); }
        loop {
            out.push(self.value()?);
            self.ws();
            match self.data.get(self.pos) {
                Some(b',') => { self.pos += 1; }
                Some(b']') => { self.pos += 1; break; }
                _ => return Err("expected ',' or ']'".to_string()),
            }
        }
        Ok(Json::Arr(out))
    }
    fn string(&mut self) -> Result<String, String> {
        if self.data.get(self.pos) != Some(&b'"') { return Err("expected string".to_string()); }
        self.pos += 1;
        let mut out = String::new();
        while let Some(&b) = self.data.get(self.pos) {
            self.pos += 1;
            match b {
                b'"' => return Ok(out),
                b'\\' => {
                    let esc = *self.data.get(self.pos).ok_or("truncated escape")?;
                    self.pos += 1;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let h = self.data.get(self.pos..self.pos + 4).ok_or("truncated unicode escape")?;
                            let h = String::from_utf8_lossy(h);
                            let n = u32::from_str_radix(&h, 16).map_err(|_| "bad unicode escape")?;
                            self.pos += 4;
                            out.push(char::from_u32(n).unwrap_or('\u{fffd}'));
                        }
                        _ => return Err("bad escape".to_string()),
                    }
                }
                b if b < 0x80 => out.push(b as char),
                _ => {
                    // Reconstruct the remaining UTF-8 sequence.
                    let start = self.pos - 1;
                    let width = if b & 0xe0 == 0xc0 { 2 } else if b & 0xf0 == 0xe0 { 3 } else { 4 };
                    let end = (start + width).min(self.data.len());
                    out.push_str(&String::from_utf8_lossy(&self.data[start..end]));
                    self.pos = end;
                }
            }
        }
        Err("unterminated string".to_string())
    }
    fn number(&mut self) -> Result<Json, String> {
        let start = self.pos;
        while self.pos < self.data.len() && matches!(self.data[self.pos], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') { self.pos += 1; }
        let s = String::from_utf8_lossy(&self.data[start..self.pos]);
        s.parse::<f64>().map(Json::Num).map_err(|_| format!("bad number {s}"))
    }
    fn literal(&mut self, s: &str) -> Result<(), String> {
        if self.data[self.pos..].starts_with(s.as_bytes()) { self.pos += s.len(); Ok(()) } else { Err("bad literal".to_string()) }
    }
}

fn json_get<'a>(j: &'a Json, key: &str) -> Option<&'a Json> {
    if let Json::Obj(fields) = j {
        fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    } else { None }
}

fn json_str(j: &Json, key: &str) -> String {
    match json_get(j, key) {
        Some(Json::Str(s)) => s.clone(),
        Some(Json::Null) | None => String::new(),
        Some(Json::Num(n)) => format!("{n}"),
        Some(Json::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

fn json_u64(j: &Json, key: &str) -> u64 {
    match json_get(j, key) {
        Some(Json::Num(n)) => *n as u64,
        Some(Json::Str(s)) => s.parse().unwrap_or(0),
        _ => 0,
    }
}
