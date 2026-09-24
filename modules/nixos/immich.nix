{ config, pkgs, ... }:
let
  version = "v3.2.2";
  mediaPath = "/srv/data/fast/Photos";
  passwordMount = "${config.sops.secrets.immich_db_password.path}:/run/secrets/db_password:ro";
  configFile = (pkgs.formats.json { }).generate "immich.json" {
    server.externalDomain = "https://photos.mirsella.mooo.com";
    machineLearning.urls = [ "http://immich_ml:3003" ];
    backup.database.enabled = false;
  };
  healthcheckOptions = command: [
    "--health-cmd=${command}"
    "--health-interval=30s"
    "--health-start-period=120s"
    "--health-on-failure=kill"
  ];
in
{
  sops.secrets.immich_db_password = {
    sopsFile = ../../secrets/immich.yaml;
    key = "db_password";
    restartUnits = [ "podman-immich_postgres.service" "podman-immich_server.service" ];
  };

  virtualisation.oci-containers.backend = "podman";

  systemd.tmpfiles.rules = [
    "d /var/lib/immich-pg 0700 999 999 -"
  ];

  systemd.services = {
    podman-immich_postgres = {
      requires = [ "sops-install-secrets.service" ];
      after = [ "sops-install-secrets.service" ];
      serviceConfig.RestartSec = "5s";
    };
    podman-immich_server = {
      requires = [ "sops-install-secrets.service" "zfs-mount.service" ];
      after = [ "sops-install-secrets.service" "zfs-mount.service" ];
      unitConfig.RequiresMountsFor = [ mediaPath ];
      serviceConfig.RestartSec = "5s";
      path = [ pkgs.util-linux ];
      preStart = ''
        # Native ZFS mounts are not declared in fstab. Refuse the bare root filesystem.
        if ! findmnt --source fast/data --mountpoint /srv/data/fast >/dev/null; then
          echo "Immich requires fast/data mounted at /srv/data/fast" >&2
          exit 1
        fi
        mkdir -p ${mediaPath}
      '';
    };
    podman-immich_redis.serviceConfig.RestartSec = "5s";
    podman-immich_ml.serviceConfig.RestartSec = "5s";
  };

  virtualisation.oci-containers.containers = {
    immich_postgres = {
      image = "ghcr.io/immich-app/postgres:14-vectorchord0.4.3-pgvectors0.2.0@sha256:bcf63357191b76a916ae5eb93464d65c07511da41e3bf7a8416db519b40b1c23";
      environment = {
        POSTGRES_USER = "immich";
        POSTGRES_DB = "immich";
        POSTGRES_PASSWORD_FILE = "/run/secrets/db_password";
        POSTGRES_INITDB_ARGS = "--data-checksums";
        TZ = config.time.timeZone;
      };
      volumes = [ "/var/lib/immich-pg:/var/lib/postgresql/data" passwordMount ];
      networks = [ "podman" ];
      podman.sdnotify = "healthy";
      extraOptions = healthcheckOptions "pg_isready -h 127.0.0.1 -U immich -d immich"
        ++ [ "--shm-size=128m" ];
    };
    immich_redis = {
      image = "docker.io/valkey/valkey:9@sha256:70739f85ad2ee01a726a965584a0f94895f01b0c60b3cc8b0aeef11eaa6888cf";
      networks = [ "podman" ];
      podman.sdnotify = "healthy";
      extraOptions = healthcheckOptions "redis-cli ping | grep -q PONG";
    };
    immich_server = {
      image = "ghcr.io/immich-app/immich-server:${version}@sha256:6b0eaf8cfea6d4d6f86a45dfd0c9e3b068ed4493d8b7fa1efeca4cc87792fbe8";
      dependsOn = [ "immich_postgres" "immich_redis" ];
      ports = [ "127.0.0.1:2283:2283" ];
      environment = {
        IMMICH_CONFIG_FILE = "/etc/immich.json";
        DB_HOSTNAME = "immich_postgres";
        DB_USERNAME = "immich";
        DB_DATABASE_NAME = "immich";
        DB_PASSWORD_FILE = "/run/secrets/db_password";
        REDIS_HOSTNAME = "immich_redis";
        TZ = config.time.timeZone;
      };
      volumes = [
        "${mediaPath}:/data"
        "${configFile}:/etc/immich.json:ro"
        passwordMount
      ];
      networks = [ "podman" ];
      podman.sdnotify = "healthy";
      extraOptions = healthcheckOptions "immich-healthcheck";
    };
    immich_ml = {
      image = "ghcr.io/immich-app/immich-machine-learning:${version}@sha256:390453a571ca73b563cc3d9a12a37a4d71217b12608de72f26884f5e2ebc4896";
      volumes = [ "immich-model-cache:/cache" ];
      networks = [ "podman" ];
      podman.sdnotify = "healthy";
      extraOptions = healthcheckOptions "python3 healthcheck.py";
    };
  };
}
