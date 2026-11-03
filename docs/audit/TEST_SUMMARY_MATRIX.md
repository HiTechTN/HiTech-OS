# HiTech-OS Phase 8.5 — Test Summary Matrix

Date: 2026-11-03
Commit: `fcec72d09c328f4d22200e572783ac5cede3d299`
Legend: PASS = observed; BLOCKED = external prerequisite missing;
GAP = product capability absent; N/A = not applicable to current scope.

| ID | Test | Expected | Result | Evidence |
|---|---|---|---|---|
| T1 | Build final NetInstaller ISO | ISO builds at tested commit | **PASS** | `pjgicn7...` ISO, 2,097,152,000 B, SHA-256 `30e3b7e1...` |
| T2 | Build final Live ISO | ISO builds at tested commit | **PASS** | `l7g4c3...` ISO, 3,546,349,568 B, SHA-256 `bcc44a1a...` |
| T3 | Verify ISO payload | Both ISOs contain the tested `hitech-installer` | **PASS** | Both squashfs stores contain `i5g7srfd...-hitech-installer-2.0.0`; fresh `nix build .#hitech-installer` returns the same path |
| T4 | Boot final Live ISO | GNOME desktop + ORION-X installer | **PASS** | `/data/e2e-hitechos/live-final-screen.png` |
| T5 | Clean install from NetInstaller | API reports success; target reboot-ready | **PASS** | `running=false progress=100`, `Installation complete. Target is ready to reboot.` |
| T6 | Installer status responsiveness | `/api/status` responds during install | **PASS** | Status polled through completion; no hang after `b3378f1` |
| T7 | Target disk boot | UEFI boot to SDDM without ISO | **PASS** | `/data/e2e-hitechos/final2c-sddm.png` |
| T8 | Session start | SDDM starts `hitech-shell`, creates `wayland-1` | **PASS** | session seat0/VT2; PID 33775; `/run/user/1000/wayland-1` |
| T9 | Wayland client render | Real client maps and renders | **PASS** | `foot` PID 34040; `/data/e2e-hitechos/final2-foot.png` |
| T10 | 30-second stability | Compositor/client stay alive, video captured | **PASS** | `/data/e2e-hitechos/final-clean-stability.mp4` (30.000 s) |
| T11 | Fresh install with ISO defaults | Install from GitHub ref | **BLOCKED** | `fcec72d` is not pushed; default ISO fetch would fail. Test used `file:///hostrepo` |
| T12 | SDDM autologin | Automatic session without password | **N/A** | Installer does not generate `autoLogin`; manual login required |
| T13 | Installed terminal/client | Clean install ships a usable client | **GAP** | `foot` was fetched at runtime with `nix shell nixpkgs#foot`; not in target closure |
| T14 | Physical hardware | Real GPU/input/display | **NOT TESTED** | QEMU/OVMF/virtio-gpu only |
| T15 | Long soak | Multi-hour idle/load, suspend/resume | **NOT TESTED** | 30 s smoke only |

## Commands and artifacts

- Final ISO build: `nix build --no-link --print-out-paths .#nixosConfigurations.{netinstaller,iso}.config.system.build.isoImage`
- Clean install backend: port 17871 with
  `HITECH_REPO_URL=file:///hostrepo`,
  `HITECH_REPO_REF=fcec72d09c328f4d22200e572783ac5cede3d299`,
  `HITECH_STATE_VERSION=26.11`.
- Target inspection: `ssh -p 2225 commander@127.0.0.1` (lab key injected offline).
- Client launch:
  `XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1 foot ...`
- Capture: `ffmpeg -f x11grab -video_size 1280x800 -framerate 10 -t 30 ...`
- Installer status text and target configuration are quoted in
  `INSTALLATION_VERIFICATION.md`.
