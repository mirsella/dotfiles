# Cargo OOM Protection

Chezmoi tracks `~/.local/share/cargo/bin/cargo` as
`dot_local/share/cargo/bin/executable_cargo`. The wrapper launches each Cargo
command in a dedicated transient systemd scope with `oom_score_adj=500`. This
lets the kernel prefer Cargo and its compiler/linker children as OOM victims,
while systemd-oomd can kill the complete build scope without killing the shell
or terminal. No hard memory limit is imposed.

Apply only the wrapper:

```bash
chezmoi apply ~/.local/share/cargo/bin/cargo
```

Enable the global swap safety net on a systemd-based Linux host:

```bash
sudo systemctl enable --now systemd-oomd.service
sudo systemctl set-property -- -.slice ManagedOOMSwap=kill
```

Verify the setup:

```bash
command -v cargo
systemctl is-active systemd-oomd.service
systemctl show -p ManagedOOMSwap -- -.slice
oomctl --no-pager
```

`command -v cargo` should report `~/.local/share/cargo/bin/cargo`. The default
systemd-oomd swap threshold is 90% of swap usage, not RAM usage.

Keep `~/.local/share/cargo/bin` before `/usr/bin` in `PATH`. Rustup toolchain
selection through `cargo +nightly`, `cargo +stable`, `rust-toolchain.toml`, and
directory overrides remains covered. `rustup run nightly cargo ...` bypasses
the wrapper and should be avoided when OOM protection is wanted.

Rio starts each new tab through `~/.local/bin/rio-shell-scope`. The wrapper
places Nushell and its descendants in a transient systemd scope with
`oom_score_adj=300`, while retaining the tab's working directory. This keeps a
large non-Cargo process or a Cargo invocation that bypasses the Cargo wrapper
from sharing Rio's application cgroup. Existing tabs retain their old cgroup
until they are closed and reopened.

`ManagedOOMPreference=avoid` on Rio's user unit does not protect it from this
root swap policy. For swap decisions, systemd-oomd ignores preferences on
user-owned cgroups, so the separate tab and Cargo cgroups provide the useful
protection boundaries.

To remove the global swap policy while retaining the kernel's normal OOM
killer and the Cargo score adjustment:

```bash
sudo systemctl set-property -- -.slice ManagedOOMSwap=auto
sudo systemctl disable --now systemd-oomd.service
```
