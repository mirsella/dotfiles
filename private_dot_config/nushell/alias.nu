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
alias gclipp = do { git clone (wl-paste) }
alias gcl = git clone
alias lj = lazyjj
alias j = jj
alias jp = jj git push
alias jdi = jj diff
alias fd = fd -HL -E run -E media -E sys -E proc -E dosdevices
alias fda = fd -I
alias update = paru -Syu --noconfirm
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
alias cfa = cargo fix --allow-dirty --allow-staged --all-features --all-targets
alias cwt = cargo watch -qc -x test
alias cw = cargo watch -qc
alias cch = cargo check --all-targets --all-features --workspace
alias ccy = cargo clippy --all-features --all-targets
alias ccyp = cargo clippy --all-targets --all-features --workspace -- -D clippy::pedantic
alias cad = cargo add
alias cr = cargo run
alias cb = cargo build
alias ct = cargo test
alias ce = cargo expand --theme "Monokai Extended Light"
alias d = docker
alias dc = docker compose
alias p = pnpm
alias rm = rm --trash --recursive
alias rmf = rm --permanent --interactive --recursive
alias clip = wl-copy
alias clipp = wl-paste
alias ju = just
alias dark = darkman set dark
alias light = darkman set light
alias fg = job unfreeze
alias suspend = systemctl suspend
