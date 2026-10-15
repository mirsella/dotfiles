# Installed alongside T3 Code itself: common.nix imports this next to
# agent-tools.nix, which bootstraps T3, so every machine that gets T3 also
# gets the prompt-cache warmer binary, its user service, and the
# chezmoi-managed default config. The only per-machine manual step is
# `t3-cache-warmer --config ~/.config/t3-cache-warmer/config.json login`, and
# activation reminds you while no credential exists.
{
  isNixOS,
  lib,
  pkgs,
  ...
}:
let
  servicePath =
    "PATH=%h/.local/share/cargo/bin:%h/.local/bin:"
    + (
      if isNixOS then
        "%h/.nix-profile/bin:/etc/profiles/per-user/%u/bin:/run/current-system/sw/bin"
      else
        "%h/.nix-profile/bin:/usr/local/sbin:/usr/local/bin:/usr/bin"
    );
in
{
  home.file.".local/bin/t3-cache-warmer" = {
    source = lib.getExe pkgs.t3-cache-warmer;
    force = true;
  };

  # Sends a one-word keepalive to active T3 Code threads before their prompt
  # cache TTL expires. Reads ~/.config/t3-cache-warmer/config.json (chezmoi)
  # and keeps its credential in ~/.local/state/t3-cache-warmer (mode 600).
  systemd.user.services.t3-cache-warmer = {
    Unit = {
      Description = "T3 Code prompt-cache warmer";
      After = [ "network.target" ];
      PartOf = [ "default.target" ];
    };
    Service = {
      Type = "simple";
      Environment = servicePath;
      ExecStart = "${lib.getExe pkgs.t3-cache-warmer} --config %h/.config/t3-cache-warmer/config.json run";
      Restart = "always";
      RestartSec = "60s";
    };
    Install.WantedBy = [ "default.target" ];
  };

  home.activation.t3CacheWarmerLogin = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
    if [ ! -f "$HOME/.local/state/t3-cache-warmer/credential.json" ]; then
      echo "t3-cache-warmer has no credential yet: run 't3-cache-warmer --config ~/.config/t3-cache-warmer/config.json login', then 'systemctl --user start t3-cache-warmer'."
    fi
  '';
}
