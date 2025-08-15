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
  
  http post $"https://api.telegram.org/bot($env.TgToken)/sendMessage" --content-type "application/json" $data 
  | get ok result.text 
  | to json -r
}

def bak [file: path] {
  cp -r $file $"($file).bak"
}

def bakm [file: path] {
  mv $file $"($file).bak"
}

def psaux [name: string] {
  ps -l | where name =~ $name
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
