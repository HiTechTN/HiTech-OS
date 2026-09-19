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
  services.udisks2.enable = false;
  security.polkit.enable = lib.mkDefault false;

  # --- OTA (Phase 6) ---
  # Rollback natif : chaque switch crée une nouvelle génération de boot ;
  # `nixos-rebuild switch --rollback` (ou le sélecteur GRUB au boot) revient
  # instantanément à la précédente si la nouvelle casse quelque chose.
  system.autoUpgrade = {
    enable = true;
    flake = "github:HiTechTN/vibe-os#patient-zero";
    dates = "04:00"; # fenêtre de maintenance nocturne, à ajuster par site
    allowReboot = false; # un reboot auto sur un noeud de prod est un choix a valider, pas un defaut
  };
  # Garde-fou : on ne garde que les N dernieres generations pour ne pas
  # saturer le disque avec l'historique OTA.
  boot.loader.grub.configurationLimit = 10;

  # --- Durcissement (Phase 7) ---
  security.sudo.wheelNeedsPassword = true;
  services.openssh.settings.PermitRootLogin = "no";
  services.openssh.settings.KbdInteractiveAuthentication = false;
  networking.firewall.enable = true; # explicite : tout est ferme sauf allowedTCPPorts ci-dessus
  security.auditd.enable = true; # journalisation des actions systeme (utile pour la piste d'audit AgentOS)

  system.stateVersion = "24.05";
}
