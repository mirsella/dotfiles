{ ... }:
{
  services.caddy = {
    enable = true;
    # The idle policy tracks TCP sessions; do not advertise blocked HTTP/3 ports.
    globalConfig = ''
      servers {
        protocols h1 h2
      }
    '';
    virtualHosts = {
      "mirsella.mooo.com".extraConfig = ''
        root * ${./site}
        encode zstd gzip
        redir /nextcloud /nextcloud/ 308
        @dav path /.well-known/carddav /.well-known/caldav
        redir @dav /nextcloud/remote.php/dav/ 301
        @discovery path /.well-known/webfinger /.well-known/nodeinfo
        redir @discovery /nextcloud/index.php{uri} 301
        handle_path /nextcloud/* {
          reverse_proxy 127.0.0.1:8080
        }
        handle {
          header {
            Referrer-Policy no-referrer
            X-Content-Type-Options nosniff
            X-Frame-Options DENY
          }
          file_server
        }
      '';
      "https://mirsella.mooo.com:4096".extraConfig = ''
        reverse_proxy 192.168.1.131:4096
      '';
      "https://mirsella.mooo.com:4097".extraConfig = ''
        reverse_proxy 192.168.1.131:4097
      '';
      "https://mirsella.mooo.com:14096".extraConfig = ''
        bind 192.168.1.19
        reverse_proxy 127.0.0.1:14096
      '';
      "https://mirsella.mooo.com:14097".extraConfig = ''
        bind 192.168.1.19
        reverse_proxy 127.0.0.1:14097
      '';
      "photos.mirsella.mooo.com".extraConfig = ''
        reverse_proxy 127.0.0.1:2283
      '';
      "http://:80".extraConfig = ''
        root * ${./site}
        encode zstd gzip
        file_server
      '';
    };
  };
}
