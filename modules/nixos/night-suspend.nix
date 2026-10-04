{ config, pkgs, ... }:
{
  systemd.services.night-suspend = {
    description = "Suspend overnight when nobody has used the server recently";
    path = with pkgs; [ iproute2 procps util-linux coreutils config.boot.zfs.package ];
    environment.PYTHONPATH = "${pkgs.writeTextDir "storage_events.py" (builtins.readFile ../../hosts/predator/storage_events.py)}";
    serviceConfig = {
      Type = "oneshot";
      ExecStart = "${pkgs.python3}/bin/python3 ${./night-suspend.py}";
    };
    startAt = [
      "*-*-* 00:30..59:00"
      "*-*-* 01..06:*:00"
    ];
  };
  # Do not replay missed overnight checks on boot.
  systemd.timers.night-suspend.timerConfig.Persistent = false;
}
