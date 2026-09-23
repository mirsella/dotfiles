{ pkgs, ... }:
{
  systemd.services.night-suspend = {
    description = "Suspend at midnight when nobody has used the server recently";
    path = with pkgs; [ iproute2 procps util-linux ];
    serviceConfig = {
      Type = "oneshot";
      ExecStart = "${pkgs.python3}/bin/python3 ${./night-suspend.py}";
    };
    startAt = "00:00";
  };
  # Never run an overdue midnight check on boot or wake.
  systemd.timers.night-suspend.timerConfig.Persistent = false;
}
