{ lib, pkgs, ... }:
let
  hdds = [ "wwn-0x5000c500aa3cc143" "wwn-0x500003961228993f" ];
  # Include the media SSD so background photo processing prevents suspend.
  activityDisks = hdds ++ [ "ata-CT240BX500SSD1_2004E3E6DE68" ];
  idleSeconds = 45 * 60;
  check = pkgs.writeShellApplication {
    name = "night-suspend";
    runtimeInputs = with pkgs; [ coreutils iproute2 systemd jq procps ];
    text = ''
      stay_awake() {
        printf 'night-suspend: blocked: %s\n' "$@"
        exit 0
      }

      blockers=()
      counters=()
      read -r now _ < /proc/uptime
      now=''${now%.*}
      for id in ${lib.escapeShellArgs activityDisks}; do
        if ! dev=$(readlink "/dev/disk/by-id/$id"); then
          blockers+=("$id: device missing")
          continue
        fi
        read -r _ _ rsec _ _ _ wsec _ pending _ < "/sys/block/''${dev##*/}/stat"
        counters+=("$id:$rsec:$wsec:$pending")
        if (( pending > 0 )); then
          blockers+=("$id: I/O in progress")
        fi
      done

      # All disks must be quiet, so any activity resets one shared deadline.
      state="$RUNTIME_DIRECTORY/idle"
      lastActive=$now previous=""
      if [ -f "$state" ]; then
        read -r lastActive previous < "$state"
      fi
      current="''${counters[*]}"
      if [ "$current" != "$previous" ]; then
        lastActive=$now
        printf '%s %s\n' "$now" "$current" > "$state"
        echo "Disk activity: idle clock reset"
      fi
      if (( now - lastActive < ${toString idleSeconds} )); then
        blockers+=("less than 45 minutes quiet")
      fi

      # Keep the idle baseline during the day, without querying disks themselves.
      hour=$(date +%H)
      if [ "$hour" -gt 6 ]; then
        exit 0
      fi
      if (( ''${#blockers[@]} )); then
        stay_awake "''${blockers[@]}"
      fi

      sessions=$(loginctl list-sessions --json=short | jq '[.[] | select(.class | startswith("user"))] | length')
      if (( sessions > 0 )); then
        stay_awake "logged-in sessions"
      fi
      conns=$(ss -Htn state established '( sport = :22 or sport = :80 or sport = :443 or sport = :4096 or sport = :4097 or sport = :14096 or sport = :14097 or sport = :2283 )')
      if [ -n "$conns" ]; then
        stay_awake "active SSH or service connections"
      fi
      maintenance=$(systemctl list-units --no-pager --no-legend --plain \
        --state=active,activating,reloading,deactivating \
        nixos-upgrade.service sanoid.service zfs-scrub.service zpool-trim.service nextcloud-setup.service nextcloud-cron.service)
      if [ -n "$maintenance" ]; then
        stay_awake "maintenance: $maintenance"
      fi
      if pgrep -G nixbld >/dev/null; then
        stay_awake "Nix build workers"
      else
        status=$?
        if (( status != 1 )); then
          exit "$status"
        fi
      fi

      alarm=$(date -d '07:00' +%s)
      echo 0 > /sys/class/rtc/rtc0/wakealarm
      echo "$alarm" > /sys/class/rtc/rtc0/wakealarm
      echo "night-suspend: RTC alarm set for 07:00, requesting suspend"
      systemctl --check-inhibitors=yes suspend
    '';
  };
in
{
  systemd.services.hd-idle = {
    description = "Spin down tank HDDs after 45 minutes idle";
    wantedBy = [ "multi-user.target" ];
    serviceConfig = {
      ExecStart = "${pkgs.hd-idle}/bin/hd-idle -i 0 "
        + lib.concatMapStringsSep " " (id: "-a /dev/disk/by-id/${id} -i ${toString idleSeconds} -c scsi") hdds
        + " -l /var/log/hd-idle.log";
      Restart = "on-failure";
      RestartSec = "5s";
    };
  };
  services.logrotate.settings.hd-idle = {
    files = [ "/var/log/hd-idle.log" ];
    weekly = true;
    rotate = 4;
    compress = true;
    missingok = true;
    notifempty = true;
    copytruncate = true;
  };

  systemd.services.night-suspend = {
    description = "Track disk idleness and suspend safely during the night window";
    serviceConfig = {
      Type = "oneshot";
      ExecStart = lib.getExe check;
      RuntimeDirectory = "night-suspend";
      RuntimeDirectoryPreserve = "yes";
    };
    startAt = [ "*-*-* 07..23:0/5:00" "*-*-* 00..06:*:00" ];
  };
}
