# Decode literal quotes without evaluating shell expressions or parsing bare words.
export def unquote [word: string] {
  if $word =~ r#'^["'`]'# {
    $"[($word)]" | from nuon | first
  } else {
    $word
  }
}
