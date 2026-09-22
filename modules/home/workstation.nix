{ config, ... }:
{
  targets.genericLinux.gpu.enable = false;

  sops = {
    age.sshKeyPaths = [ "/home/mirsella/.ssh/id_ed25519" ];
    defaultSopsFile = ../../secrets/services.yaml;
    secrets =
      let
        userSecret = name: {
          path = "${config.home.homeDirectory}/.config/${name}";
        };
      in
      {
        telegram_env = userSecret "telegram.env";
        opencode_server = userSecret "opencode/server.env";
        openchamber_server = userSecret "openchamber/server.env";
        env_secrets = userSecret "environment.d/55-secrets.conf";
        stuff_config = userSecret "stuff/config.toml";
        context7_accounts = userSecret "context7-account-broker/accounts.json";
        gdrive_gcp = userSecret "google-drive-mcp/gcp-oauth.keys.json";
        gdrive_tokens = userSecret "google-drive-mcp/tokens.json";
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

  systemd.user.services.rclone-nextcloud = {
    Unit = {
      Description = "Nextcloud WebDAV mount";
      After = [ "sops-nix.service" ];
      # Home Manager restarts sops-nix on activation; keep mounted files available.
      Wants = [ "sops-nix.service" ];
    };
    Install.WantedBy = [ "default.target" ];
    Service = {
      Type = "notify";
      ExecStartPre = "/usr/bin/mkdir -p %h/Documents/Nextcloud";
      ExecStart = "/usr/bin/rclone mount nextcloud: %h/Documents/Nextcloud --vfs-cache-mode writes --poll-interval 0";
      SuccessExitStatus = "143";
      Restart = "always";
      RestartSec = 10;
    };
  };
}
