set -euo pipefail
date --iso-8601=seconds
/nix/store/y5x8nw8lvhjjmxp9m1aiy4hkgsfsrr7g-predator-pm-study-0.1.0/bin/predator-pm-wake-validation check /var/lib/predator-pm-wake-validation/wake-20261031-plan.conf 767cd2aff3f22cc1b8691ae63e3435d83279f7952b763870a53e0b680214b942
systemctl list-timers --all --no-pager nextcloud-cron.timer
test ! -e /var/lib/predator-pm-wake-validation/wake-20261031-requested.txt
systemd-run --unit=predator-pm-wake-wake-20261031 --on-calendar=@1793481300 --timer-property=AccuracySec=1s --timer-property=RandomizedDelaySec=0 --timer-property=Persistent=false --property=Type=exec --property=TimeoutStartSec=30s --property=RuntimeMaxSec=2400s --property=TimeoutStopSec=180s --property=Environment=PATH=/run/current-system/sw/bin --property='ExecStopPost=/nix/store/y5x8nw8lvhjjmxp9m1aiy4hkgsfsrr7g-predator-pm-study-0.1.0/bin/predator-pm-wake-validation restore /var/lib/predator-pm-wake-validation/wake-20261031-plan.conf 767cd2aff3f22cc1b8691ae63e3435d83279f7952b763870a53e0b680214b942' /nix/store/y5x8nw8lvhjjmxp9m1aiy4hkgsfsrr7g-predator-pm-study-0.1.0/bin/predator-pm-wake-validation run /var/lib/predator-pm-wake-validation/wake-20261031-plan.conf 767cd2aff3f22cc1b8691ae63e3435d83279f7952b763870a53e0b680214b942
systemctl show predator-pm-wake-wake-20261031.service -p Type -p ActiveState -p RuntimeMaxUSec -p TimeoutStopUSec -p ExecStopPost
systemctl list-timers --all --no-pager predator-pm-wake-wake-20261031.timer
test "$(systemctl show predator-pm-wake-wake-20261031.service -p ActiveState --value)" = inactive
test ! -e /var/lib/predator-pm-wake-validation/wake-20261031-requested.txt
