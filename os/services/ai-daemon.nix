{ config, lib, pkgs, ... }:

let
  cfg = config.services.hitechos-ai-daemon;
  aiDaemon = pkgs.callPackage ../../ai-daemon { };
in
{
  options.services.hitechos-ai-daemon = {
    enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Enable the HiTech-OS Rust AI inference & kernel nerve daemon.";
    };
    bindAddress = lib.mkOption {
      type = lib.types.str;
      default = "127.0.0.1:11435";
    };
    modelPath = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/hitechos-ai/model.gguf";
    };
  };

  config = lib.mkIf cfg.enable {
    systemd.tmpfiles.rules = [ "d /var/lib/hitechos-ai 0750 root root -" ];
    systemd.services.hitechos-ai-daemon = {
      description = "HiTech-OS Rust Native AI Cortex & Kernel Nerve Daemon";
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" ];
      before = [ "genio.service" ];
      environment = {
        HITECHOS_AI_BIND = cfg.bindAddress;
        HITECHOS_MODEL_PATH = cfg.modelPath;
      };
      serviceConfig = {
        ExecStart = "${aiDaemon}/bin/hitechos-ai-daemon";
        Restart = "always";
        RestartSec = 2;
        NoNewPrivileges = true;
      };
    };
    environment.systemPackages = [ aiDaemon ];
  };
}
