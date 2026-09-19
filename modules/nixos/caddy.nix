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
        handle_path /nextcloud/* {
          reverse_proxy 127.0.0.1:8080
        }
        handle {
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
        reverse_proxy 127.0.0.1:14096
      '';
      "https://mirsella.mooo.com:14097".extraConfig = ''
        reverse_proxy 127.0.0.1:14097
      '';
      "http://predator, http://predator.local, http://localhost".extraConfig = ''
        root * ${./site}
        encode zstd gzip
        file_server
      '';
    };
  };
}
