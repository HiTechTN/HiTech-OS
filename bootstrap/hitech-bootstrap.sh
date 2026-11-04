#!/usr/bin/env bash
# HiTech-OS universal bootstrap / morphing engine.
#
# Turns a stock NixOS, Debian/Ubuntu, Arch, Fedora, or Windows WSL2 host into a
# HiTech-OS node. The NixOS path is declarative (`nixos-rebuild boot --flake`);
# the generic Linux path installs Nix and the HiTech runtime packages into the
# system profile, then registers the session and systemd units.
#
# Engineering and documentation: Mohamed Azmi Kaaniche (AZMI), HiTechTN,
# HiTechLab. SPDX-License-Identifier: AGPL-3.0-only
set -Eeuo pipefail
IFS=$'\n\t'

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROFILE="orion"
DRY_RUN=0
VERIFY_ONLY=0
ASSUME_YES=0
NO_SERVICES=0
ROOT="/"
SOURCE_DIR="${HITECH_SOURCE_DIR:-}"
NIX_PROFILE="/nix/var/nix/profiles/default"
BACKUP_ROOT="/var/backups/hitech-os"
LOG_FILE="${HITECH_BOOTSTRAP_LOG:-/var/log/hitech-bootstrap.log}"
FLAKE_REF=""
OS_ID="unknown"
OS_LIKE=""
OS_NAME="unknown"
OS_VERSION="unknown"
DISTRO_FAMILY="unknown"
WSL=0
SYSTEMD=0
CPU_ARCH="unknown"
CPU_VENDOR="unknown"
CPU_MODEL="unknown"
RAM_MIB=0
GPU_VENDOR="unknown"
GPU_MODEL="unknown"
VRAM_MIB=0

if [[ -t 1 ]]; then
  C_RESET=$'\033[0m'; C_CYAN=$'\033[36m'; C_GREEN=$'\033[32m'
  C_YELLOW=$'\033[33m'; C_RED=$'\033[31m'; C_BOLD=$'\033[1m'
else
  C_RESET=""; C_CYAN=""; C_GREEN=""; C_YELLOW=""; C_RED=""; C_BOLD=""
fi

log()  { printf '%s[hitech-bootstrap]%s %s\n' "$C_CYAN" "$C_RESET" "$*" >&2; }
ok()   { printf '%s[ ok ]%s %s\n' "$C_GREEN" "$C_RESET" "$*" >&2; }
warn() { printf '%s[warn]%s %s\n' "$C_YELLOW" "$C_RESET" "$*" >&2; }
die()  { printf '%s[fail]%s %s\n' "$C_RED" "$C_RESET" "$*" >&2; exit 1; }

usage() {
  cat <<'USAGE'
HiTech-OS universal bootstrap

Usage:
  sudo ./bootstrap/hitech-bootstrap.sh [options]

Options:
  --profile <orion|edge|server|minimal>  Target profile (default: orion)
  --source <path>                        HiTech-OS source/flake directory
  --dry-run                              Detect and print actions; change nothing
  --verify                               Read-only verification of an existing install
  --no-services                          Do not create/enable systemd units
  --root <path>                          Root for generated files (tests only)
  -y, --yes                              Assume yes for installer prompts
  -h, --help                             Show this help

Environment:
  HITECH_SOURCE_DIR       Source override (same as --source)
  HITECH_FLAKE            Flake ref override, e.g. github:HiTechTN/HiTech-OS
  HITECH_BOOTSTRAP_LOG    Log path (default: /var/log/hitech-bootstrap.log)

Examples:
  sudo ./bootstrap/hitech-bootstrap.sh --profile orion
  sudo ./bootstrap/hitech-bootstrap.sh --profile edge --dry-run
  sudo ./bootstrap/hitech-bootstrap.sh --verify
  powershell -ExecutionPolicy Bypass -File .\bootstrap\hitech-bootstrap.ps1 -Profile orion
USAGE
}

root_path() {
  local path=$1
  if [[ "$ROOT" == "/" ]]; then printf '%s' "$path"; else printf '%s%s' "${ROOT%/}" "$path"; fi
}

as_root() {
  if [[ "$ROOT" != "/" ]]; then
    "$@"
    return
  fi
  if [[ $(id -u) -eq 0 ]]; then "$@"; else sudo -- "$@"; fi
}

run() {
  if [[ $DRY_RUN -eq 1 ]]; then log "[dry-run] $*"; return 0; fi
  log "+ $*"
  "$@"
}

run_root() {
  if [[ $DRY_RUN -eq 1 ]]; then log "[dry-run] root: $*"; return 0; fi
  as_root "$@"
}

cmd_exists() { command -v "$1" >/dev/null 2>&1; }

random_hex() {
  local bytes=$1
  if cmd_exists openssl; then
    openssl rand -hex "$bytes"
  else
    od -An -N "$bytes" -tx1 /dev/urandom | tr -d ' \n'
  fi
}

ensure_root() {
  if [[ "$ROOT" == "/" && $(id -u) -ne 0 ]]; then
    die "run as root (sudo) or use --dry-run/--verify"
  fi
}

write_if_missing() {
  local path=$1 mode=$2 content=$3 target
  target=$(root_path "$path")
  if [[ -f "$target" ]]; then
    return 0
  fi
  write_file "$path" "$mode" "$content"
}

write_file() {
  local path mode content target tmp
  path=$1; mode=$2; content=$3
  target=$(root_path "$path")
  if [[ $DRY_RUN -eq 1 ]]; then
    log "[dry-run] write $target ($mode, ${#content} bytes)"
    return 0
  fi
  mkdir -p -- "$(dirname -- "$target")"
  if [[ -f "$target" ]] && cmp -s <(printf '%b' "$content") "$target"; then
    return 0
  fi
  tmp=$(mktemp "${target}.tmp.XXXXXX")
  printf '%b' "$content" >"$tmp"
  chmod "$mode" "$tmp"
  mv -f -- "$tmp" "$target"
}

append_once() {
  local path marker line target
  path=$1; marker=$2; line=$3
  target=$(root_path "$path")
  if [[ $DRY_RUN -eq 1 ]]; then log "[dry-run] append '$line' to $target"; return 0; fi
  mkdir -p -- "$(dirname -- "$target")"
  touch -- "$target"
  grep -Fq -- "$marker" "$target" 2>/dev/null && return 0
  printf '\n%s\n' "$line" >>"$target"
}

detect_os() {
  local release_file="${HITECH_OS_RELEASE:-/etc/os-release}" id like
  if [[ -n "${WSL_DISTRO_NAME:-}" ]] || [[ "$(uname -r 2>/dev/null || true)" == *microsoft* ]]; then
    WSL=1
  fi
  if [[ -r "$release_file" ]]; then
    # shellcheck disable=SC1090
    id=$(unset ID; . "$release_file" >/dev/null 2>&1; printf '%s' "${ID:-unknown}")
    # shellcheck disable=SC1090
    like=$(unset ID_LIKE; . "$release_file" >/dev/null 2>&1; printf '%s' "${ID_LIKE:-}")
    # shellcheck disable=SC1090
    OS_NAME=$(unset NAME; . "$release_file" >/dev/null 2>&1; printf '%s' "${NAME:-unknown}")
    # shellcheck disable=SC1090
    OS_VERSION=$(unset VERSION_ID; . "$release_file" >/dev/null 2>&1; printf '%s' "${VERSION_ID:-unknown}")
    OS_ID=$id
    OS_LIKE=$like
  fi
  case " $OS_ID $OS_LIKE " in
    *" nixos "*) DISTRO_FAMILY="nixos" ;;
    *" debian "*|*" ubuntu "*) DISTRO_FAMILY="debian" ;;
    *" arch "*) DISTRO_FAMILY="arch" ;;
    *" fedora "*) DISTRO_FAMILY="fedora" ;;
    *)
      case "$OS_ID" in
        nixos) DISTRO_FAMILY="nixos" ;;
        debian|ubuntu|linuxmint|pop) DISTRO_FAMILY="debian" ;;
        arch|manjaro|endeavouros) DISTRO_FAMILY="arch" ;;
        fedora|rhel|centos) DISTRO_FAMILY="fedora" ;;
        *) DISTRO_FAMILY="unknown" ;;
      esac
      ;;
  esac
  if [[ $WSL -eq 1 ]]; then OS_ID="wsl2"; fi
  SYSTEMD=0
  [[ -d /run/systemd/system ]] && SYSTEMD=1
}

detect_hardware() {
  local mem_kb gpu_line vram_file vram_bytes cpuinfo
  CPU_ARCH=$(uname -m 2>/dev/null || printf 'unknown')
  cpuinfo="${HITECH_CPUINFO:-/proc/cpuinfo}"
  if [[ -r "$cpuinfo" ]]; then
    CPU_VENDOR=$(awk -F: '/^vendor_id/ {gsub(/^[ \t]+|[ \t]+$/, "", $2); print $2; exit}' "$cpuinfo")
    CPU_MODEL=$(awk -F: '/^model name/ {gsub(/^[ \t]+|[ \t]+$/, "", $2); print $2; exit}' "$cpuinfo")
    if [[ -z "$CPU_VENDOR" ]]; then
      case "$CPU_ARCH" in
        aarch64|arm64|armv7l) CPU_VENDOR="ARM" ;;
        *) CPU_VENDOR="unknown" ;;
      esac
    fi
  fi
  CPU_VENDOR=${CPU_VENDOR:-unknown}
  CPU_MODEL=${CPU_MODEL:-unknown}
  mem_kb=$(awk '/^MemTotal:/ {print $2; exit}' /proc/meminfo 2>/dev/null || printf '0')
  RAM_MIB=$(( ${mem_kb:-0} / 1024 ))
  if cmd_exists lspci; then
    gpu_line=$(lspci 2>/dev/null | grep -Ei 'VGA compatible controller|3D controller|Display controller' | head -1 || true)
    GPU_MODEL=${gpu_line#*: }
    case "$gpu_line" in
      *NVIDIA*|*nvidia*) GPU_VENDOR="nvidia" ;;
      *AMD*|*Advanced*Micro*Devices*|*ATI*) GPU_VENDOR="amd" ;;
      *Intel*) GPU_VENDOR="intel" ;;
      *) GPU_VENDOR="unknown" ;;
    esac
  fi
  GPU_MODEL=${GPU_MODEL:-unknown}
  for vram_file in /sys/class/drm/card*/device/mem_info_vram_total; do
    [[ -r "$vram_file" ]] || continue
    vram_bytes=$(cat "$vram_file" 2>/dev/null || printf '0')
    if (( ${vram_bytes:-0} / 1024 / 1024 > VRAM_MIB )); then
      VRAM_MIB=$(( vram_bytes / 1024 / 1024 ))
    fi
  done
  if cmd_exists nvidia-smi; then
    local nv
    nv=$(nvidia-smi --query-gpu=memory.total --format=csv,noheader,nounits 2>/dev/null | head -1 | tr -dc '0-9' || true)
    if [[ -n "$nv" ]] && (( nv > VRAM_MIB )); then VRAM_MIB=$nv; GPU_VENDOR="nvidia"; fi
  fi
}

print_detection() {
  printf '%sHiTech-OS bootstrap detection%s\n' "$C_BOLD" "$C_RESET"
  printf '  OS:       %s %s (%s)\n' "$OS_NAME" "$OS_VERSION" "$OS_ID"
  printf '  Family:   %s%s\n' "$DISTRO_FAMILY" "$([[ $WSL -eq 1 ]] && printf ' / WSL2')"
  printf '  Systemd:  %s\n' "$([[ $SYSTEMD -eq 1 ]] && printf yes || printf no)"
  printf '  CPU:      %s (%s)\n' "$CPU_MODEL" "$CPU_ARCH"
  printf '  CPU ven:  %s\n' "$CPU_VENDOR"
  printf '  RAM:      %s MiB\n' "$RAM_MIB"
  printf '  GPU:      %s (%s)\n' "$GPU_MODEL" "$GPU_VENDOR"
  printf '  VRAM:     %s MiB\n' "$VRAM_MIB"
  printf '  Profile:  %s\n' "$PROFILE"
}

valid_profile() {
  case "$1" in orion|edge|server|minimal) return 0 ;; *) return 1 ;; esac
}

profile_packages() {
  case "$PROFILE" in
    orion) printf '%s\n' hitech-shell hitech-installer ai-daemon genio hitech-store foot mosquitto influxdb2 telegraf ;;
    edge|server) printf '%s\n' hitech-installer ai-daemon genio hitech-store foot mosquitto influxdb2 telegraf ;;
    minimal) printf '%s\n' ai-daemon foot mosquitto ;;
    *) die "unknown profile: $PROFILE" ;;
  esac
}

service_units() {
  case "$PROFILE" in
    minimal) printf '%s\n' hitechos-ai-daemon mosquitto ;;
    *) printf '%s\n' hitech-store genio hitechos-ai-daemon mosquitto influxdb2 telegraf ;;
  esac
}

resolve_source() {
  if [[ -n "$SOURCE_DIR" ]]; then
    [[ -f "$SOURCE_DIR/flake.nix" ]] || die "--source does not contain flake.nix: $SOURCE_DIR"
    SOURCE_DIR=$(cd -- "$SOURCE_DIR" && pwd)
    FLAKE_REF="path:$SOURCE_DIR"
    return
  fi
  if [[ -f "$SCRIPT_DIR/../flake.nix" ]]; then
    SOURCE_DIR=$(cd -- "$SCRIPT_DIR/.." && pwd)
    FLAKE_REF="path:$SOURCE_DIR"
    return
  fi
  if [[ -n "${HITECH_FLAKE:-}" ]]; then
    FLAKE_REF="$HITECH_FLAKE"
    return
  fi
  SOURCE_DIR="/opt/hitech-os"
  FLAKE_REF="github:HiTechTN/HiTech-OS"
  if [[ $DRY_RUN -eq 1 ]]; then return; fi
  if [[ ! -d "$SOURCE_DIR/.git" ]]; then
    run_root mkdir -p "$(dirname "$SOURCE_DIR")"
    run_root git clone --depth 1 https://github.com/HiTechTN/HiTech-OS.git "$SOURCE_DIR"
  else
    run_root git -C "$SOURCE_DIR" fetch --depth 1 origin main
    run_root git -C "$SOURCE_DIR" reset --hard FETCH_HEAD
  fi
  FLAKE_REF="path:$SOURCE_DIR"
}

install_nix() {
  local installer
  if cmd_exists nix; then
    ok "Nix already installed: $(nix --version 2>/dev/null || true)"
  elif [[ $DRY_RUN -eq 1 ]]; then
    log "[dry-run] install multi-user Nix from https://nixos.org/nix/install"
  else
    cmd_exists curl || die "curl is required to install Nix"
    installer=$(mktemp)
    log "downloading the official Nix installer"
    curl -fsSL https://nixos.org/nix/install -o "$installer"
    if [[ $WSL -eq 1 && $SYSTEMD -eq 0 ]]; then
      warn "WSL2 without systemd: using single-user Nix (enable systemd=true in /etc/wsl.conf for daemon mode)"
      run_root sh "$installer" --no-daemon
    elif [[ $ASSUME_YES -eq 1 ]]; then
      NIX_INSTALLER_YES=1 run_root sh "$installer" --daemon
    else
      run_root sh "$installer" --daemon
    fi
    rm -f "$installer"
    # Make the daemon's nix binary visible to the rest of this process.
    export PATH="/nix/var/nix/profiles/default/bin:$PATH"
    command -v nix >/dev/null 2>&1 || die "Nix installer finished but nix is not on PATH"
  fi
  if [[ $DRY_RUN -eq 0 ]]; then
    if [[ ! -f "$(root_path /etc/nix/nix.conf)" ]]; then
      write_file /etc/nix/nix.conf 0644 "experimental-features = nix-command flakes
trusted-users = root ${SUDO_USER:-}
sandbox = relaxed
"
    else
      append_once /etc/nix/nix.conf "experimental-features" "experimental-features = nix-command flakes"
      append_once /etc/nix/nix.conf "trusted-users" "trusted-users = root ${SUDO_USER:-}"
      append_once /etc/nix/nix.conf "sandbox" "sandbox = relaxed"
    fi
    if [[ $SYSTEMD -eq 1 ]]; then run_root systemctl restart nix-daemon.service >/dev/null 2>&1 || true; fi
  else
    log "[dry-run] enable nix-command + flakes in /etc/nix/nix.conf"
  fi
}

ensure_rsync() {
  if cmd_exists rsync; then return; fi
  if [[ $DRY_RUN -eq 1 ]]; then log "[dry-run] install rsync"; return; fi
  case "$DISTRO_FAMILY" in
    debian) run_root apt-get update; run_root apt-get install -y rsync ;;
    arch) run_root pacman -Sy --noconfirm rsync ;;
    fedora) run_root dnf install -y rsync ;;
    *) die "rsync is required; install it and rerun" ;;
  esac
}

backup_nixos() {
  local stamp backup
  stamp=$(date -u +%Y%m%dT%H%M%SZ)
  backup="$(root_path "$BACKUP_ROOT")/nixos-$stamp"
  run_root mkdir -p "$backup"
  if [[ -d "$(root_path /etc/nixos)" ]]; then
    run_root rsync -a --delete "$(root_path /etc/nixos)/" "$backup/"
    ok "backed up /etc/nixos to $backup"
  else
    warn "/etc/nixos does not exist; no host configuration to back up"
  fi
}

morph_nixos() {
  local target host_src
  target="$(root_path /etc/nixos/hitech-os)"
  host_src="$(root_path /etc/nixos)"
  ensure_rsync
  backup_nixos
  run_root mkdir -p "$target" "$target/host"
  if [[ "$(realpath -m -- "$SOURCE_DIR")" != "$(realpath -m -- "$target")" ]]; then
    run_root rsync -aL --delete --exclude .git --exclude host "$SOURCE_DIR/" "$target/"
  fi
  # Preserve the host's generated hardware/bootloader/network modules. They
  # are evaluated by os/profiles/* through flake's hostConfiguration.
  if [[ -d "$host_src" ]]; then
    run_root rsync -a --delete --exclude hitech-os --exclude host "$host_src/" "$target/host/"
  fi
  [[ $DRY_RUN -eq 1 ]] || [[ -f "$target/flake.nix" ]] || die "source copy did not produce $target/flake.nix"
  log "running nixos-rebuild boot --flake $target#$PROFILE"
  run_root nixos-rebuild boot --flake "$target#$PROFILE"
  ok "HiTech-OS $PROFILE staged for the next boot"
}

profile_install_one() {
  local pkg=$1 flake=$2 nixbin=$3
  if [[ $DRY_RUN -eq 1 ]]; then
    log "[dry-run] nix profile install $flake#$pkg"
    return 0
  fi
  run_root "$nixbin" profile remove --profile "$NIX_PROFILE" "$pkg" >/dev/null 2>&1 || true
  run_root "$nixbin" profile install --profile "$NIX_PROFILE" "$flake#$pkg"
}

install_nix_packages() {
  local nixbin pkg
  nixbin=$(command -v nix) || die "nix is not installed"
  for pkg in $(profile_packages); do
    profile_install_one "$pkg" "$FLAKE_REF" "$nixbin"
  done
}

install_services() {
  local bin profiles
  [[ $NO_SERVICES -eq 1 ]] && { warn "systemd units skipped (--no-services)"; return; }
  [[ $SYSTEMD -eq 1 ]] || { warn "systemd not available; service units were not enabled"; return; }
  profiles="/nix/var/nix/profiles/default/bin"
  for user in hitech-store genio mosquitto influxdb2 telegraf; do
    if ! getent group "$user" >/dev/null 2>&1; then
      run_root groupadd --system "$user" || true
    fi
    if ! id "$user" >/dev/null 2>&1; then
      run_root useradd --system --gid "$user" --home "/var/lib/$user" --shell /usr/sbin/nologin "$user" || true
    fi
  done
  run_root mkdir -p /var/lib/hitech-store /var/lib/genio /var/lib/mosquitto /var/lib/influxdb2 /var/lib/telegraf /etc/hitechos
  write_if_missing /etc/hitechos/hitech-store.env 0600 "SECRET_KEY=$(random_hex 32)\nSTORAGE_DIR=/var/lib/hitech-store/storage\nAGENT_SHARED_SECRET=$(random_hex 24)\nDEV_OTP_FALLBACK=0\nCOOKIE_SECURE=0\n"
  write_file /etc/hitechos/genio.env 0640 "HITECH_STORE_URL=http://127.0.0.1:8095\n"
  write_file /etc/hitechos/mosquitto.conf 0644 "listener 1883 127.0.0.1\nallow_anonymous true\npersistence true\npersistence_location /var/lib/mosquitto/\n"
  write_file /etc/hitechos/influxdb2.conf 0644 "[http]\n  bind-address = \"127.0.0.1:8086\"\n\n[storage]\n  bolt-path = \"/var/lib/influxdb2/influxd.bolt\"\n  engine-path = \"/var/lib/influxdb2/engine\"\n"
  write_file /etc/hitechos/telegraf.conf 0644 "[[inputs.mqtt_consumer]]\n  servers = [\"tcp://127.0.0.1:1883\"]\n  topics = [\"hitechos/+/+/+\"]\n  data_format = \"json\"\n[[outputs.influxdb_v2]]\n  urls = [\"http://127.0.0.1:8086\"]\n  token = \"\${INFLUX_TOKEN}\"\n  organization = \"HiTechTN\"\n  bucket = \"hitechos\"\n"
  write_file /etc/systemd/system/hitech-store.service 0644 "[Unit]\nDescription=HiTech Store (HiTech Drive)\nAfter=network.target\n[Service]\nUser=hitech-store\nGroup=hitech-store\nWorkingDirectory=/var/lib/hitech-store\nEnvironmentFile=/etc/hitechos/hitech-store.env\nEnvironment=LOG_DIR=/var/lib/hitech-store/logs\nExecStart=$profiles/hitech-store app:app --host 127.0.0.1 --port 8095 --workers 2\nRestart=on-failure\nNoNewPrivileges=true\nProtectSystem=full\n[Install]\nWantedBy=multi-user.target\n"
  write_file /etc/systemd/system/genio.service 0644 "[Unit]\nDescription=Genio local runtime and dashboard\nAfter=network.target hitech-store.service\n[Service]\nUser=genio\nGroup=genio\nEnvironmentFile=/etc/hitechos/genio.env\nEnvironment=GENIO_HOST=127.0.0.1\nEnvironment=GENIO_PORT=8765\nEnvironment=GENIO_STATE_DIR=/var/lib/genio\nExecStart=$profiles/genio-daemon\nRestart=on-failure\nNoNewPrivileges=true\n[Install]\nWantedBy=multi-user.target\n"
  write_file /etc/systemd/system/hitechos-ai-daemon.service 0644 "[Unit]\nDescription=HiTech-OS local inference daemon\nAfter=network.target\n[Service]\nType=oneshot\nRemainAfterExit=yes\nExecStart=$profiles/hitechos-ai-daemon\n[Install]\nWantedBy=multi-user.target\n"
  write_file /etc/systemd/system/mosquitto.service 0644 "[Unit]\nDescription=Mosquitto MQTT broker (HiTech-OS)\nAfter=network.target\n[Service]\nUser=mosquitto\nGroup=mosquitto\nExecStart=$profiles/mosquitto -c /etc/hitechos/mosquitto.conf\nRestart=on-failure\n[Install]\nWantedBy=multi-user.target\n"
  write_file /etc/systemd/system/influxdb2.service 0644 "[Unit]\nDescription=InfluxDB 2 (HiTech-OS telemetry)\nAfter=network.target\n[Service]\nUser=influxdb2\nGroup=influxdb2\nEnvironment=INFLUXD_BOLT_PATH=/var/lib/influxdb2/influxd.bolt\nEnvironment=INFLUXD_ENGINE_PATH=/var/lib/influxdb2/engine\nEnvironment=INFLUXD_HTTP_BIND_ADDRESS=127.0.0.1:8086\nExecStart=$profiles/influxd\nRestart=on-failure\n[Install]\nWantedBy=multi-user.target\n"
  write_file /etc/systemd/system/telegraf.service 0644 "[Unit]\nDescription=Telegraf MQTT to InfluxDB bridge (HiTech-OS)\nAfter=network.target mosquitto.service influxdb2.service\n[Service]\nUser=telegraf\nGroup=telegraf\nEnvironmentFile=-/etc/hitechos/telegraf.env\nExecStart=$profiles/telegraf --config /etc/hitechos/telegraf.conf\nRestart=on-failure\n[Install]\nWantedBy=multi-user.target\n"
  if [[ $DRY_RUN -eq 1 ]]; then return; fi
  run_root systemctl daemon-reload
  for unit in $(service_units); do
    run_root systemctl enable --now "$unit.service" || warn "$unit.service failed to start"
  done
}

install_desktop() {
  local exec_bin="/nix/var/nix/profiles/default/bin/hitech-shell"
  write_file /usr/share/wayland-sessions/hitech-shell.desktop 0644 "[Desktop Entry]\nName=HiTech Shell\nComment=HiTech-OS Orion-X Wayland compositor\nExec=$exec_bin\nType=Application\nDesktopNames=HiTechShell\n"
  write_file /etc/sddm.conf.d/10-hitechos.conf 0644 "[General]\nDisplayServer=wayland\n[Users]\nDefaultSession=hitech-shell\n"
  write_file /etc/xdg/foot/foot.ini 0644 "[main]\nfont=monospace:size=11\n[colors]\nbackground=101820\nforeground=e8f4ff\nregular0=101820\nregular1=ff4d6d\nregular2=39ff88\nregular3=ffd166\nregular4=38bdf8\nregular5=c084fc\nregular6=22d3ee\nregular7=e8f4ff\n"
  if [[ $DRY_RUN -eq 0 ]] && cmd_exists update-desktop-database; then run_root update-desktop-database /usr/share/wayland-sessions >/dev/null 2>&1 || true; fi
}

install_branding() {
  local build_id
  build_id=$(date -u +%Y%m%dT%H%M%SZ)
  write_file /etc/hitechos-release 0644 "NAME=\"HiTech-OS\"\nID=hitechos\nID_LIKE=nixos\nVERSION=\"Orion-X\"\nPRETTY_NAME=\"HiTech-OS Orion-X\"\nHOME_URL=\"https://github.com/HiTechTN/HiTech-OS\"\nSUPPORT_URL=\"https://github.com/HiTechTN/HiTech-OS/issues\"\nBUILD_ID=\"$build_id\"\nAUTHOR=\"Mohamed Azmi Kaaniche (AZMI) / HiTechTN / HiTechLab\"\n"
  write_file /etc/hitechos/os-release 0644 "$(cat <<RELEASE
NAME="HiTech-OS"
ID=hitechos
ID_LIKE=nixos
VERSION="Orion-X"
PRETTY_NAME="HiTech-OS Orion-X"
HOME_URL="https://github.com/HiTechTN/HiTech-OS"
BUILD_ID="$build_id"
AUTHOR="Mohamed Azmi Kaaniche (AZMI) / HiTechTN / HiTechLab"
RELEASE
)"
  write_file /etc/hitechos/theme/orion-x.css 0644 ":root {\n  --hitech-bg: #101820;\n  --hitech-fg: #e8f4ff;\n  --hitech-cyan: #22d3ee;\n  --hitech-magenta: #ff2bd6;\n  --hitech-emerald: #39ff88;\n}\n"
  write_file /etc/motd 0644 "HiTech-OS Orion-X\nSovereign, reproducible edge infrastructure.\nEngineering: Mohamed Azmi Kaaniche (AZMI) / HiTechTN / HiTechLab\n"
}

verify_install() {
  local failures=0 path
  log "verifying HiTech-OS installation"
  detect_os; detect_hardware; print_detection
  for path in /etc/hitechos-release /etc/hitechos/os-release /etc/xdg/foot/foot.ini; do
    if [[ -f "$(root_path "$path")" ]]; then ok "$path"; else warn "missing $path"; failures=$((failures + 1)); fi
  done
  if [[ "$DISTRO_FAMILY" == "nixos" ]]; then
    if [[ -f "$(root_path /etc/nixos/hitech-os/flake.nix)" ]]; then ok "NixOS flake source"; else warn "missing /etc/nixos/hitech-os/flake.nix"; failures=$((failures + 1)); fi
    if cmd_exists nixos-rebuild; then ok "nixos-rebuild available"; else warn "nixos-rebuild missing"; failures=$((failures + 1)); fi
  else
    if [[ -x "$(root_path /nix/var/nix/profiles/default/bin/hitech-shell)" ]]; then ok "hitech-shell installed"; else warn "hitech-shell not in system profile"; failures=$((failures + 1)); fi
    if [[ -f "$(root_path /usr/share/wayland-sessions/hitech-shell.desktop)" ]]; then ok "Wayland session registered"; else warn "Wayland session missing"; failures=$((failures + 1)); fi
  fi
  if [[ $SYSTEMD -eq 1 ]]; then
    for unit in $(service_units); do
      if systemctl is-enabled --quiet "$unit.service" 2>/dev/null; then ok "$unit enabled"; else warn "$unit not enabled"; failures=$((failures + 1)); fi
    done
  fi
  (( failures == 0 )) || die "$failures verification check(s) failed"
  ok "verification passed"
}

save_report() {
  local target
  target=$(root_path /etc/hitechos/bootstrap-report)
  [[ $DRY_RUN -eq 1 ]] && return
  mkdir -p "$(dirname "$target")"
  {
    printf 'profile=%s\n' "$PROFILE"
    printf 'os=%s\n' "$OS_ID"
    printf 'family=%s\n' "$DISTRO_FAMILY"
    printf 'wsl=%s\n' "$WSL"
    printf 'systemd=%s\n' "$SYSTEMD"
    printf 'cpu_arch=%s\n' "$CPU_ARCH"
    printf 'cpu_vendor=%s\n' "$CPU_VENDOR"
    printf 'cpu_model=%s\n' "$CPU_MODEL"
    printf 'ram_mib=%s\n' "$RAM_MIB"
    printf 'gpu_vendor=%s\n' "$GPU_VENDOR"
    printf 'gpu_model=%s\n' "$GPU_MODEL"
    printf 'vram_mib=%s\n' "$VRAM_MIB"
    printf 'timestamp=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  } >"$target"
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --profile) [[ $# -ge 2 ]] || die "--profile requires a value"; PROFILE=$2; shift 2 ;;
      --profile=*) PROFILE=${1#*=}; shift ;;
      --source) [[ $# -ge 2 ]] || die "--source requires a value"; SOURCE_DIR=$2; shift 2 ;;
      --source=*) SOURCE_DIR=${1#*=}; shift ;;
      --dry-run) DRY_RUN=1; shift ;;
      --verify) VERIFY_ONLY=1; shift ;;
      --no-services) NO_SERVICES=1; shift ;;
      --root) [[ $# -ge 2 ]] || die "--root requires a value"; ROOT=$2; shift 2 ;;
      --root=*) ROOT=${1#*=}; shift ;;
      -y|--yes) ASSUME_YES=1; shift ;;
      -h|--help) usage; exit 0 ;;
      *) die "unknown argument: $1 (try --help)" ;;
    esac
  done
  valid_profile "$PROFILE" || die "invalid profile: $PROFILE"
  ROOT="${ROOT%/}"; [[ -n "$ROOT" ]] || ROOT="/"
}

main() {
  parse_args "$@"
  if [[ $DRY_RUN -eq 0 && $VERIFY_ONLY -eq 0 && "$ROOT" == "/" ]]; then
    mkdir -p "$(dirname "$LOG_FILE")" 2>/dev/null || true
    exec > >(tee -a "$LOG_FILE") 2>&1
  fi
  detect_os
  detect_hardware
  print_detection
  if [[ $VERIFY_ONLY -eq 1 ]]; then verify_install; return; fi
  log "profile=$PROFILE family=$DISTRO_FAMILY wsl=$WSL systemd=$SYSTEMD dry_run=$DRY_RUN"
  if [[ $DRY_RUN -eq 1 ]]; then
    resolve_source
    log "[dry-run] would install Nix and apply the $PROFILE profile"
    log "[dry-run] source: $FLAKE_REF"
    log "[dry-run] packages: $(profile_packages | tr '\n' ' ')"
    log "[dry-run] desktop: /usr/share/wayland-sessions/hitech-shell.desktop"
    log "[dry-run] services: hitech-store genio hitechos-ai-daemon mosquitto influxdb2 telegraf"
    log "[dry-run] branding: /etc/hitechos-release, /etc/motd, /etc/xdg/foot/foot.ini"
    ok "dry-run complete; no changes made"
    return
  fi
  ensure_root
  resolve_source
  install_nix
  if [[ "$DISTRO_FAMILY" == "nixos" ]]; then
    morph_nixos
  else
    install_nix_packages
    install_desktop
    install_services
  fi
  install_branding
  save_report
  ok "HiTech-OS $PROFILE bootstrap complete"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi
