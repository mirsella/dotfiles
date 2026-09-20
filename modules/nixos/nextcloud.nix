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
      onlyoffice = pkgs.fetchNextcloudApp {
        appName = "onlyoffice";
        appVersion = "9.9.0";
        sha256 = "Nj8siAeNyN/lyq+K5y92ZXMqRLmhawtR9Q6b5dq7LL0=";
        url = "https://github.com/ONLYOFFICE/onlyoffice-nextcloud/releases/download/v9.9.0/onlyoffice.tar.gz";
        license = "agpl3Plus";
      };
      integration_immich = pkgs.fetchNextcloudApp {
        appName = "integration_immich";
        appVersion = "1.5.1";
        sha256 = "BRHNg/hFmFfB21M57yZqVd99+cJswwT4Gxug5JditG0=";
        url = "https://github.com/xXRoxXeRXx/integration_immich/releases/download/v1.5.1/integration_immich.tar.gz";
        license = "agpl3Plus";
      };
    };
    # onlyoffice 9.9.0 caps max-version at 31, so the module's plain
    # app:enable would fail setup; both apps are enabled manually via occ.
    extraAppsEnable = false;
    secrets.mail_smtppassword = config.sops.secrets.nextcloud-resend.path;
  };

  services.nginx.virtualHosts."mirsella.mooo.com".listen = [
    {
      addr = "127.0.0.1";
      port = 8080;
    }
  ];
}
