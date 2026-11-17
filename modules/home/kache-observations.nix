{ pkgs, lib, ... }:
{
  systemd.user.services.kache-observe = {
    Unit.Description = "Read-only Kache reuse and physical-disk observations";
    Service = {
      Type = "oneshot";
      ExecStart = "${pkgs.host-tools}/bin/kache-observe --cache %h/.cache/kache --output %h/.local/state/kache-observations --kache %h/.local/bin/kache --config %h/.config/kache/config.toml";
      Environment = "PATH=${lib.makeBinPath [ pkgs.sqlite pkgs.btrfs-progs pkgs.coreutils pkgs.systemd ]}";
      UMask = "0077";
      Nice = 19;
      IOSchedulingClass = "idle";
      CPUQuota = "25%";
      MemoryMax = "768M";
      TimeoutStartSec = "5min";
    };
  };
  systemd.user.timers.kache-observe = {
    Unit.Description = "Capture Kache evidence twice daily";
    Timer = {
      OnCalendar = "*-*-* 00,12:15:00";
      RandomizedDelaySec = "10min";
      Persistent = true;
    };
    Install.WantedBy = [ "timers.target" ];
  };
}
