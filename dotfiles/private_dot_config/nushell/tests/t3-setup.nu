# Run with: nu --no-config-file ~/.config/nushell/tests/t3-setup.nu /path/to/scratch
use std/assert
source ../alias.nu

def main [scratch: path] {
  let root = (mktemp --directory --tmpdir-path $scratch t3-setup-XXXXXXXX)
  $env.XDG_DATA_HOME = ($root | path join data)
  $env.config.rm.always_trash = true

  try {
    let aliases = ($env.FILE_PWD | path join ../alias.nu | path expand)
    # gtrash accepts directories without --recursive, unlike native rm.
    let ordinary = ($root | path join ordinary-directory)
    mkdir $ordinary
    'keep recoverable' | save ($ordinary | path join content.txt)
    rm $ordinary
    assert (not ($ordinary | path exists))
    assert equal (open --raw ($env.XDG_DATA_HOME | path join Trash files ordinary-directory content.txt)) 'keep recoverable'

    # T3's generated script removes itself before invoking the setup command.
    let setup = ($root | path join '.t3-setup-test.nu')
    let ran = ($root | path join setup-ran)
    [
      $"source ($aliases | to nuon)"
      '$env.config.rm.always_trash = true'
      $"rm --permanent ($setup | to nuon)"
      $"'ran' | save ($ran | to nuon)"
    ] | str join (char newline) | save $setup
    let result = (^$nu.current-exe --no-config-file $setup | complete)
    assert equal $result.exit_code 0 $result.stderr
    assert (not ($setup | path exists))
    assert equal (open --raw $ran) 'ran'
    assert (not ($env.XDG_DATA_HOME | path join Trash files '.t3-setup-test.nu' | path exists))

    # The outer wrapper also cleans up a script if its child failed early.
    'leftover' | save $setup
    rm --permanent $setup
    assert (not ($setup | path exists))
    print 'T3 native cleanup and default gtrash behavior checks passed.'
  } catch {|err|
    %rm --permanent --recursive --force $root
    error make $err
  }
  %rm --permanent --recursive --force $root
}
