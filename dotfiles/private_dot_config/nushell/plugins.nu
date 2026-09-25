def nu-plugins-update [] {
  let plugin_dir = ($nu.data-dir | path join "plugins")
  let nu_version = (version | get version)
  # Bypass the rustup shim, which starts a transient systemd scope for each call.
  let cargo = (rustup which --toolchain nightly cargo | str trim)
  let rustc = (rustup which --toolchain nightly rustc | str trim)

  for repo in [
    "yybit/nu_plugin_compress"
    "FMotalleb/nu_plugin_clipboard"
    "FMotalleb/nu_plugin_image"
    "JosephTLyons/nu_plugin_units"
  ] {
    let name = ($repo | path basename)
    let plugin_path = ($plugin_dir | path join $name)
    let manifest = ($plugin_path | path join "Cargo.toml")
    let binary = ($plugin_path | path join "target" "release" $name)

    if not ($plugin_path | path exists) {
      print $"Cloning ($name)..."
      let clone = do { git clone $"https://github.com/($repo)" $plugin_path } | complete
      if $clone.exit_code != 0 {
        error make { msg: $"($name): clone failed: ($clone.stderr | str trim)" }
      }
    }

    if not ($manifest | path exists) {
      error make { msg: $"($name): missing Cargo.toml in ($plugin_path)" }
    }

    let align = do { ^$cargo add --manifest-path $manifest $"nu-plugin@($nu_version)" $"nu-protocol@($nu_version)" } | complete
    if $align.exit_code != 0 {
      error make { msg: $"($name): dependency update failed: ($align.stderr | str trim)" }
    }

    print $"Building ($name)..."
    let build = do { with-env { RUSTC: $rustc } { ^$cargo build --release --manifest-path $manifest } } | complete
    if $build.exit_code != 0 {
      error make { msg: $"($name): build failed: ($build.stderr | str trim)" }
    }

    try {
      plugin add $binary
    } catch {|err|
      error make { msg: $"($name): register failed: ($err.msg)" }
    }
  }
}
