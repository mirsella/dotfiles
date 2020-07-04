source $ZDOTDIR/lib/clipboard.zsh
source $ZDOTDIR/lib/completion.zsh
source $ZDOTDIR/lib/keybinds.zsh
source $ZDOTDIR/lib/spectrum.zsh
source $ZDOTDIR/lib/zprofile.zsh
source /usr/share/fzf/completion.zsh
source /usr/share/fzf/key-bindings.zsh

export LANGUAGE=en_US.UTF-8
export LANG=en_US.UTF-8
export LC_ALL=en_US.UTF-8
export EDITOR=/usr/bin/vim
export TERMINAL=/usr/bin/konsole
export BROWSER=/usr/bin/chromium
export QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1
export WORDCHARS=${WORDCHARS/\*\?\_\-\.\[\]\~\=\/\&\;\!\#\$\%\^\(\)\{\}\<\>} 

# fzf config
export FZF_DEFAULT_COMMAND='rg -i --files --no-ignore --hidden --follow --glob "!.git/*"'
export FZF_DEFAULT_OPTS='--preview="bat -pp --color=always {}"'

# bat config
export BAT_THEME='Monokai Extended Origin'

# vimv use rmtrash. i hope my pull request will be merged lol.
export VIMV_USE_RMTRASH=1

# git
export GIT_COMMITTER_EMAIL=mirsella@protonmail.com
export GIT_AUTHOR_EMAIL=mirsella@protonmail.com

([[ -f /home/mirsella/.gtkrc-2.0 ]] && rm -v /home/mirsella/.gtkrc-2.0) &!

function clip() {
  $@ | tee >(clipcopy)
}

function gitclearcommit() {
  branch=$(git branch | grep '\*' | sed 's/\* //')
  echo "WARNING !! ALL NON PUSHED CHANGE WILL BE GONE FOREVER !!
  branch to clear : $branch"
  sleep 10
  cp -r ../$(basename $(dirname $(realpath $0))) ~/.cache/$(basename $(dirname $(realpath $0)))
  git checkout --orphan ${branch}cleared
  git add -A
  git commit -m "cleared commit history $(date)"
  git branch -D ${branch}
  git branch -m ${branch}
  git push -f origin ${branch}
  echo "the repo was backed up in ~/.cache/$(basename $(dirname $(realpath $0)))"
}

hash -d s=/run/media/mirsella/ssd/
hash -d w=/run/media/mirsella/windows/Users/mirsella/
hash -d k=/home/mirsella/.config/keebie/
hash -d b=/home/mirsella/.config/backupscript/
hash -d m=/run/media/mirsella/ssd/Music/
alias -s odt='libreoffice'
alias -s pdf='chromium'
alias -s html='vscodium'
alias fanm='nbfc set -f 1 -s 100 && nbfc set -f 0 -s 100'
alias fan='nbfc set -f 1 -a && nbfc set -f 0 -a'
alias removelock='sudo rm /var/lib/pacman/db.lck'
alias end='sudo pkill -f'
alias psaux='ps aux | rg'
alias sshrpi='ssh -Ct mirsella@192.168.1.166'
alias sshrpiw='ssh -t mirsella@mirsella.mooo.com'
alias NU='2> /dev/null' #Silences stderr
alias NUL='> /dev/null 2>&1' #Silences both stdout and stderr
alias yt='youtube-dl -xic --audio-format mp3 -f bestaudio --audio-quality 0' 
alias mkdir='mkdir -p'
alias cp='cp -r'
alias rm='sudo rmtrash -rf'
alias trash-restore='sudo trash-restore'
alias trash-empty='sudo trash-empty'
alias trash-list='sudo trash-list'
alias grep='nocorrect grep --color=auto -Ei'
alias ls='ls -AhX --group-directories-first --color=auto'
alias ll='ls -AhXl --group-directories-first --color=auto'
alias v='echo -ne "\e[0 q"; sudo -E vim -p'
alias S='sudo systemctl'
alias sudo='sudo -E '
alias s='sudo '
alias gcp='git commit -am "backup $(date)"; git push'
alias bat='bat -pp --color=always'
alias q='exit'
alias chownm='sudo chown -R mirsella:mirsella'
alias dut='du -cksh'
alias rg='rg --no-ignore --hidden -i'
alias rge='rg --no-ignore --hidden -e'
alias rga='rga --no-ignore --hidden -i'
alias tree='tree -Ca'
alias nvidia-settings='nvidia-settings --config="$XDG_CONFIG_HOME"/nvidia/settings'
alias https='https --style=monokai'
alias http='https'
alias wget='https -d'
alias fd='fd -HI'
alias gb='git branch'
alias gc='git checkout'
