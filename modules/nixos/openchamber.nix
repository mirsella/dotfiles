{ config, pkgs, ... }:
{
  systemd.services.openchamber = {
    description = "OpenChamber web server";
    documentation = [ "https://github.com/openchamber/openchamber" ];
    after = [ "sops-install-secrets.service" ];
    requires = [ "sops-install-secrets.service" ];
    wantedBy = [ "multi-user.target" ];
    path = [ pkgs.opencode ];
    environment.HOME = "/home/mirsella";
    serviceConfig = {
      Type = "simple";
      User = "mirsella";
      WorkingDirectory = "/home/mirsella";
      EnvironmentFile = config.sops.secrets.openchamber_server.path;
      ExecStart = "${pkgs.openchamber}/bin/openchamber serve --host 127.0.0.1 --port 14097 --foreground";
      Restart = "on-failure";
      RestartSec = "2s";
    };
  };
}
