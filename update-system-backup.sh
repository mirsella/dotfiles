#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
host=${1:-$(hostname -s)}

if [[ ! $host =~ ^[A-Za-z0-9._-]+$ ]]; then
    printf 'error: invalid hostname: %s\n' "$host" >&2
    exit 1
fi

destination="$repo_root/system/$host"
mkdir -p "$repo_root/system"
staging=$(mktemp -d "$repo_root/system/.update-$host.XXXXXX")
trap 'rm -rf -- "$staging"' EXIT

copy_file() {
    local source=$1
    local relative_path=$2
    local requirement=${3:-required}
    local target="$staging/$relative_path"

    if [[ ! -f $source ]] && ! sudo test -f "$source"; then
        if [[ $requirement == required ]]; then
            printf 'error: required file is missing: %s\n' "$source" >&2
            exit 1
        fi

        printf 'skip: optional file is missing: %s\n' "$source"
        return
    fi

    mkdir -p "$(dirname -- "$target")"
    if [[ -r $source ]]; then
        install -m 0644 -- "$source" "$target"
    else
        sudo install -m 0644 -o "$(id -u)" -g "$(id -g)" -- "$source" "$target"
    fi
    printf 'copied: %s\n' "$source"
}

copy_file /etc/fstab etc/fstab
copy_file /etc/mkinitcpio.conf etc/mkinitcpio.conf
copy_file /etc/systemd/zram-generator.conf etc/systemd/zram-generator.conf optional
copy_file /etc/sysctl.d/99-swappiness.conf etc/sysctl.d/99-swappiness.conf optional
copy_file /etc/coolercontrol/config.toml etc/coolercontrol/config.toml optional
copy_file /boot/loader/loader.conf boot/loader/loader.conf

mkdir -p "$staging/boot/loader/entries"
sudo find /boot/loader/entries -maxdepth 1 -type f -name '*.conf' \
    -exec install -m 0644 -o "$(id -u)" -g "$(id -g)" \
    -t "$staging/boot/loader/entries" -- {} +

shopt -s nullglob
boot_entries=("$staging"/boot/loader/entries/*.conf)
if ((${#boot_entries[@]} == 0)); then
    printf 'error: no systemd-boot entries found\n' >&2
    exit 1
fi
printf 'copied: %d systemd-boot entries\n' "${#boot_entries[@]}"

mkdir -p "$destination"
rm -rf -- "${destination:?}/etc" "${destination:?}/boot"
mv -- "$staging/etc" "$destination/etc"
mv -- "$staging/boot" "$destination/boot"

printf 'updated: %s\n' "$destination"
