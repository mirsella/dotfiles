{ config, pkgs, ... }:
{
  systemd.services.storage-alerts = {
    description = "Notify Telegram of storage connection events";
    wants = [ "network-online.target" ];
    after = [ "network-online.target" "sops-install-secrets.service" ];
    path = [ config.systemd.package ];
    environment.PYTHONPATH = "${pkgs.writeTextDir "storage_events.py" (builtins.readFile ./storage_events.py)}";
    serviceConfig = {
      Type = "oneshot";
      ExecStart = "${pkgs.python3}/bin/python3 ${./storage-alerts.py}";
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
