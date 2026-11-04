# HiTech-OS Orion profile — workstation / cockpit target.
#
# Used by `hitech-bootstrap --profile orion` and by the native installer.
# The host's own hardware/bootloader configuration is merged by the flake
# (`host-configuration.nix` when present); this module only adds the
# desktop/session contract.
{ lib, ... }:
{
  imports = [ ../installation.nix ];

  services.xserver.enable = lib.mkForce true;
  services.displayManager.sddm.enable = true;
  services.displayManager.sddm.wayland.enable = true;
  services.displayManager.defaultSession = "hitech-shell";
  services.hitech-shell.enable = true;

  services.libinput.enable = true;
  services.udisks2.enable = true;
  security.polkit.enable = true;
}
