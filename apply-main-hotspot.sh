#!/usr/bin/env bash
set -euo pipefail

die() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

if [[ $(id -u) -eq 0 ]]; then
    die 'run this as your regular user; the script uses sudo for system changes'
fi

[[ $(hostname -s) == main ]] || die 'this configuration is only for host main'

for command in pass-cli nmcli sudo systemctl dockerd; do
    command -v "$command" >/dev/null 2>&1 || die "$command is required"
done

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
snapshot="$repo_root/system/main"
docker_config="$snapshot/etc/docker/daemon.json"
docker_dropin="$snapshot/etc/systemd/system/docker.service.d/hotspot-forwarding.conf"
forwarding_script="$snapshot/usr/local/sbin/configure-hotspot-forwarding"

for file in "$docker_config" "$docker_dropin" "$forwarding_script"; do
    [[ -f $file ]] || die "required snapshot file is missing: $file"
done

dockerd --validate --config-file "$docker_config" >/dev/null
sh -n "$forwarding_script"

password=$(pass-cli item view 'pass://Personal/main hotspot/password')
password_length=${#password}
if ((password_length < 8 || password_length > 63)); then
    unset password
    die 'Proton Pass returned an invalid WPA2 password length'
fi

sudo install -Dm0644 "$docker_config" /etc/docker/daemon.json
sudo install -Dm0644 "$docker_dropin" \
    /etc/systemd/system/docker.service.d/hotspot-forwarding.conf
sudo install -Dm0755 "$forwarding_script" \
    /usr/local/sbin/configure-hotspot-forwarding

profile='main hotspot'
if ! sudo nmcli connection show "$profile" >/dev/null 2>&1; then
    sudo nmcli connection add type wifi ifname wlan0 con-name "$profile" ssid "$profile"
fi

sudo nmcli connection modify "$profile" \
    connection.id "$profile" \
    connection.interface-name wlan0 \
    connection.autoconnect yes \
    connection.autoconnect-priority 100 \
    connection.permissions '' \
    802-11-wireless.ssid "$profile" \
    802-11-wireless.mode ap \
    802-11-wireless.band bg \
    802-11-wireless.channel 6 \
    802-11-wireless-security.key-mgmt wpa-psk \
    802-11-wireless-security.proto rsn \
    802-11-wireless-security.pairwise ccmp \
    802-11-wireless-security.group ccmp \
    802-11-wireless-security.pmf disable \
    802-11-wireless-security.psk-flags 0 \
    ipv4.method shared \
    ipv4.addresses '' \
    ipv6.method auto

{
    printf 'set 802-11-wireless-security.psk %s\n' "$password"
    printf 'save persistent\nquit\n'
} | sudo nmcli connection edit "$profile" >/dev/null
unset password

sudo systemctl daemon-reload
sudo systemctl enable --now NetworkManager.service
sudo systemctl enable docker.service >/dev/null
sudo systemctl restart docker.service
sudo nmcli connection up "$profile" >/dev/null

printf 'Restored and activated %s.\n' "$profile"
