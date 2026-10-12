{ config, ... }:
let
  proxyMainUi = port: ''
    reverse_proxy https://192.168.1.131:${toString port} {
      header_up Host {hostport}
      transport http {
        tls_trust_pool file ${./main-ui-root.crt}
      }
    }
  '';
  serveIndex = ''
    import private_ui
    file_server {
      root ${./site}
    }
  '';
in
{
  services.caddy = {
    enable = true;
    extraConfig = ''
      (private_ui) {
        basic_auth {
          mirsella {$HERMES_VIEWER_PASSWORD_HASH}
        }
        header {
          Cache-Control no-store
          Referrer-Policy no-referrer
          X-Content-Type-Options nosniff
          X-Frame-Options DENY
        }
      }
    '';
    # The firewall only allows TCP/443, so do not advertise HTTP/3.
    globalConfig = ''
      servers {
        protocols h1 h2
      }
    '';
    virtualHosts = {
      "mirsella.mooo.com".extraConfig = ''
        # HSTS covers every port; main uses a private CA on 4096/4097.
        header >Strict-Transport-Security "max-age=0"
        encode zstd gzip
        redir /nextcloud /nextcloud/ 308
        @dav path /.well-known/carddav /.well-known/caldav
        redir @dav /nextcloud/remote.php/dav/ 301
        @discovery path /.well-known/webfinger /.well-known/nodeinfo
        redir @discovery /nextcloud/index.php{uri} 301
        handle_path /nextcloud/* {
          reverse_proxy 127.0.0.1:8080
        }
        redir /beszel /beszel/ 308
        handle_path /beszel/* {
          reverse_proxy 127.0.0.1:8090
        }
        handle {
          ${serveIndex}
        }
      '';
      "https://mirsella.mooo.com:4096".extraConfig = proxyMainUi 4096;
      "https://mirsella.mooo.com:4097".extraConfig = proxyMainUi 4097;
      "https://mirsella.mooo.com:14096".extraConfig = ''
        bind 192.168.1.19
        ${proxyMainUi 4096}
      '';
      "https://mirsella.mooo.com:14097".extraConfig = ''
        bind 192.168.1.19
        ${proxyMainUi 4097}
      '';
      "photos.mirsella.mooo.com".extraConfig = ''
        reverse_proxy 127.0.0.1:2283
      '';
      "http://:80".extraConfig = ''
        redir https://mirsella.mooo.com{uri} 308
      '';
    };
  };
  # DHCP may not have assigned the LAN bind address when Caddy first starts.
  systemd.services.caddy.serviceConfig = {
    EnvironmentFile = [ config.sops.secrets.hermes_caddy_env.path ];
    Restart = "on-failure";
    RestartSec = "5s";
  };
}
