alias nvidia-settings='nvidia-settings --config="$XDG_CONFIG_HOME"/nvidia/settings'
clip() { xclip -in -selection clipboard < "${@:-/dev/stdin}"; }
clipp() { xclip -out -selection clipboard; }
alias rmf='s /bin/rm -rf '
alias rm='s rmtrash -rf '
alias autoxrandr='xrandr --auto && xrandr --output DVI-D-0 --mode 1280x1024 --left-of HDMI-0 --mode 1920x1080 --rate 144'
export PATH="$PATH:$HOME/dev/bin"
hash -d d=/run/media/mirsella/ssd/documents.git/
hash -d s=/run/media/mirsella/ssd/
hash -d w=/run/media/mirsella/windows/Users/mirsella/

_zsh_system_clipboard_set=(xclip -in -selection clipboard)
_zsh_system_clipboard_get=(xclip -out -selection clipboard)
