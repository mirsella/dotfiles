export PATH="/mnt/nfs/homes/lgillard/.local/bin:$PATH"
alias rmf='/bin/rm -rf '
# alias rm='rmtrash -rf '
# unalias s trash-empty trash-list trash-restore
alias rm='trash -rf'
bak() { cp -r "${1}" "${1}.bak" }
bakm() { mv "${1}" "${1}.bak" }
clipp() { xclip -out -selection clipboard; }
clip() { xclip -in -selection clipboard < "${@:-/dev/stdin}"; }
# [ ! -d ~/sgoinfre/.cache ] && mkdir ~/sgoinfre/.cache
# export XDG_CACHE_HOME="$HOME/sgoinfre/.cache"
#export XDG_DATA_HOME="$HOME/sgoinfre/.local/share"
#export XDG_STATE_HOME="$HOME/sgoinfre/.local/state"

# ~/.zshrc
# source ~/.config/zsh/exportxdg
# source ~/.config/zsh/.zshrc
