{
  config,
  pkgs,
  lib,
  ...
}:
let
  request = "${config.home.homeDirectory}/.local/state/predator-pm-study/next-boot-assist.conf";
  cfg = config.programs.predator-pm-study;
in
{
  options.programs.predator-pm-study.enable = lib.mkEnableOption "the one-shot Predator diagnostic reboot assistant";
  config = lib.mkIf cfg.enable {
    xdg.configFile."predator-pm-study/initrd-known-hosts".text = ''
      [192.168.1.19]:2223 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIE30R+lkv326b0gw0uzKYuuWLJhGTmObzorr62YY5qei
    '';
    systemd.user.services.predator-pm-next-boot-assist = {
      Unit = {
        Description = "Unlock and arm the requested next Predator TPM-free boot";
        ConditionPathExists = request;
      };
      Service = {
        Type = "oneshot";
        TimeoutStartSec = "5min";
        UMask = "0077";
        Environment = [
          "TZ=Europe/Paris"
          "PATH=${
            lib.makeBinPath [
              pkgs.openssh
              pkgs.curl
              pkgs.jq
              pkgs.coreutils
            ]
          }:${config.home.profileDirectory}/bin:/usr/bin:/bin"
        ];
        EnvironmentFile = "${config.home.homeDirectory}/.config/environment.d/55-secrets.conf";
        ExecStart = "${pkgs.predator-pm-study}/bin/predator-pm-study assist-next-boot ${request}";
      };
    };
    systemd.user.timers.predator-pm-next-boot-assist = {
      Unit.Description = "Watch for the single requested Predator diagnostic reboot";
      Timer = {
        OnStartupSec = "30s";
        OnUnitInactiveSec = "60s";
        AccuracySec = "5s";
      };
      Install.WantedBy = [ "timers.target" ];
    };
  };
}
