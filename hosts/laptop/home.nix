{
  isNixOS,
  lib,
  pkgs,
  ...
}:
{
  imports = [
    ../../modules/home/workstation.nix
    (import ../../modules/home/llama-server.nix {
      alias = "qwen3.6-35b-a3b-abliterated-v4";
      repo = "Bahushruth/Qwen3.6-35B-A3B-abliterated-v4-GGUF";
      revision = "b8a9ab20c8bde880621a7bab65073c0078ef33f7";
      file = "Qwen3.6-35B-A3B-abliterated-v4-IQ3_M.gguf";
      hash = "sha256-cKtfQv8JUt7aKfyA8ze/Sz/7PguQbAofcY2EyYF1ldo=";
    })
  ];
  programs.git.settings.user.signingkey = "E88ECCA3AA187BC1";
  home.packages = [ pkgs.host-tools ];

  home.file.".local/share/arch-uki" = lib.mkIf (!isNixOS) {
    source = import ../../arch/laptop-boot.nix { inherit pkgs lib; };
  };

  systemd.user.services.ryzenadj-laptop = lib.mkIf (!isNixOS) {
    Unit.Description = "Apply RyzenAdj performance limits";
    Service = {
      Type = "simple";
      ExecStartPre = "/usr/bin/sudo -n /usr/bin/modprobe ryzen_smu";
      ExecStart = "${lib.getExe pkgs.host-tools} ryzenadj --watch";
      Environment = "PATH=/usr/bin:/bin";
      Restart = "on-failure";
      RestartSec = 5;
    };
    Install.WantedBy = [ "default.target" ];
  };

  systemd.user.services.lu-acton-2-a2dp-watch = {
    Unit = {
      Description = "Keep LU ACTON 2 on A2DP output";
      After = [
        "wireplumber.service"
        "pipewire.service"
        "pipewire-pulse.service"
      ];
      Wants = [
        "wireplumber.service"
        "pipewire.service"
        "pipewire-pulse.service"
      ];
    };
    Service = {
      Type = "simple";
      ExecStart = "${lib.getExe pkgs.host-tools} audio-watch";
      Environment = "PATH=${if isNixOS then lib.makeBinPath [ pkgs.pulseaudio ] else "/usr/bin:/bin"}";
      Restart = "always";
      RestartSec = 2;
    };
    Install.WantedBy = [ "default.target" ];
  };
}
