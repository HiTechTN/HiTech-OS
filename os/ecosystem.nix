# HiTech-OS — provisionnement de l'écosystème d'outillage HiTechTN
# (AgentOS + app2nix) sur le nœud patient-zero.
#
# Importé par ./installation.nix (donc patient-zero ET toute installation
# Calamares) — jamais par l'ISO live (iso.nix) ni par le profil headless
# minimaliste documenté dans le README : ce module ajoute du poids (daemon
# Docker) incompatible avec l'objectif <150 Mo RAM idle du nœud "nu". Sur un
# nœud de production léger, retirer simplement l'import de ce fichier.
#
# Audit du 22/10/2026 : avant cet ajout, patient-zero ne contenait NI git
# NI docker (seule l'ISO live avait git — iso.nix), alors que :
# - AgentOS (README, "Prerequisites") exige : git · docker · docker
#   compose · bash 4+  — il s'installe via install.sh → docker compose up
#   (stack PostgreSQL+pgvector, MinIO, Jaeger, services applicatifs).
# - app2nix s'installe via Docker (`docker run ghcr.io/hitechtn/app2nix`),
#   via pip (Python), ou nativement via SON propre flake.nix (donc nix
#   flakes + nix-command — déjà activés dans ./common.nix).
# - bash 4+ et curl sont déjà dans la closure de base NixOS (vérifié via
#   `nix eval .#nixosConfigurations.patient-zero.config.environment.systemPackages`).

{ config, lib, pkgs, ... }:

{
  # --- Outils de base requis par les deux outils ---
  environment.systemPackages = with pkgs; [
    git # clone des dépôts AgentOS/app2nix + usage de leurs scripts
    docker-compose # API `docker compose` (AgentOS : install.sh)
    python3 # chemin "pip" natif d'app2nix (pyproject.toml/uv.lock)
    uv # gestion d'environnement Python d'app2nix (recommandé par son repo)
  ];

  # --- Conteneurs (AgentOS et app2nix recommandent tous les deux Docker) ---
  virtualisation.docker.enable = true;

  # --- L'administrateur pilote Docker sans root (nécessaire pour
  # `docker compose up -d` d'AgentOS en tant que hitechos) ---
  users.users.hitechos.extraGroups = [ "docker" ];

  # NOTE Sécurité (à documenter lors du déploiement) : Docker publie ses
  # ports via ses propres chaînes iptables/nftables, ce qui peut contourner
  # le firewall NixOS (networking.firewall dans ./common.nix) pour les
  # ports publiés par le démon Docker. Restreindre les publications de
  # ports aux interfaces de confiance si le nœud est exposé.
}
