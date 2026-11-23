{
  lib,
  llama-cpp,
  fetchFromGitHub,
  nodejs_latest,
  npmHooks,
  mesa,
  makeWrapper,
  symlinkJoin,
}:

let
  engine = (llama-cpp.override { vulkanSupport = true; }).overrideAttrs (
    final: previous: {
      version = "polaris-2026-11-22";
      src = fetchFromGitHub {
        owner = "Lanilen";
        repo = "llama.cpp";
        rev = "7d6db83683b3a080cb9899d6bfeeaab1f0d52ef2";
        hash = "sha256-NnJoCmJ3d5JeB8SrgVVNCYUnJPIQmmdVWU1rkZY2+d8=";
      };
      # nixpkgs' test initialization patch is already in this newer source.
      patches = lib.filter (
        patch: !(lib.hasInfix "07ccce23cf7fe3c84e13d810accff672b1d966bf" patch)
      ) previous.patches;
      # This diagnostic links directly to ggml-cpu, which nixpkgs loads dynamically.
      postPatch = (previous.postPatch or "") + ''
        substituteInPlace tools/moe-trace/CMakeLists.txt \
          --replace-fail 'set(TARGET llama-mmid-glu-check)' $'if(NOT GGML_BACKEND_DL)\nset(TARGET llama-mmid-glu-check)'
        echo 'endif()' >> tools/moe-trace/CMakeLists.txt
      '';
      disabledTests = lib.unique (
        previous.disabledTests
        ++ [
          # CTest would otherwise run the disabled network fixture as a dependency.
          "test-llama-context-model"
          # Checkpoint conversion tests require NumPy and PyTorch.
          "test-conversion-fp8"
          "test-kv-cache-scales"
        ]
      );

      # OpenCode uses the API; omit UI assets and their npm build entirely.
      nativeBuildInputs = lib.subtractLists [
        nodejs_latest
        npmHooks.npmConfigHook
      ] previous.nativeBuildInputs;
      npmDeps = null;
      preConfigure = "";
      cmakeFlags =
        lib.filter (
          flag: !(lib.hasPrefix "-DLLAMA_BUILD_NUMBER" flag || lib.hasPrefix "-DLLAMA_BUILD_COMMIT" flag)
        ) previous.cmakeFlags
        ++ [
          (lib.cmakeBool "LLAMA_BUILD_UI" false)
          (lib.cmakeBool "LLAMA_USE_PREBUILT_UI" false)
          (lib.cmakeFeature "LLAMA_BUILD_NUMBER" "0")
          (lib.cmakeFeature "LLAMA_BUILD_COMMIT" final.src.rev)
        ];
      meta = previous.meta // {
        description = "Polaris llama.cpp with Vulkan and a matching Radeon driver";
        homepage = "https://github.com/Lanilen/llama.cpp/tree/polaris";
        mainProgram = "llama-server";
        platforms = lib.platforms.linux;
      };
    }
  );
in
symlinkJoin {
  name = "llama-vulkan-${engine.version}";
  inherit (engine) version meta;
  paths = [ engine ];
  nativeBuildInputs = [ makeWrapper ];
  # A Nix executable cannot resolve Arch's relative Radeon ICD library path.
  # Bundle the matching driver so the same runtime works on Arch and NixOS.
  postBuild = ''
    for program in llama-server llama-bench; do
      wrapProgram "$out/bin/$program" --set VK_DRIVER_FILES \
        "${mesa}/share/vulkan/icd.d/radeon_icd.x86_64.json"
    done
  '';
}
