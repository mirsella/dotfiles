# plugins.nu - Minimal Nushell Plugin Manager

let $PLUGIN_DIR = $nu.data-dir | path join "plugins"

def ensure-plugin [repo: string] {
  let name = ($repo | path basename | str replace ".git" "")
  let url = $"https://github.com/($repo)"
  let plugin_path = ($PLUGIN_DIR | path join $name)
  
  if not ($plugin_path | path exists) {
    print $"Installing ($name)..."
    let clone = do { git clone $url $plugin_path } | complete
    if $clone.exit_code != 0 {
      print --stderr $"($name): clone failed, skipping"
      return
    }
    # Real cargo binary, not the rustup shim: the shim wraps every call in a
    # systemd-run transient scope, which fails on headless NixOS sessions.
    let cargo = (rustup which --toolchain nightly cargo | str trim)
    # Fresh clones pin whatever nu-plugin the author used; the plugin must
    # match the running nushell exactly, so align before building.
    let align = do { ^$cargo update --manifest-path $"($plugin_path)/Cargo.toml" -p nu-plugin -p nu-protocol } | complete
    if $align.exit_code != 0 {
      print --stderr $"($name): dep align failed, skipping"
      return
    }
    let build = do { ^$cargo build --release --manifest-path $"($plugin_path)/Cargo.toml" --locked } | complete
    let add = do { plugin add $"($plugin_path)/target/release/($name)" } | complete
    if $add.exit_code != 0 {
      print --stderr $"($name): register failed, skipping"
      return
    }
  }
}

ensure-plugin "yybit/nu_plugin_compress"
# ensure-plugin "fdncred/nu_plugin_file"
ensure-plugin "FMotalleb/nu_plugin_clipboard"
ensure-plugin "FMotalleb/nu_plugin_image"
ensure-plugin "JosephTLyons/nu_plugin_units"
