# XDG Base Directory specification
$env.GTK_USE_PORTAL = 1
$env.XDG_CONFIG_HOME = $"($env.HOME)/.config"
$env.XDG_CACHE_HOME = $"($env.HOME)/.cache"
$env.XDG_DATA_HOME = $"($env.HOME)/.local/share"
$env.XDG_STATE_HOME = $"($env.HOME)/.local/state"

$env.PATH ++= [
  $"($env.HOME)/.local/bin"
  $"($env.HOME)/.local/share/gem/bin"
  $"($env.HOME)/.local/share/npm/bin"
  $"($env.HOME)/.local/share/cargo/bin"
  $env.PNPM_HOME
  "/opt/osxcross/target/bin"
]

$env.EDITOR = "nvim"
if ((which zen-browser | length) > 0) {
    $env.BROWSER = "zen-browser"
} else if ((which zen-browser | length) > 0) {
    $env.BROWSER = "firefox"
}

# Locale settings
$env.LANG = "en_US.UTF-8"
$env.LANGUAGE = "en_US.UTF-8"
$env.LC_ADDRESS = "fr_FR.UTF-8"
$env.LC_COLLATE = "fr_FR.UTF-8"
$env.LC_CTYPE = "fr_FR.UTF-8"
$env.LC_IDENTIFICATION = "fr_FR.UTF-8"
$env.LC_MEASUREMENT = "fr_FR.UTF-8"
$env.LC_MESSAGES = "fr_FR.UTF-8"
$env.LC_MONETARY = "fr_FR.UTF-8"
$env.LC_NAME = "fr_FR.UTF-8"
$env.LC_NUMERIC = "fr_FR.UTF-8"
$env.LC_PAPER = "fr_FR.UTF-8"
$env.LC_TELEPHONE = "fr_FR.UTF-8"
$env.LC_TIME = "fr_FR.UTF-8"

# Application-specific directories
$env.GRADLE_USER_HOME = $"($env.XDG_DATA_HOME)/gradle"
$env.CARGO_HOME = $"($env.XDG_DATA_HOME)/cargo"
$env.RUSTUP_HOME = $"($env.XDG_DATA_HOME)/rustup"
$env.GOPATH = $"($env.XDG_DATA_HOME)/go"
$env.KDEHOME = $"($env.XDG_CONFIG_HOME)/kde"
$env.SCREENRC = $"($env.XDG_CONFIG_HOME)/screen/screenrc"
$env.GTK_RC_FILES = $"($env.XDG_CONFIG_HOME)/gtkrc-1.0"
$env.GTK2_RC_FILES = $"($env.XDG_CONFIG_HOME)/gtkrc-2.0"
$env.WGETRC = $"($env.XDG_CONFIG_HOME)/wget/wgetrc"
$env.LESSHISTFILE = "-"
$env.INPUTRC = $"($env.XDG_CONFIG_HOME)/inputrc"
$env.GNUPGHOME = $"($env.XDG_DATA_HOME)/gnupg"
$env.WINEPREFIX = $"($env.XDG_DATA_HOME)/wineprefixes/wine64"
$env.PASSWORD_STORE_DIR = $"($env.XDG_DATA_HOME)/password-store"
$env.ANDROID_HOME = "/opt/android-sdk"
$env.ANDROID_AVD_HOME = $"($env.XDG_CONFIG_HOME)/.android/avd"

# Set NDK_HOME if the directory exists
if ($env.ANDROID_HOME | path join "ndk" | path exists) {
  let ndk_dirs = (ls $"($env.ANDROID_HOME)/ndk" | where type == dir | sort-by name)
  if ($ndk_dirs | length) > 0 {
    $env.NDK_HOME = $ndk_dirs | last | get name
  }
}

$env.PYTHONSTARTUP = "~/.config/python/pythonrc.py"
$env.PYLINTHOME = $"($env.XDG_CACHE_HOME)/pylint"
$env.ZDOTDIR = "~/.config/zsh"
$env.CUDA_PATH = "/opt/cuda"
$env.CUDA_CACHE_PATH = $"($env.XDG_CACHE_HOME)/nv"
$env.NODE_REPL_HISTORY = $"($env.XDG_DATA_HOME)/node_repl_history"
$env.TS_NODE_HISTORY = $"($env.XDG_DATA_HOME)/ts_node_repl_history"
$env.MYSQL_HISTFILE = $"($env.XDG_DATA_HOME)/mysql_history"
$env.NPM_CONFIG_USERCONFIG = $"($env.XDG_CONFIG_HOME)/npm/npmrc"
$env.GEM_HOME = $"($env.XDG_DATA_HOME)/gem"
$env.GEM_SPEC_CACHE = $"($env.XDG_CACHE_HOME)/gem"
$env.NPM_CONFIG_STORE_DIR = $env.XDG_CACHE_HOME
$env._JAVA_OPTIONS = "-Djava.util.prefs.userRoot=\"$($env.XDG_CONFIG_HOME)/java\""
$env.XINITRC = $"($env.XDG_CONFIG_HOME)/X11/xinitrc"
$env.PSQLRC = $"($env.XDG_CONFIG_HOME)/pg/psqlrc"
$env.PSQL_HISTORY = $"($env.XDG_STATE_HOME)/psql_history"
$env.PGPASSFILE = $"($env.XDG_CONFIG_HOME)/pg/pgpass"
$env.PGSERVICEFILE = $"($env.XDG_CONFIG_HOME)/pg/pg_service.conf"
$env.DUB_HOME = $"($env.XDG_CACHE_HOME)/dub"
$env.MOZ_ENABLE_WAYLAND = 1
$env.VAGRANT_HOME = $"($env.XDG_DATA_HOME)/vagrant"
$env.VAGRANT_ALIAS_FILE = $"($env.XDG_DATA_HOME)/vagrant/aliases"
$env.JAVA_HOME = "/usr/lib/jvm/default"
$env.CAPACITOR_ANDROID_STUDIO_PATH = "/usr/bin/android-studio"
$env.ANSIBLE_HOME = $"($env.XDG_CONFIG_HOME)/ansible"
$env.ANSIBLE_CONFIG = $"($env.XDG_CONFIG_HOME)/ansible.cfg"
$env.ANSIBLE_GALAXY_CACHE_DIR = $"($env.XDG_CACHE_HOME)/ansible/galaxy_cache"
$env.DOCKER_CONFIG = $"($env.XDG_CONFIG_HOME)/docker"
$env.FLY_CONFIG_DIR = $"($env.XDG_CONFIG_HOME)/fly"
$env.PNPM_HOME = "/home/mirsella/.local/share/pnpm"
$env.WASMER_DIR = $"($env.XDG_CONFIG_HOME)/wasmer"
$env.WASMER_CACHE_DIR = $"($env.XDG_CACHE_HOME)/wasmer"
