def winboot [] {
  sudo bootctl set-oneshot auto-windows
  sudo reboot
}

def waitforjob [job_number: int = 1] {
  # Simplified job tracking - may need adjustment for Nushell
  while (jobs | where id == $job_number | length) > 0 {
    sleep 2sec
  }
}

def notif [...args] {
  let message = if ($args | is-empty) { "empty" } else { $args | str join " " }
  
  let data = {
    chat_id: $env.TgId
    text: $message
  } | to json
  
  http post $"https://api.telegram.org/bot($env.TgToken)/sendMessage" --content-type "application/json" $data -m 2sec
  | get ok result.text 
  | to json -r
}

def bak [file: path] {
  cp -r $"($file)" $"($file).bak"
}

def bakm [file: path] {
  mv $"($file)" $"($file).bak"
}

def psaux [query: string] {
  ps -l | where name =~ $query or command =~ $query
}

def gam [...args] {
  let message = if ($args | is-empty) { "quick commit" } else { $args | str join " " }
  git add -A
  git commit -m $message
}

def gamp [...args] {
  let message = if ($args | is-empty) { "quick commit" } else { $args | str join " " }
  git add -A
  git commit -m $message
  git push
}

# Create a new Rift workspace for the current repository.
#
# Defaults to ../<branch>, with the directory name sanitized for the filesystem.
# Custom paths are used as-is. The Git branch name itself is preserved.
def riftnew [
  branch: string # Git branch to switch to or create
  path?: path    # Target path, defaults to ../<sanitized-branch>
] {
  let valid_branch = (git check-ref-format --branch $branch | complete)
  if ($valid_branch.exit_code != 0) {
    print -e ($valid_branch.stderr | str trim)
    return
  }

  let raw_target = if ($path == null) {
    let safe_branch = ($branch | str replace --regex --all "[^A-Za-z0-9._-]+" "-" | str trim --char "-")
    if ($safe_branch | is-empty) {
      print -e $"branch name '($branch)' does not produce a usable directory name; pass an explicit path"
      return
    }

    ".." | path join $safe_branch
  } else {
    $path
  }

  let target = ($raw_target | path expand)
  let into = ($target | path dirname)
  let name = ($target | path basename)

  let created = (rift create --into $into --name $name | complete)
  if ($created.exit_code != 0) {
    print -e ($created.stderr | str trim)
    return
  }

  let branch_exists = (git -C $target rev-parse --verify --quiet $"refs/heads/($branch)" | complete)
  let switched = if ($branch_exists.exit_code == 0) {
    git -C $target switch $branch | complete
  } else {
    git -C $target switch -c $branch | complete
  }

  if ($switched.exit_code != 0) {
    print -e ($switched.stderr | str trim)
    return
  }

  let reset = (git -C $target reset --hard $branch | complete)
  if ($reset.exit_code != 0) {
    print -e ($reset.stderr | str trim)
    return
  }

  print ($reset.stdout | str trim)
}

def jd [...args] {
  jj describe -m ($args | str join " ")
}

def jc [...args] {
  jj commit -m ($args | str join " ")
}

def jci [...args] {
  jj commit -i -m ($args | str join " ")
}

def ports [] {
    # Run lsof with sudo.
    # We use 'complete' to capture the output without throwing an error if lsof returns exit code 1.
    let lsof_out = (^sudo lsof -iTCP -sTCP:LISTEN -n -P | complete)

    if ($lsof_out.stdout | is-empty) {
        return
    }

    $lsof_out.stdout
    | detect columns --guess
    | each {|row|
        # Extract the final TCP port from IPv4/IPv6 listener strings.
        let port = (
            $row.NAME
            | parse --regex '.*:(?<port>\d+)(?:\s+\(LISTEN\))?$'
            | get 0.port
            | into int
        )

        # Fetch the full command line using ps.
        let cmd = (
            try { 
                ^ps -p $row.PID -o command= 
                | str trim 
                | str replace --regex '^-' '' 
            } catch { 
                $row.COMMAND 
            }
        )

        # Return a record for this row.
        # Nushell automatically formats lists of records as a table.
        {
            port: $port,
            pid: ($row.PID | into int),
            command: $cmd
        }
    }
    # Sort numerically by port
    | sort-by port
}
