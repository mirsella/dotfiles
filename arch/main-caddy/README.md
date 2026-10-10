# Main web UI ingress (Arch)

Main runs the Arch Caddy package with a manually installed configuration. These
files are the tracked sources for `/etc/caddy/Caddyfile` and
`/etc/systemd/system/caddy.service.d/recovery.conf`.

## Network and services

Freebox forwards TCP 4096 and 4097 directly to main. TCP 80 and 443 go to
Predator. Main's web UIs work independently of Predator.

| Machine | Wired MAC | IPv4 address |
| --- | --- | --- |
| main | `04:d9:f5:f8:e9:0b` | `192.168.1.131` |
| predator | `98:29:a6:3a:a7:50` | `192.168.1.19` |

Both addresses have matching Freebox DHCP reservations. Predator also uses a
manual NetworkManager address; its reservation keeps the DHCP pool from giving
that address to another device.

| UI | Public URL | Loopback backend | User unit |
| --- | --- | --- | --- |
| OpenCode | `https://mirsella.mooo.com:4096` | `127.0.0.1:14096` | `opencode.service` |
| OpenChamber | `https://mirsella.mooo.com:4097` | `127.0.0.1:14097` | `openchamber.service` |

HTTPS also works at the LAN IP on those ports. Local HTTP access uses
`http://localhost:4096` and `http://localhost:4097`.

Home Manager enables the user units and owns their credential environment files.
User lingering lets the units start without a desktop login. Caddy runs as the
system user `caddy`; its configuration is `root:caddy`, mode `0640`.

The optional HTTP homepage uses `/srv/mirsella-home`. Its files need to be
readable by `caddy`; directories can use mode `0755` and static files `0644`.

## Install or restore

Run from the repository root on main, after applying its Home Manager
configuration and installing the OpenCode and OpenChamber packages:

```sh
sudo pacman -S --needed caddy &&
sudo systemd-tmpfiles --create /usr/lib/tmpfiles.d/caddy.conf &&
sudo install -d -o root -g caddy -m 0750 /etc/caddy &&
sudo -u caddy env XDG_DATA_HOME=/var/lib XDG_CONFIG_HOME=/etc caddy validate --config - --adapter caddyfile < arch/main-caddy/Caddyfile &&
sudo install -o root -g caddy -m 0640 arch/main-caddy/Caddyfile /etc/caddy/Caddyfile &&
sudo install -D -o root -g root -m 0644 arch/main-caddy/caddy.service.d/recovery.conf /etc/systemd/system/caddy.service.d/recovery.conf &&
sudo systemctl daemon-reload &&
sudo systemctl enable caddy NetworkManager-wait-online.service &&
sudo loginctl enable-linger mirsella &&
systemctl --user start opencode.service openchamber.service &&
sudo systemctl restart caddy
```

The command chain stops at the first failure. Validation reads the source through
stdin, so the `caddy` user does not need access to the private home directory.
A failed validation prevents that file from being installed.

Caddy's packaged unit orders startup after `network-online.target`. The drop-in
retries failures every five seconds, with at most ten starts in a 30-second
window. The window permits continued retries during a delayed LAN connection.

## Private certificates and browser access

The `main-ui` private CA issues 30-day leaf certificates through a 90-day
intermediate. Caddy renews both automatically before expiry. A stored browser
exception may need to be accepted again when the leaf certificate renews.

Caddy stores CA keys and certificates under `/var/lib/caddy`. This state is
disposable for the browser-exception setup: if it is lost, Caddy generates a new
CA and certificates, and clients need new exceptions. No CA installation or
certificate synchronization with Predator is required.

Predator configures `Strict-Transport-Security: max-age=0` on its HTTPS site.
HSTS covers every port of a hostname, so
enabling it there would prevent exceptions for main's private certificates.
If an existing browser profile still reports HSTS, visit
`https://mirsella.mooo.com/?clear-hsts=1`, sign in if Predator requests
authentication, then force a refresh. Reopen the UI and accept the certificate
warning. An unauthenticated `401` response may not include the reset header.

Lifetime changes apply when Caddy issues or renews certificates. It keeps
existing valid certificates until renewal.

## Checks

```sh
systemctl is-active caddy
systemctl --user is-active opencode.service openchamber.service
sudo systemctl show caddy -p Restart -p RestartUSec -p StartLimitIntervalUSec -p StartLimitBurst
sudo curl --cacert /var/lib/caddy/pki/authorities/main-ui/root.crt -I https://mirsella.mooo.com:4096/
sudo curl --cacert /var/lib/caddy/pki/authorities/main-ui/root.crt -I https://mirsella.mooo.com:4097/
```

OpenCode returns `401` before authentication; OpenChamber returns its unlock
page with `200`. After the next cold boot, check the units above and
`journalctl -b -u caddy` to verify startup on the reserved LAN address.
