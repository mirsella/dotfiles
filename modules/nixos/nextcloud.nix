{ config, ... }:
{
  sops.secrets.nextcloud-adminpass = {
    sopsFile = ../../secrets/nextcloud.yaml;
    owner = "nextcloud";
    restartUnits = [
      "nextcloud-setup.service"
      "phpfpm-nextcloud.service"
    ];
  };

  services.nextcloud = {
    enable = true;
    hostName = "mirsella.mooo.com";
    https = true;
    database.createLocally = true;
    configureRedis = true;
    maxUploadSize = "16G";
    config = {
      dbtype = "pgsql";
      adminuser = "admin";
      adminpassFile = config.sops.secrets.nextcloud-adminpass.path;
    };
    settings = {
      trusted_proxies = [ "127.0.0.1" ];
      trusted_domains = [ "mirsella.mooo.com" ];
      overwritehost = "mirsella.mooo.com";
      overwriteprotocol = "https";
      overwritewebroot = "/nextcloud";
      "overwrite.cli.url" = "https://mirsella.mooo.com/nextcloud";
    };
    extraApps = {
      inherit (config.services.nextcloud.package.packages.apps) memories;
    };
  };

  services.nginx.virtualHosts."mirsella.mooo.com".listen = [
    {
      addr = "127.0.0.1";
      port = 8080;
    }
  ];
}
