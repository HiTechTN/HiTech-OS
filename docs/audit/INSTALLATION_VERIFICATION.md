# HiTech-OS Phase 8.5 — Final Installation Verification

Status: **PASS, with a documented lab repository override**
Date: 2026-11-03
Tested commit: `fcec72d09c328f4d22200e572783ac5cede3d299`
Repo: `github.com/HiTechTN/HiTech-OS`

## Scope

This report covers the final NetInstaller ISO, the final Live ISO, one clean
QEMU install from the NetInstaller, the first boot of the installed disk, the
SDDM → `hitech-shell` session transition, and rendering of a real Wayland
client. It does **not** claim physical-hardware certification or a long soak.

## Final image artifacts

| Image | Size (bytes) | SHA-256 | Store path |
|---|---:|---|---|
| NetInstaller | 2,097,152,000 | `30e3b7e18e5b203b93a3e3d0166b64395721957de1396f715eb4c03380c4ca9f` | `/nix/store/pjgicn7k2asa0rxaiprrqyg6vlqq6rgq-nixos-netinstaller-26.11.20260919.20b1ddd-x86_64-linux.iso/iso/nixos-netinstaller-26.11.20260919.20b1ddd-x86_64-linux.iso` |
| Live (GNOME + ORION-X) | 3,546,349,568 | `bcc44a1afbf40f8c260dd591b4f500bb07709d19308dfd0ef5a44524ee72b359` | `/nix/store/l7g4c3wimrfmwypganlx54zlaxxargs7-nixos-26.11.20260919.20b1ddd-x86_64-linux.iso/iso/nixos-26.11.20260919.20b1ddd-x86_64-linux.iso` |

Both images were checked for payload identity. Their squashfs stores contain
`i5g7srfdxxfgy3qmlwmmjigs4rb2b4sp-hitech-installer-2.0.0`, and a fresh
`nix build .#hitech-installer` at `fcec72d` returns the same store path. The
NetInstaller also contains the Cage kiosk wrapper
`kdbv28gy6nzad9cnpjbvlvirkha43swj-hitech-installer-kiosk`.

The Live ISO was booted with QEMU/OVMF and reached the GNOME desktop with the
ORION-X installer running in a window. Evidence:
`/data/e2e-hitechos/live-final-screen.png`.

## Clean-install procedure

- Hypervisor: QEMU 6.2.0 with KVM, OVMF UEFI, 8 vCPU, 8 GiB RAM,
  `virtio-vga`, `virtio-net-pci`, 60 GiB qcow2 target.
- Target disk: `/data/e2e-hitechos/final-target2.qcow2`.
- NVRAM: `/data/e2e-hitechos/final2-OVMF_VARS.fd`.
- Install parameters: `mode=erase`, `disk=/dev/vda`, `desktop=orion`,
  `hostname=hitechos-final`, `username=commander`.
- Installer backend: second instance on port `17871`, because the ISO default
  repository ref is not yet published. Environment:
  `HITECH_REPO_URL=file:///hostrepo`,
  `HITECH_REPO_REF=fcec72d09c328f4d22200e572783ac5cede3d299`,
  `HITECH_STATE_VERSION=26.11`, with the host repository exposed through 9p.
- The first attempt failed with
  `git -C /mnt/etc/nixos/hitech-os fetch --depth 1 origin fcec72d exited with
  exit status: 128` because a 7-character SHA was used. Re-running with the
  full 40-character SHA succeeded. This is a lab-script issue, not an installer
  crash.

Final observed status from `/api/status`:

```text
running=false progress=100
status=Installation complete. Target is ready to reboot.
```

No panic occurred, and the status endpoint remained responsive throughout the
run. The earlier empty-status failure was caused by `network_online()` running
under the status mutex; that was fixed in `b3378f1`. The later UTF-8 panic came
from truncating the install log in the middle of a character; that was fixed in
`fcec72d`.

## Installed-target verification

The installer-generated `/etc/nixos/configuration.nix` contains:

- `networking.hostName = "hitechos-final"`
- `users.users."commander"` with a SHA-512 `initialHashedPassword`
- groups: `wheel`, `networkmanager`, `video`, `render`, `input`
- `services.hitech-shell.enable = true`
- `services.displayManager.sddm.enable = true`
- `services.displayManager.sddm.wayland.enable = true`
- `system.stateVersion = "26.11"`

The installer does **not** generate `services.displayManager.autoLogin`.
The final target therefore stops at the SDDM login screen and requires the
operator password. This is expected from the current generated configuration;
the autologin seen on the earlier debug target was a manual test configuration,
not installer output.

After manual login:

| Check | Result |
|---|---|
| Hostname | `hitechos-final` |
| Kernel | `7.2.6` |
| NixOS generation | `/nix/store/nx88aibx3xgw6d44qcclzzzmqhi8mbi7-nixos-system-hitechos-final-26.11pre-git` |
| Disk layout | `vda1` 512 MiB vfat `HITECH-EFI`; `vda2` 59.5 GiB ext4 `nixos` |
| `display-manager` | active |
| NetworkManager | active |
| Graphical session | seat0, VT 2, user `commander` |
| Compositor | `/run/current-system/sw/bin/hitech-shell`, PID 33775 |
| Wayland socket | `/run/user/1000/wayland-1` |
| Client rendering | `foot` 1.28.0 renders a CSD window and terminal text |

Evidence:

- SDDM: `/data/e2e-hitechos/final2c-sddm.png`
- Rendered client: `/data/e2e-hitechos/final2-foot.png`
- 30-second stability clip: `/data/e2e-hitechos/final-clean-stability.mp4`
- Last-frame proof: `/data/e2e-hitechos/final-clean-stability-frame.png`
- Clean-install recording: `/data/e2e-hitechos/final-clean-install.mp4`
  (1800 s)

## Lab-only changes made for verification

- An SSH public key was added offline to
  `/home/commander/.ssh/authorized_keys` so the target could be inspected over
  QEMU `hostfwd` port 2225. This did not alter the installer output or the
  session contract.
- `foot` is not part of the installed closure. For the render test it was
  fetched at runtime with `nix shell nixpkgs#foot` and launched against
  `wayland-1`. This proves the compositor can host a real client, but it is not
  evidence that a clean install ships a usable terminal/application set.

## Release blockers and gaps

1. **Unpublished repository ref.** The final ISOs default to
   `https://github.com/HiTechTN/HiTech-OS.git` at `fcec72d...`. That commit is
   not pushed. A fresh install from the ISO without the local `/hostrepo`
   override will fail at profile fetch. Either push the commit or rebuild the
   ISOs with a bundled/local repository source before publishing.
2. **No autologin.** SDDM login is required. Add an explicit product decision
   and installer option if autologin is desired.
3. **No installed client.** The Orion session currently lands on an empty
   compositor surface. The render proof uses a runtime-fetched `foot`; a shipped
   terminal or first-party shell UI is still missing.
4. **Physical hardware not tested.** All results are QEMU/virtio-gpu with
   software rendering. Real GPU, input, and display hotplug remain open.
