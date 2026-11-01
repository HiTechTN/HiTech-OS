# HiTech-OS — service natif « hitech-store » (portail universel FastAPI)
#
# Packagé via la derivation « à la app2nix » : apps/hitech-store/default.nix.
# Importé par os/installation.nix, donc présent sur patient-zero ET sur
# toute installation Calamares (active par défaut, désactivable).
#
# Secrets : /run/secrets/hitech-store.env est un placeholder créé par
# systemd-tmpfiles UNIQUEMENT s'il n'existe pas. os/services/secrets.nix le
# remplace avant le démarrage si /etc/hitechos/hitech-store.env.enc existe.
# Le panel admin reste utilisable sans SMTP en mode DEV_OTP_FALLBACK=1
# (PIN écrit dans le journal) ; passer devOtpFallback=false en production.

{ config, lib, pkgs, ... }:

let
  cfg = config.services.hitech-store;
  hitechStore = pkgs.callPackage ../../apps/hitech-store/default.nix { };

  # The tmpfiles placeholder ships a publicly known SECRET_KEY so that the
  # service can boot before sops secrets are provisioned. That key signs the
  # admin JWT: if it survives into an exposed deployment, anyone can forge an
  # admin session. This oneshot replaces it with a per-boot random key when the
  # placeholder (or an empty value) is still present; a real sops-provisioned
  # key is never touched. Runs after hitechos-secrets, before the service.
  envKeygen = pkgs.writeShellScript "hitech-store-keygen" ''
    set -eu
    umask 077
    env_file=${lib.escapeShellArg cfg.environmentFile}
    [ -f "$env_file" ] || exit 0

    placeholder='^SECRET_KEY=dev-only-placeholder-change-me$'
    if ${pkgs.gnugrep}/bin/grep -q "$placeholder" "$env_file" \
      || ! ${pkgs.gnugrep}/bin/grep -q '^SECRET_KEY=.' "$env_file"; then
      new_key="$(${pkgs.openssl}/bin/openssl rand -hex 32)"
      tmp="$env_file.tmp.$$"
      trap '${pkgs.coreutils}/bin/rm -f "$tmp"' EXIT
      ${pkgs.gawk}/bin/awk -v key="$new_key" '
        BEGIN { replaced = 0 }
        /^SECRET_KEY=/ { print "SECRET_KEY=" key; replaced = 1; next }
        { print }
        END { if (!replaced) print "SECRET_KEY=" key }
      ' "$env_file" > "$tmp"
      ${pkgs.coreutils}/bin/chmod --reference="$env_file" "$tmp" 2>/dev/null || true
      ${pkgs.coreutils}/bin/chown --reference="$env_file" "$tmp" 2>/dev/null || true
      ${pkgs.coreutils}/bin/mv -f "$tmp" "$env_file"
      trap - EXIT
      echo "hitech-store-keygen: generated a per-boot SECRET_KEY (placeholder was still present)" >&2
    fi
  '';
in
{
  options.services.hitech-store = {
    enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Enable the HiTech Store portal.";
    };

    stateDir = lib.mkOption {
      type = lib.types.str;
      default = "/data/hitech-store";
      description = "Persistent storage and runtime state directory.";
    };

    listenAddress = lib.mkOption {
      type = lib.types.str;
      default = "0.0.0.0";
      description = "Address uvicorn binds to.";
    };

    port = lib.mkOption {
      type = lib.types.port;
      default = 8095;
      description = "HiTech Store HTTP port.";
    };

    environmentFile = lib.mkOption {
      type = lib.types.str;
      default = "/run/secrets/hitech-store.env";
      description = "systemd EnvironmentFile containing store secrets.";
    };

    devOtpFallback = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        Keep OTP login usable when SMTP is unavailable by writing the PIN to
        the service journal. Set to false in production to fail closed.
      '';
    };

    cookieSecure = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = ''
        Mark the admin session cookie Secure. Keep false for plain HTTP on
        localhost; enable only behind HTTPS.
      '';
    };

    openFirewall = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Open the store port in the NixOS firewall.";
    };
  };

  config = lib.mkIf cfg.enable {
    # --- Utilisateur dédié (least privilege) ---
    users.users.hitech-store = {
      isSystemUser = true;
      group = "hitech-store";
      home = cfg.stateDir;
      createHome = false;
    };
    users.groups.hitech-store = { };

    # --- Arborescence runtime + placeholders de secrets ---
    systemd.tmpfiles.rules = [
      "d /data 0755 root root -"
      "d ${cfg.stateDir} 0755 hitech-store hitech-store -"
      "d ${cfg.stateDir}/storage 0750 hitech-store hitech-store -"
      "d ${cfg.stateDir}/storage/public 0750 hitech-store hitech-store -"
      "d ${cfg.stateDir}/storage/private 0750 hitech-store hitech-store -"
      "d ${cfg.stateDir}/logs 0750 hitech-store hitech-store -"
      "d ${cfg.stateDir}/.office_cache 0750 hitech-store hitech-store -"
      "f ${cfg.stateDir}/.otp_state.json 0640 hitech-store hitech-store -"
      "f ${cfg.stateDir}/.otp_state.lock 0600 hitech-store hitech-store -"
      "f ${cfg.stateDir}/.shares.json 0640 hitech-store hitech-store -"
      "f ${cfg.stateDir}/.shares.lock 0600 hitech-store hitech-store -"
      "f ${cfg.stateDir}/.office.lock 0600 hitech-store hitech-store -"
      # Une variable par ligne : plusieurs paires sur une ligne forment une
      # seule variable mal déclarée dans EnvironmentFile.
      "f /run/secrets/hitech-store.env 0640 root hitech-store - SECRET_KEY=dev-only-placeholder-change-me\\nSTORAGE_DIR=${cfg.stateDir}/storage\\nSMTP_HOST=localhost\\nSMTP_PORT=25\\nSMTP_USER=\\nSMTP_PASSWORD=\\nAGENT_SHARED_SECRET=\\nDEV_OTP_FALLBACK=${if cfg.devOtpFallback then "1" else "0"}\\nCOOKIE_SECURE=${if cfg.cookieSecure then "1" else "0"}\\nGITHUB_CLIENT_ID=\\nGITHUB_CLIENT_SECRET=\\nGITHUB_ALLOWED_LOGIN=\\nGITHUB_REDIRECT_URI="
    ];

    # --- Remplace le SECRET_KEY placeholder par une clé aléatoire ---
    systemd.services.hitech-store-keygen = {
      description = "Ensure hitech-store has a non-placeholder SECRET_KEY";
      wantedBy = [ "multi-user.target" ];
      after = [
        "systemd-tmpfiles-setup.service"
        "hitechos-secrets.service"
      ];
      before = [ "hitech-store.service" ];
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        ExecStart = "${envKeygen}";
      };
    };

    # --- Service systemd ---
    systemd.services.hitech-store = {
      description = "hitech-store — portail universel HiTechTN (FastAPI/uvicorn)";
      after = [
        "network.target"
        "hitechos-secrets.service"
        "hitech-store-keygen.service"
      ];
      requires = [ "hitech-store-keygen.service" ];
      wantedBy = [ "multi-user.target" ];

      serviceConfig = {
        ExecStart =
          "${hitechStore}/bin/hitech-store app:app --host ${cfg.listenAddress} --port ${toString cfg.port} --workers 2";
        WorkingDirectory = cfg.stateDir;
        User = "hitech-store";
        Group = "hitech-store";
        Restart = "on-failure";
        RestartSec = 5;

        EnvironmentFile = cfg.environmentFile;
        Environment = [
          "LOG_DIR=${cfg.stateDir}/logs"
          "OTP_STATE_FILE=${cfg.stateDir}/.otp_state.json"
          "OTP_LOCK_FILE=${cfg.stateDir}/.otp_state.lock"
          "SHARES_FILE=${cfg.stateDir}/.shares.json"
          "SHARES_LOCK_FILE=${cfg.stateDir}/.shares.lock"
          "OFFICE_CACHE_DIR=${cfg.stateDir}/.office_cache"
          "OFFICE_LOCK_FILE=${cfg.stateDir}/.office.lock"
        ];

        NoNewPrivileges = true;
        ProtectSystem = "full";
        ProtectHome = true;
        ReadWritePaths = [ cfg.stateDir ];
      };
    };

    networking.firewall.allowedTCPPorts = lib.optional cfg.openFirewall cfg.port;
  };
}
