# HiTech-OS Phase 8.6 — Runtime Stability Report

Status: **PASS for the exercised 30-second autologin session smoke test**
Date: 2026-11-04
Tested commit: `7691f3107b214237c5cb2cdd65ab1e1b5d5ca059`
Primary target: fresh QEMU install on `/data/e2e-hitechos/field-target.qcow2`

## What was exercised

1. Boot the installed disk without the ISO under OVMF.
2. Observe SDDM autologin as `hitech` into the `hitech-shell` Wayland session.
3. Confirm `hitech-shell` starts on seat0/tty1 and creates `wayland-1`.
4. Confirm `foot` is present in the installed system path.
5. Launch `foot` against the compositor socket and record for 30 seconds.

## Results

| Observation | Result |
|---|---|
| SDDM | Active; `sddm-helper ... --autologin` for user `hitech` |
| Session | ID 1, `Type=wayland`, `Class=user`, `Active=yes`, `TTY=tty1` |
| Session command | `hitech-shell` |
| Compositor process | PID 1075, alive for the full test |
| Wayland socket | `/run/user/1000/wayland-1` present |
| Installed client | `/run/current-system/sw/bin/foot`, version 1.28.0 |
| Client process | `foot` PID 1411, alive for the full recording |
| Client window | CSD title bar `HiTech Field Terminal`; terminal text visible |
| Recording | 30.000 s, H.264 |
| Crash/panic | None observed |
| Process check after capture | compositor uptime 1:32; client uptime 1:10 |

## Evidence

- `/data/e2e-hitechos/field-autologin.png` — autologin session surface
- `/data/e2e-hitechos/field-foot.png` — installed `foot` rendering
- `/data/e2e-hitechos/field-stability.mp4` — 30 s recording
- `/data/e2e-hitechos/field-stability-frame.png` — final frame
- `/data/e2e-hitechos/flake-check.log` — `nix flake check` output

## Root causes fixed before this run

1. **`libEGL`/Mesa not visible to the session.** `cbbdb5e` added the graphics
   runtime paths for `libGL`, `mesa`, `libgbm`, `libdrm`, `libinput`, `wayland`,
   `libxkbcommon`, `seatd`, and `udev`, plus `LIBGL_DRIVERS_PATH`.
2. **Clients hung during `wl_display.sync`.** `8e08547` flushes client replies
   after dispatching messages.
3. **Unreadable compositor background.** The clear colour is
   `[0.015, 0.09, 0.12, 1.0]`, so a working session is distinct from a failed
   black screen.

## Client protocol observations

`foot` emitted non-fatal warnings against the current compositor:

- no primary-selection interface
- no XDG activation; urgent-bell fallback
- no fractional scaling
- no server-side cursor; client-side fallback
- no `xdg-toplevel-icon`
- no text-input interface; IME disabled
- no decoration manager; client-side decorations used unconditionally

These are feature gaps, not crashes. A real
`wl_compositor`/`xdg_wm_base`/`wl_shm` client maps and renders.

## Known limitations

- No panel, launcher, or first-party shell chrome yet; Orion autologins to a
  bare compositor surface until a client is started.
- No `linux-dmabuf`; the exercised path is shared-memory (`wl_shm`) only.
- Move/resize and server-side cursor are still stubs.
- No physical GPU or physical input validation; QEMU uses virtio-gpu with
  software rendering.
- 30 seconds is a smoke test, not a soak. Multi-hour idle/load, repeated client
  launch/exit, VT switching, and suspend/resume remain open.
