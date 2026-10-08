{ pkgs, ... }:
{
  imports = [
    ../../modules/home/workstation.nix
    (import ../../modules/home/llama-server.nix {
      alias = "qwen3.6-27b-ablit";
      repo = "mradermacher/Huihui-Qwen3.6-27B-abliterated-i1-GGUF";
      revision = "e81c7c1dcdbaf798dc10f2333eb34d98f67a24a1";
      file = "Huihui-Qwen3.6-27B-abliterated.i1-IQ3_M.gguf";
      hash = "sha256-AwsTrS6P/XTQN9UgF9KTxl5mOmZji5RC6PGe+6CDaAw=";
      contextSize = 32768;
      gpuOnly = true;
      package = pkgs.llama-vulkan;
    })
  ];
  programs.git.settings.user.signingkey = "E53202A06B2614A4";

  xdg.configFile."systemd/user/xdg-desktop-portal.service.d/override.conf".text = ''
    [Service]
    MemoryMax=1G
    Restart=always
    RestartSec=1
  '';
}
