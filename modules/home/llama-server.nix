{ quant, hash }:
{
  lib,
  pkgs,
  hostName,
  isNixOS,
  ...
}:
let
  alias = "inkling-2-mini-uncensored";
  host = "127.0.0.1";
  port = "8080";
  baseURL = "http://${host}:${port}";
  # Checksummed models belong to the Nix closure and are fetched before activation.
  modelFile = pkgs.fetchurl {
    url = "https://huggingface.co/AtomicChat/Inkling-2-Mini-Uncensored-GGUF/resolve/40886ab636cabb4decf4998224f790b462ab048e/Inkling-2-Mini-Uncensored-AD-${quant}.gguf";
    inherit hash;
  };
  preset = (pkgs.formats.ini { }).generate "llama-server-models.ini" {
    ${alias} = {
      model = "${modelFile}";
      load-on-startup = false;
      device = "Vulkan0";
      # Fail visibly on insufficient GPU memory instead of spilling into CPU math.
      n-gpu-layers = "all";
      fit = "off";
      ctx-size = 16384;
      parallel = 1;
      flash-attn = "on";
      cache-type-k = "q8_0";
      cache-type-v = "q8_0";
      jinja = true;
      reasoning = "on";
      reasoning-effort = "low";
      cache-ram = 0;
      sleep-idle-seconds = 300;
    };
  };
in
{
  # The router lists the model at login; only an inference request loads its weights.
  systemd.user.services.llama-server = {
    Unit = {
      Description = "Local ${alias} inference (Vulkan/RADV)";
      ConditionHost = hostName;
      # Bound router restart loops; model failures are reported by the inference API.
      StartLimitIntervalSec = "5min";
      StartLimitBurst = 3;
    };
    Service = {
      Type = "exec";
      # Keep unrelated cached GGUFs (such as Handy's speech model) out of this router.
      CacheDirectory = "llama-server";
      Environment = [
        "LLAMA_CACHE=%C/llama-server"
      ];
      ExecStart = lib.escapeShellArgs [
        (lib.getExe' pkgs.llama-vulkan "llama-server")
        "--models-preset"
        "${preset}"
        "--models-max"
        "1"
        "--models-autoload"
        "--host"
        host
        "--port"
        port
        "--no-ui"
        "--cors-origins"
        baseURL
      ];
      # Wait for the inference API to be ready before OpenCode starts.
      ExecStartPost = lib.escapeShellArgs [
        (if isNixOS then lib.getExe pkgs.curl else "/usr/bin/curl")
        "--fail"
        "--silent"
        "--show-error"
        "--retry"
        "30"
        "--retry-connrefused"
        "--retry-delay"
        "1"
        "--max-time"
        "2"
        "${baseURL}/health"
      ];
      # systemd owns the total readiness deadline; curl bounds each probe.
      TimeoutStartSec = "30s";
      Restart = "on-failure";
      RestartSec = 10;
    };
    Install.WantedBy = [ "default.target" ];
  };

}
