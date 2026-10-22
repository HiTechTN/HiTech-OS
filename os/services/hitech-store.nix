# HiTech-OS — service natif « hitech-store » (portail universel FastAPI)
#
# Packagé via la derivation « à la app2nix » : /data/hitech-store/default.nix
# (+ flake.nix pour `nix build .` in-site). Contenu : env Python complet
# (fastapi, uvicorn + libs d'extraction pypdf/python-docx/openpyxl/
# python-pptx), assets static/ + templates/ dans le store, wrapper
# uvicorn → $out/bin/hitech-store.
#
# Importé par ./configuration.nix. Dégradation gracieuse des secrets
# (stratégie validée le 22/10/2026 sur mosquitto/telegraf) :
#   - /run/secrets/hitech-store.env : placeholder créé au boot par
#     systemd-tmpfiles UNIQUEMENT s'il n'existe pas (ligne `f`) ;
#     agenix/sops-nix le remplacera au déploiement (Phase 6) sans
#     l'écraser. SECRET_KEY de dev → sessions révoquées au vrai secret ;
#     AGENT_SHARED_SECRET vide → API agent désactivée (fail-closed,
#     loggé par app.py au démarrage).
#
# Sources vendorisées dans le dépôt (apps/hitech-store/) pour un éval
# flake PUR/HÉRMÉTIQUE — le lab de dev /data/hitech-store reste le lieu
# de `nix build .` in-site (même derivation, synchronisée à la main).

{ config, lib, pkgs, ... }:

let
  stateDir = "/data/hitech-store";
  # Derivation vendorisée (cf. apps/hitech-store/default.nix — identique
  # à /data/hitech-store/default.nix).
  hitechStore = pkgs.callPackage ../../apps/hitech-store/default.nix { };
in
{
  # --- Utilisateur dédié (least privilege) ---
  users.users.hitech-store = {
    isSystemUser = true;
    group = "hitech-store";
    home = stateDir;
    createHome = false; # tmpfiles crée le répertoire avec bons droits
  };
  users.groups.hitech-store = { };

  # --- Arborescence runtime + placeholders de secrets ---
  systemd.tmpfiles.rules = [
    "d /data 0755 root root -"
    "d ${stateDir} 0755 hitech-store hitech-store -"
    "d ${stateDir}/storage 0750 hitech-store hitech-store -"
    "d ${stateDir}/logs 0750 hitech-store hitech-store -"
    "d ${stateDir}/.office_cache 0750 hitech-store hitech-store -"
    # Fichiers d'état pré-créés (les loaders app.py tolèrent un fichier
    # vide = état absent) : ownership correct garanti, jamais écrasés.
    "f ${stateDir}/.otp_state.json 0640 hitech-store hitech-store -"
    "f ${stateDir}/.otp_state.lock 0600 hitech-store hitech-store -"
    "f ${stateDir}/.shares.json 0640 hitech-store hitech-store -"
    "f ${stateDir}/.shares.lock 0600 hitech-store hitech-store -"
    "f ${stateDir}/.office.lock 0600 hitech-store hitech-store -"
    # .env placeholder — UNE variable par ligne (pige EnvironmentFile
    # corrigé le 22/10/2026 sur telegraf : plusieurs paires sur une
    # seule ligne forment une seule variable mal déclarée).
    "f /run/secrets/hitech-store.env 0640 root hitech-store - SECRET_KEY=dev-only-placeholder-change-me\\nSTORAGE_DIR=${stateDir}/storage\\nSMTP_HOST=localhost\\nSMTP_PORT=25\\nSMTP_USER=\\nSMTP_PASSWORD=\\nAGENT_SHARED_SECRET="
  ];

  # --- Service systemd ---
  systemd.services.hitech-store = {
    description = "hitech-store — portail universel HiTechTN (FastAPI/uvicorn)";
    after = [ "network.target" ];
    wantedBy = [ "multi-user.target" ];

    # NOTE office→PDF : la conversion appelle `soffice` ; si elle doit
    # fonctionner in-system, ajouter ici `path = [ pkgs.libreoffice ];`
    # (closure ~+1 Go, volontairement hors périmètre de ce module).
    serviceConfig = {
      ExecStart =
        "${hitechStore}/bin/hitech-store app:app --host 0.0.0.0 --port 8095 --workers 2";
      WorkingDirectory = stateDir;
      User = "hitech-store";
      Group = "hitech-store";
      Restart = "on-failure";
      RestartSec = 5;

      # Secrets/config : placeholder tmpfiles (voir ci-dessus).
      EnvironmentFile = "/run/secrets/hitech-store.env";
      # Redirection de l'état runtime hors store (les 7 chemins sont
      # env-overridables via le postPatch de la derivation).
      Environment = [
        "LOG_DIR=${stateDir}/logs"
        "OTP_STATE_FILE=${stateDir}/.otp_state.json"
        "OTP_LOCK_FILE=${stateDir}/.otp_state.lock"
        "SHARES_FILE=${stateDir}/.shares.json"
        "SHARES_LOCK_FILE=${stateDir}/.shares.lock"
        "OFFICE_CACHE_DIR=${stateDir}/.office_cache"
        "OFFICE_LOCK_FILE=${stateDir}/.office.lock"
      ];

      # Durcissement minimal
      NoNewPrivileges = true;
      ProtectSystem = "full";
      ProtectHome = true;
      ReadWritePaths = [ stateDir ];
    };
  };
}
