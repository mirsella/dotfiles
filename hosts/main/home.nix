{ ... }:
{
  imports = [
    ../../modules/home/workstation.nix
    (import ../../modules/home/llama-server.nix {
      alias = "qwen3.6-27b-ablit";
      repo = "sci4ai/Qwen3.6-27B-Ablit-IQ4_XS-GGUF";
      revision = "fabccbe97a30bcb3013b7689776d7fc9425e4f4e";
      file = "Qwen3.6-27B-Ablit-IQ4_XS.gguf";
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
