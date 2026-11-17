{ config, lib, pkgs, ... }:
let
  users = [ "mirsella" "root" ];
in
{
  environment.systemPackages = [ pkgs.gtrash ];
  # Interactive Bash/Zsh only; scripts and an explicit `command rm` stay native.
  # Nushell's editable wrapper lives in chezmoi's nushell/alias.nu.
  environment.shellAliases.rm = "${pkgs.gtrash}/bin/gtrash put";

  systemd.services = lib.listToAttrs (map (name:
    lib.nameValuePair "gtrash-prune-${name}" {
      description = "Prune ${name}'s trash entries older than 30 days";
      after = [ "zfs-import.target" ];
      environment.XDG_DATA_HOME = "${config.users.users.${name}.home}/.local/share";
      serviceConfig = {
        Type = "oneshot";
        User = name;
        SetLoginEnvironment = true;
        ExecStart = "${pkgs.gtrash}/bin/gtrash prune --day 30 --force";
        TimeoutStartSec = "30min";
        UMask = "0077";
        # Discovery ignores read-only data mounts, even with writable Trash
        # exceptions. Protect the OS, but keep normal per-user data permissions.
        ProtectSystem = "full";
        ProtectHome = false;
        PrivateTmp = true;
        NoNewPrivileges = true;
      };
    }) users);
  systemd.timers = lib.genAttrs (map (name: "gtrash-prune-${name}") users) (_: {
    wantedBy = [ "timers.target" ];
    timerConfig = {
      OnCalendar = "daily";
      RandomizedDelaySec = "15min";
      Persistent = true;
    };
  });
}
