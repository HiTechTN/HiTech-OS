# HiTech-OS Phase 8.6 — Field-Readiness Verification

Status: **PASS**
Date: 2026-11-04
Tested commit: `7691f3107b214237c5cb2cdd65ab1e1b5d5ca059`
Repo: `github.com/HiTechTN/HiTech-OS`

## Scope

This report covers the final NetInstaller and Live ISOs at `7691f31`, a fresh
install using the ISO's default GitHub repository ref, first boot without the
ISO, SDDM autologin into the Orion `hitech-shell` session, and the installed
`foot` terminal. It does **not** claim physical-hardware certification, a Live
ISO re-boot in this run, or a long soak.

## Final image artifacts

| Image | Size (bytes) | SHA-256 | Store path |
|---|---:|---|---|
| NetInstaller | 2,097,152,000 | `e755e53640bf9030131496a820abe6be2e0b6b4de8e34f9a42c407b16b877b9b` | `/nix/store/my1fzycd2y58dh6bpxms3419h9l5q6zk-nixos-netinstaller-26.11.20260919.20b1ddd-x86_64-linux.iso/iso/nixos-netinstaller-26.11.20260919.20b1ddd-x86_64-linux.iso` |
| Live (GNOME + ORION-X) | 3,546,349,568 | `2661af8188de134b596a98b3852379b48aa20268c6c443f39c267aebb625c2b7` | `/nix/store/i5gzayvn36m7xcfggpij894igw51gxh1-nixos-26.11.20260919.20b1ddd-x86_64-linux.iso/iso/nixos-26.11.20260919.20b1ddd-x86_64-linux.iso` |

Both ISOs contain `bi1v7b4gr9bx1akgyjpkykq295zcihb6-hitech-installer-2.0.0`,
matching a fresh `nix build .#hitech-installer` at `7691f31`. The NetInstaller
also contains `da18gax852pv95qr1fdk3lni5x2nd50i-hitech-installer-kiosk`.

The Live ISO source payload contains the `foot` package and the installer
autologin changes. The Live GUI was validated at the parent commit; this run
rebuilt and payload-checked it but did not re-boot the Live ISO.

## Clean install with ISO defaults

- Hypervisor: QEMU 6.2.0, KVM, OVMF UEFI, 8 vCPU, 8 GiB RAM, `virtio-vga`,
  `virtio-net-pci`, 40 GiB qcow2 target.
- Target disk: `/data/e2e-hitechos/field-target.qcow2`.
- NVRAM: `/data/e2e-hitechos/field-OVMF_VARS.fd`.
- Boot media: newly built NetInstaller ISO, no local `/hostrepo` or 9p source.
- Install parameters: `mode=erase`, `disk=/dev/vda`, `desktop=orion`,
  `hostname=hitechos-field`, `username=hitech`.
- The on-ISO backend ran on its default `127.0.0.1:17870`; the request was
  driven over the serial console. No repository override was set.
- Installer fetch log shows the pushed ref:
  `git fetch https://github.com/HiTechTN/HiTech-OS.git (7691f3107b214237c5cb2cdd65ab1e1b5d5ca059)`
  followed by `HEAD is now at 7691f31`.
- `cache.hitechos.local` was unresolved in the lab, so Nix fell back to
  `cache.nixos.org`; the install completed on attempt 1.

Final observed status:

```text
running=false success=true progress=100
status=Installation complete. Target is ready to reboot.
```

## Installed-target verification

The installer-generated `/etc/nixos/configuration.nix` contains:

- `networking.hostName = "hitechos-field"`
- `users.users."hitech"` with a SHA-512 `initialHashedPassword`
- groups: `wheel`, `networkmanager`, `video`, `render`, `input`
- `services.hitech-shell.enable = true`
- `services.displayManager.defaultSession = "hitech-shell";`
- `services.displayManager.autoLogin.enable = true;`
- `services.displayManager.autoLogin.user = "hitech";`
- `system.stateVersion = "26.11"`

The installed disk booted without the ISO. Verification over SSH:

| Check | Result |
|---|---|
| Hostname | `hitechos-field` |
| Kernel | `7.2.6` |
| Installed ref | `7691f3107b214237c5cb2cdd65ab1e1b5d5ca059` |
| SDDM | active; `sddm-helper ... --start hitech-shell --user hitech --autologin` |
| Session | ID 1, user `hitech`, seat0, `tty1`, type `wayland`, active |
| Compositor | `/run/current-system/sw/bin/hitech-shell`, PID 1075 |
| Wayland socket | `/run/user/1000/wayland-1` |
| Installed terminal | `/run/current-system/sw/bin/foot`, version 1.28.0 |
| Client render | `foot` maps a CSD window and prints terminal text |
| 30-second stability | compositor and `foot` alive for the full recording |

Evidence:

- Autologin session: `/data/e2e-hitechos/field-autologin.png`
- Installed `foot`: `/data/e2e-hitechos/field-foot.png`
- 30-second stability clip: `/data/e2e-hitechos/field-stability.mp4`
- Last-frame proof: `/data/e2e-hitechos/field-stability-frame.png`
- New ISO hashes: `/data/e2e-hitechos/new-iso-sha256.txt`
- Flake check log: `/data/e2e-hitechos/flake-check.log`

## Lab-only changes

- An SSH public key was added offline to
  `/home/hitech/.ssh/authorized_keys` so the installed target could be inspected
  over QEMU `hostfwd` port 2226. This did not alter installer output or the
  session contract.
- The serial console was used to invoke the on-ISO backend; it is an access
  path, not a product change.

## Remaining gaps

1. **Physical hardware not tested.** Results are QEMU/virtio-gpu with software
   rendering; real GPU, input, and display hotplug remain open.
2. **No long soak.** The stability check is 30 seconds; multi-hour idle/load,
   repeated client launch/exit, VT switching, and suspend/resume are untested.
3. **Bare compositor.** Orion autologins to `hitech-shell`, but still has no
   panel, launcher, or first-party shell chrome. `foot` is the installed
   terminal; `linux-dmabuf`, server-side cursors, and move/resize remain gaps.
4. **Live ISO not re-booted this run.** Its payload and source were verified;
   the GNOME/ORION-X GUI path was last boot-tested at `fcec72d`.
