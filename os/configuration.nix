# HiTech-OS — configuration NixOS (Phase 1)
#
# Objectif : boot headless minimal, empreinte RAM idle la plus basse
# possible (cible <150 Mo), conteneurisation native (systemd-nspawn).
#
# Volontairement SANS dépendance à des paramètres bas niveau figés
# (boot.loader.grub.device, filesystems) qui dépendent du hardware réel
# du nœud — voir hardware-configuration.nix (généré par
# `nixos-generate-config` sur le matériel cible, ou fourni ici en
# variante QEMU pour les tests en CI/local).

{ config, lib, pkgs, ... }:

{
  imports = [
    ./hardware-configuration.nix
  ];

  # --- Boot ---
  boot.loader.grub.enable = lib.mkDefault true;
  boot.loader.grub.device = lib.mkDefault "/dev/sda";
  boot.kernelPackages = pkgs.linuxPackages_latest;

  # Modules noyau désactivés : pas d'audio, pas de GPU desktop, pas de BT
  boot.blacklistedKernelModules = [ "snd" "bluetooth" "btusb" ];
  boot.kernelParams = [ "quiet" ];

  # --- Pas d'interface graphique ---
  services.xserver.enable = false;
  documentation.enable = false;
  documentation.nixos.enable = false;

  # --- Réseau ---
  networking.hostName = "hitechos-patient-zero";
  networking.useDHCP = lib.mkDefault true;
  networking.firewall.allowedTCPPorts = [ 1883 ]; # MQTT (Mosquitto)

  # --- Conteneurisation native ---
  boot.enableContainers = true;

  # --- Services du noeud (Phase 2/3) ---
  services.mosquitto = {
    enable = true;
    listeners = [{
      port = 1883;
      users = { }; # authentification a definir en Phase 2
    }];
  };

  # --- Utilisateur minimal pour l'administration ---
  users.users.hitechos = {
    isNormalUser = true;
    extraGroups = [ "wheel" ];
    openssh.authorizedKeys.keys = [
      # TODO : ajouter la cle publique SSH de deploiement
    ];
  };
  services.openssh.enable = true;
  services.openssh.settings.PasswordAuthentication = false;

  # --- Empreinte minimale ---
  services.udisks2.enable = false;
  security.polkit.enable = lib.mkDefault false;

  system.stateVersion = "24.05";
}
