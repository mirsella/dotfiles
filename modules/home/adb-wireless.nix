{ config, isNixOS, lib, pkgs, ... }:
let
  # Use Android Studio's SDK on Arch so both clients share the same ADB version.
  adb = if isNixOS then "${pkgs.android-tools}/bin/adb"
    else "${config.home.homeDirectory}/.local/share/android/sdk/platform-tools/adb";
  idleMinutes = config.services.adb-wireless.idleMinutes;
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
  options.services.adb-wireless.idleMinutes = lib.mkOption {
    type = lib.types.nullOr lib.types.ints.positive;
    default = null;
    description = ''
      Turn Wireless debugging off on the phone after this many minutes without
      ADB traffic. The setting lives on the phone, so leave it unset on hosts
      that share the phone with another host running this service.
    '';
  };
  config.systemd.user.services.adb-wireless = {
    Unit.Description = "Reconnect the paired Android phone through mDNS";
    Service = {
      Type = "simple";
      ExecStartPre = "${waitForNetwork}";
      ExecStart = "${lib.getExe pkgs.host-tools} adb-watch --adb ${adb} --serial 188b31ee"
        + lib.optionalString (idleMinutes != null) " --idle-minutes ${toString idleMinutes}";
      # `ss` reports the payload bytes of the device socket, which decide idleness.
      Environment = "PATH=${lib.makeBinPath [ pkgs.coreutils pkgs.iproute2 ]}";
      Restart = "on-failure";
      RestartSec = "2s";
    };
    Install.WantedBy = [ "default.target" ];
  };
}
