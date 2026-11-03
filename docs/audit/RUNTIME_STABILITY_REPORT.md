# HiTech-OS Phase 8.5 — Runtime Stability Report

Status: **PASS for the exercised 30-second clean-install session smoke test**
Date: 2026-11-03
Tested commit: `fcec72d09c328f4d22200e572783ac5cede3d299`
Primary target: clean QEMU install on `/data/e2e-hitechos/final-target2.qcow2`

## What was exercised

1. Boot the installed disk without the ISO under OVMF.
2. Reach SDDM with the `HiTech Shell` Wayland session selected.
3. Log in as `commander` with the installer-created password.
4. Confirm `hitech-shell` starts on seat0/VT2 and creates `wayland-1`.
5. Launch `foot` against the compositor socket.
6. Record the rendered desktop for 30 seconds and check process liveness.

## Results

| Observation | Result |
|---|---|
| SDDM | Active; greeter rendered; user `commander` listed |
| Session command | `hitech-shell` selected as Wayland session |
| Compositor process | PID 33775, state `Ssl+`, alive for the full test |
| Wayland socket | `/run/user/1000/wayland-1` present |
| Client process | `foot` PID 34040, alive for the full recording |
| Client window | CSD title bar `HiTech Final — Clean Install`; terminal text visible |
| Recording | 30.000 s, 30,179 bytes, H.264 |
| Crash/panic | None observed in the session |
| Process check at end | compositor uptime 3:01; client uptime 2:01 |

The earlier target at `8e08547` also produced rendered `foot` evidence after
the compositor flush fix: 15-second clip
`/data/e2e-hitechos/hitechos-stability-test.mp4` and frame
`/data/e2e-hitechos/stability-frame.png`.

## Evidence

- `/data/e2e-hitechos/final-clean-stability.mp4` — 30 s clean-target recording
- `/data/e2e-hitechos/final-clean-stability-frame.png` — final frame
- `/data/e2e-hitechos/final2-foot.png` — still of the rendered terminal
- `/data/e2e-hitechos/final2c-sddm.png` — SDDM login screen
- `/data/e2e-hitechos/hitechos-stability-test.mp4` — earlier 15 s target clip

## Root causes fixed before this run

1. **`libEGL`/Mesa not visible to the session.** The product wrapper did not
   export the graphics runtime paths. `cbbdb5e` added `LD_LIBRARY_PATH` for
   `libGL`, `mesa`, `libgbm`, `libdrm`, `libinput`, `wayland`,
   `libxkbcommon`, `seatd`, and `udev`, plus `LIBGL_DRIVERS_PATH=${mesa}/lib/dri`.
2. **Clients hung during `wl_display.sync`.** The compositor dispatched client
   messages but did not flush replies. `8e08547` adds
   `display.flush_clients()` after `dispatch_clients()`.
3. **Unreadable compositor background.** The clear colour was changed to
   `[0.015, 0.09, 0.12, 1.0]` so a working session is visually distinct from a
   black failed session.

## Client protocol observations

`foot` emitted non-fatal warnings against the current compositor:

- no primary-selection interface
- no XDG activation; urgent-bell fallback
- no fractional scaling
- no server-side cursor; client-side fallback
- no `xdg-toplevel-icon`
- no text-input interface; IME disabled
- no decoration manager; client-side decorations used unconditionally

These are feature gaps, not crashes. The important result is that a real
`wl_compositor`/`xdg_wm_base`/`wl_shm` client maps and renders.

## Known limitations

- No panel, launcher, or first-party shell chrome yet; the session is a bare
  compositor surface until a client is started externally.
- No `linux-dmabuf`, so the exercised path is shared-memory (`wl_shm`) only.
- Move/resize and server-side cursor are still stubs.
- No physical GPU or physical input validation; QEMU uses virtio-gpu with
  software rendering.
- 30 seconds is a smoke test, not a soak. A multi-hour idle/load test, repeated
  client launch/exit, VT switching, and suspend/resume remain open.
