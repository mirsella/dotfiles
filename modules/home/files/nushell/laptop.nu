# Framework laptop only, installed via Home Manager when hostName == "laptop".

def charge-limit [limit?: int] {
  sudo framework_tool --charge-limit ...([$limit] | compact)
}
