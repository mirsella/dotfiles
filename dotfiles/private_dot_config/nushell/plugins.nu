let $PLUGIN_DIR = $nu.data-dir | path join "plugins"
let nu_version = (version | get version)
let registered = (plugin list --registry | where status != "invalid" | get filename)

def ensure-plugin [repo: string] {
  let name = ($repo | path basename)
  let plugin_path = ($PLUGIN_DIR | path join $name)
  let binary = ($plugin_path | path join "target" "release" $name)
  let manifest = ($plugin_path | path join "Cargo.toml")

  if not ($plugin_path | path exists) {
    print $"Installing ($name)..."
    let clone = do { git clone $"https://github.com/($repo)" $plugin_path } | complete
    if $clone.exit_code != 0 {
      print --stderr $"($name): clone failed: ($clone.stderr | str trim)"
      return
    }
  }

  if not ($manifest | path exists) {
    print --stderr $"($name): missing Cargo.toml in ($plugin_path)"
    return
  }

  let dependency = (open $manifest | get dependencies | get nu-plugin)
  let plugin_version = if ($dependency | describe) == "string" { $dependency } else { $dependency.version }
  let version_changed = $plugin_version != $nu_version
  let needs_build = (not ($binary | path exists) or $version_changed)
  if $needs_build {
    print $"Building ($name)..."
    if $version_changed {
      # Never leave an old binary looking current after an interrupted upgrade.
      rm --permanent --force $binary
    }
    # Real cargo binary, not the rustup shim: the shim wraps every call in a
    # systemd-run transient scope, which fails on headless NixOS sessions.
    let cargo = (rustup which --toolchain nightly cargo | str trim)
    let rustc = (rustup which --toolchain nightly rustc | str trim)
    if $version_changed {
      # The plugin protocol must match the running Nushell, not upstream's pin.
      let align = do { ^$cargo add --manifest-path $manifest $"nu-plugin@($nu_version)" $"nu-protocol@($nu_version)" } | complete
      if $align.exit_code != 0 {
        print --stderr $"($name): dependency update failed: ($align.stderr | str trim)"
        return
      }
    }
    let build = do { with-env { RUSTC: $rustc } { ^$cargo build --release --manifest-path $manifest } } | complete
    if $build.exit_code != 0 {
      print --stderr $"($name): build failed: ($build.stderr | str trim)"
      return
    }
  }

  if ($needs_build or $binary not-in $registered) {
    try {
      plugin add $binary
    } catch {|err|
      print --stderr $"($name): register failed: ($err.msg)"
    }
  }
}

for repo in [
  "yybit/nu_plugin_compress"
  "FMotalleb/nu_plugin_clipboard"
  "FMotalleb/nu_plugin_image"
  "JosephTLyons/nu_plugin_units"
] {
  ensure-plugin $repo
}
