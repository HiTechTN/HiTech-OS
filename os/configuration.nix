# HiTech-OS — configuration NixOS minimale (Phase 1)
# Objectif : boot headless, <150 Mo RAM idle, conteneurisation native
# via systemd-nspawn. À itérer au fil de la Phase 1.

{ config, pkgs, ... }:

{
  # Kernel allégé : pas d'audio, pas de GPU desktop, pas de Bluetooth
  boot.kernelPackages = pkgs.linuxPackages_latest;
  boot.blacklistedKernelModules = [ "snd" "bluetooth" ];

  # Pas d'interface graphique
  services.xserver.enable = false;

  # Réseau minimal
  networking.useDHCP = true;
  networking.firewall.allowedTCPPorts = [ 1883 ]; # MQTT (Mosquitto)

  # Conteneurisation native
  boot.enableContainers = true;

  # Services de base pour le nœud (Phase 2/3)
  services.mosquitto.enable = true;

  system.stateVersion = "24.05";
}
