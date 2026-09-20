{ config, ... }:
{
  sops.secrets.onlyoffice-jwt = {
    sopsFile = ../../secrets/nextcloud.yaml;
    key = "onlyoffice_jwt";
    owner = "onlyoffice";
    restartUnits = [ "onlyoffice-documentserver.service" ];
  };

  sops.secrets.onlyoffice-nonce = {
    sopsFile = ../../secrets/nextcloud.yaml;
    key = "onlyoffice_nonce";
  };
  sops.templates."onlyoffice-nonce.conf" = {
    content = ''set $secure_link_secret "${config.sops.placeholder.onlyoffice-nonce}";'';
    owner = "root";
    group = "onlyoffice";
    mode = "0440";
  };

  services.onlyoffice = {
    enable = true;
    port = 8000;
    allowLocalConnections = true;
    jwtSecretFile = config.sops.secrets.onlyoffice-jwt.path;
    securityNonceFile = config.sops.templates."onlyoffice-nonce.conf".path;
  };
}
