{ config, pkgs, ... }:
{
  systemd.services.night-suspend = {
    description = "Suspend overnight when nobody has used the server recently";
    path = with pkgs; [ iproute2 procps util-linux coreutils config.boot.zfs.package ];
    serviceConfig = {
      Type = "oneshot";
      ExecStart = "${pkgs.host-tools}/bin/host-tools night-suspend";
    };
    startAt = [
      "*-*-* 00:30..59:00"
      "*-*-* 01..06:*:00"
    ];
  };
  # Do not replay missed overnight checks on boot.
  systemd.timers.night-suspend.timerConfig.Persistent = false;
}
