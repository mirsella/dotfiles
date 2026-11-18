{ config, lib, pkgs, utils, ... }:
let
  photosOrigin = "https://ente.mirsella.mooo.com";
  apiOrigin = "https://ente-api.mirsella.mooo.com";
  storageOrigin = "https://ente-s3.mirsella.mooo.com";
  dataPath = "/srv/data/fast/Ente";
  bucket = "ente-photos";
  region = "us-east-1";
  museumConfig = "/run/ente_api/museum.yaml";
  secretPath = name: config.sops.secrets."ente_${name}".path;
  secret = name: { _secret = secretPath name; };
  museumSettings = {
    db = {
      host = "ente_postgres";
      port = 5432;
      name = "ente";
      user = "ente";
      password = secret "db_password";
    };
    s3 = {
      are_local_buckets = false;
      use_path_style_urls = true;
      "b2-eu-cen" = {
        key = "ente";
        secret = secret "s3_password";
        endpoint = storageOrigin;
        inherit bucket region;
      };
    };
    key = {
      encryption = secret "encryption_key";
      hash = secret "hash_key";
    };
    jwt.secret = secret "jwt_secret";
    smtp = {
      host = "smtp.resend.com";
      port = 465;
      username = "resend";
      password = secret "smtp_password";
      email = "noreply@voxride.com";
      sender-name = "Ente Photos (Predator)";
      encryption = "ssl";
    };
    internal = {
      admins = [ 1580559962386438 ];
      disable-registration = true;
    };
  };
  healthcheck = command: [
    "--health-cmd=${command}"
    "--health-interval=30s"
    "--health-start-period=120s"
    "--health-on-failure=kill"
  ];
  mountGuard = "${pkgs.host-tools}/bin/host-tools require-mount fast/data /srv/data/fast ${dataPath}";
in
{
  sops.secrets = lib.genAttrs
    (map (name: "ente_${name}") [ "db_password" "s3_password" "encryption_key" "hash_key" "jwt_secret" ])
    (name: {
      sopsFile = ../../secrets/ente.yaml;
      key = lib.removePrefix "ente_" name;
      restartUnits = [ "podman-ente_api.service" ]
        ++ lib.optional (name == "ente_db_password") "podman-ente_postgres.service"
        ++ lib.optional (name == "ente_s3_password") "podman-ente_s3.service";
    }) // {
    ente_smtp_password = {
      sopsFile = ../../secrets/nextcloud.yaml;
      key = "resend_api_key";
      restartUnits = [ "podman-ente_api.service" ];
    };
  };

  services.caddy.virtualHosts = {
    "ente.mirsella.mooo.com".extraConfig = "reverse_proxy 127.0.0.1:8300";
    "ente-api.mirsella.mooo.com".extraConfig = "reverse_proxy 127.0.0.1:8180";
    "ente-s3.mirsella.mooo.com".extraConfig = "reverse_proxy 127.0.0.1:8320";
  };

  systemd.services = {
    podman-ente_postgres = {
      requires = [ "sops-install-secrets.service" ];
      after = [ "sops-install-secrets.service" ];
      unitConfig.RequiresMountsFor = [ dataPath ];
      path = [ pkgs.util-linux ];
      serviceConfig = {
        RestartSec = "5s";
        ExecStartPre = lib.mkBefore [
          mountGuard
          "${pkgs.coreutils}/bin/install -d -m 0700 ${dataPath}"
          "${pkgs.coreutils}/bin/install -d -m 0700 -o 999 -g 999 ${dataPath}/postgres"
        ];
      };
    };
    podman-ente_s3 = {
      requires = [ "sops-install-secrets.service" ];
      after = [ "sops-install-secrets.service" ];
      unitConfig.RequiresMountsFor = [ dataPath ];
      path = [ pkgs.util-linux ];
      serviceConfig = {
        RestartSec = "5s";
        ExecStartPre = lib.mkBefore [
          mountGuard
          "${pkgs.coreutils}/bin/install -d -m 0700 ${dataPath} ${dataPath}/objects"
        ];
        ExecStartPost = "${pkgs.curl}/bin/curl --fail --silent --show-error --retry 30 --retry-connrefused --retry-delay 1 --max-time 5 http://127.0.0.1:8320/minio/health/ready";
      };
    };
    podman-ente_api = {
      requires = [ "sops-install-secrets.service" ];
      wants = [ "podman-ente_postgres.service" "podman-ente_s3.service" ];
      after = [ "sops-install-secrets.service" "podman-ente_postgres.service" "podman-ente_s3.service" ];
      # Put bucket creation in Museum's own start so a failed check is retried.
      preStart = ''
        s3Password=$(<${lib.escapeShellArg (secretPath "s3_password")})
        AWS_ACCESS_KEY_ID=ente AWS_SECRET_ACCESS_KEY="$s3Password" \
          ${lib.getExe pkgs.rclone} mkdir :s3:${bucket} --config=/dev/null \
          --s3-provider=Minio --s3-env-auth --s3-endpoint=http://127.0.0.1:8320 \
          --s3-region=${region} --retries=1 --low-level-retries=1 --contimeout=10s --timeout=30s

        # Serialize the actual secrets at runtime, escaping them as JSON strings.
        umask 077
        ${utils.genJqSecretsReplacementSnippet museumSettings "${museumConfig}.new"}
        ${pkgs.coreutils}/bin/mv -T ${museumConfig}.new ${museumConfig}
      '';
      serviceConfig = {
        RestartSec = "5s";
        RuntimeDirectoryMode = "0700";
        # The database backup can still read the last complete config if Museum stops.
        RuntimeDirectoryPreserve = "yes";
      };
    };
    podman-ente_web = {
      wants = [ "podman-ente_api.service" ];
      after = [ "podman-ente_api.service" ];
      serviceConfig.RestartSec = "5s";
    };
  };

  virtualisation.oci-containers = {
    backend = "podman";
    containers = {
      ente_postgres = {
        image = "docker.io/library/postgres:15@sha256:2663a9f0fde52de94f5dad3603a1942359f49472f1017c1a6d64f66bfedacb40";
        environment = {
          POSTGRES_USER = "ente";
          POSTGRES_DB = "ente";
          POSTGRES_PASSWORD_FILE = "/run/secrets/db_password";
          POSTGRES_INITDB_ARGS = "--data-checksums";
          TZ = config.time.timeZone;
        };
        volumes = [
          "${dataPath}/postgres:/var/lib/postgresql/data"
          "${config.sops.secrets.ente_db_password.path}:/run/secrets/db_password:ro"
        ];
        networks = [ "podman" ];
        podman.sdnotify = "healthy";
        extraOptions = healthcheck "pg_isready -q -h 127.0.0.1 -U ente -d ente"
          ++ [ "--shm-size=128m" ];
      };
      ente_s3 = {
        # Maintained MinIO distribution used by Ente's official self-host quickstart.
        image = "docker.io/pgsty/silo:latest@sha256:c20d112390b9a9ce8c11b8af95ad35bcaa62f4759663c09c936cd8d2b511f329";
        cmd = [ "server" "/data" "--address" ":3200" "--console-address" ":3201" ];
        environment = {
          MINIO_ROOT_USER = "ente";
          MINIO_ROOT_PASSWORD_FILE = "/run/secrets/s3_password";
          MINIO_BROWSER = "off";
          # The official desktop client uses a registered Chromium scheme.
          MINIO_API_CORS_ALLOW_ORIGIN = "${photosOrigin},ente://app";
        };
        volumes = [
          "${dataPath}/objects:/data"
          "${config.sops.secrets.ente_s3_password.path}:/run/secrets/s3_password:ro"
        ];
        ports = [ "127.0.0.1:8320:3200" ];
        networks = [ "podman" ];
      };
      ente_api = {
        image = "ghcr.io/ente/server:latest@sha256:f8c33e9bc1d5b7079dcda51522dd9f4fb28d93bef8655d95be54a978acc43408";
        volumes = [ "${museumConfig}:/museum.yaml:ro" ];
        ports = [ "127.0.0.1:8180:8080" ];
        networks = [ "podman" ];
        podman.sdnotify = "healthy";
        extraOptions = healthcheck "wget --quiet --tries=1 --spider http://127.0.0.1:8080/ping";
      };
      ente_web = {
        image = "ghcr.io/ente/web:latest@sha256:f135d345510d9f85545960543961a7b6cd0b78d20e004704a2a0165b7692f3ef";
        environment.ENTE_API_ORIGIN = apiOrigin;
        ports = [ "127.0.0.1:8300:3000" ];
        networks = [ "podman" ];
      };
    };
  };
}
