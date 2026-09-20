{ config, ... }:
let
  version = "v3.2.2";
in
{
  sops.secrets.immich_db_password = {
    sopsFile = ../../secrets/immich.yaml;
    key = "db_password";
  };
  sops.secrets.immich_jwt = {
    sopsFile = ../../secrets/immich.yaml;
    key = "jwt_secret";
  };
  sops.templates."immich.env".content = ''
    DB_PASSWORD=${config.sops.placeholder.immich_db_password}
    JWT_SECRET=${config.sops.placeholder.immich_jwt}
  '';

  virtualisation.oci-containers.backend = "podman";

  virtualisation.oci-containers.containers = {
    immich_postgres = {
      image = "ghcr.io/immich-app/postgres:14-vectorchord0.4.3-pgvectors0.2.0@sha256:bcf63357191b76a916ae5eb93464d65c07511da41e3bf7a8416db519b40b1c23";
      environment = {
        POSTGRES_USER = "immich";
        POSTGRES_DB = "immich";
        POSTGRES_INITDB_ARGS = "--data-checksums";
        TZ = "Europe/Paris";
      };
      environmentFiles = [ config.sops.templates."immich.env".path ];
      volumes = [ "/var/lib/immich-pg:/var/lib/postgresql/data" ];
      extraOptions = [ "--shm-size=128m" ];
    };
    immich_redis = {
      image = "docker.io/valkey/valkey:9@sha256:70739f85ad2ee01a726a965584a0f94895f01b0c60b3cc8b0aeef11eaa6888cf";
    };
    immich_server = {
      image = "ghcr.io/immich-app/immich-server:${version}";
      dependsOn = [ "immich_postgres" "immich_redis" ];
      ports = [ "127.0.0.1:2283:2283" ];
      environment = {
        IMMICH_VERSION = version;
        DB_HOSTNAME = "immich_postgres";
        DB_USERNAME = "immich";
        DB_DATABASE_NAME = "immich";
        REDIS_HOSTNAME = "immich_redis";
        TZ = "Europe/Paris";
      };
      environmentFiles = [ config.sops.templates."immich.env".path ];
      volumes = [
        "/srv/immich:/data"
        "/etc/localtime:/etc/localtime:ro"
      ];
    };
    immich_ml = {
      image = "ghcr.io/immich-app/immich-machine-learning:${version}";
      dependsOn = [ "immich_server" ];
      environmentFiles = [ config.sops.templates."immich.env".path ];
      volumes = [ "immich-model-cache:/cache" ];
    };
  };
}
