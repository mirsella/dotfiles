set -euo pipefail
cd /var/lib
tar -cf - predator-pm-diagnostic/d-processors-01-20261030-* predator-pm-study/last-completed.txt
