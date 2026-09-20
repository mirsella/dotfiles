{ config, ... }:
{
  sops.secrets.onlyoffice-jwt = {
    sopsFile = ../../secrets/nextcloud.yaml;
    key = "onlyoffice_jwt";
    owner = "onlyoffice";
    restartUnits = [ "onlyoffice-docservice.service" ];
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
    port = 8001;
    allowLocalConnections = true;
    jwtSecretFile = config.sops.secrets.onlyoffice-jwt.path;
    securityNonceFile = config.sops.templates."onlyoffice-nonce.conf".path;
  };

  # module defaults the vhost to :80 (caddy owns it); docservice itself
  # sits on 8001, nginx fronts it on loopback 8000 for caddy :14098.
  services.nginx.virtualHosts."localhost".listen = [
    {
      addr = "127.0.0.1";
      port = 8000;
    }
  ];
}
