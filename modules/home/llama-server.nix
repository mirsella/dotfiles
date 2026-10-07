model:
{ config, lib, pkgs, hostName, isNixOS, ... }:
let
  contextSize = 65536;
  preset = (pkgs.formats.ini { }).generate "llama-server-models.ini" {
    ${model.alias} = {
      model = "${config.home.homeDirectory}/dev/models/${model.alias}/${model.revision}/${model.file}";
      model-url = "https://huggingface.co/${model.repo}/resolve/${model.revision}/${model.file}";
      load-on-startup = false;
      device = "Vulkan0";
      n-gpu-layers = "auto";
      fit = "on";
      ctx-size = contextSize;
      parallel = 1;
      flash-attn = "on";
      cache-type-k = "q8_0";
      cache-type-v = "q8_0";
      jinja = true;
      reasoning = "off";
      cache-ram = 0;
      sleep-idle-seconds = 300;
    };
  };
in
{
  # OpenCode merges this generated provider with chezmoi's editable opencode.jsonc.
  xdg.configFile."opencode/opencode.json".source = (pkgs.formats.json { }).generate "opencode-local-llama.json" {
    "$schema" = "https://opencode.ai/config.json";
    provider."llama.cpp" = {
      npm = "@ai-sdk/openai-compatible";
      name = "Local llama.cpp";
      options.baseURL = "http://127.0.0.1:8080/v1";
      models.${model.alias} = {
        name = model.alias;
        tool_call = true;
        reasoning = true;
        interleaved.field = "reasoning_content";
        options.reasoning_format = "deepseek";
        variants = {
          thinking.chat_template_kwargs.enable_thinking = true;
          # Qwen uses a thinking toggle, not OpenAI's effort levels.
          low.disabled = true;
          medium.disabled = true;
          high.disabled = true;
        };
        limit = { context = contextSize; output = 8192; };
        modalities = { input = [ "text" ]; output = [ "text" ]; };
      };
    };
  };

  # The router lists the model at login; only an inference request loads its weights.
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
      # Keep unrelated cached GGUFs (such as Handy's speech model) out of this router.
      CacheDirectory = "llama-server";
      Environment = [ "LLAMA_CACHE=%C/llama-server" ]
        ++ lib.optional (!isNixOS) "VK_DRIVER_FILES=/usr/share/vulkan/icd.d/radeon_icd.json";
      ExecStart = lib.escapeShellArgs [
        (if isNixOS then lib.getExe' (pkgs.llama-cpp.override { vulkanSupport = true; }) "llama-server"
         else "/usr/bin/llama-server")
        "--models-preset" "${preset}"
        "--models-max" "1"
        "--models-autoload"
        "--host" "127.0.0.1"
        "--port" "8080"
        "--no-ui"
        "--cors-origins" "http://127.0.0.1:8080"
      ];
      # Wait for the inference API to be ready before OpenCode starts.
      ExecStartPost = lib.escapeShellArgs [
        (if isNixOS then lib.getExe pkgs.curl else "/usr/bin/curl")
        "--fail" "--silent" "--show-error"
        "--retry" "15" "--retry-connrefused" "--retry-delay" "1"
        "--retry-max-time" "20" "--max-time" "2"
        "http://127.0.0.1:8080/health"
      ];
      TimeoutStartSec = "30s";
      Restart = "on-failure";
      RestartSec = 10;
    };
    Install.WantedBy = [ "default.target" ];
  };

  systemd.user.services.opencode.Unit = {
    Wants = [ "llama-server.service" ];
    After = [ "llama-server.service" ];
  };
}
