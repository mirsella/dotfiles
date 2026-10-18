{ config, isNixOS, lib, pkgs, ... }:
let
  # Use Android Studio's SDK on Arch so both clients share the same ADB version.
  adb = if isNixOS then "${pkgs.android-tools}/bin/adb"
    else "${config.home.homeDirectory}/.local/share/android/sdk/platform-tools/adb";
  # The ADB server binds its mDNS responder to the interfaces that exist when
  # it starts. A login that beats the Wi-Fi lease leaves discovery dead until
  # the server is restarted, so wait for a default route before starting it.
  waitForNetwork = pkgs.writeShellScript "adb-wireless-wait-network" ''
    export PATH=${lib.makeBinPath [ pkgs.coreutils pkgs.iproute2 ]}
    for _ in $(seq 1 90); do
      [ -n "$(ip -4 route show default)" ] && exit 0
      sleep 1
    done
    echo "adb-wireless: no default route after 90s, starting ADB anyway" >&2
  '';
in
{
  systemd.user.services.adb-wireless = {
    Unit.Description = "Reconnect the paired Android phone through mDNS";
    Service = {
      Type = "simple";
      ExecStartPre = "${waitForNetwork}";
      ExecStart = "${lib.getExe pkgs.host-tools} adb-watch --adb ${adb} --serial 188b31ee";
      Environment = "PATH=${lib.makeBinPath [ pkgs.coreutils ]}";
      Restart = "on-failure";
      RestartSec = "2s";
    };
    Install.WantedBy = [ "default.target" ];
  };
}
