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
