# Define the timeout for long-running commands in seconds
let notification_timeout = 20

# Define a list of commands to blacklist from notifications
let notification_blacklist = [
  "bacon","cch"
  "nvim",
  "lazygit",
  "ju sc"
  "lg",
  "v",
  "man",
  "less",
  "vi",
  "vim",
  "code",
  "emacs",
  "top",
  "htop",
  "watch",
]

# Variable to store the start time of the last executed command
# We need to make this an environment variable so it persists between hooks
$env._last_command_start_time = null

# Variable to store the last executed command string
$env._last_command_string = null

# Variable to store the last command's exit status
$env._last_command_status = null

$env.config.hooks = ($env.config.hooks | upsert pre_execution [{||
  # Record the start time and command string before execution
  $env._last_command_start_time = (date now)
  $env._last_command_string = (commandline)
  # Reset status before the command runs
  $env._last_command_status = null
}])

$env.config.hooks = ($env.config.hooks | upsert pre_prompt [{||
  if ($env._last_command_start_time != null) {
    let end_time = (date now)
    let duration = ($end_time - $env._last_command_start_time)
    let duration_in_seconds = $duration
      | format duration sec
      | split words
      | $in.0
      | into int

    # Use LAST_EXIT_CODE as the previous command's status
    let status = if ($env.LAST_EXIT_CODE == null) { "?" } else { $env.LAST_EXIT_CODE }

    if ($duration_in_seconds >= $notification_timeout) and not (
      $notification_blacklist
      | any {|el| $env._last_command_string | str starts-with $el }
    ) {
      notif $"Command finished after ($duration | format duration sec) with status ($status): ($env._last_command_string)"
    }

    $env._last_command_start_time = null
    $env._last_command_string = null
    $env._last_command_status = null
  }
}])
