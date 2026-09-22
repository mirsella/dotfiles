{ lib, hostName, ... }:
let
  models = {
    main = {
      alias = "qwen3.6-27b-ablit";
      repo = "sci4ai/Qwen3.6-27B-Ablit-IQ4_XS-GGUF";
      revision = "fabccbe97a30bcb3013b7689776d7fc9425e4f4e";
      file = "Qwen3.6-27B-Ablit-IQ4_XS.gguf";
    };
    laptop = {
      alias = "qwen3.6-35b-a3b-abliterated-v4";
      repo = "Bahushruth/Qwen3.6-35B-A3B-abliterated-v4-GGUF";
      revision = "b8a9ab20c8bde880621a7bab65073c0078ef33f7";
      file = "Qwen3.6-35B-A3B-abliterated-v4-IQ3_M.gguf";
    };
  };
  model = models.${hostName};
  modelDir = "%h/dev/models/${model.alias}/${model.revision}";
in
{
  # Arch owns llama-cpp, ggml-vulkan and Mesa; Home Manager owns this unit.
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
      Environment = "VK_DRIVER_FILES=/usr/share/vulkan/icd.d/radeon_icd.json";
      ExecStart = lib.escapeShellArgs [
        "/usr/bin/llama-server"
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
