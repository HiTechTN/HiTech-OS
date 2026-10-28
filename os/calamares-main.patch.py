#!/usr/bin/env python3
"""Patch the stock Calamares NixOS job for HiTech-OS.

This runs at Nix build time. It must fail loudly if the upstream source moves
so a silently unpatched ISO can never be produced.
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
marker = 'cfggnome = """\\\n'
if 'cfg_orion = """\\\n' not in text:
    if marker not in text:
        raise SystemExit('Calamares source changed: cfg_gnome marker missing')
    text = text.replace(marker, 'cfg_orion = """\\\n' + cfg_orion + '"""\n\n' + marker, 1)

old_desktop = '''    if gs.value("packagechooser_packagechooser") == "gnome":
        cfg += cfg_gnome
'''
if old_desktop not in text:
    # The actual upstream variable is cfggnome (no underscore).
    old_desktop = '''    if gs.value("packagechooser_packagechooser") == "gnome":
        cfg += cfggnome
'''
new_desktop = '''    if gs.value("packagechooser_packagechooser") == "orion":
        cfg += cfg_orion
    elif gs.value("packagechooser_packagechooser") == "gnome":
        cfg += cfggnome
'''
if old_desktop not in text:
    raise SystemExit('Calamares source changed: desktop branch missing')
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

main.chmod(0o644)
main.write_text(text)
print(f'patched {main}: Orion desktop, hostname fallback, retrying installer')
