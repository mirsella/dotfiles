{ config, lib, pkgs, ... }:
{
  imports = [ ./nextcloud-preferences.nix ];

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
    package = pkgs.nextcloud33;
    hostName = "mirsella.mooo.com";
    https = true;
    database.createLocally = true;
    configureRedis = true;
    maxUploadSize = "16G";
    phpOptions.memory_limit = lib.mkForce "512M";
    cli.memoryLimit = "1G";
    poolSettings = {
      "pm" = "dynamic";
      "pm.max_children" = 4;
      "pm.start_servers" = 1;
      "pm.min_spare_servers" = 1;
      "pm.max_spare_servers" = 2;
      "pm.max_requests" = 500;
    };
    config = {
      dbtype = "pgsql";
      adminuser = "admin";
      adminpassFile = config.sops.secrets.nextcloud-adminpass.path;
    };
    settings = {
      # UTC, so expensive jobs run after the 07:00 local wake-up year-round.
      maintenance_window_start = 6;
      default_phone_region = "FR";
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
      integration_immich = pkgs.fetchNextcloudApp {
        appName = "integration_immich";
        appVersion = "1.5.1";
        sha256 = "BRHNg/hFmFfB21M57yZqVd99+cJswwT4Gxug5JditG0=";
        url = "https://github.com/xXRoxXeRXx/integration_immich/releases/download/v1.5.1/integration_immich.tar.gz";
        license = "agpl3Plus";
      };
      checksum = pkgs.fetchNextcloudApp {
        appName = "checksum";
        appVersion = "2.1.2";
        sha256 = "6qPZvsml3LBYuuDnMwHg4WssxyQjr6op3AKlsMBLCGk=";
        url = "https://github.com/westberliner/checksum/releases/download/v2.1.2/checksum.tar.gz";
        license = "agpl3Plus";
      };
    };
    secrets.mail_smtppassword = config.sops.secrets.nextcloud-resend.path;
  };

  systemd.services.nextcloud-setup = {
    requires = [ "sops-install-secrets.service" ];
    after = [ "sops-install-secrets.service" ];
    unitConfig.RequiresMountsFor = [ "/var/lib/nextcloud/data" ];
    script = lib.mkAfter ''
      nextcloud-occ app:enable twofactor_totp suspicious_login
    '';
  };
  systemd.services.phpfpm-nextcloud.unitConfig.RequiresMountsFor = [ "/var/lib/nextcloud/data" ];
  systemd.services.nextcloud-cron.unitConfig.RequiresMountsFor = [ "/var/lib/nextcloud/data" ];

  services.nginx.virtualHosts."mirsella.mooo.com".listen = [
    {
      addr = "127.0.0.1";
      port = 8080;
    }
  ];
}
