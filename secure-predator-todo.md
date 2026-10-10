# Secure Predator TODO

Bind both SSDs' TPM unlock to approved boot images and their intended unlock
phases, with the OS PCR separator running before root unlock. This is the
canonical migration checklist; Secure Boot and LUKS encryption already work.

## Last observed state

Read-only checks on 2026-10-10 confirmed Secure Boot, signed Lanzaboote entries,
measured boot, an available TPM and both active LUKS2 mappings. Both TPM tokens
pinned static **PCRs 0, 2 and 7 in SHA-1**, without signed PCR 11 authorization.
The separator was absent from the running boot and is explicitly disabled in
[boot.nix](hosts/predator/boot.nix). Running and activated generations differed.
Systemd also skipped a low-priority `login` NV allocation because TPM NV space
was exhausted. Recheck this state before migration.

## Constraints throughout migration

- Keep working TPM tokens until both replacements have been verified. Never use
  `--wipe-slot=tpm2` during preparation; preserve passphrase recovery slots.
- Keep the separator disabled in ordinary deployments until both SSDs have
  compatible enrollments. Activation does not enroll credentials or fetch passwords.
- Enroll from a TPM-enabled boot, coordinated with the power-management study.
  The `pm-study` specialisation disables the TPM driver.
- Root unlocks in initrd; data disks unlock through bounded, optional host units.
  Preserve `nofail` mounts, degraded RAIDZ1 imports and mount-gated writers so
  missing data disks leave OS/SSH startup available.
- Keep private signing keys and header backups root-only on encrypted storage,
  outside Git and the Nix store. Keep passwords out of arguments, logs and files.
- Inspect EFI images with read-only parsers or disposable copies.
  `objcopy --dump-section` rewrites its input unless given a separate output image
  and can strip its signature. Compare hashes before/after inspection; verify
  actual signatures and certificate-table bounds.

## 1. Inventory and recovery preparation

- [ ] Review Predator's checkout and `flake.lock`, preserving concurrent work.
  Review stashed invariants against the actual checkout before applying them.
- [ ] Confirm TPM availability and automatic unlocking of `crypt-root` and
  `fast-crypt`. Record running/activated generations, next boot entry, systemd/
  Lanzaboote versions, Secure Boot state and signed artifacts.
- [ ] Resolve both devices through [disks.nix](hosts/predator/disks.nix) and
  crypttab. Record token/keyslot associations, recovery slots, PCR banks,
  public-key policies and TPM algorithms without exposing secrets.
- [ ] Back up both headers, the Secure Boot signing bundle and existing policy
  keys. Verify usable passphrase recovery for both disks and reboot recovery access.
- [ ] Identify NV allocations and whether the new policy needs more space.
  Do not clear the TPM or delete unknown indices to resolve the warning.

## 2. Design the signed-image and phase policies

The separator creates a firmware-to-OS boundary; image and phase authorization
need their own policy. Laptop's ukify configuration is not a Lanzaboote integration.

- [ ] Use Lanzaboote's measurements and the event log to define authorization
  for the authenticated kernel, initramfs and command line. Ensure the pinned
  systemd/Lanzaboote integration supports the required signed policy, reference
  and signature delivery; implement or update it in Nix before enrollment.
- [ ] Verify SHA-256 measurements for Secure Boot state and signed PCR 11.
  Decide whether static PCRs 0/2 are needed and document firmware-update behavior.
  Validate the signing key against Predator's TPM algorithms.
- [ ] Define root's early-initrd authorization and a separate early-host policy
  for `fast-crypt`. Specify each unlock's ordering and the phase transition
  that ends its authorization. Reject broader phase signatures.
- [ ] Deliver signatures to both cryptsetup paths and generate them for routine
  boot-image updates without re-enrollment. Preserve an approved recovery generation.

## 3. Implement and build the target generation

- [ ] Implement enrollment/prediction in Rust under `pkgs/host-tools`, compiled
  by Nix with pinned dependencies. Predict the selected post-separator PCRs using
  the target systemd's event encoding, bank and measurement sequence, matching
  its PCR list and `--event-type=os-separator os-separator` invocation.
- [ ] Prepare the target separator configuration in
  [boot.nix](hosts/predator/boot.nix), preserving `ConditionSecurity=measured-os`,
  ordering before cryptsetup, its initrd executable and TPM-disabled exclusion.
- [ ] Update [tests/predator.nix](tests/predator.nix) for the enabled separator's
  ordering/policy invariants, retaining storage and missing-disk invariants.
  Add focused Rust tests for prediction, policy validation, interrupted/resumed
  enrollment and preservation of working tokens/recovery slots.
- [ ] Run those tests and `nix flake check path:. --no-build --no-update-lock-file`,
  resolving evaluation blockers. Build the target system/initrd without activation.
- [ ] Generate and validate phase signatures for that generation's kernel,
  initramfs and command line. Reject missing or mismatched authorization. Freeze
  these inputs through enrollment; changed inputs require revalidation.

## 4. Enroll both SSDs before deployment

- [ ] Authorize enrollment with existing TPM credentials where possible. If it
  requires a passphrase, use `pass-cli` in the owner's normal session and pipe to
  `systemd-cryptenroll --unlock-key-file=/dev/stdin`; verify this handoff before reboot.
- [ ] Add each disk's new token alongside its working token, using the predicted
  post-separator values and that disk's image/phase policy from the target generation.
- [ ] Validate both disks' token/keyslot associations, banks, public keys and
  references against the intended policies; confirm recovery slots remain usable.

## 5. Deploy and verify the first boot

- [ ] Sync reviewed changes and run a detached system rebuild on Predator:
  `sudo nixos-rebuild switch --flake path:/home/mirsella/dev/dotfiles#predator`.
  Check its log/exit status and correspondence to the tested generation.
- [ ] Verify installed EFI signatures, phase signatures and the next default
  entry after activation. Keep recovery available and coordinate the reboot with
  the PM study. Successful activation is not unlock verification.
- [ ] Reboot with recovery access/passwords available. Confirm firmware acceptance,
  separator/phase ordering, both automatic unlocks without passphrase fallback,
  and normal pools, mounts and dependent services. Verify inspection preserved hashes.
- [ ] Test each new token's rejection in unauthorized later phases using
  nonactivating, token-specific checks. Do not close mappings, expose key material
  or let a legacy token mask the result. Confirm the successful boot used each
  replacement token before retiring legacy authorization.

## 6. Retire legacy authorization and prove updates work

- [ ] Remove only the verified legacy tokens and their associated keyslots.
  Inspect final metadata for remaining weak authorization and intact recovery.
  Reboot without legacy tokens and confirm both SSDs unlock automatically.
- [ ] Verify the signature/update path also works for a subsequent generated
  boot image, without weakening phase restrictions or requiring re-enrollment.
- [ ] Update README/agent guidance to describe the final boot and recovery flow.
  Retire one-time migration tooling and this TODO once the transition is complete,
  retaining the ongoing signing and validation integration. Keep deployment
  evidence in Git history rather than permanent acceptance reports.
