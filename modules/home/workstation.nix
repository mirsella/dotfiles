{ config, ... }:
{
  targets.genericLinux.gpu.enable = false;

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
}
