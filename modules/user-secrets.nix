# Shared SOPS keys and paths relative to ~/.config on NixOS and Arch.
{
  env_secrets = "environment.d/55-secrets.conf";
  stuff_config = "stuff/config.toml";
  context7_accounts = "context7-account-broker/accounts.json";
  atuin_key = "atuin/key";
}
