# HiTech-OS — configuration NixOS du nœud "patient-zero" (disque/VM réels).
#
# Réglages spécifiques à une installation persistante sur disque : boot
# GRUB, filesystems (via hardware-configuration.nix), OTA. Tout le reste
# (services, durcissement, empreinte minimale) est partagé avec l'image
# ISO live via ./common.nix.

{ config, lib, pkgs, ... }:

{
  imports = [
    ./hardware-configuration.nix
    ./common.nix
    ./ecosystem.nix # outillage HiTechTN (AgentOS/app2nix) — audit 22/10/2026
  ];

  # --- Boot (spécifique disque, sans objet sur une ISO live) ---
  boot.loader.grub.enable = lib.mkDefault true;
  boot.loader.grub.device = lib.mkDefault "/dev/sda";

  # --- OTA (Phase 6) ---
  # Rollback natif : chaque switch crée une nouvelle génération de boot ;
  # `nixos-rebuild switch --rollback` (ou le sélecteur GRUB au boot) revient
  # instantanément à la précédente si la nouvelle casse quelque chose.
  system.autoUpgrade = {
    enable = true;
    # Ancien nom du dépôt (redirigé 301 par GitHub aujourd'hui, mais
    # fragile : le flake OTA doit suivre le renommage vibe-os → HiTech-OS).
    flake = "github:HiTechTN/HiTech-OS#patient-zero";
    dates = "04:00"; # fenêtre de maintenance nocturne, à ajuster par site
    allowReboot = false; # un reboot auto sur un nœud de prod est un choix à valider, pas un défaut
  };
  # Garde-fou : on ne garde que les N dernières générations pour ne pas
  # saturer le disque avec l'historique OTA.
  boot.loader.grub.configurationLimit = 10;
}
