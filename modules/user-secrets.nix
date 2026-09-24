# Shared SOPS keys and paths relative to ~/.config on NixOS and Arch.
{
  env_secrets = "environment.d/55-secrets.conf";
  stuff_config = "stuff/config.toml";
  context7_accounts = "context7-account-broker/accounts.json";
  gdrive_gcp = "google-drive-mcp/gcp-oauth.keys.json";
  gdrive_tokens = "google-drive-mcp/tokens.json";
}
