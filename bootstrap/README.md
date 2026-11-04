# HiTech-OS universal bootstrap

`hitech-bootstrap` turns a stock host into a HiTech-OS node. It is the
non-ISO path used by Method B in the repository README.

## Supported hosts

| Host | Strategy |
| --- | --- |
| NixOS | Backup `/etc/nixos`, copy the HiTech-OS flake to `/etc/nixos/hitech-os`, preserve the host's generated `host/` configuration, then `nixos-rebuild boot --flake /etc/nixos/hitech-os#<profile>`. |
| Debian / Ubuntu / Pop!_OS | Install multi-user Nix, install the HiTech runtime into the Nix system profile, register the Wayland session, and enable systemd units. |
| Arch / Manjaro / EndeavourOS | Same generic-Linux strategy using `pacman` only for the `rsync` prerequisite. |
| Fedora / RHEL-family | Same generic-Linux strategy using `dnf` only for the `rsync` prerequisite. |
| Windows 11 + WSL2 | `hitech-bootstrap.ps1` detects Windows hardware, validates/converts the WSL distro to WSL2, then runs the Linux engine inside WSL. |

## Usage

```bash
# Preview every action
sudo ./bootstrap/hitech-bootstrap.sh --profile orion --dry-run

# NixOS: stage the next boot
sudo ./bootstrap/hitech-bootstrap.sh --profile orion

# Generic Linux: install Nix + HiTech services
sudo ./bootstrap/hitech-bootstrap.sh --profile edge

# Read-only verification
sudo ./bootstrap/hitech-bootstrap.sh --verify
```

```powershell
powershell -ExecutionPolicy Bypass -File .\bootstrap\hitech-bootstrap.ps1 -Profile orion -DryRun
powershell -ExecutionPolicy Bypass -File .\bootstrap\hitech-bootstrap.ps1 -Profile orion
```

## Profiles

| Profile | Desktop | Core runtime | Telemetry |
| --- | --- | --- | --- |
| `orion` | HiTech Shell + SDDM + foot | installer, AI daemon, Genio, Store | Mosquitto, InfluxDB 2, Telegraf |
| `edge` | foot | installer, AI daemon, Genio, Store | Mosquitto, InfluxDB 2, Telegraf |
| `server` | foot | installer, AI daemon, Genio, Store | Mosquitto, InfluxDB 2, Telegraf |
| `minimal` | foot | AI daemon | Mosquitto |

## Generated paths

| Path | Purpose |
| --- | --- |
| `/etc/nixos/hitech-os` | NixOS flake checkout injected by bootstrap. |
| `/etc/nixos/hitech-os/host` | Preserved host `configuration.nix`, `hardware-configuration.nix`, and local modules. |
| `/var/backups/hitech-os/nixos-<UTC>` | Timestamped backup of the original `/etc/nixos`. |
| `/usr/share/wayland-sessions/hitech-shell.desktop` | Generic-Linux Wayland session entry. |
| `/etc/hitechos/hitech-store.env` | Generated secrets; preserved across reruns. |
| `/etc/systemd/system/hitech-store.service` | Store API unit. |
| `/etc/systemd/system/genio.service` | Genio local runtime unit. |
| `/etc/systemd/system/mosquitto.service` | MQTT broker unit. |
| `/etc/systemd/system/influxdb2.service` | Telemetry database unit. |
| `/etc/hitechos/bootstrap-report` | Detected hardware/profile report. |

## Tests

```bash
bash bootstrap/tests/test-bootstrap.sh
```

Engineering and documentation: Mohamed Azmi Kaaniche (AZMI), HiTechTN,
HiTechLab. License: AGPL-3.0-only.
