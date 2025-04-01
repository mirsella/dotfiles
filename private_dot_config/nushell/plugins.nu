# plugins.nu - Minimal Nushell Plugin Manager

use ($env.XDG_DATA_HOME | path join "nushell" "nupm")

# Define plugin directory if not set already
let PLUGIN_DIR = ($env.XDG_DATA_HOME | path join "nushell" "plugins")

# Ensure plugin directory exists
mkdir $PLUGIN_DIR

# Simple function to download a plugin if it doesn't exist
def ensure-plugin [repo: string] {
  let name = ($repo | path basename | str replace ".git" "")
  let plugin_path = ($PLUGIN_DIR | path join $name)
  
  if not ($plugin_path | path exists) {
    print $"Downloading ($name)..."
    git clone $repo $plugin_path
  }
}

# Download essential plugins if they don't exist
ensure-plugin "https://github.com/nushell/nu_scripts.git"
ensure-plugin "https://github.com/fdncred/nu_plugin_plot.git"
# Add more plugins as needed

# Load commonly used plugins
