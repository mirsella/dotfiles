# Run with: nu --no-config-file ~/.config/nushell/tests/completions.nu
use std/assert
source ../completions.nu
source ../autoload/zoxide.nu
source ../autoload/zoxide.nu
alias shortcut = cd alpha
let config = ($env.FILE_PWD | path join ../completions.nu | path expand)
let integration = ($env.FILE_PWD | path join ../autoload/zoxide.nu | path expand)

def complete-line [line: string] {
  # A fresh engine snapshots the fixture's current directory for native completion.
  ^nu --no-config-file -c $"source ($config | to nuon); source ($integration | to nuon); ($line | to nuon) | commandline complete --detailed | to nuon"
  | from nuon
}

def completion-path [value: string] {
  $"[($value | str replace --regex '^cdi? ' '')]" | from nuon | first | path expand
}

let root = (^mktemp --directory --tmpdir=/tmp/opencode nu-completion.XXXXXX | str trim)
$env._ZO_DATA_DIR = ($root | path join database)
$env._ZO_FZF_OPTS = '--filter=__no_such_directory__'
let origin = $env.PWD

try {
  let paths = [
    ($root | path join alpha project)
    ($root | path join 'quoted "project"')
    ($root | path join 'unicode é project\$literal')
  ]
  mkdir $env._ZO_DATA_DIR ...$paths
  ^ln --symbolic -- $paths.0 ($root | path join linked-project)
  ^zoxide add -- ...$paths
  %cd $root
  '' | save ($root | path join plain-file)

  assert equal ($env.config.hooks.env_change.PWD | where __zoxide_hook? == true | length) 1
  let cases = [
    {line: 'cd project', prefix: '', keywords: [project]}
    {line: 'cd alpha project', prefix: '', keywords: [alpha project]}
    {line: 'cd alpha ', prefix: '', keywords: [alpha '']}
    {line: 'pwd; cd alpha project', prefix: 'pwd; ', keywords: [alpha project]}
    {line: 'print "é"; cd alpha project', prefix: 'print "é"; ', keywords: [alpha project]}
    {line: '[1] | each { cd alpha project', prefix: '[1] | each { ', keywords: [alpha project]}
    {line: 'shortcut project', prefix: '', keywords: [alpha project]}
    {line: 'cd "quoted \"project\""', prefix: '', keywords: ['quoted "project"']}
    {line: "cd 'quoted'", prefix: '', keywords: [quoted]}
    {line: 'cd `quoted`', prefix: '', keywords: [quoted]}
  ]
  for case in $cases {
    let expected = (^zoxide query --list --exclude $env.PWD -- ...$case.keywords | lines)
    assert ($expected | is-not-empty)
    let suggestions = ($case.line | commandline complete --detailed | take ($expected | length))
    assert equal ($suggestions | get value | each {|value| completion-path $value }) $expected $case.line
    for suggestion in $suggestions {
      assert equal $suggestion.span {
        start: ($case.prefix | str length --utf-8-bytes)
        end: ($case.line | str length --utf-8-bytes)
      }
    }
  }

  for command in [cd cdi] {
    let values = ($"($command) project" | commandline complete)
    assert equal $values ($"cd project" | commandline complete | str replace --regex '^cd ' $"($command) ")
    assert ($"($root)/alpha" in ($"($command) ($root)/al" | commandline complete | each {|value| completion-path $value }))
    assert ($"($root)/alpha" in ($"($command) ($root)/pha" | commandline complete | each {|value| completion-path $value }))
    assert (($"($command) ($root)/plain" | commandline complete) | is-empty)
    assert (($"($command) __no_such_directory__" | commandline complete) | is-empty)
    $"($command) \"unfinished" | commandline complete | ignore
  }

  # Execute accepted completions to check quoting against Nushell's real parser.
  assert equal ('cd alpha project' | commandline complete | first) 'cd alpha/project'
  for suggestion in ('cd project' | commandline complete) {
    let expected = (completion-path $suggestion)
    let actual = (^nu --no-config-file -c $"source ($integration | to nuon); ($suggestion); $env.PWD" | str trim)
    assert equal $actual $expected
  }

  cd alpha
  assert equal $env.PWD ($root | path join alpha)
  cd -
  assert equal $env.PWD $root
  cd alpha project
  assert equal $env.PWD $paths.0
  assert (try { cd __no_such_directory__; false } catch { true })
  assert equal $env.PWD $paths.0
  assert (try { cdi __no_such_directory__; false } catch { true })
  assert equal $env.PWD $paths.0
  cd
  assert equal $env.PWD $nu.home-dir

  %cd $root
  for command in [ls nvim] {
    assert ($"($root)/plain-file" in ($"($command) ($root)/plfi" | commandline complete))
  }
  assert equal ($"nvim ($root)/plain" | commandline complete) [$"($root)/plain-file"]
  let ranked = (^zoxide query --list --exclude $env.PWD -- project | lines)
  for command in [cd cdi] {
    let merged = (complete-line $"($command) project")
    assert equal ($merged | get display_override) ($ranked | each {|path| $"($path | path relative-to $root)/" })
    assert equal ($merged | get value | each {|value| completion-path $value }) $ranked
  }
  %cd $paths.0
  let outside = (complete-line 'cd project')
  assert equal ($outside | get display_override) (^zoxide query --list --exclude $env.PWD -- project | lines | each {|path| $"($path)/" })
  %cd $root
  let relative = (complete-line 'nvim plfi')
  assert equal ($relative | select value span) [{value: plain-file, span: {start: 5, end: 9}}]
  for name in ['123' 'true'] {
    '' | save ($root | path join $name)
    assert equal (complete-line $"nvim ($name)" | get value) [$name]
  }
  for line in ['git --ver' 'nvim --vers'] {
    assert ('--version' in ($line | commandline complete))
  }
  for path in ($paths | skip 1) {
    let line = $"nvim ($root)/"
    let target = ($"($path)/" | to nuon)
    assert equal ($line | commandline complete | where $it == $target | length) 1
  }

  for case in [
    {name: 'out>', line: 'cd out', indexed: true}
    {name: '--help', line: 'cd help', indexed: true}
    {name: '~literal', line: 'cd literal', indexed: true}
    {name: 'native-only', line: 'cd unmatched native-only', indexed: false}
    {name: 'space directory', line: 'cd unmatched "space', indexed: false}
  ] {
    let path = ($root | path join $case.name)
    mkdir $path
    if $case.indexed { ^zoxide add -- $path }
    let matches = (complete-line $case.line)
    let target = ($matches | where display_override == $"($case.name)/")
    assert equal ($target | length) 1 $case.line
    assert equal $target.0.span {start: 0, end: ($case.line | str length --utf-8-bytes)}
    let actual = (^nu --no-config-file -c $"source ($integration | to nuon); ($target.0.value); $env.PWD" | str trim)
    assert equal $actual $path
  }

  let blocked = ($root | path join blocked)
  mkdir $blocked ($root | path join other blocked)
  ^zoxide add -- ($root | path join other blocked)
  ^chmod 000 $blocked
  let denied = (^nu --no-config-file -c $"source ($integration | to nuon); cd blocked; $env.PWD" | complete)
  ^chmod 700 $blocked
  assert ($denied.exit_code != 0)
  assert ($denied.stderr | str contains 'Permission denied')
  print 'Completion, replacement-span, quoting, directory-jump, and failure checks passed.'
} catch {|err|
  %cd $origin
  rm --recursive --force $root
  error make $err
}
%cd $origin
rm --recursive --force $root
