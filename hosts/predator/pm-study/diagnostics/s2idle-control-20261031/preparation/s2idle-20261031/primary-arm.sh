set -euo pipefail
date --iso-8601=seconds
/nix/store/glb3dymq7hxaym5dg6602m1h3isjf17n-predator-pm-study-0.1.0/bin/predator-pm-s2idle check /var/lib/predator-pm-s2idle/s2idle-20261031-plan.conf f22e464bb3aae8404d25ec05f7b4ae3002ba0c757a9d5e69f1199195f13a5820
systemctl list-timers --all --no-pager nextcloud-cron.timer
test ! -e /var/lib/predator-pm-s2idle/s2idle-20261031-requested.txt
systemd-run --unit=predator-pm-s2idle-s2idle-20261031 --on-calendar=@1793444820 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=TimeoutStartSec=30s --property=RuntimeMaxSec=2400s --property=TimeoutStopSec=180s --property=Environment=PATH=/run/current-system/sw/bin --property='ExecStopPost=/nix/store/glb3dymq7hxaym5dg6602m1h3isjf17n-predator-pm-study-0.1.0/bin/predator-pm-s2idle restore /var/lib/predator-pm-s2idle/s2idle-20261031-plan.conf f22e464bb3aae8404d25ec05f7b4ae3002ba0c757a9d5e69f1199195f13a5820' /nix/store/glb3dymq7hxaym5dg6602m1h3isjf17n-predator-pm-study-0.1.0/bin/predator-pm-s2idle run /var/lib/predator-pm-s2idle/s2idle-20261031-plan.conf f22e464bb3aae8404d25ec05f7b4ae3002ba0c757a9d5e69f1199195f13a5820
systemctl show predator-pm-s2idle-s2idle-20261031.service -p Type -p ActiveState -p RuntimeMaxUSec -p TimeoutStopUSec -p ExecStopPost
systemctl list-timers --all --no-pager predator-pm-s2idle-s2idle-20261031.timer
test "$(systemctl show predator-pm-s2idle-s2idle-20261031.service -p ActiveState --value)" = inactive
test ! -e /var/lib/predator-pm-s2idle/s2idle-20261031-requested.txt
