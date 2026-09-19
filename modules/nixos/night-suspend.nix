{ pkgs, ... }:
{
  systemd.services.night-suspend = {
    description = "Suspend after midnight if the tank HDDs are idle";
    serviceConfig.Type = "oneshot";
    path = with pkgs; [ coreutils ];
    script = ''
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
      sleep 300
      if [ "$(snap)" = "$before" ]; then
        systemctl suspend
      else
        echo "tank HDDs active, skipping suspend"
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
