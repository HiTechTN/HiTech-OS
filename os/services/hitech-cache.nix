# HiTech-OS — binary cache for the NetInstaller and OTA closures.
#
# Design rationale (see docs/audit/PHASE8-NETINSTALLER-RUST-DE.md):
#   - Harmonia is the primary cache server: Rust, low overhead, multi-worker,
#     and it signs store paths with a dedicated key.
#   - nix-serve is documented as a fallback if a host already uses it, but it
#     does not sign and is single-process; this module intentionally uses
#     Harmonia for the new cache.
#   - The cache is write-protected and read-only to clients. CI never gets a
#     private key in a writable repository: it builds closures and pushes them
#     to the cache host over SSH; the cache host signs them.
#   - TLS terminates in nginx with ACME. Plain HTTP is only for the loopback
#     listener.
#
# Deployment contract:
#   /run/secrets/hitech-cache-signing-key — secret key generated offline with
#       nix-store --generate-binary-cache-key hitechos-cache-1 <secret> <public>
#   services.hitech-cache.publicKey       — the matching public key, embedded
#       in clients and in the NetInstaller ISO.
#
# IMPORTANT: keep the private key out of Git. The public key alone is safe to
# commit and must be embedded in the installer so cache signatures can be
# verified.

{ config, lib, pkgs, ... }:

let
  cfg = config.services.hitech-cache;
in
{
  options.services.hitech-cache = {
    enable = lib.mkEnableOption "Harmonia binary cache server for HiTech-OS closures";

    package = lib.mkPackageOption pkgs "harmonia" { };

    bindAddress = lib.mkOption {
      type = lib.types.str;
      default = "127.0.0.1";
      description = "Address Harmonia binds to; nginx proxies external traffic.";
    };

    port = lib.mkOption {
      type = lib.types.port;
      default = 5000;
      description = "Internal Harmonia TCP port.";
    };

    domain = lib.mkOption {
      type = lib.types.str;
      default = "cache.hitechos.local";
      description = "Public hostname served by nginx (TLS/ACME).";
    };

    enableTls = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = ''
        Terminate TLS with nginx + ACME. Enable once `domain` resolves to this
        host and port 80/443 are reachable from the internet. Disable only for
        a private network/VPN cache.
      '';
    };

    signKeyPath = lib.mkOption {
      type = lib.types.path;
      default = "/run/secrets/hitech-cache-signing-key";
      description = ''
        Path to the secret signing key. Provision it with sops/agenix, never
        with systemd-tmpfiles and never in the Nix store.
      '';
    };

    publicKey = lib.mkOption {
      type = lib.types.str;
      default = "";
      example = "hitechos-cache-1:AbCdEf...=";
      description = ''
        Public key corresponding to `signKeyPath`. Required when the server is
        enabled; clients must trust this key to accept HiTech-OS closures.
      '';
    };

    openFirewall = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Open the public cache ports in the NixOS firewall.";
    };

    client = {
      enable = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = ''
          Configure this host to substitute from the HiTech-OS cache. Enable on
          patient-zero, installed targets and the NetInstaller live system.
        '';
      };

      url = lib.mkOption {
        type = lib.types.str;
        default = "https://cache.hitechos.local";
        description = "Base URL of the HiTech-OS binary cache.";
      };

      publicKey = lib.mkOption {
        type = lib.types.str;
        default = "";
        description = "Trusted public key for `url` (must match the server).";
      };
    };
  };

  config = lib.mkMerge [
    (lib.mkIf cfg.enable {
      assertions = [
        {
          assertion = cfg.publicKey != "";
          message = ''
            services.hitech-cache.enable requires services.hitech-cache.publicKey
            so clients can verify cache signatures.
          '';
        }
      ];

      services.harmonia = {
        inherit (cfg) package;

        cache = {
          enable = true;
          signKeyPaths = [ cfg.signKeyPath ];
          settings.bind = "${cfg.bindAddress}:${toString cfg.port}";
        };
      };

      services.nginx = lib.mkIf cfg.enableTls {
        enable = true;
        recommendedProxySettings = true;
        recommendedTlsSettings = true;

        virtualHosts.${cfg.domain} = {
          enableACME = true;
          forceSSL = true;
          locations."/".proxyPass = "http://${cfg.bindAddress}:${toString cfg.port}";
        };
      };

      security.acme.acceptTerms = lib.mkIf cfg.enableTls true;

      networking.firewall.allowedTCPPorts = lib.optionals cfg.openFirewall (
        if cfg.enableTls then
          [ 80 443 ]
        else
          [ cfg.port ]
      );
    })

    (lib.mkIf cfg.client.enable {
      nix.settings = {
        substituters = lib.mkAfter [ cfg.client.url ];
        trusted-public-keys = lib.mkAfter (
          lib.optional (cfg.client.publicKey != "") cfg.client.publicKey
        );
      };
    })
  ];
}
