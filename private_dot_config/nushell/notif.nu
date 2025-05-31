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

$env.config.hooks = ($env.config.hooks | upsert pre_execution [{||
  # Record the start time and command string before execution
  $env._last_command_start_time = (date now)
  $env._last_command_string = (commandline)
}])

$env.config.hooks = ($env.config.hooks | upsert pre_prompt [{||
  # Check if a command was executed
  if ($env._last_command_start_time != null) {
    let end_time = (date now)
    let duration = ($end_time - $env._last_command_start_time)
    let duration_in_seconds = $duration | format duration sec | split words | $in.0 | into int

    # Check if the duration exceeds the timeout and the command is not blacklisted
    if ($duration_in_seconds >= $notification_timeout) and not ($notification_blacklist | any {|el| $env._last_command_string | str starts-with $el}) {
      notif $"Command finished after ($duration | format duration sec): ($env._last_command_string)"
    }

    # Reset the variables
    $env._last_command_start_time = null
    $env._last_command_string = null
  }
}])
