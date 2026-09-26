{ isNixOS, lib, pkgs, ... }:
let
  audioWatch = pkgs.writeShellApplication {
    name = "lu-acton-2-a2dp-watch";
    runtimeInputs = with pkgs; [ pulseaudio ripgrep systemd ];
    text = lib.removePrefix "#!/usr/bin/env bash\n"
      (builtins.readFile ../../dotfiles/dot_local/bin/executable_lu-acton-2-a2dp-watch);
  };
in
{
  imports = [
    ../../modules/home/workstation.nix
    (import ../../modules/home/llama-server.nix {
      alias = "qwen3.6-35b-a3b-abliterated-v4";
      repo = "Bahushruth/Qwen3.6-35B-A3B-abliterated-v4-GGUF";
      revision = "b8a9ab20c8bde880621a7bab65073c0078ef33f7";
      file = "Qwen3.6-35B-A3B-abliterated-v4-IQ3_M.gguf";
    })
  ];
  programs.git.settings.user.signingkey = "E88ECCA3AA187BC1";

  systemd.user.services.ryzenadj-laptop = lib.mkIf (!isNixOS) {
    Unit.Description = "Apply RyzenAdj performance limits";
    Service = {
      Type = "simple";
      ExecStartPre = "/usr/bin/sudo -n /usr/bin/modprobe ryzen_smu";
      ExecStart = "%h/.local/bin/ryzenadj-laptop --watch";
      Restart = "on-failure";
      RestartSec = 5;
    };
    Install.WantedBy = [ "default.target" ];
  };

  systemd.user.services.lu-acton-2-a2dp-watch = {
    Unit = {
      Description = "Keep LU ACTON 2 on A2DP output";
      After = [ "wireplumber.service" "pipewire.service" "pipewire-pulse.service" ];
      Wants = [ "wireplumber.service" "pipewire.service" "pipewire-pulse.service" ];
    };
    Service = {
      Type = "simple";
      ExecStart = if isNixOS then lib.getExe audioWatch else "%h/.local/bin/lu-acton-2-a2dp-watch";
      Restart = "always";
      RestartSec = 2;
    };
    Install.WantedBy = [ "default.target" ];
  };
}
