{ config, pkgs, ... }:
{
  systemd.services.storage-alerts = {
    description = "Notify Telegram of storage connection events";
    wants = [ "network-online.target" ];
    after = [ "network-online.target" "sops-install-secrets.service" ];
    path = [ config.systemd.package ];
    serviceConfig = {
      Type = "oneshot";
      ExecStart = "${pkgs.host-tools}/bin/host-tools storage-alerts";
      EnvironmentFile = config.sops.secrets.env_secrets.path;
      DynamicUser = true;
      SupplementaryGroups = [ "systemd-journal" ];
      StateDirectory = "storage-alerts";
      StateDirectoryMode = "0700";
      ProtectHome = true;
      # Bounds the streaming journal reader and Telegram request together.
      TimeoutStartSec = "90s";
    };
  };

  systemd.timers.storage-alerts = {
    description = "Check for USB storage errors every minute";
    wantedBy = [ "timers.target" ];
    timerConfig = {
      OnBootSec = "1min";
      OnUnitInactiveSec = "1min";
      AccuracySec = "5s";
    };
  };
}
