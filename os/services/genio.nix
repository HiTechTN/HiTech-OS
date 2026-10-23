# HiTech-OS — service natif « genio » (daemon local + dashboard + CLI)
#
# Remplace le lancement manuel `python -m genio_server.daemon` du lab :
#   - package reproductible (apps/genio/default.nix) ;
#   - état persistant dans /var/lib/genio (mémoire, audit, fichiers) ;
#   - clé de capacité générée une seule fois au premier boot ;
#   - Ollama local démarré et modèle préchargé (si pullModel = true) ;
#   - dépendance souple à hitech-store (le dashboard continue de démarrer
#     même si le store est indisponible, avec dégradation explicite).
#
# Le daemon n'écoute QUE sur la loopback par défaut : le dashboard n'a pas
# d'authentification utilisateur. Pour y accéder à distance, utiliser un
# tunnel SSH (`ssh -L 8765:127.0.0.1:8765 ...`) ou activer openFirewall en
# connaissance de cause.

{ config, lib, pkgs, ... }:

let
  cfg = config.services.genio;
  genio = pkgs.callPackage ../../apps/genio/default.nix { };

  keygen = pkgs.writeShellScript "genio-keygen" ''
    set -eu
    key=${lib.escapeShellArg "${cfg.stateDir}/cap.key"}
    if [ ! -s "$key" ]; then
      umask 077
      ${pkgs.openssl}/bin/openssl rand -hex 32 > "$key"
      chmod 0600 "$key"
    fi
  '';
in
{
  options.services.genio = {
    enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Enable the Genio daemon, dashboard and CLI.";
    };

    host = lib.mkOption {
      type = lib.types.str;
      default = "127.0.0.1";
      description = "Address the Genio HTTP daemon listens on.";
    };

    port = lib.mkOption {
      type = lib.types.port;
      default = 8765;
      description = "Genio HTTP port (dashboard is served at /ui/).";
    };

    stateDir = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/genio";
      description = "Persistent Genio state directory (memory DB, audit, files).";
    };

    model = lib.mkOption {
      type = lib.types.str;
      default = "qwen2.5:7b-instruct";
      description = "Ollama model used by the local Genio backend.";
    };

    ollama.enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Enable the local Ollama service for Genio.";
    };

    ollama.pullModel = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        Pull `services.genio.model` into Ollama on first boot. Model weights
        are downloaded over the network and are not part of the Nix closure.
      '';
    };

    openFirewall = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Open the Genio port in the NixOS firewall (not recommended: no user auth).";
    };
  };

  config = lib.mkIf cfg.enable {
    users.users.genio = {
      isSystemUser = true;
      group = "genio";
      home = cfg.stateDir;
      createHome = false;
    };
    users.groups.genio = { };

    systemd.tmpfiles.rules = [
      "d ${cfg.stateDir} 0750 genio genio -"
      "d ${cfg.stateDir}/memory 0750 genio genio -"
      "d ${cfg.stateDir}/audit 0750 genio genio -"
      "d ${cfg.stateDir}/files 0750 genio genio -"
      "d ${cfg.stateDir}/tmp 0700 genio genio -"
      # Placeholder non destructif : sops (os/services/secrets.nix) remplace
      # ce fichier avant le démarrage du service si un secret chiffré existe.
      "f /run/secrets/genio.env 0640 root genio - HITECH_STORE_URL=http://127.0.0.1:8095\\nHITECH_STORE_ADMIN_TOKEN=\\nHITECH_STORE_AGENT_TOKEN=\\nGENIO_OPENROUTER_KEY=\\n"
    ];

    systemd.services.genio-keygen = {
      description = "Generate the persistent Genio capability key";
      wantedBy = [ "multi-user.target" ];
      before = [ "genio.service" ];
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        User = "genio";
        Group = "genio";
        ExecStart = "${keygen}";
      };
    };

    systemd.services.genio = {
      description = "Genio — local AI daemon and dashboard";
      wantedBy = [ "multi-user.target" ];
      after = [
        "network.target"
        "genio-keygen.service"
        "ollama.service"
        "hitech-store.service"
      ];
      wants = [
        "genio-keygen.service"
        "hitech-store.service"
      ] ++ lib.optional cfg.ollama.enable "ollama.service";
      requires = [ "genio-keygen.service" ];

      environment = {
        GENIO_HOST = cfg.host;
        GENIO_PORT = toString cfg.port;
        GENIO_STATE_DIR = cfg.stateDir;
        GENIO_CAP_KEY_FILE = "${cfg.stateDir}/cap.key";
        GENIO_OLLAMA_URL = "http://127.0.0.1:11434";
        GENIO_MODEL = cfg.model;
        HITECH_STORE_URL = "http://127.0.0.1:8095";
        TMPDIR = "${cfg.stateDir}/tmp";
      };

      serviceConfig = {
        ExecStart = "${genio}/bin/genio-daemon";
        User = "genio";
        Group = "genio";
        Restart = "on-failure";
        RestartSec = 5;

        EnvironmentFile = "/run/secrets/genio.env";

        NoNewPrivileges = true;
        ProtectSystem = "full";
        ProtectHome = true;
        PrivateTmp = true;
        ReadWritePaths = [ cfg.stateDir ];
      };
    };

    services.ollama = lib.mkIf cfg.ollama.enable {
      enable = true;
      host = "127.0.0.1";
      port = 11434;
      loadModels = lib.optional cfg.ollama.pullModel cfg.model;
    };

    networking.firewall.allowedTCPPorts = lib.optional cfg.openFirewall cfg.port;

    # `service.restart` est un outil R2 : sans règle polkit, systemctl
    # refuserait l'action pour l'utilisateur non-root `genio`. On autorise
    # uniquement les deux unités applicatives, jamais genio lui-même.
    security.polkit.enable = true;
    security.polkit.extraConfig = ''
      polkit.addRule(function(action, subject) {
        if (action.id == "org.freedesktop.systemd1.manage-units" &&
            subject.isInGroup("genio") &&
            ["hitech-store.service", "ollama.service"].indexOf(action.lookup("unit")) >= 0) {
          return polkit.Result.YES;
        }
      });
    '';

    environment.systemPackages = [ genio ];
  };
}
