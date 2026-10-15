{ lib, pkgs, ... }:
let
  host-tools = lib.getExe pkgs.host-tools;
  theme-watch = "${host-tools} theme-watch --dconf ${lib.getExe' pkgs.dconf "dconf"}";
in
{
  # Route Settings through GTK and keep its gsettings color scheme aligned
  # with Plasma if kde-gtk-config stops mirroring the palette. Only write
  # genuine changes: fake transitions to wake Electron's theme listener
  # (chromium#462191707, electron#48736) visibly flash listening applications.
  xdg.configFile."xdg-desktop-portal/kde-portals.conf" = {
    force = true;
    text = ''
      [preferred]
      default=kde
      org.freedesktop.impl.portal.Settings=gtk
      org.freedesktop.impl.portal.Secret=kwallet
      org.freedesktop.impl.portal.Notification=plasmanotify
    '';
  };

  systemd.user.services.electron-theme-watch = {
    Unit = {
      Description = "Keep the desktop portal color scheme aligned with Plasma";
      After = [
        "graphical-session.target"
        "xdg-desktop-portal.service"
      ];
      PartOf = [ "graphical-session.target" ];
    };
    Service = {
      Type = "simple";
      ExecStart = theme-watch;
      Restart = "on-failure";
      RestartSec = "5s";
    };
    Install.WantedBy = [ "graphical-session.target" ];
  };
}
