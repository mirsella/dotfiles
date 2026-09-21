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


def --wrapped oc [...args] {
  let dir = (pwd)
  let server_env = $"($env.HOME)/.config/opencode/server.env"
  if not ($server_env | path exists) {
    error make { msg: $"OpenCode server credentials not found at ($server_env)" }
    return
  }

  let password_lines = (
    open --raw $server_env
    | lines
    | where { |line| $line starts-with "OPENCODE_SERVER_PASSWORD=" }
  )
  if (($password_lines | length) != 1) {
    error make { msg: $"Expected one OPENCODE_SERVER_PASSWORD entry in ($server_env)" }
    return
  }

  let password = ($password_lines | first | str replace "OPENCODE_SERVER_PASSWORD=" "")
  with-env { OPENCODE_SERVER_PASSWORD: $password } {
    ^opencode attach http://127.0.0.1:14096 --dir $dir ...$args
  }
}

def cfm [] { cargo fmt; gam "cargo fmt" }
