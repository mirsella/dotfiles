# Locally maintained zoxide integration.

export-env {
  let pwd_hooks = ($env.config.hooks.env_change.PWD? | default [])
  if not ($pwd_hooks | any { $in.__zoxide_hook? == true }) {
    $env.config.hooks.env_change.PWD = ($pwd_hooks | append {
      __zoxide_hook: true,
      code: {|_, dir| ^zoxide add -- $dir}
    })
  }
}

def "nu-complete zoxide" [place: record, buffer: string, token: record] {
  use std/util
  use ../completion-strings.nu unquote

  let keywords = try {
    $place.command | skip 1 | each {|word| unquote $word }
  } catch {
    # An unfinished quoted argument can still use native directory completion.
    null
  }
  let indexed = if $keywords == null { [] } else {
    ^zoxide query --list --exclude $env.PWD -- ...$keywords | lines
  }
  let paths = (
    $indexed | append (
      $token.text | commandline complete --type directory | each {|value|
        let path = (unquote $value)
        # Only ~/ expands to home; ~name is a literal local directory.
        let absolute = if ($path starts-with '~/') { $path } else { $env.PWD | path join $path }
        $absolute | path expand --no-symlink
      }
    )
    | wrap path
    | insert canonical {|row| $row.path | path expand }
    | uniq-by canonical
    | get path
  )
  if ($paths | is-empty) { return [] }

  # Replace the whole call, including any keywords supplied by an alias.
  let head = (util structure $buffer | where kind == internalcall | last)
  let span = {start: $head.span.start, end: $place.target.end}
  $paths | each {|path|
    let relative = try { $path | path relative-to $env.PWD } catch { $path }
    # NUON's bare strings can be shell syntax, such as the redirection `out>`.
    let argument = if $relative =~ '^[\w./][\w./-]*$' {
      $relative
    } else {
      $relative | to nuon
    }
    {
      value: $"($place.command.0) ($argument)"
      display_override: $"($relative | str trim -r -c '/')/"
      span: $span
    }
  }
}

# Jump to a directory using only keywords.
@complete "nu-complete zoxide"
export def --env --wrapped cd [...rest: directory] {
  match $rest {
    [] => { %cd }
    ['-'] => { %cd - }
    [$arg] if ($arg | path expand | path type) == dir => { %cd $arg }
    _ => {
      %cd (^zoxide query --exclude $env.PWD -- ...$rest | str trim -r -c "\n")
    }
  }
}

# Jump to a directory using interactive search.
@complete "nu-complete zoxide"
export def --env --wrapped cdi [...rest: directory] {
  %cd (^zoxide query --interactive -- ...$rest | str trim -r -c "\n")
}
