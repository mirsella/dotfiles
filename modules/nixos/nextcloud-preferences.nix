{ lib, pkgs, ... }:
let
  mounts = [
    { name = "Fast"; path = "/srv/data/fast"; users = [ "mirsella" "admin" ]; }
    { name = "Archive"; path = "/srv/data/archive"; users = [ "mirsella" "admin" ]; }
    { name = "TankBackup"; path = "/srv/backup"; users = [ "admin" ]; }
  ];
  sharing = pkgs.writers.writeJSON "nextcloud-sharing.json" { apps.core = {
    shareapi_enabled = "yes";
    shareapi_allow_links = "yes";
    shareapi_allow_public_upload = "yes";
    shareapi_default_permissions = "1";
    shareapi_expire_after_n_days = null;
  }; };
in
{
  services.nextcloud.settings.quota_include_external_storage = true;

  systemd.services.nextcloud-setup.script = lib.mkAfter ''
    nextcloud-occ app:enable files_external twofactor_backupcodes
    nextcloud-occ config:import ${sharing}
    # config:import accepts strings/integers/null, not typed boolean app settings.
    for option in shareapi_enable_link_password_by_default shareapi_enforce_links_password \
      shareapi_default_expire_date shareapi_enforce_expire_date; do
      nextcloud-occ config:app:set core "$option" --type=boolean --value=false
    done

    mounts=$(nextcloud-occ files_external:list --output=json)
    ${lib.concatMapStringsSep "\n" (mount: ''
      mount_id=$(printf '%s' "$mounts" | ${pkgs.jq}/bin/jq -r \
        --arg name /${mount.name} --arg path ${lib.escapeShellArg mount.path} '
        [.[] | select(.mount_point == $name)] |
        if length == 0 then ""
        elif length == 1
          and .[0].storage == "\\OC\\Files\\Storage\\Local"
          and .[0].authentication_type == "null::null"
          and .[0].configuration.datadir == $path then .[0].mount_id
        else error("Unexpected external mount configuration for " + $name) end')
      if [ -z "$mount_id" ]; then
        nextcloud-occ files_external:create /${mount.name} local null::null \
          --config datadir=${lib.escapeShellArg mount.path} \
          ${lib.concatMapStringsSep " " (user: "--applicable-user=${lib.escapeShellArg user}") mount.users}
      fi
    '') mounts}

    # Nextcloud has no global sharing default for new external mounts.
    mount_ids=$(nextcloud-occ files_external:list --output=json | ${pkgs.jq}/bin/jq -r '.[].mount_id')
    for mount_id in $mount_ids; do
      nextcloud-occ files_external:option "$mount_id" enable_sharing true
    done
  '';
}
