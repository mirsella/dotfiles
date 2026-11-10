{ lib, pkgs, ... }:
{
  # Plasma 6.8's APOD provider still scrapes NASA's retired website. Use the
  # replacement JSON feed until the native provider supports it on both hosts.
  systemd.user.services.nasa-wallpaper = {
    Unit = {
      Description = "NASA Astronomy Picture of the Day wallpaper";
      After = [ "plasma-plasmashell.service" ];
      PartOf = [ "graphical-session.target" ];
    };
    Service = {
      Type = "oneshot";
      ExecStart = "${lib.getExe pkgs.host-tools} nasa-wallpaper";
      TimeoutStartSec = "3min";
    };
  };

  systemd.user.timers.nasa-wallpaper = {
    Unit = {
      Description = "Refresh the daily NASA wallpaper during Plasma sessions";
      PartOf = [ "graphical-session.target" ];
    };
    Timer = {
      OnActiveSec = "15s";
      # Retry after offline starts and pick up the next day's publication.
      OnUnitInactiveSec = "1h";
    };
    Install.WantedBy = [ "graphical-session.target" ];
  };
}
