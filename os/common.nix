# HiTech-OS — réglages communs (Immutable Core, services, durcissement).
#
# Ce module NE contient AUCUN réglage lié au disque/bootloader (grub,
# fileSystems) : ceux-ci dépendent de la cible (nœud réel/VM vs image ISO
# live) et vivent dans configuration.nix ou iso.nix respectivement.

{ config, lib, pkgs, ... }:

{
  boot.kernelPackages = pkgs.linuxPackages_latest;

  # Modules noyau désactivés : pas d'audio, pas de GPU desktop, pas de BT
  boot.blacklistedKernelModules = [ "snd" "bluetooth" "btusb" ];
  boot.kernelParams = [ "quiet" ];

  # --- Pas d'interface graphique (par défaut ; l'ISO graphique/installeur
  # écrase ce réglage — voir os/iso.nix) ---
  services.xserver.enable = lib.mkDefault false;
  documentation.enable = false;
  documentation.nixos.enable = false;

  # --- Réseau ---
  networking.hostName = lib.mkDefault "hitechos-patient-zero";
  networking.useDHCP = lib.mkDefault true;
  networking.firewall.allowedTCPPorts = [ 1883 ]; # MQTT (Mosquitto)

  # --- Conteneurisation native ---
  boot.enableContainers = true;

  # --- Télémétrie (Phase 2) ---
  services.mosquitto = {
    enable = true;
    listeners = [{
      port = 1883;
      users."hitechos-agent" = {
        acl = [ "readwrite hitechos/#" ];
        # Mot de passe défini hors-dépôt via `mosquitto_passwd`, injecté au
        # déploiement (jamais en clair dans ce fichier versionné) :
        #   mosquitto_passwd -c /var/lib/mosquitto/passwd hitechos-agent
        hashedPasswordFile = "/run/secrets/mosquitto-hitechos-agent"; # géré hors dépôt (agenix/sops-nix à brancher en Phase 6)
      };
    }];
  };

  services.telegraf = {
    enable = true;
    environmentFiles = [ "/run/secrets/telegraf-influx-token" ]; # fournit $INFLUX_TOKEN
    extraConfig = {
      inputs.mqtt_consumer = [{
        servers = [ "tcp://127.0.0.1:1883" ];
        topics = [ "hitechos/+/+/+" ];
        data_format = "json";
        name_override = "hitechos_telemetry";
      }];
      outputs.influxdb_v2 = [{
        urls = [ "http://127.0.0.1:8086" ];
        token = "$INFLUX_TOKEN";
        organization = "HiTechTN";
        bucket = "hitechos";
      }];
    };
  };

  services.influxdb2.enable = lib.mkDefault true;

  # --- Utilisateur minimal pour l'administration ---
  users.users.hitechos = {
    isNormalUser = true;
    extraGroups = [ "wheel" ];
    openssh.authorizedKeys.keys = [
      "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIHiZ0ohzgPMrdYSwKK1wU2U8VYzwDs/XpIuzDDtkO8O+ azmi.hitech@gmail.com"
      "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIG1Kb2DNzB8mL5j3PgrHwWcPUAd8bTONw/mO9Iqz1nyA nixos@GLF-OS"
    ];
  };
  services.openssh.enable = true;
  services.openssh.settings.PasswordAuthentication = false;

  # --- Empreinte minimale ---
  services.udisks2.enable = lib.mkDefault false;
  security.polkit.enable = lib.mkDefault false;

  # --- Durcissement (Phase 7) ---
  security.sudo.wheelNeedsPassword = true;
  services.openssh.settings.PermitRootLogin = "no";
  services.openssh.settings.KbdInteractiveAuthentication = false;
  networking.firewall.enable = true; # explicite : tout est fermé sauf allowedTCPPorts ci-dessus
  security.auditd.enable = true; # journalisation des actions système (utile pour la piste d'audit AgentOS)

  system.stateVersion = "24.05";
}
