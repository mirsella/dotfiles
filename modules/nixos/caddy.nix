{ config, lib, ... }:
let
  index = builtins.readFile ./site/index.html;
  inlineHash = element:
    let content = builtins.match ".*<${element}>(.*)</${element}>.*" index;
    in assert lib.assertMsg (content != null) "Dashboard inline ${element} block is missing";
    "'sha256-${builtins.convertHash {
      hash = builtins.hashString "sha256" (builtins.head content);
      hashAlgo = "sha256";
      toHashFormat = "base64";
    }}'";
  serveIndex = ''
    import private_ui
    header Content-Security-Policy "default-src 'none'; img-src data:; style-src ${inlineHash "style"}; script-src ${inlineHash "script"}; base-uri 'none'; frame-ancestors 'none'; form-action 'none'"
    file_server {
      root ${./site}
    }
  '';
in
{
  sops.secrets.hermes_caddy_env = {
    sopsFile = ../../secrets/hermes.yaml;
    restartUnits = [ "caddy.service" ];
  };
  systemd.services.caddy.serviceConfig.EnvironmentFile = [ config.sops.secrets.hermes_caddy_env.path ];
  networking.firewall.allowedTCPPorts = [ 80 443 ];
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
      "photos.mirsella.mooo.com".extraConfig = ''
        reverse_proxy 127.0.0.1:2283
      '';
      "http://:80".extraConfig = ''
        redir https://mirsella.mooo.com{uri} 308
      '';
    };
  };
}
