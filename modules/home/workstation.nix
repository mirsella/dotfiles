{ config, lib, ... }:
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

  home.activation.nextcloudDir =
    lib.hm.dag.entryAfter [ "writeBoundary" ] ''mkdir -p "$HOME/Nextcloud"'';

  systemd.user.services.rclone-nextcloud = {
    Unit.Description = "Nextcloud WebDAV mount";
    Install.WantedBy = [ "default.target" ];
    Service = {
      ExecStart = "/usr/bin/rclone mount nextcloud: %h/Nextcloud --vfs-cache-mode writes --dir-cache-time 5m --poll-interval 30s";
      ExecStop = "/usr/bin/fusermount -u %h/Nextcloud";
      Restart = "always";
      RestartSec = 10;
    };
  };
}
