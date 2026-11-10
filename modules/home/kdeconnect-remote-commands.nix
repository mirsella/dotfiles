{
  config,
  lib,
  pkgs,
  isNixOS,
  ...
}:
let
  # KDE Connect's remote-command list is stored on the desktop, per paired
  # device. The phone only caches the list for its widgets and sends the key
  # back when a command is tapped.
  #
  # Unlock is intentionally absent: logind requires admin authentication to
  # unlock a session, and a remote unlock would defeat the lock screen.
  # "Wake Screen" only restores DPMS and leaves the session locked.
  commands = {
    "hibernate" = {
      name = "Hibernate";
      command = "systemctl hibernate";
    };
    "lock-screen" = {
      name = "Lock Screen";
      command = "loginctl lock-session";
    };
    "power-off" = {
      name = "Power Off";
      command = "systemctl poweroff";
    };
    "reboot" = {
      name = "Reboot";
      command = "systemctl reboot";
    };
    "screen-off" = {
      name = "Screen Off";
      command = "kscreen-doctor --dpms off";
    };
    "screen-on" = {
      name = "Wake Screen";
      command = "kscreen-doctor --dpms on";
    };
    "suspend" = {
      name = "Suspend";
      command = "systemctl suspend";
    };
  };

  commandsJson = builtins.toJSON commands;
  # KConfig stores this as a quoted string containing @ByteArray(...); escape
  # the JSON quotes and backslashes so QSettings reads it back as a byte array.
  escapedJson = lib.replaceStrings [ "\\" "\"" ] [ "\\\\" "\\\"" ] commandsJson;

  apply = pkgs.writeShellScript "kdeconnect-remote-commands" ''
    set -eu

    if ! command -v kdeconnect-cli >/dev/null 2>&1 \
      || ! command -v qdbus6 >/dev/null 2>&1 \
      || ! command -v kwriteconfig6 >/dev/null 2>&1; then
      exit 0
    fi

    configured=0
    for id in $(kdeconnect-cli --list-devices --id-only 2>/dev/null); do
      type=$(qdbus6 org.kde.kdeconnect "/modules/kdeconnect/devices/$id" \
        org.kde.kdeconnect.device.type 2>/dev/null || true)
      [ "$type" = "phone" ] || continue

      dir="$HOME/.config/kdeconnect/$id"
      mkdir -p "$dir/kdeconnect_runcommand"
      cat > "$dir/kdeconnect_runcommand/config" <<'EOF'
    [General]
    commands="@ByteArray(${escapedJson})"
    EOF

      kwriteconfig6 --file "$dir/config" --group Plugins \
        --key kdeconnect_runcommandEnabled true
      qdbus6 org.kde.kdeconnect "/modules/kdeconnect/devices/$id" \
        org.kde.kdeconnect.device.reloadPlugins >/dev/null 2>&1 || true
      configured=$((configured + 1))
    done

    if [ "$configured" -gt 0 ]; then
      echo "kdeconnect-remote-commands: configured $configured phone device(s)"
    fi
  '';
in
{
  config = lib.mkIf (!isNixOS) {
    home.file.".local/bin/kdeconnect-remote-commands" = {
      source = apply;
      force = true;
    };

    home.activation.kdeconnectRemoteCommands = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
      run ${apply}
    '';
  };
}
