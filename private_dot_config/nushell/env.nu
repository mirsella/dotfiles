let android_home = ($env.ANDROID_HOME? | default $"($env.HOME)/.local/share/android/sdk")
let cargo_home = ($env.CARGO_HOME? | default $"($env.HOME)/.local/share/cargo")
let go_path = ($env.GOPATH? | default $"($env.HOME)/.local/share/go")
let gem_home = ($env.GEM_HOME? | default $"($env.HOME)/.local/share/gem")
let pnpm_home = ($env.PNPM_HOME? | default $"($env.HOME)/.local/share/pnpm")
let xdg_cache_home = ($env.XDG_CACHE_HOME? | default $"($env.HOME)/.cache")

$env.GTRASH_HOME_TRASH_FALLBACK_COPY = "true"

for path in ([
  $"($android_home)/platform-tools"
  $"($android_home)/emulator"
  $"($android_home)/cmdline-tools/latest/bin"
] | reverse) {
  if not ($path in $env.PATH) {
    $env.PATH = [$path] ++ $env.PATH
  }
}

for path in [
  $"($env.HOME)/.local/bin"
  $"($gem_home)/bin"
  $"($env.HOME)/.local/share/npm/bin"
  $"($cargo_home)/bin"
  $"($go_path)/bin"
  $"($xdg_cache_home)/.bun/bin"
  $"($pnpm_home)/bin"
  $pnpm_home
] {
  if not ($path in $env.PATH) {
    $env.PATH = $env.PATH ++ [$path]
  }
}

if ("ANDROID_HOME" in $env) {
  let default_android_ndk_root = $"($env.ANDROID_HOME)/ndk/27.0.12077973"
  let configured_android_ndk_root = if ("ANDROID_NDK_HOME" in $env) { $env.ANDROID_NDK_HOME } else { $default_android_ndk_root }

  if ($configured_android_ndk_root | path exists) {
    $env.ANDROID_NDK_HOME = $configured_android_ndk_root
    $env.ANDROID_NDK_ROOT = $configured_android_ndk_root
    $env.NDK_HOME = $configured_android_ndk_root
  } else {
    let android_ndk_roots = (glob $"($env.ANDROID_HOME)/ndk/*" | sort --natural | reverse)
    if (($android_ndk_roots | length) > 0) {
      let android_ndk_root = ($android_ndk_roots | first)
      print --stderr $"ANDROID_NDK_HOME points to missing path '($configured_android_ndk_root)'; using '($android_ndk_root)'"
      $env.ANDROID_NDK_HOME = $android_ndk_root
      $env.ANDROID_NDK_ROOT = $android_ndk_root
      $env.NDK_HOME = $android_ndk_root
    } else {
      print --stderr $"ANDROID_NDK_HOME points to missing path '($configured_android_ndk_root)'; unsetting Android NDK environment"
      hide-env ANDROID_NDK_HOME ANDROID_NDK_ROOT NDK_HOME
    }
  }
} else {
  print --stderr "ANDROID_HOME is not set; skipping Android NDK environment"
}
