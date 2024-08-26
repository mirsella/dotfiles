export PATH="$PATH:$HOME/sgoinfre/cargo/bin"
export CARGO_HOME="$HOME/sgoinfre/cargo"
export RUSTUP_HOME="$HOME/sgoinfre/rustup"
export ANDROID_SDK_HOME="$HOME/sgoinfre/android"
export ANDROID_SDK_ROOT="$HOME/sgoinfre/android"
export ANDROID_HOME="$HOME/sgoinfre/android"
export BROWSER="firefox-esr"
alias rmf='/bin/rm -rf '
alias android-studio='flatpak run --env=ANDROID_SDK_HOME="$HOME/sgoinfre/android" --env=ANDROID_SDK_ROOT="$HOME/sgoinfre/android" --env=ANDROID_HOME="$HOME/sgoinfre/android" com.google.AndroidStudio'
alias V='echo -n z | xclip -selection clipboard'
bak() { cp -r "${1}" "${1}.bak" }
bakm() { mv "${1}" "${1}.bak" }
clipp () { xclip -out -selection clipboard; }
clip () { xclip -in -selection clipboard < "${@:-/dev/stdin}"; }

# [ ! -d ~/sgoinfre/.cache ] && mkdir ~/sgoinfre/.cache
# export XDG_CACHE_HOME="$HOME/sgoinfre/.cache"
# export XDG_DATA_HOME="$HOME/sgoinfre/.local/share"
# export XDG_STATE_HOME="$HOME/sgoinfre/.local/state"

alias colemak='xkbcomp $HOME/.config/zsh/lib/42/42_btoz.xkb $DISPLAY'
if [ ! -f /tmp/loadcustomxkb ]; then
  touch /tmp/loadcustomxkb
  xkbcomp ./42_btoz.xkb $DISPLAY
fi
