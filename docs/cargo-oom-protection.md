# Cargo OOM Protection

Chezmoi tracks `~/.local/share/cargo/bin/cargo` as
`dot_local/share/cargo/bin/executable_cargo`. The wrapper launches each Cargo
command in a dedicated transient systemd scope with `oom_score_adj=500`. This
lets the kernel prefer Cargo and its compiler/linker children as OOM victims,
while systemd-oomd can kill the complete build scope without killing unrelated
applications. Each scope is a child of the dedicated `cargo.slice`; no hard
memory limit is imposed.

Chezmoi also tracks `~/.config/systemd/user/cargo.slice`. Its
`ManagedOOMSwap=kill` setting makes only Cargo scopes eligible for swap-based
systemd-oomd kills. The root slice uses `ManagedOOMSwap=auto`, so Rio, shells,
OpenCode, and other non-Cargo cgroups are not candidates under this policy.

Apply the setup:

```bash
chezmoi apply
```

The onchange script reloads the user manager, starts `cargo.slice`, enables
systemd-oomd, and removes the previous root-wide swap policy. To configure the
same policy manually:

```bash
systemctl --user daemon-reload
systemctl --user start cargo.slice
sudo systemctl enable --now systemd-oomd.service
sudo systemctl set-property -- -.slice ManagedOOMSwap=auto
```

Verify the setup:

```bash
command -v cargo
systemctl is-active systemd-oomd.service
systemctl --user show -p ManagedOOMSwap -p ControlGroup cargo.slice
systemctl show -p ManagedOOMSwap -- -.slice
oomctl --no-pager
```

`command -v cargo` should report `~/.local/share/cargo/bin/cargo`. The default
systemd-oomd swap action requires both memory and swap usage to exceed 90%.
`oomctl` should list `cargo.slice` as the swap-monitored cgroup and should not
list `/`.

Keep `~/.local/share/cargo/bin` before `/usr/bin` in `PATH`. Rustup toolchain
selection through `cargo +nightly`, `cargo +stable`, `rust-toolchain.toml`, and
directory overrides remains covered. `rustup run nightly cargo ...` bypasses
the wrapper and should be avoided when OOM protection is wanted.

If no wrapped Cargo command is running, systemd-oomd has no eligible cgroup to
kill under this policy. A non-Cargo memory runaway therefore falls back to the
kernel OOM killer. Explicit `/usr/bin/cargo`, `rustup run ... cargo`, containers,
and environments that replace `PATH` can also bypass the Cargo wrapper.
