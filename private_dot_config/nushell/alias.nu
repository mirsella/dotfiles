alias journalctlerrors = ^journalctl -p 3 -xb
alias clearnotif = do {
  let max_id = (notify-send -p " " -t 1 | str trim | into int)
  for id in ($max_id..0) {
    qdbus6 org.kde.plasmashell /org/freedesktop/Notifications org.freedesktop.Notifications.CloseNotification $id
  }
}

alias lsd = lsd -A --hyperlink=auto
# alias ll = lsd -Al --hyperlink=auto
alias ls = ls -a
alias ll = ls -al
alias tree = lsd -A --tree -I node_modules -I .git
alias bat = bat -pp --color=always --theme="Solarized (dark)"
alias q = exit
alias dut = ^du -cksh
alias rg = rg --hidden -S
alias rga = rga --hidden -S
alias lg = lazygit
alias g = git
alias gs = git switch
alias gw = git worktree
alias gbro = gh browse
alias gbi = git bisect
alias gch = git checkout
alias gb = git branch
alias gd = git diff HEAD
alias gdd = git diff HEAD^
alias ga = git add -A
alias gt = git stash
alias gc = git commit
alias gp = git push
alias gplease = git push --force-with-lease
alias gclipp = do { git clone (wl-paste) }
alias gcl = git clone
alias lj = lazyjj
alias j = jj
alias jp = jj git push
alias jdi = jj diff
alias fd = fd -HL -E run -E media -E sys -E proc -E dosdevices
alias fda = fd -I
alias paruupdate = paru -Syu --noconfirm
alias chownm = sudo chown -R $"($env.USER):"
alias cp = cp -r
alias S = sudo systemctl
alias s = sudo -E
alias uefireboot = systemctl reboot --firmware-setup
alias arduino-cli = arduino-cli --config-file $env.XDG_CONFIG_HOME/arduino15/arduino-cli.yaml
alias v = nvim -p
alias parus = paru -S --noconfirm --needed
alias rga = rga --no-ignore --hidden -S
alias kdesend = kdeconnect-cli -n phone --share
alias colemak = setxkbmap us -variant colemak_dh_iso
alias qwerty = setxkbmap us
alias c = cargo
alias cfa = cargo clippy --fix --allow-dirty --allow-staged --all-features --all-targets
alias cwt = cargo watch -c -x test
alias cw = cargo watch -c
alias cch = cargo check --all-targets --all-features --workspace
alias cdoc = cargo doc --all-features --workspace
alias ccy = cargo clippy --all-features --all-targets
alias ccyp = cargo clippy --all-targets --all-features --workspace -- -D clippy::pedantic
alias cad = cargo add
alias cr = cargo run
alias crr = cargo run --release
alias cb = cargo build
alias ct = cargo test
alias ce = cargo expand --theme "Monokai Extended Light"
alias d = docker
alias dc = docker compose
alias npm = sfw npm
alias npx = sfw npx
alias pnpm = sfw pnpm
alias pnpx = sfw pnpx
alias yarn = sfw yarn
alias bun = sfw bun
alias p = pnpm
alias rm = gtrash put
alias rmf = /bin/rm -rf
alias clip = wl-copy
alias clipp = wl-paste
alias ju = just
alias dark = darkman set dark
alias light = darkman set light
alias fg = job unfreeze
alias suspend = systemctl suspend
alias f = fzf --multi
def --wrapped oc [...args] {
  let dir = (pwd)
  let server_env = $"($env.HOME)/.config/opencode/server.env"
  if not ($server_env | path exists) {
    error make { msg: $"OpenCode server credentials not found at ($server_env)" }
    return
  }

  let password_lines = (
    open --raw $server_env
    | lines
    | where { |line| $line starts-with "OPENCODE_SERVER_PASSWORD=" }
  )
  if (($password_lines | length) != 1) {
    error make { msg: $"Expected one OPENCODE_SERVER_PASSWORD entry in ($server_env)" }
    return
  }

  let password = ($password_lines | first | str replace "OPENCODE_SERVER_PASSWORD=" "")
  with-env { OPENCODE_SERVER_PASSWORD: $password } {
    ^opencode attach http://127.0.0.1:14096 --dir $dir ...$args
  }
}
alias ocgit-build = do {
  cd ~/dev/opencode
  bun run --cwd packages/cli build -- --single --skip-install
}
def --wrapped ocgit [...args] {
  cd ~/dev/opencode
  let binary = (glob "~/dev/opencode/packages/cli/dist/*/bin/opencode2" | first)
  if ($binary == null) {
    error make {
      msg: "opencode2 not found; run ocgit-build to build the compiled CLI first"
    }
    return
  }
  ^$binary ...$args
}
alias occ = oc --continue
alias autocommit = opencode run --model "openai/gpt-5.3-codex-spark" --variant high --command "commitdiff"
alias dm = darkman
alias codex = codex --dangerously-bypass-approvals-and-sandbox
alias opencode-plugin-update = do { cd ~/.cache/opencode/packages; fd package.json -E node_modules | lines | each { $in | path dirname } | par-each { do { cd $in; bun update --latest } } }
