{ lib, pkgs, ... }:
let
  host-tools = lib.getExe pkgs.host-tools;
  theme-watch = "${host-tools} theme-watch --dconf ${lib.getExe' pkgs.dconf "dconf"}";
in
{
  # Chromium and Electron only resolve the KDE Wayland system color scheme
  # after xdg-desktop-portal emits a real org.freedesktop.portal.Settings
  # change (chromium#462191707, electron#48736): apps started before the
  # portal settles come up with the opposite appearance and never correct
  # themselves. Route Settings to the GTK backend, which follows gsettings
  # (kept in sync with the Plasma scheme by kde-gtk-config), re-assert the
  # current scheme whenever new renderers appear, and repair the stored
  # scheme if the watcher is stopped mid-nudge.
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
      Description = "Re-assert the system color scheme for new Chromium/Electron apps (electron#48736 workaround)";
      After = [
        "graphical-session.target"
        "xdg-desktop-portal.service"
      ];
      PartOf = [ "graphical-session.target" ];
    };
    Service = {
      Type = "simple";
      ExecStart = theme-watch;
      ExecStopPost = "${theme-watch} --recover";
      Restart = "on-failure";
      RestartSec = "5s";
    };
    Install.WantedBy = [ "graphical-session.target" ];
  };
}
