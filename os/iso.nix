# HiTech-OS — image ISO live (Live CD) pour test sur matériel réel.
#
# Basée sur le module d'installation minimal officiel de NixOS (fourni par
# flake.nix via l'input `nixpkgs` — pas via la syntaxe <nixpkgs/...>, qui
# dépend de NIX_PATH et n'est pas fiable en évaluation pure/flakes/CI), avec
# notre socle "Immutable Core" (services, durcissement, empreinte minimale)
# importé depuis ./common.nix. Contrairement à configuration.nix (nœud sur
# disque), pas de boot.loader.grub.device ni de fileSystems figés ici :
# le module cd-dvd gère lui-même le boot live et le squashfs.
#
# system.autoUpgrade (OTA) est volontairement absent : ça n'a pas de sens
# sur une image live qui ne persiste pas d'un boot à l'autre.

{ config, lib, pkgs, ... }:

{
  imports = [
    ./common.nix
  ];

  networking.hostName = "hitechos-live";

  # Le mot de passe root vide par défaut du module d'installation minimal
  # ne convient pas à un système qui expose SSH par clé publique — on le
  # désactive explicitement au profit de la clé de common.nix.
  users.users.root.initialHashedPassword = lib.mkForce null;

  isoImage.isoName = lib.mkForce "hitechos-live-${config.system.nixos.label}-${pkgs.stdenv.hostPlatform.system}.iso";
}
