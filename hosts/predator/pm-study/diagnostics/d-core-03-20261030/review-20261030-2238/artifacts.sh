set -euo pipefail
cd /var/lib
tar -cf - predator-pm-diagnostic/d-core-03-20261030-* predator-pm-study/last-completed.txt
