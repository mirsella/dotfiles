model:
{ lib, pkgs, hostName, isNixOS, ... }:
let
  modelDir = "%h/dev/models/${model.alias}/${model.revision}";
in
{
  # Start explicitly so a login does not load several GiB of model weights.
  systemd.user.services.llama-server = {
    Unit = {
      Description = "Local ${model.alias} inference (Vulkan/RADV)";
      ConditionHost = hostName;
      # Stop repeated GPU failures instead of continuously loading and dumping cores.
      StartLimitIntervalSec = "5min";
      StartLimitBurst = 3;
    };
    Service = {
      Type = "exec";
      Environment = lib.mkIf (!isNixOS) "VK_DRIVER_FILES=/usr/share/vulkan/icd.d/radeon_icd.json";
      ExecStart = lib.escapeShellArgs [
        (if isNixOS then lib.getExe' (pkgs.llama-cpp.override { vulkanSupport = true; }) "llama-server"
         else "/usr/bin/llama-server")
        "--model" "${modelDir}/${model.file}"
        "--model-url" "https://huggingface.co/${model.repo}/resolve/${model.revision}/${model.file}"
        "--alias" model.alias
        "--host" "127.0.0.1"
        "--port" "8080"
        "--device" "Vulkan0"
        "--n-gpu-layers" "auto"
        "--fit" "on"
        "--ctx-size" "65536"
        "--parallel" "1"
        "--flash-attn" "on"
        "--cache-type-k" "q8_0"
        "--cache-type-v" "q8_0"
        "--jinja"
        "--reasoning" "off"
        "--cache-ram" "0"
        "--sleep-idle-seconds" "300"
        "--no-ui"
        "--cors-origins" "http://127.0.0.1:8080"
      ];
      Restart = "on-failure";
      RestartSec = 10;
    };
  };
}
