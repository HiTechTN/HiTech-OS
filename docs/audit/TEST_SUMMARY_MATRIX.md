# HiTech-OS Phase 8.6 — Test Summary Matrix

Date: 2026-11-04
Commit: `7691f3107b214237c5cb2cdd65ab1e1b5d5ca059`
Legend: PASS = observed; GAP = product capability absent; N/A = not applicable
to current scope.

| ID | Test | Expected | Result | Evidence |
|---|---|---|---|---|
| T1 | Build final NetInstaller ISO | ISO builds at tested commit | **PASS** | `my1fzycd...` ISO, 2,097,152,000 B, SHA-256 `e755e536...` |
| T2 | Build final Live ISO | ISO builds at tested commit | **PASS** | `i5gzayvn...` ISO, 3,546,349,568 B, SHA-256 `2661af81...` |
| T3 | Verify ISO payload | Both ISOs contain the tested `hitech-installer` | **PASS** | Both squashfs stores contain `bi1v7b4g...-hitech-installer-2.0.0`; fresh build returns the same path |
| T4 | Live ISO source payload | Bundles `foot` and autologin changes | **PASS** | Live squashfs source contains `foot` in `os/common.nix` and `autoLogin` in installer source |
| T5 | Clean install with ISO defaults | No `/hostrepo`; fetch pushed commit | **PASS** | Fetch log: `7691f31...`; status `success=true progress=100` |
| T6 | Installer status responsiveness | `/api/status` responds during install | **PASS** | Polled through completion; no hang or panic |
| T7 | Target disk boot | UEFI boot without ISO | **PASS** | Target booted `-boot c`; EFI contains systemd-boot, kernel, initrd |
| T8 | SDDM autologin | Automatic session without password | **PASS** | `sddm-helper ... --start hitech-shell --user hitech --autologin`; session 1 active on tty1/seat0 |
| T9 | Installed terminal | Clean install ships `foot` | **PASS** | `/run/current-system/sw/bin/foot`; version 1.28.0; system-path symlink present |
| T10 | Wayland client render | Real client maps and renders | **PASS** | `foot` CSD window; `/data/e2e-hitechos/field-foot.png` |
| T11 | 30-second stability | Compositor/client stay alive, video captured | **PASS** | `/data/e2e-hitechos/field-stability.mp4`; compositor/foot alive after capture |
| T12 | GitHub default ref | Fresh install uses pushed `7691f31` | **PASS** | `git fetch` + `HEAD is now at 7691f31` in install log |
| T13 | Flake check | `nix flake check` passes | **PASS** | `all checks passed!`; `/data/e2e-hitechos/flake-check.log` |
| T14 | Physical hardware | Real GPU/input/display | **NOT TESTED** | QEMU/OVMF/virtio-gpu only |
| T15 | Long soak | Multi-hour idle/load, suspend/resume | **NOT TESTED** | 30 s smoke only |
| T16 | Live ISO GUI re-boot | GNOME + ORION-X at `7691f31` | **NOT TESTED** | Rebuilt and payload-checked; GUI last boot-tested at `fcec72d` |

## Commands and artifacts

- ISO build:
  `nix build --no-link --print-out-paths .#nixosConfigurations.{netinstaller,iso}.config.system.build.isoImage`
- Flake check:
  `nix flake check --print-build-logs`
- Install request (on-ISO backend):
  `POST /api/install mode=erase disk=/dev/vda desktop=orion hostname=hitechos-field username=hitech`
- Target inspection: `ssh -p 2226 hitech@127.0.0.1` (lab key injected offline).
- Client launch:
  `XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1 foot ...`
- Capture: `ffmpeg -f x11grab -video_size 1280x800 -framerate 10 -t 30 ...`
- New ISO hashes: `/data/e2e-hitechos/new-iso-sha256.txt`
- Install/status evidence and screenshots: `/data/e2e-hitechos/field-*`
