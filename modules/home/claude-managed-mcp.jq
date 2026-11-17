if ($dynamic | type) != "object" then
  error("Claude runtime configuration must be a JSON object")
elif $dynamic.mcpServers == null then
  # Missing and null maps have no managed entries to remove. In particular,
  # jaq's delpaths cannot traverse a missing parent; preserve runtime state.
  $dynamic
elif ($dynamic.mcpServers | type) != "object" then
  error("Claude mcpServers must be an object or null")
else
  $dynamic | delpaths($static | map(["mcpServers", .]))
end
