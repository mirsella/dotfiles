# Install boot service manually: sh -c 'tmp=$(mktemp) && trap "rm -f \"$tmp\"" EXIT && chezmoi execute-template < ~/.local/share/chezmoi/run_once_after_install-ryzenadj-laptop.sh.tmpl > "$tmp" && sh "$tmp"'
def charge-limit [limit?: int] {
  sudo framework_tool --charge-limit ...([$limit] | compact)
}

def ryzenadj-laptop [] {
  sudo ryzenadj --stapm-limit=45000 --fast-limit=53000 --slow-limit=35000 --apu-slow-limit=41000 --skin-temp-limit=45000 --tctl-temp=100 --apu-skin-temp=90 -i
}
