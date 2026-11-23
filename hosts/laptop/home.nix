{
  isNixOS,
  lib,
  pkgs,
  ...
}:
{
  imports = [
    ../../modules/home/workstation.nix
    (import ../../modules/home/llama-server.nix {
      quant = "IQ2_M";
      hash = "sha256-/KmdSIv/XUAci8awEQxTd8dD8P+zptGcGuSjuQcfos0=";
    })
  ];
  programs.git.settings.user.signingkey = "E88ECCA3AA187BC1";

  desktop.plasma = {
    screen = {
      width = 1504;
      height = 1003;
    };
    cpuTemperature = {
      sensor = "lmsensors/cros_ec-isa-000c/temp4";
      label = "CPU temperature";
    };
    rootDisk = "a843fb85-bef1-48c5-93b4-47cc00c2fe62";
    peerLoadHost = "main";
  };
  programs.plasma.configFile.kscreenlockerrc.Authenticators.Fingerprint = true;
  services.adb-wireless.serials = [ "188b31ee" "A20AAFYS0202" ];
  home.packages = [ pkgs.host-tools ];

  home.file.".local/share/arch-uki" = lib.mkIf (!isNixOS) {
    source = import ../../arch/laptop-boot.nix { inherit pkgs lib; };
  };

  systemd.user.services.ryzenadj-laptop = lib.mkIf (!isNixOS) {
    Unit.Description = "Maintain RyzenAdj limits and recover stuck performance throttling";
    Service = {
      Type = "simple";
      ExecStartPre = "/usr/bin/sudo -n /usr/bin/modprobe ryzen_smu";
      ExecStart = "${lib.getExe pkgs.host-tools} ryzenadj --watch";
      Environment =
        [ "PATH=/usr/bin:/bin" "DBUS_SESSION_BUS_ADDRESS=unix:path=%t/bus" ];
      Restart = "on-failure";
      RestartSec = 5;
      StateDirectory = "ryzenadj-laptop";
      StateDirectoryMode = "0700";
    };
    Install.WantedBy = [ "default.target" ];
  };

  systemd.user.services.hardware-alerts = lib.mkIf (!isNixOS) {
    Unit = {
      Description = "Show persistent desktop alerts for kernel hardware warnings";
      # A broken follower must surface as a failed unit, not restart forever.
      StartLimitIntervalSec = 300;
      StartLimitBurst = 5;
      # Notifications are exported by plasmashell once the session is up.
      After = [ "plasma-plasmashell.service" ];
    };
    Service = {
      Type = "simple";
      ExecStart = "${lib.getExe pkgs.host-tools} hardware-alerts";
      # Optional: mirrors alerts to Telegram when the env file with the bot
      # credentials exists; local desktop alerts work without it.
      EnvironmentFile = "-%h/.config/telegram.env";
      Environment = [
        "PATH=/usr/bin:/bin"
        "DBUS_SESSION_BUS_ADDRESS=unix:path=%t/bus"
      ];
      Restart = "always";
      RestartSec = 5;
      StateDirectory = "hardware-alerts";
      StateDirectoryMode = "0700";
    };
    Install.WantedBy = [ "default.target" ];
  };

  systemd.user.services.lu-acton-2-a2dp-watch = {
    Unit = {
      Description = "Keep LU ACTON 2 on A2DP output";
      After = [
        "wireplumber.service"
        "pipewire.service"
        "pipewire-pulse.service"
      ];
      Wants = [
        "wireplumber.service"
        "pipewire.service"
        "pipewire-pulse.service"
      ];
    };
    Service = {
      Type = "simple";
      ExecStart = "${lib.getExe pkgs.host-tools} audio-watch";
      Environment = "PATH=${if isNixOS then lib.makeBinPath [ pkgs.pulseaudio ] else "/usr/bin:/bin"}";
      Restart = "always";
      RestartSec = 2;
    };
    Install.WantedBy = [ "default.target" ];
  };
}
