{ pkgs, lib }:
let
  rootUuid = "b05803cd-53dd-4796-9be2-40242e4d30bc";
  files = {
    "etc/kernel/cmdline" = ''
      rd.luks.name=${rootUuid}=root rd.luks.options=${rootUuid}=tpm2-device=auto root=/dev/mapper/root rootflags=subvol=@ rw rootfstype=btrfs zswap.enabled=0 resume=/dev/mapper/root resume_offset=57190605
    '';
    "etc/kernel/uki.conf" = ''
      [UKI]
      SecureBootPrivateKey=/var/lib/sbctl/keys/db/db.key
      SecureBootCertificate=/var/lib/sbctl/keys/db/db.pem
      SecureBootSigningTool=sbsign
      SignKernel=no
      PCRBanks=sha256
      PCRPKey=/etc/kernel/tpm2-pcr-public-key.pem

      [PCRSignature:initrd]
      PCRPrivateKey=/etc/kernel/tpm2-pcr-private-key.pem
      PCRPublicKey=/etc/kernel/tpm2-pcr-public-key.pem
      Phases=enter-initrd
      PolicyRef=initrd
    '';
    "boot/loader/loader.conf" = ''
      default arch-linux-cachyos.efi
      timeout 5
      editor no
    '';
  } // lib.genAttrs
    (map (kernel: "etc/mkinitcpio.d/${kernel}.preset") [ "linux" "linux-cachyos" ])
    (path:
      let
        kernel = lib.removeSuffix ".preset" (builtins.baseNameOf path);
      in
      ''
        # Managed by the laptop Home Manager profile and host-tools arch-uki.
        ALL_kver="/boot/vmlinuz-${kernel}"
        ALL_cmdline="/etc/kernel/cmdline"
        PRESETS=('default')
        default_uki="/boot/EFI/Linux/arch-${kernel}.efi"
        default_options="-t /var/lib/arch-uki/build"
      ''
    );
in
pkgs.runCommand "laptop-boot-config" { } (
  lib.concatStringsSep "\n" (
    lib.mapAttrsToList (path: text: ''
      install -Dm644 ${pkgs.writeText (builtins.baseNameOf path) text} "$out/${path}"
    '') files
  )
)
