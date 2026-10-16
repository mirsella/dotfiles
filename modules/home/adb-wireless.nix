{ config, isNixOS, lib, pkgs, ... }:
let
  # Use Android Studio's SDK on Arch so both clients share the same ADB version.
  adb = if isNixOS then "${pkgs.android-tools}/bin/adb"
    else "${config.home.homeDirectory}/.local/share/android/sdk/platform-tools/adb";
in
{
  systemd.user.services.adb-wireless = {
    Unit.Description = "Reconnect the paired Android phone through mDNS";
    Service = {
      Type = "simple";
      ExecStart = "${lib.getExe pkgs.host-tools} adb-watch --adb ${adb} --serial 188b31ee";
      Environment = "PATH=${lib.makeBinPath [ pkgs.coreutils ]}";
      Restart = "on-failure";
      RestartSec = "2s";
    };
    Install.WantedBy = [ "default.target" ];
  };
}
