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

# Create a new Rift workspace backed by a Git worktree.
#
# Defaults to ../<branch>, with the directory name sanitized for the filesystem.
# Custom paths are used as-is. The Git branch name itself is preserved.
def --env riftnew [
  branch: string # Git branch to switch to or create
  path?: path    # Target path, defaults to ../<sanitized-branch>
] {
  let valid_branch = (git check-ref-format --branch $branch | complete)
  if ($valid_branch.exit_code != 0) {
    print -e ($valid_branch.stderr | str trim)
    return
  }

  let configured_worktree = (git config --local --get core.worktree | complete)
  if ($configured_worktree.exit_code == 0) and (not (($configured_worktree.stdout | str trim) | is-empty)) {
    print -e $"this Git repository has core.worktree set to '($configured_worktree.stdout | str trim)'; unset it before creating a Rift Git worktree"
    return
  }

  let source_root = (git rev-parse --show-toplevel | complete)
  if ($source_root.exit_code != 0) {
    print -e ($source_root.stderr | str trim)
    return
  }

  let source = (($source_root.stdout | str trim) | path expand)

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

  let created = (rift create --into $into --name $name $source | complete)
  if ($created.exit_code != 0) {
    print -e ($created.stderr | str trim)
    return
  }

  let temp_worktree = ($into | path join $".git-worktree-($name)-(random uuid)")
  let local_branch = (git -C $source rev-parse --verify --quiet $"refs/heads/($branch)" | complete)
  let remote_branches = (
    git -C $source remote
    | lines
    | each {|remote|
      let remote_branch = $"refs/remotes/($remote)/($branch)"
      let remote_branch_exists = (git -C $source rev-parse --verify --quiet $remote_branch | complete)
      if ($remote_branch_exists.exit_code == 0) { $remote } else { null }
    }
    | compact
  )

  let worktree_added = if ($local_branch.exit_code == 0) {
    git -C $source worktree add --no-checkout $temp_worktree $branch | complete
  } else if (($remote_branches | length) == 1) {
    let remote_branch = $"($remote_branches.0)/($branch)"
    git -C $source worktree add --no-checkout --track -b $branch $temp_worktree $remote_branch | complete
  } else if ($remote_branches | is-empty) {
    git -C $source worktree add --no-checkout -b $branch $temp_worktree HEAD | complete
  } else {
    {
      exit_code: 1,
      stdout: "",
      stderr: $"branch '($branch)' exists on multiple remotes: ($remote_branches | str join ', '); create the local branch explicitly first"
    }
  }

  if ($worktree_added.exit_code != 0) {
    print -e ($worktree_added.stderr | str trim)
    print -e $"Rift workspace was created at ($target), but Git worktree linking failed. Commits there will not be stored in ($source)."
    return
  }

  let worktree_git_dir_result = (git -C $temp_worktree rev-parse --git-dir | complete)
  if ($worktree_git_dir_result.exit_code != 0) {
    print -e ($worktree_git_dir_result.stderr | str trim)
    return
  }

  let target_git = ($target | path join ".git")
  let temp_git = ($temp_worktree | path join ".git")
  let worktree_git_dir = (($worktree_git_dir_result.stdout | str trim) | path expand)

  let removed_copied_git = (^rm -rf $target_git | complete)
  if ($removed_copied_git.exit_code != 0) {
    print -e ($removed_copied_git.stderr | str trim)
    return
  }

  let moved_git = (^mv $temp_git $target_git | complete)
  if ($moved_git.exit_code != 0) {
    print -e ($moved_git.stderr | str trim)
    return
  }

  try {
    $"($target_git)\n" | save -f ($worktree_git_dir | path join "gitdir")
  } catch {|err|
    print -e $err.msg
    return
  }

  let removed_temp_worktree = (^rm -rf $temp_worktree | complete)
  if ($removed_temp_worktree.exit_code != 0) {
    print -e ($removed_temp_worktree.stderr | str trim)
    return
  }

  let reset = (git -C $target reset --hard $branch | complete)
  if ($reset.exit_code != 0) {
    print -e ($reset.stderr | str trim)
    return
  }

  let cleaned = (git -C $target clean -fd -e target/ | complete)
  if ($cleaned.exit_code != 0) {
    print -e ($cleaned.stderr | str trim)
    return
  }

  print ($reset.stdout | str trim)
  print ($cleaned.stdout | str trim)
  cd $target
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
