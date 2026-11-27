{ ... }:
{
  imports = [
    ../../modules/home/workstation.nix
    (import ../../modules/home/llama-server.nix {
      quant = "IQ3_S";
      hash = "sha256-I6jmeSSBtvHQ661ROZ6e4qLCvoUlsJ7Rrmc3gRFZ+JI=";
      contextSize = 32768;
      # Leave room for the larger KV cache on the 16 GiB GPU.
      microBatchSize = 256;
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
    peerLoadHost = "laptop";
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
