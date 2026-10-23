# HiTech-OS — déchiffrement sops des secrets de service au boot.
#
# Pourquoi un oneshot et pas sops-nix comme module NixOS ?
# L'installateur Calamares génère un /etc/nixos/configuration.nix NON-flake.
# Un `imports = [ sops-nix.nixosModules.sops ]` y serait impossible sans
# flake/input disponible au moment du `nixos-rebuild` local. On garde donc
# le contrat NixOS simple : des fichiers chiffrés déposés hors dépôt, et
# sops (paquet) appelé au démarrage.
#
# Contrat de déploiement (répertoire root:root 0700) :
#   /etc/hitechos/age.key                  clé age de l'hôte (jamais commitée)
#   /etc/hitechos/hitech-store.env.enc     secrets du portail (format dotenv)
#   /etc/hitechos/genio.env.enc            tokens store/OpenRouter de Genio
#
# Création (exemple) :
#   sops --encrypt --age "$(cat /etc/hitechos/age.pub)" \
#        --input-type dotenv --output-type dotenv \
#        /etc/hitechos/hitech-store.env > /etc/hitechos/hitech-store.env.enc
#
# Sans fichiers .enc, le service ne fait rien : les placeholders tmpfiles
# restent en place et les services démarrent en mode dégradé explicite.

{ config, lib, pkgs, ... }:

let
  secretsDir = "/etc/hitechos";
  decrypt = pkgs.writeShellScript "hitechos-decrypt-secrets" ''
    set -eu
    umask 077
    ${pkgs.coreutils}/bin/install -d -m 0700 /run/secrets

    group_exists() {
      while IFS=: read -r name _; do
        [ "$name" = "$1" ] && return 0
      done < /etc/group
      return 1
    }

    decrypt_one() {
      src="$1"
      dst="$2"
      group="$3"
      [ -f "$src" ] || return 0
      if [ ! -r "${secretsDir}/age.key" ]; then
        echo "hitechos-secrets: $src exists but ${secretsDir}/age.key is missing" >&2
        return 1
      fi
      if ! group_exists "$group"; then
        echo "hitechos-secrets: group $group absent, skipping $dst" >&2
        return 0
      fi
      tmp="$(${pkgs.coreutils}/bin/mktemp)"
      trap '${pkgs.coreutils}/bin/rm -f "$tmp"' EXIT
      SOPS_AGE_KEY_FILE="${secretsDir}/age.key" \
        ${pkgs.sops}/bin/sops --decrypt "$src" > "$tmp"
      ${pkgs.coreutils}/bin/install -o root -g "$group" -m 0640 "$tmp" "$dst"
      ${pkgs.coreutils}/bin/rm -f "$tmp"
      trap - EXIT
    }

    decrypt_one "${secretsDir}/hitech-store.env.enc" /run/secrets/hitech-store.env hitech-store
    decrypt_one "${secretsDir}/genio.env.enc" /run/secrets/genio.env genio
  '';
in
{
  environment.etc."hitechos/README".text = ''
    HiTech-OS secrets
    =================
    Place the age private key in /etc/hitechos/age.key (mode 0600).
    Encrypt dotenv files with sops and name them:
      hitech-store.env.enc
      genio.env.enc
    They are decrypted to /run/secrets/ before the services start.
    Required hitech-store keys: SECRET_KEY, SMTP_HOST, SMTP_PORT, SMTP_USER,
      SMTP_PASSWORD, AGENT_SHARED_SECRET, STORAGE_DIR.
    Optional GitHub OAuth keys: GITHUB_CLIENT_ID, GITHUB_CLIENT_SECRET,
      GITHUB_ALLOWED_LOGIN, GITHUB_REDIRECT_URI.
    Required genio keys: HITECH_STORE_ADMIN_TOKEN, HITECH_STORE_AGENT_TOKEN;
      optional GENIO_OPENROUTER_KEY / OPENROUTER_API_KEY.
  '';

  systemd.services.hitechos-secrets = {
    description = "Decrypt HiTech-OS service secrets with sops";
    wantedBy = [ "multi-user.target" ];
    before = [ "hitech-store.service" "genio.service" ];
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      ExecStart = "${decrypt}";
    };
  };
}
