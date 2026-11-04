# HiTech-OS Minimal profile — hardened headless base with no optional apps.
{ lib, ... }:
{
  imports = [ ../installation.nix ];

  services.hitech-shell.enable = lib.mkForce false;
  services.genio.enable = lib.mkForce false;
  services.hitech-store.enable = lib.mkForce false;
  services.ollama.enable = lib.mkForce false;
  services.mosquitto.enable = lib.mkForce false;
  services.telegraf.enable = lib.mkForce false;
  services.influxdb2.enable = lib.mkForce false;
}
