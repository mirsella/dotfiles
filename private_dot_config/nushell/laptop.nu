# Install boot service manually: sh -c 'tmp=$(mktemp) && trap "rm -f \"$tmp\"" EXIT && chezmoi execute-template < ~/.local/share/chezmoi/run_once_after_install-ryzenadj-laptop.sh.tmpl > "$tmp" && sh "$tmp"'
def charge-limit [limit?: int] {
  sudo framework_tool --charge-limit ...([$limit] | compact)
}
