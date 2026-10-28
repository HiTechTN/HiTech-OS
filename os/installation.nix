# HiTech-OS — profil d'installation disque (utilisé par Calamares ET patient-zero).
#
# Ce fichier est le point d'entrée que l'installateur ajoute au
# /etc/nixos/configuration.nix généré. Il ne contient AUCUN réglage
# disque/bootloader : ceux-ci restent dans le configuration.nix de la cible.
#
# Il regroupe les modules qui doivent exister sur TOUTE installation
# HiTech-OS, y compris après une installation Calamares vierge :
#   - common.nix      : socle, durcissement, MQTT/InfluxDB, git/curl
#   - ecosystem.nix   : Docker/AgentOS/app2nix
#   - hitech-store    : portail de stockage, actif par défaut
#   - genio           : daemon IA local + dashboard, actif par défaut
#   - secrets         : déchiffrement sops optionnel au premier boot
#   - hitech-cache    : cache binaire Harmonia (serveur désactivé par défaut ;
#                       client activable, Phase 8 NetInstaller/OTA)
#   - hitech-shell    : contrat NixOS du DE Wayland Rust (désactivé par défaut,
#                       Phase 8 scaffold — voir docs/audit/PHASE8-…)

{ ... }:

{
  imports = [
    ./common.nix
    ./hardware-universal.nix
    ./ecosystem.nix
    ./services/hitech-store.nix
    ./services/genio.nix
    ./services/secrets.nix
    ./services/hitech-cache.nix
    ./services/hitech-shell.nix
  ];
}
