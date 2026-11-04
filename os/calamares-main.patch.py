#!/usr/bin/env python3
"""Patch the stock Calamares NixOS job for HiTech-OS.

This runs at Nix build time. It must fail loudly if the upstream source moves
so a silently unpatched ISO can never be produced.

Phase 8.3 (2026-10-28): bundle the HiTech-OS source tree inside the ISO and
make the nixos Calamares job copy it to the target + add the installation.nix
import BEFORE running nixos-install, so services.hitech-shell (and every other
HiTech-OS module) is available during the install evaluation.
"""
import sys
from pathlib import Path

main = Path(sys.argv[1])
install_wrapper = sys.argv[2]
text = main.read_text()

# 1. Orion is a packagechooser choice but has no upstream desktop stanza.
cfg_orion = '''    # Enable the HiTech-OS Orion cockpit shell.
    services.xserver.enable = true;
    services.displayManager.sddm.enable = true;
    services.displayManager.sddm.wayland.enable = true;
    services.hitech-shell.enable = true;

'''
# The upstream variable is cfggnome (no underscore) in recent nixpkgs;
# older versions used cfg_gnome. Accept either.
for marker in ('cfg_gnome = """\\\n', 'cfggnome = """\\\n'):
    if marker in text:
        break
else:
    raise SystemExit('Calamares source changed: no cfg_gnome/cfggnome marker found')
if 'cfg_orion = """\\\n' not in text:
    text = text.replace(marker, 'cfg_orion = """\\\n' + cfg_orion + '"""\n\n' + marker, 1)

# The gnome desktop branch uses cfggnome (or cfg_gnome in older nixpkgs).
old_desktop = None
for candidate in (
    '''    if gs.value("packagechooser_packagechooser") == "gnome":
        cfg += cfggnome
''',
    '''    if gs.value("packagechooser_packagechooser") == "gnome":
        cfg += cfg_gnome
''',
):
    if candidate in text:
        old_desktop = candidate
        break
if old_desktop is None:
    raise SystemExit('Calamares source changed: desktop branch missing')
new_desktop = '''    if gs.value("packagechooser_packagechooser") == "orion":
        cfg += cfg_orion
    elif gs.value("packagechooser_packagechooser") == "gnome":
        cfg += cfggnome
'''
text = text.replace(old_desktop, new_desktop, 1)

# 2. The users module may hand us the OEM placeholder. Never write it as the
#    target hostname; prefer the operator's choice and fall back to hitechos.
old_host = '''    if gs.value("hostname") is None:
        catenate(variables, "hostname", "nixos")
    else:
        catenate(variables, "hostname", gs.value("hostname"))
'''
new_host = '''    hostname = gs.value("hostname")
    if hostname is None or hostname == "hitech-tobefilledbyoem":
        hostname = "hitechos"
    catenate(variables, "hostname", hostname)
'''
if old_host not in text:
    raise SystemExit('Calamares source changed: hostname block missing')
text = text.replace(old_host, new_host, 1)

# 3. Route nixos-install through the retrying wrapper. The wrapper adds
#    fallback/substitute options and retries transient squashfs/network I/O.
old_install = '            "nixos-install",\n'
new_install = f'            "{install_wrapper}",\n'
if old_install not in text:
    raise SystemExit('Calamares source changed: nixos-install command missing')
text = text.replace(old_install, new_install, 1)

# 4. Phase 8.3: inject HiTech-OS source bundling + installation.nix import
#    BEFORE nixos-install runs. The ISO ships /etc/hitech-os-source (the full
#    repo) via os/iso.nix. The nixos Calamares job must copy it into the target
#    and add the import line so services.hitech-shell is available during the
#    install evaluation (the old flow cloned git AFTER nixos-install, which was
#    too late).
bundle_code = '''    # HiTech-OS: copy bundled source from ISO and add installation.nix import
    # so services.hitech-shell is available during nixos-install evaluation.
    import os
    import re
    import shutil
    hitech_src = Path("/etc/hitech-os-source")
    if hitech_src.exists():
        target_repo = Path(root_mount_point + "/etc/nixos/hitech-os")
        import_rel = "./hitech-os/os/installation.nix"
        if target_repo.exists():
            shutil.rmtree(target_repo)
        shutil.copytree(hitech_src, target_repo, symlinks=False)
        # Nix store inputs are read-only; make the copied tree writable so
        # the installer can patch/reuse it idempotently.
        for dirpath, dirnames, filenames in os.walk(target_repo):
            for name in dirnames + filenames:
                copied = Path(dirpath) / name
                if not copied.is_symlink():
                    copied.chmod(copied.stat().st_mode | 0o200)
        cfg_path = Path(root_mount_point + "/etc/nixos/configuration.nix")
        if cfg_path.exists():
            cfg_text = cfg_path.read_text()
            if import_rel not in cfg_text:
                # Keep the import inside the existing `imports = [ ... ]`
                # list; a second `imports =` attribute would be invalid.
                match = re.search(r"imports\\s*=\\s*\\[", cfg_text)
                if match:
                    cfg_text = cfg_text[:match.end()] + "\\n    " + import_rel + cfg_text[match.end():]
                else:
                    cfg_text = re.sub(r"^\\s*\\{", "{\\n  imports = [ " + import_rel + " ];", cfg_text, count=1, flags=re.M)
                cfg_path.write_text(cfg_text)
'''

# Insert the statements before the nixos-install command list is built. The
# wrapper argument itself lives inside that list, so inserting there would put
# Python statements inside a list literal and break the Calamares job.
install_cmd_marker = '    # build nixos-install command\n'
if install_cmd_marker not in text:
    install_cmd_marker = '    nixosInstallCmd = [ "pkexec" ]\n'
if install_cmd_marker not in text:
    raise SystemExit('Calamares source changed: nixos-install command marker missing')

text = text.replace(install_cmd_marker, bundle_code + '\n' + install_cmd_marker, 1)

main.chmod(0o644)
main.write_text(text)
print(f'patched {main}: Orion desktop, hostname fallback, bundled hitech-os source + installation.nix import before nixos-install')
