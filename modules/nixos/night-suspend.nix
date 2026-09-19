{ pkgs, ... }:
{
  systemd.services.night-suspend = {
    description = "Suspend during the night window unless something is using the box";
    serviceConfig.Type = "oneshot";
    path = with pkgs; [ coreutils iproute2 ];
    script = ''
      blockers=()
      log() { echo "night-suspend: $1"; }

      hour=$(date +%H)
      if [ "$hour" -lt 0 ] || [ "$hour" -gt 6 ]; then
        exit 0
      fi

      users_now=$(users)
      if [ -n "$users_now" ]; then
        blockers+=("sessions: $users_now")
      fi

      conns=$(ss -Htn state established '( sport = :80 or sport = :443 or sport = :4096 or sport = :4097 or sport = :14096 or sport = :14097 )' | wc -l)
      if [ "$conns" -gt 0 ]; then
        blockers+=("$conns active service connections (web/apps)")
      fi

      now=$(date +%s)
      for id in wwn-0x5000c500aa3cc143 wwn-0x500003961228993f; do
        state="/var/lib/hdd-sleep/$id"
        dev=$(readlink "/dev/disk/by-id/$id" 2>/dev/null) || { blockers+=("$id: device missing"); continue; }
        read -r _ _ rsec _ _ _ wsec _ < "/sys/block/$(basename "$dev")/stat"
        if [ ! -f "$state" ]; then
          blockers+=("$id: no idle baseline yet")
          continue
        fi
        read -r prsec pwsec lastActive < "$state"
        if [ "$rsec" != "$prsec" ] || [ "$wsec" != "$pwsec" ]; then
          blockers+=("$id: recent disk activity")
        elif [ $(( (now - lastActive) / 60 )) -lt 45 ]; then
          blockers+=("$id: only $(( (now - lastActive) / 60 )) min quiet (< 45)")
        fi
      done

      if [ "''${#blockers[@]}" -gt 0 ]; then
        log "blocked: $(IFS='; '; echo "''${blockers[*]}")"
      else
        alarm=$(date -d '07:00' +%s)
        echo 0 > /sys/class/rtc/rtc0/wakealarm 2>/dev/null || true
        echo "$alarm" > /sys/class/rtc/rtc0/wakealarm
        log "RTC alarm set for 07:00, suspending"
        systemctl suspend
      fi
    '';
  };

  systemd.timers.night-suspend = {
    wantedBy = [ "timers.target" ];
    timerConfig.OnCalendar = "*-*-* 00..06:*:00";
  };
}
