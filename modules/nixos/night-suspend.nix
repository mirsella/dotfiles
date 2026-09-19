{ pkgs, ... }:
{
  systemd.services.night-suspend = {
    description = "Suspend after midnight unless something is using the box";
    serviceConfig.Type = "oneshot";
    path = with pkgs; [ coreutils iproute2 ];
    script = ''
      blockers=()
      log() { echo "night-suspend: $1"; }

      users_now=$(users)
      if [ -n "$users_now" ]; then
        blockers+=("sessions: $users_now")
      else
        log "no logged-in users (tty or ssh)"
      fi

      conns=$(ss -Htn state established '( sport = :80 or sport = :443 or sport = :4096 or sport = :4097 or sport = :14096 or sport = :14097 )' | wc -l)
      if [ "$conns" -gt 0 ]; then
        blockers+=("$conns active service connections (web/apps)")
      else
        log "no active connections to web/app ports"
      fi

      devs=()
      for id in wwn-0x5000c500aa3cc143 wwn-0x500003961228993f; do
        devs+=("$(basename "$(readlink "/dev/disk/by-id/$id")")")
      done
      snap() {
        for sd in "''${devs[@]}"; do
          read -r _ _ r _ _ _ w _ < "/sys/block/$sd/stat"
          printf '%s %s\n' "$r" "$w"
        done
      }
      before=$(snap)
      log "disk counters sampled, waiting 5 min"
      sleep 300
      if [ "$(snap)" != "$before" ]; then
        blockers+=("tank HDDs saw reads/writes in the last 5 min")
      else
        log "tank HDDs idle for 5 min"
      fi

      if [ "''${#blockers[@]}" -gt 0 ]; then
        log "blocked: $(IFS='; '; echo "''${blockers[*]}")"
      else
        log "all clear, suspending"
        systemctl suspend
      fi
    '';
  };

  systemd.timers.night-suspend = {
    wantedBy = [ "timers.target" ];
    timerConfig.OnCalendar = [
      "00:30"
      "02:30"
      "04:30"
    ];
  };
}
