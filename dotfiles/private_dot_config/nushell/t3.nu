# T3's POSIX setup wrapper prints a literal $? in Nu. Report the real status
# when the command returns to the prompt, including when it fails before printf.
# Remove when https://github.com/pingdotgg/t3code/issues/24423 is fixed.
if $nu.is-interactive and ($env.T3CODE_PROJECT_ROOT? | is-not-empty) {
  $env.config.hooks.pre_execution = ($env.config.hooks.pre_execution | append {||
    $env._T3_SETUP_MARKER = (commandline
      | parse --regex r#'\); printf '\\n(?<marker>__T3_SETUP_DONE___[a-f0-9]{32}:)%s\\n' "\$\?"$'#
      | get --optional 0.marker)
  })
  $env.config.hooks.pre_prompt = ($env.config.hooks.pre_prompt | prepend {||
    let marker = $env._T3_SETUP_MARKER?
    if ($marker | is-not-empty) {
      hide-env _T3_SETUP_MARKER
      print $"\n($marker)($env.LAST_EXIT_CODE)"
    }
  })
}
