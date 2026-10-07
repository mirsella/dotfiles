{ lib, pkgs, ... }:
{
  boot.loader.systemd-boot.enable = true;
  boot.loader.efi.canTouchEfiVariables = true;
  boot.initrd.systemd.enable = true;

  # LUKS2 containers unlock through the TPM once each host is enrolled.
  boot.initrd.luks.devices = {
    cryptroot = {
      device = "/dev/disk/by-partlabel/nixos-root";
      allowDiscards = true;
      crypttabExtraOpts = [ "tpm2-device=auto" ];
    };
    cryptswap = {
      device = "/dev/disk/by-partlabel/nixos-swap";
      allowDiscards = true;
      crypttabExtraOpts = [ "tpm2-device=auto" ];
    };
  };
  boot.resumeDevice = "/dev/mapper/cryptswap";

  boot.supportedFilesystems = [ "xfs" ];
  fileSystems."/" = {
    device = "/dev/mapper/cryptroot";
    fsType = "xfs";
  };
  swapDevices = [ { device = "/dev/mapper/cryptswap"; } ];

  boot.tmp.useTmpfs = true;
  boot.kernelParams = [ "zswap.enabled=0" "hibernate.compressor=lzo" ];
  zramSwap = {
    enable = true;
    algorithm = "zstd";
    memoryPercent = 100;
    priority = 100;
  };

  systemd.oomd = {
    settings.OOM.SwapUsedLimit = "95%";
    # Leave victim selection to the kernel instead of monitoring whole sessions.
    enableRootSlice = false;
    enableSystemSlice = false;
    enableUserSlices = false;
  };

  systemd.units."workstation-oom-protect.service" = {
    text = lib.replaceStrings
      [ "/usr/local/libexec/host-tools" ]
      [ "${pkgs.host-tools}/bin/host-tools" ]
      (builtins.readFile ../workstation-oom-protect.service);
    wantedBy = [ "multi-user.target" ];
  };

  services.desktopManager.plasma6.enable = true;
  services.displayManager.sddm = {
    enable = true;
    wayland.enable = true;
  };
  services.power-profiles-daemon.enable = true;
  services.printing.enable = true;
  services.fstrim.enable = true;
  services.xserver.xkb = {
    layout = "us";
    variant = "colemak_dh_iso";
    model = "pc105";
    options = "terminate:ctrl_alt_bksp";
  };
  console.useXkbConfig = true;
  services.pipewire = {
    enable = true;
    alsa.enable = true;
    alsa.support32Bit = true;
    pulse.enable = true;
  };
  security.rtkit.enable = true;
  hardware.graphics = {
    enable = true;
    enable32Bit = true;
  };
  hardware.bluetooth.enable = true;
  virtualisation.libvirtd.enable = true;

  programs.gnupg.agent.enable = true;
  environment.systemPackages = with pkgs; [
    wezterm
    nerd-fonts.jetbrains-mono
    nvtopPackages.amd
    chromium
    helium
    zen-browser
    pear-desktop
    android-studio
    keepassxc
    qemu_full
    virt-manager
    bottles
    wine
    gimp
    blender
    libreoffice
    obs-studio
    audacity
    mpv
    vlc
    transmission_4-qt
    kdePackages.filelight
    kdePackages.kcalc
    kdePackages.kdeconnect-kde
    kdePackages.kdenlive
    kdePackages.krdc
    kdePackages.partitionmanager
    krita
  ];
}
