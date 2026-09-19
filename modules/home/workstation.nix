{ config, ... }:
{
  targets.genericLinux.gpu.enable = false;

  sops = {
    age.keyFile = "/home/mirsella/.ssh/id_ed25519";
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
      };
  };
}
