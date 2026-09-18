{ ... }:
{
  services.caddy = {
    enable = true;
    virtualHosts = {
      "mirsella.mooo.com".extraConfig = ''
        root * ${./site}
        encode zstd gzip
        header {
          Referrer-Policy no-referrer
          X-Content-Type-Options nosniff
          X-Frame-Options DENY
        }
        handle_path /opencode-predator/* {
          reverse_proxy 127.0.0.1:14096
        }
        handle_path /openchamber-predator/* {
          reverse_proxy 127.0.0.1:14097
        }
        handle_path /opencode-main/* {
          reverse_proxy 192.168.1.131:4096 {
            transport http {
              tls
              tls_server_name mirsella.mooo.com
            }
            header_up Host mirsella.mooo.com
          }
        }
        handle_path /openchamber-main/* {
          reverse_proxy 192.168.1.131:4097 {
            transport http {
              tls
              tls_server_name mirsella.mooo.com
            }
            header_up Host mirsella.mooo.com
          }
        }
        handle {
          file_server
        }
      '';
      "http://predator, http://predator.local, http://localhost".extraConfig = ''
        root * ${./site}
        encode zstd gzip
        file_server
      '';
      "cloud.mirsella.mooo.com".extraConfig = ''
        reverse_proxy 127.0.0.1:8080
      '';
    };
  };
}
