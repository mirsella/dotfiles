{ config, isNixOS, lib, pkgs, ... }:
let
  bin = package: name: if isNixOS then lib.getExe' package name else "/usr/bin/${name}";
  servicePath = "PATH=%h/.local/share/cargo/bin:%h/.local/bin:" + (
    if isNixOS then "%h/.nix-profile/bin:/etc/profiles/per-user/%u/bin:/run/current-system/sw/bin"
    else "/usr/local/sbin:/usr/local/bin:/usr/bin"
  );
in
{
  programs.git.settings.commit.gpgsign = true;
  home.packages = lib.optionals isNixOS [ pkgs.opencode pkgs.openchamber ];

  systemd.user.slices.cargo = {
    Unit.Description = "Cargo build scopes";
    Slice.ManagedOOMSwap = "kill";
  };

  xdg.configFile."systemd/user/app-org.wezfurlong.wezterm@.service.d/override.conf".text = ''
    [Service]
    OOMPolicy=continue
  '';

  sops = {
    age.sshKeyPaths = [ "${config.home.homeDirectory}/.ssh/id_ed25519" ];
    defaultSopsFile = ../../secrets/services.yaml;
    secrets = builtins.mapAttrs (_: path: {
      path = "${config.home.homeDirectory}/.config/${path}";
    }) ((import ../user-secrets.nix) // {
      telegram_env = "telegram.env";
      opencode_server = "opencode/server.env";
      openchamber_server = "openchamber/server.env";
    }) // {
      davfs2_secrets = {
        sopsFile = ../../secrets/webdav.yaml;
        path = "${config.home.homeDirectory}/.davfs2/secrets";
        mode = "0600";
      };
      rclone_conf = {
        sopsFile = ../../secrets/webdav.yaml;
        path = "${config.home.homeDirectory}/.config/rclone/rclone.conf";
        mode = "0600";
      };
    };
  };

  systemd.user.services.opencode = {
    Unit = {
      Description = "OpenCode server";
      After = [ "network.target" "sops-nix.service" ];
      PartOf = [ "default.target" ];
      X-SwitchMethod = "keep-old";
    };
    Service = {
      Type = "simple";
      WorkingDirectory = "%h";
      EnvironmentFile = [
        config.sops.secrets.telegram_env.path
        config.sops.secrets.opencode_server.path
      ];
      Environment = servicePath;
      ExecStart = "${bin pkgs.opencode "opencode"} serve --hostname 127.0.0.1 --port 14096";
      Restart = "on-failure";
      RestartSec = "2s";
    };
    Install.WantedBy = [ "default.target" ];
  };

  systemd.user.services.openchamber = {
    Unit = {
      Description = "OpenChamber web server";
      After = [ "network.target" "opencode.service" "sops-nix.service" ];
      Wants = [ "opencode.service" "sops-nix.service" ];
      PartOf = [ "default.target" ];
    };
    Service = {
      Type = "simple";
      WorkingDirectory = "%h";
      EnvironmentFile = [
        config.sops.secrets.opencode_server.path
        config.sops.secrets.openchamber_server.path
      ];
      Environment = servicePath;
      ExecStart = "${bin pkgs.openchamber "openchamber"} serve --host 127.0.0.1 --port 14097 --foreground";
      Restart = "on-failure";
      RestartSec = "2s";
    };
    Install.WantedBy = [ "default.target" ];
  };

  systemd.user.services.rclone-nextcloud = let
    mountPoint = "%h/Documents/Nextcloud";
  in {
    Unit = {
      Description = "Nextcloud WebDAV mount";
      After = [ "sops-nix.service" ];
      # Home Manager restarts sops-nix on activation; keep mounted files available.
      Wants = [ "sops-nix.service" ];
    };
    Install.WantedBy = [ "default.target" ];
    Service = {
      Type = "notify";
      ExecStartPre = "${bin pkgs.coreutils "mkdir"} -p ${mountPoint}";
      ExecStart = "${bin pkgs.rclone "rclone"} mount nextcloud: ${mountPoint} --vfs-cache-mode writes --poll-interval 0";
      SuccessExitStatus = "143";
      Restart = "always";
      RestartSec = 10;
    };
  };

  systemd.user.services.kache = {
    Unit.Description = "kache build cache daemon";
    Service = {
      Type = "simple";
      ExecStart = (if isNixOS then lib.getExe' pkgs.kache "kache" else "%h/.local/share/cargo/bin/kache") + " daemon run";
      Restart = "on-failure";
      RestartSec = "5s";
      Environment = "KACHE_LOG=kache=info";
    };
    Install.WantedBy = [ "default.target" ];
  };
}
