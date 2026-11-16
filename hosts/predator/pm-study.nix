# Baseline held fixed for the suspend study in ./pm-study/README.md. Remove this
# import when the study ends.
{ lib, ... }:
{
  # Every trial so far ran without the Acer WMI driver and without the boot-time
  # EC write; loading either would be a new variable.
  hardware.predator-acer-wmi.enable = false;
  boot.blacklistedKernelModules = [
    "acer_wmi"
    "acer_wmi_ph31751"
  ];
  systemd.services.predator-kbd-backlight-off.enable = lib.mkForce false;
  # An unattended suspend after about 8 hours awake hangs until a forced reboot.
  systemd.timers.night-suspend.enable = lib.mkForce false;
  # Fixed address with magic-packet wake, independent of the router's lease.
  networking.networkmanager.ensureProfiles.profiles.predator-pm-study-lan = {
    connection = {
      id = "predator-pm-study-lan";
      uuid = "c4e6863b-7f76-4a4d-9a48-9937af0b5a36";
      type = "ethernet";
      interface-name = "enp3s0f1";
      autoconnect = true;
      autoconnect-priority = 100;
    };
    ethernet = {
      mac-address = "98:29:a6:3a:a7:50";
      wake-on-lan = 64;
    };
    ipv4 = {
      method = "manual";
      address1 = "192.168.1.19/24,192.168.1.1";
      dns = "192.168.1.1;";
    };
    ipv6.method = "auto";
  };
}
