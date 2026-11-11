{ config, isNixOS, lib, pkgs, ... }:
let
  # Use Android Studio's SDK on Arch so both clients share the same ADB version.
  adb = if isNixOS then "${pkgs.android-tools}/bin/adb"
    else "${config.home.homeDirectory}/.local/share/android/sdk/platform-tools/adb";
  idleMinutes = config.services.adb-wireless.idleMinutes;
  serials = config.services.adb-wireless.serials;
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
  options.services.adb-wireless.serials = lib.mkOption {
    type = lib.types.nonEmptyListOf (lib.types.strMatching "[A-Za-z0-9-]+");
    default = [ "188b31ee" ];
    description = ''
      Android serial numbers (ro.serialno) to keep connected through mDNS.
      Add a phone only on hosts paired with it: ADB reports authentication
      errors for a serial the host does not know.
    '';
  };
  options.services.adb-wireless.idleMinutes = lib.mkOption {
    type = lib.types.nullOr (lib.types.ints.between 1 (7 * 24 * 60));
    default = 180;
    description = ''
      Turn Wireless debugging off on each watched phone once no workstation has
      used it for this many minutes. Each watcher records its own ADB use on the
      phone, so every host sharing a phone should set the same value; null never
      turns it off.
    '';
  };
  config.systemd.user.services.adb-wireless = {
    Unit.Description = "Reconnect the paired Android phone through mDNS";
    Service = {
      Type = "simple";
      ExecStartPre = "${waitForNetwork}";
      ExecStart = "${lib.getExe pkgs.host-tools} adb-watch --adb ${adb}"
        + lib.concatMapStrings (serial: " --serial ${serial}") serials
        + lib.optionalString (idleMinutes != null) " --idle-minutes ${toString idleMinutes}";
      # `ss` reports the payload bytes of the device socket, which decide idleness.
      Environment = "PATH=${lib.makeBinPath [ pkgs.coreutils pkgs.iproute2 ]}";
      Restart = "on-failure";
      RestartSec = "2s";
      # The ADB server the watcher starts also serves USB phones and every
      # other ADB client, so restarting the watcher must not take it down.
      KillMode = "process";
    };
    Install.WantedBy = [ "default.target" ];
  };
}
