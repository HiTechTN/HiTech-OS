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
    # Profil commun aux nœuds patient-zero et aux installations Calamares :
    # common + ecosystem + hitech-store + genio + secrets.
    ./installation.nix
  ];

  # --- Boot (spécifique disque, sans objet sur une ISO live) ---
  boot.loader.grub.enable = lib.mkDefault true;
  boot.loader.grub.device = lib.mkDefault "/dev/sda";

  # --- Taille du disque de la VM de test (qemu-vm, build-vm) : 100 Go ---
  # Nécessaire pour héberger AgentOS (stack Docker ~8 images) et des LLM
  # locaux (poids des modèles GGUF) sans saturer le disque racine.
  # `virtualisation.diskSize` est en Mo (100000 Mo ≈ 97,7 Go) et n'est
  # lu QUE par le module qemu-vm lors de `nixos-rebuild build-vm` :
  # totalement inerte sur une installation disque réelle (le module est
  # présent dans la module-list par défaut, donc l'option existe partout,
  # mais son effet est exclusif au wrapper QEMU). L'image qcow2 générée
  # est SPARSE : seul l'espace réellement écrit est consommé sur l'hôte.
  virtualisation.diskSize = 100000;

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
