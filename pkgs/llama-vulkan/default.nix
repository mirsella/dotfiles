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
      version = "0.6.0";
      src = fetchFromGitHub {
        owner = "ggml-org";
        repo = "llama.cpp";
        rev = "d81235049384534c167caea52b85a694f6103d14";
        hash = "sha256-l6l6JIlIVTaVC6xh5M4fRHFtXsweQuugtkNTWHcZZF4=";
      };

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
        description = "Upstream llama.cpp with Vulkan and a matching Radeon driver";
        homepage = "https://github.com/ggml-org/llama.cpp";
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
