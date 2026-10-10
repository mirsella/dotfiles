{ ... }:
let
  serveIndex = ''
    header {
      Referrer-Policy no-referrer
      X-Content-Type-Options nosniff
      X-Frame-Options DENY
    }
    file_server {
      root ${./site}
    }
  '';
in
{
  networking.firewall.allowedTCPPorts = [ 80 443 ];
  services.caddy = {
    enable = true;
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
      "photos.mirsella.mooo.com".extraConfig = ''
        reverse_proxy 127.0.0.1:2283
      '';
      "http://:80".extraConfig = ''
        encode zstd gzip
        ${serveIndex}
      '';
    };
  };
}
