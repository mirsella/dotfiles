{ config, hostName, lib, ... }:
{
  targets.genericLinux.gpu.enable = false;

  systemd.user.slices.cargo = {
    Unit.Description = "Cargo build scopes";
    Slice.ManagedOOMSwap = "kill";
  };

  xdg.configFile."systemd/user/app-org.wezfurlong.wezterm@.service.d/override.conf".text = ''
    [Service]
    OOMPolicy=continue
  '';

  sops = {
    age.sshKeyPaths = [ "${config.home.homeDirectory}/.ssh/id_ed25519" ];
    defaultSopsFile = ../../secrets/services.yaml;
    secrets = builtins.mapAttrs (_: path: {
      path = "${config.home.homeDirectory}/.config/${path}";
    }) (import ../user-secrets.nix) // {
      davfs2_secrets = {
        sopsFile = ../../secrets/webdav.yaml;
        path = "${config.home.homeDirectory}/.davfs2/secrets";
        mode = "0600";
      };
      rclone_conf = {
        sopsFile = ../../secrets/webdav.yaml;
        path = "${config.home.homeDirectory}/.config/rclone/rclone.conf";
        mode = "0600";
      };
    };
  };

  systemd.user.services.rclone-nextcloud = let
    mountPoint = "%h/Documents/Nextcloud";
  in {
    Unit = {
      Description = "Nextcloud WebDAV mount";
      After = [ "sops-nix.service" ];
      # Home Manager restarts sops-nix on activation; keep mounted files available.
      Wants = [ "sops-nix.service" ];
    };
    Install.WantedBy = [ "default.target" ];
    Service = {
      Type = "notify";
      ExecStartPre = "/usr/bin/mkdir -p ${mountPoint}";
      ExecStart = "/usr/bin/rclone mount nextcloud: ${mountPoint} --vfs-cache-mode writes --poll-interval 0";
      SuccessExitStatus = "143";
      Restart = "always";
      RestartSec = 10;
    };
  };

  systemd.user.services.ryzenadj-laptop = lib.mkIf (hostName == "laptop") {
    Unit.Description = "Apply RyzenAdj performance limits";
    Service = {
      Type = "simple";
      ExecStartPre = "/usr/bin/sudo -n /usr/bin/modprobe ryzen_smu";
      ExecStart = "%h/.local/bin/ryzenadj-laptop --watch";
      Restart = "on-failure";
      RestartSec = 5;
    };
    Install.WantedBy = [ "default.target" ];
  };

  systemd.user.services.lu-acton-2-a2dp-watch = lib.mkIf (hostName == "laptop") {
    Unit = {
      Description = "Keep LU ACTON 2 on A2DP output";
      After = [ "wireplumber.service" "pipewire.service" "pipewire-pulse.service" ];
      Wants = [ "wireplumber.service" "pipewire.service" "pipewire-pulse.service" ];
    };
    Service = {
      Type = "simple";
      ExecStart = "%h/.local/bin/lu-acton-2-a2dp-watch";
      Restart = "always";
      RestartSec = 2;
    };
    Install.WantedBy = [ "default.target" ];
  };

  systemd.user.services.kache = {
    Unit.Description = "kache build cache daemon";
    Service = {
      Type = "simple";
      ExecStart = "%h/.local/share/cargo/bin/kache daemon run";
      Restart = "on-failure";
      RestartSec = "5s";
      Environment = "KACHE_LOG=kache=info";
    };
    Install.WantedBy = [ "default.target" ];
  };

  xdg.configFile."systemd/user/xdg-desktop-portal.service.d/override.conf" =
    lib.mkIf (hostName == "main") {
      text = ''
        [Service]
        MemoryMax=1G
        Restart=always
        RestartSec=1
      '';
    };
}
