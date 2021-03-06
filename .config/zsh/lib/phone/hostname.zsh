source "$ZDOTDIR"/lib/fdm.zsh
unalias s
alias clip='termux-clipboard-set'
alias clipp='termux-clipboard-get'
# export PATH="$PATH:/data/data/com.termux/files/home/.local/share/gem/ruby/3.0.0/bin"
export PATH="$PATH:/data/data/com.termux/files/home/.local/share/gem/bin"
bak() { cp -r "${1}" "${1}.bak" }
bakm() { mv "${1}" "${1}.bak" }
alias end='pkill -f '
alias rmf='~/../usr/bin/rm -rf '
alias rm='rmtrash -rf '
alias chownm='chown -R $USER: '
alias v='nvim -p'
export XDG_RUNTIME_DIR=$PREFIX/run
hash -d s=/sdcard/
hash -d m=/sdcard/Music
hash -d r=/data/data/com.termux/files/usr
