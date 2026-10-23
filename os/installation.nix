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

{ ... }:

{
  imports = [
    ./common.nix
    ./ecosystem.nix
    ./services/hitech-store.nix
    ./services/genio.nix
    ./services/secrets.nix
  ];
}
