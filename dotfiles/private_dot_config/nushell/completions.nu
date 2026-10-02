$env.config.completions.algorithm = "fuzzy"
$env.config.completions.external.enable = true
$env.config.completions.external.completer = {|place token|
  use ./completion-strings.nu unquote

  ^fish --command 'complete --do-complete=$argv[1]' -- ($place.command | str join ' ')
  | from tsv --flexible --noheaders --no-infer
  | rename value description
  | append (
    $token.text | commandline complete --type path --detailed
    | update span $place.target
    | update value {|row| unquote $row.value }
  )
  | uniq-by value
  | update value {|row|
    let value = $row.value
    if ($value =~ '[^\w./~-]' and ($value | path exists)) {
      let expanded_path = if ($value starts-with ~) {$value | path expand --no-symlink} else {$value}
      $expanded_path | to nuon
    } else {$value}
  }
}
