def charge-limit [limit?: int] {
  sudo framework_tool --charge-limit ...([$limit] | compact)
}
