{ config, ... }:
{
  sops.secrets.onlyoffice-jwt = {
    sopsFile = ../../secrets/nextcloud.yaml;
    key = "onlyoffice_jwt";
    owner = "onlyoffice";
    restartUnits = [ "onlyoffice-documentserver.service" ];
  };

  services.onlyoffice = {
    enable = true;
    port = 8000;
    allowLocalConnections = true;
    jwtSecretFile = config.sops.secrets.onlyoffice-jwt.path;
  };
}
