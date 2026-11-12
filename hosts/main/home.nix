{ ... }:
{
  imports = [
    ../../modules/home/workstation.nix
    ../../modules/home/predator-pm-study.nix
    (import ../../modules/home/llama-server.nix {
      alias = "qwen3.6-27b-ablit";
      repo = "mradermacher/Huihui-Qwen3.6-27B-abliterated-i1-GGUF";
      revision = "e81c7c1dcdbaf798dc10f2333eb34d98f67a24a1";
      file = "Huihui-Qwen3.6-27B-abliterated.i1-IQ3_M.gguf";
      hash = "sha256-AwsTrS6P/XTQN9UgF9KTxl5mOmZji5RC6PGe+6CDaAw=";
    })
  ];
  programs.git.settings.user.signingkey = "E53202A06B2614A4";

  desktop.plasma = {
    screen = {
      width = 1920;
      height = 1080;
    };
    cpuTemperature = {
      sensor = "lmsensors/zenpower-pci-00c3/temp1";
      label = "CPU die temperature";
    };
    rootDisk = "18dc205e-4518-49a0-b693-290f81634fef";
  };
  # The tower's power button shuts down; the laptop keeps the logout dialog.
  programs.plasma.configFile.powerdevilrc."AC/SuspendAndShutdown".PowerButtonAction = 8;

  xdg.configFile."systemd/user/xdg-desktop-portal.service.d/override.conf".text = ''
    [Service]
    MemoryMax=1G
    Restart=always
    RestartSec=1
  '';
}
