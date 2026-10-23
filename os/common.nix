# HiTech-OS — réglages communs (Immutable Core, services, durcissement).
#
# Ce module NE contient AUCUN réglage lié au disque/bootloader (grub,
# fileSystems) : ceux-ci dépendent de la cible (nœud réel/VM vs image ISO
# live) et vivent dans configuration.nix ou iso.nix respectivement.

{ config, lib, pkgs, ... }:

{
  # nix-command + flakes activés partout (nœud ET ISO live) :
  # - requis par nixos-install lui-même sur ce système (NixOS 26.11) —
  #   erreur réelle rencontrée sur matériel le 20/09/2026 sans ce réglage :
  #   "experimental Nix feature 'flakes' is disabled"
  # - requis aussi par notre OTA (system.autoUpgrade.flake, Phase 6)
  nix.settings.experimental-features = [ "nix-command" "flakes" ];

  # Outils indispensables au premier boot et aux reconstructions locales.
  # `git` était déjà sur l'ISO live mais PAS dans le système installé : le
  # job Calamares ne peut donc pas compter sur le git de la cible. Il est
  # ajouté ici pour que le système installé soit autonome après coup (et
  # pour les rebuilds/OTA qui clonent le dépôt).
  environment.systemPackages = with pkgs; [
    git
    curl
  ];

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

  # Dégradation gracieuse des secrets télémétrie (fix boot 22/10/2026) :
  # AVANT ce réglage, un premier boot sans secrets embarqués plantait
  # mosquitto au démarrage ("Failed at step CREDENTIALS" — le module
  # NixOS branche hashedPasswordFile sur systemd LoadCredential=, qui
  # ÉCHOUE FATALEMENT si le fichier source est absent) et telegraf
  # ("Failed to load environment files"). Conséquence : le "système
  # nerveux" (MQTT → Telegraf → InfluxDB) mort dès le premier boot.
  # MAINTENANT : systemd-tmpfiles crée un fichier placeholder (hash
  # sha512-crypt d'un mot de passe jeton "placeholder-change-me") UNIQUEMENT
  # s'il n'existe pas encore (la ligne tmpfiles `f` laisse intact tout
  # fichier déjà présent — le vrai secret injecté au déploiement par
  # agenix/sops-nix n'est jamais écrasé). Résultat : broker MQTT joignable
  # et démarrable partout, mais authentification refusée tant que le vrai
  # secret n'est pas déployé (comportement "fail-closed" pour l'auth,
  # "fail-open" pour la disponibilité du service).
  systemd.tmpfiles.rules = [
    "d /run/secrets 0700 root root -"
    # Hash $7$ (format natif mosquitto_passwd, obligatoire avec le plugin
    # password-file strict de mosquitto 2.1.x — un hash crypt système $6$
    # est rejeté : "Unable to decode line", vérifié en réel le 22/10/2026).
    # IMPORTANT : le module NixOS PREFIXE le nom d'utilisateur lui-même
    # (addFile → "user:$(cat secret)") — le fichier secret ne doit donc
    # contenir QUE le hash, SANS préfixe "user:" (doc du module
    # hashedPasswordFile : "remove the username: prefix"). Avec le préfixe,
    # mosquitto recevait "user:user:hash" → décodage impossible → crash.
    # Généré via : mosquitto_passwd -b -c /tmp/f hitechos-agent placeholder-change-me
    # puis retrait du préfixe "hitechos-agent:".
    "f /run/secrets/mosquitto-hitechos-agent 0600 root root - $7$1000$Z2MxgetdsMKH0Pguep3B46r5K/HZF6YOGlO50UO7qNEb4JEwoZD0+NQIQcN8OAA0olKDdD0bqaoe5zla7De2Og==$YE7drJO/5evWNUEb0i4Mfeja4+CKxTvGFjVIOpE90kxdffeOHYTriB3E8Mv24QnfARuVXmWH5E8mrFE3Feg51g=="
    # Placeholder telegraf (optionnel mais même mécanisme) : un fichier
    # vide "INFLUX_TOKEN=" évite tout ÉCHOU DE TYPE du côté systemd et
    # garantit que l'option environmentFiles (type `absolute path` dans le
    # module NixOS — le prefixe "-" de systemd est interdit par le typage)
    # reste satisfiable. Comme ci-dessus : jamais écrasé si déjà présent.
    "f /run/secrets/telegraf-influx-token 0600 root root - INFLUX_TOKEN="
    # Identifiants MQTT de pontage telegraf→mosquitto : telegraf CRASHAIT
    # ("connection refused"/auth) sans broker joignable ET sans creds —
    # le champ mqtt_consumer pointe vers $MQTT_USER/$MQTT_PASSWORD via
    # l'env-file ci-contre. Placeholder = même mot de passe que le hash
    # mosquitto ci-dessus ; les vrais secrets (Phase 6) devront fournir
    # les deux fichiers de façon cohérente.
    # IMPORTANT : systemd EnvironmentFile attend UNE variable par ligne —
    # les deux paires sur une seule ligne faisaient de MQTT_USER une
    # variable unique mal formée ("hitechos-agent MQTT_PASSWORD=...")
    # et laissait MQTT_PASSWORD indéfini : telegraf (strict env handling
    # par défaut depuis v1.38) envoyait alors le littéral "$MQTT_USER"
    # → CONNACK "not authorised" en boucle, service en start-limit-hit.
    # L'échappement \n est interprété par systemd-tmpfiles (docs :
    # "may contain C-style escapes").
    "f /run/secrets/telegraf-mqtt-creds 0600 root root - MQTT_USER=hitechos-agent\\nMQTT_PASSWORD=placeholder-change-me"
  ];

  services.telegraf = {
    enable = true;
    # Le fichier est GARANTI présent par systemd-tmpfiles ci-dessus
    # (placeholder "INFLUX_TOKEN=" si absent) : le service ne rate plus
    # son démarrage sur un nœud sans secret. Auth sortante vers InfluxDB
    # en échec loggé tant que le vrai token n'est pas injecté ; dès le
    # déploiement (agenix/sops-nix) + restart, la télémétrie remonte.
    environmentFiles = [
      "/run/secrets/telegraf-influx-token" # fournit $INFLUX_TOKEN
      "/run/secrets/telegraf-mqtt-creds" # fournit $MQTT_USER/$MQTT_PASSWORD
    ];
    extraConfig = {
      inputs.mqtt_consumer = [{
        servers = [ "tcp://127.0.0.1:1883" ];
        topics = [ "hitechos/+/+/+" ];
        data_format = "json";
        name_override = "hitechos_telemetry";
        # Broker local avec allow_anonymous=false (users définis) : sans
        # creds, l'agent telegraf mourait au démarrage (CONNACK refusé).
        username = "$MQTT_USER";
        password = "$MQTT_PASSWORD";
      }];
      outputs.influxdb_v2 = [{
        urls = [ "http://127.0.0.1:8086" ];
        token = "$INFLUX_TOKEN";
        organization = "HiTechTN";
        bucket = "hitechos";
      }];
    };
  };

  # Ordre de démarrage : telegraf dial MQTT FATALEMENT au boot si le
  # broker n'écoute pas encore ("connection refused" → exit 1, observé
  # en réel). On dépend doucement (wants=, pas requires=) de mosquitto
  # pour que celui-ci démarre en premier quand les deux sont actifs.
  systemd.services.telegraf = {
    after = [ "mosquitto.service" ];
    wants = [ "mosquitto.service" ];
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
