{ lib, pkgs, ... }:
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
        "plasma-kded6.service"
        "xdg-desktop-portal.service"
      ];
      PartOf = [ "graphical-session.target" ];
    };
    Service = {
      Type = "simple";
      # Adwaita avoids Electron's stale theme state with Breeze GTK (electron#48736).
      # Let KDE write GTK settings; Plasma's automatic Breeze themes stay intact.
      ExecStartPre = "${lib.getExe' pkgs.systemd "busctl"} --user --timeout=10s call org.kde.GtkConfig /GtkConfig org.kde.GtkConfig setGtkTheme s Adwaita";
      ExecStart = "${lib.getExe pkgs.host-tools} theme-watch --dconf ${lib.getExe' pkgs.dconf "dconf"}";
      Restart = "on-failure";
      RestartSec = "5s";
    };
    Install.WantedBy = [ "graphical-session.target" ];
  };
}
