{ config, isNixOS, lib, ... }:
{
  config = lib.mkIf (!isNixOS) {
    xdg.dataFile."arch-package-cache/paccache.conf".text = ''
      [Service]
      EnvironmentFile=
      Environment=PACCACHE_ARGS=--keep=1
    '';

    # Pacman's cache is root-owned. Keep the native, sandboxed service and timer;
    # only its retention policy belongs to Home Manager.
    home.activation.archPackageCache = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
      if [[ ! -x /usr/bin/paccache ]]; then
        errorEcho "Arch package cache cleanup requires pacman-contrib (sudo pacman -S --needed pacman-contrib)."
        exit 1
      fi
      run /usr/bin/sudo -n /usr/bin/install -Dm644 \
        ${config.xdg.dataFile."arch-package-cache/paccache.conf".source} \
        /etc/systemd/system/paccache.service.d/50-home-manager.conf
      run /usr/bin/sudo -n /usr/bin/systemctl daemon-reload
      run /usr/bin/sudo -n /usr/bin/systemctl enable --now paccache.timer
    '';
  };
}
