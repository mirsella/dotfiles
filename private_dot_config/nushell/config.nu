source completions.nu
source functions.nu
const host_config = if ((sys host).hostname == "laptop") { "laptop.nu" } else { null }
source $host_config
source alias.nu
source plugins.nu
source notif.nu

$env.config.buffer_editor = "nvim"
$env.config.show_banner = false
# $env.config.edit_mode = 'vi'
$env.config.history.max_size = 100000
$env.config.rm.always_trash = true
$env.config.highlight_resolved_externals = true
$env.config.float_precision = 4
$env.config.keybindings = [
  {
    name: menu_accept
    modifier: control
    keycode: char_y
    mode: [emacs, vi_normal, vi_insert]
    event: { send: Enter }
  }
  {
    name: completion_menu_open
    modifier: control
    keycode: char_n
    mode: [emacs, vi_normal, vi_insert]
    event: {
      until: [
        { send: menu name: completion_menu }
        { send: menunext }
        { edit: complete }
      ]
    }
  }
  {
    name: open_dolphin
    modifier: control
    keycode: char_e
    mode: [emacs, vi_normal, vi_insert]
    event: { send: executehostcommand cmd: "if (which 'dolphin' | is-empty) { print --stderr 'dolphin not found' } else { job spawn --description 'open dolphin' { ^dolphin (pwd) } | ignore }" }
  }
  {
    name: accept_history_suggestion
    modifier: control
    keycode: char_y
    mode: [emacs, vi_insert]
    event: { send: HistoryHintComplete }
  }
]

# NOTE: using atuin dotfiles for now
# let tokens_dir = $env.HOME | path join ".config/token"
# for path in (glob $"($tokens_dir)/*.json") {
#   if ($path | path exists) {
#     open $path | load-env
#   }
# }

let autoload_path = ($nu.user-autoload-dirs | first)
mkdir $autoload_path

def refresh-autoload [name: string, output_path: path, command: closure] {
  let result = (do $command | complete)

  if $result.exit_code == 0 {
    $result.stdout | save -f $output_path
    return
  }

  print --stderr $"failed to refresh ($name) shell integration: exit ($result.exit_code)"

  let stderr = ($result.stderr | str trim)
  if ($stderr | is-not-empty) {
    print --stderr $stderr
  }
}

if $nu.is-interactive {
  job spawn {
    refresh-autoload starship ($autoload_path | path join "starship.nu") {|| starship init nu }
    refresh-autoload zoxide ($autoload_path | path join "zoxide.nu") {|| zoxide init --cmd cd nushell }
    refresh-autoload jj ($autoload_path | path join "jj.nu") {|| jj util completion nushell }
    refresh-autoload atuin ($autoload_path | path join "atuin.nu") {|| atuin init nu --disable-up-arrow }
    refresh-autoload rift ($autoload_path | path join "rift.nu") {|| rift shell-init nushell }
  } | ignore
}

atuin dotfiles var list | lines | parse "export {name}={value}" | reduce -f {} {|it, acc| $acc | upsert $it.name $it.value} | load-env
