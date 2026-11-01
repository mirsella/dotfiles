set -euo pipefail
date --iso-8601=seconds
test "$(cat /proc/sys/kernel/random/boot_id)" = f8eb6e44-85ae-4e56-bc00-587876699945
test "$(cat /sys/power/suspend_stats/success)" = 1
test "$(cat /sys/power/suspend_stats/fail)" = 0
test "$(systemctl show systemd-suspend.service -p ActiveState --value)" = inactive
test "$(systemctl show systemd-suspend.service -p MainPID --value)" = 0
test -z "$(systemctl list-jobs --no-legend --plain)"
test "$(systemctl show predator-pm-diagnostic-d-freezer-02-20261030.service -p ActiveState --value)" = failed
test "$(systemctl show predator-pm-diagnostic-d-freezer-02-20261030.service -p MainPID --value)" = 0
systemctl reset-failed predator-pm-diagnostic-d-freezer-02-20261030.service
mkdir /home/mirsella/dev/dotfiles/.scratch/pm-diagnostic-readiness-20261030
nix-store --realise /nix/store/gqgwph5fdwycdl019dl2gwg6x32mvhan-predator-pm-study-0.1.0 --add-root /home/mirsella/dev/dotfiles/.scratch/pm-diagnostic-readiness-20261030/runtime-root
/nix/store/gqgwph5fdwycdl019dl2gwg6x32mvhan-predator-pm-study-0.1.0/bin/predator-pm-diagnostic revalidate /var/lib/predator-pm-diagnostic/d-freezer-02-20261030-plan.conf 91e1616b734ee77a0077bccd130cecfe5621dfa0fba8b2326207e1064a8f4484
cat /var/lib/predator-pm-diagnostic/d-freezer-02-20261030-revalidated.txt
sha256sum /var/lib/predator-pm-diagnostic/d-freezer-02-20261030-revalidated.txt
test ! -e /var/lib/predator-pm-diagnostic/d-freezer-02-20261030-completed.txt
sha256sum /var/lib/predator-pm-study/last-completed.txt
systemctl --failed --no-pager
systemctl list-jobs --no-pager
