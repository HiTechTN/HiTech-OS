# HiTech-OS Server profile — service host with local storage and telemetry.
{ lib, ... }:
{
  imports = [ ../installation.nix ];

  # Server deployments commonly expose the local service APIs; keep the
  # firewall closed by default and let the operator opt in per site.
  services.hitech-store.openFirewall = lib.mkDefault false;
  services.genio.openFirewall = lib.mkDefault false;
}
