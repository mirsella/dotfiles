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

  sops.secrets.nextcloud-resend = {
    sopsFile = ../../secrets/nextcloud.yaml;
    key = "resend_api_key";
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
      mail_smtpmode = "smtp";
      mail_smtphost = "smtp.resend.com";
      mail_smtpport = 465;
      mail_smtpsecure = "ssl";
      mail_smtpauth = true;
      mail_smtpname = "resend";
      mail_from_address = "noreply";
      mail_domain = "voxride.com";
      enabledPreviewProviders = [
        "OC\\Preview\\BMP"
        "OC\\Preview\\GIF"
        "OC\\Preview\\JPEG"
        "OC\\Preview\\PNG"
        "OC\\Preview\\XBM"
        "OC\\Preview\\MP3"
        "OC\\Preview\\TXT"
        "OC\\Preview\\MarkDown"
        "OC\\Preview\\Movie"
      ];
    };
    extraApps = {
      inherit (config.services.nextcloud.package.packages.apps) memories;
    };
    secrets.mail_smtppassword = config.sops.secrets.nextcloud-resend.path;
  };

  services.nginx.virtualHosts."mirsella.mooo.com".listen = [
    {
      addr = "127.0.0.1";
      port = 8080;
    }
  ];
}
