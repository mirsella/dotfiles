# Local inference on Arch

Home Manager declares `llama-server.service` for `main` and `laptop` in
`modules/home/llama-server.nix`. The flake passes the hostname to select the model;
the unit's `ConditionHost` prevents starting it on the wrong machine. Predator
does not import this module. Pacman owns the native binaries and GPU driver:

```sh
sudo pacman -S --needed llama-cpp ggml-vulkan vulkan-radeon vulkan-icd-loader
```

Build and activate the matching Home Manager profile. Use `main` instead of
`laptop` on the desktop:

```sh
nix build --no-update-lock-file .#homeConfigurations.laptop.activationPackage
./result/activate
systemctl --user start llama-server
journalctl --user -u llama-server -f
```

The service starts on request. Its first start downloads the selected GGUF
into `~/dev/models/<alias>/<revision>/`. Each URL pins an immutable repository
revision. llama.cpp handles resumable downloads, publishing the completed file
and reusing it on subsequent starts. Model files stay outside the dotfiles
repository and Nix store. Changing the model in Nix requires a rebuild and
`systemctl --user restart llama-server`.

Systemd limits starts to three within five minutes. Rapid loading or GPU
failures leave the service failed instead of looping. After correcting the
cause, use `systemctl --user reset-failed llama-server` and start it again.

| Host | Model | Quantization | Download |
| --- | --- | --- | --- |
| main | [sci4ai Qwen3.6-27B-Ablit](https://huggingface.co/sci4ai/Qwen3.6-27B-Ablit-IQ4_XS-GGUF/tree/fabccbe97a30bcb3013b7689776d7fc9425e4f4e) | IQ4_XS | 15,082,505,568 bytes |
| laptop | [Bahushruth Qwen3.6-35B-A3B-abliterated-v4](https://huggingface.co/Bahushruth/Qwen3.6-35B-A3B-abliterated-v4-GGUF/tree/b8a9ab20c8bde880621a7bab65073c0078ef33f7) | IQ3_M | 15,440,519,328 bytes |

The desktop uses the original author's quantized release rather than its
53.8 GB BF16 file. The laptop's MoE model activates about 3B parameters per
token, but all 35B weights still need memory or file-backed storage.

Expected SHA-256 values, also published as Hugging Face's LFS hashes:

```text
3207de1c8fa4f500203148fa2db68e6cd8394e94ee1f5f152ab9590d73e93239  Qwen3.6-27B-Ablit-IQ4_XS.gguf
70ab5f42ff0952deda29fc80f337bf4b3ffb3e0b906c0a1f718d84c9817595da  Qwen3.6-35B-A3B-abliterated-v4-IQ3_M.gguf
```

The service selects RADV explicitly because this laptop also has AMD's
proprietary Vulkan driver installed. llama.cpp fits GPU offload to available
memory; some layers can remain on the CPU. The service uses one 65,536-token
slot, Flash Attention and Q8 KV caches, enables Jinja tool calling, and defaults
to non-thinking responses. Clients can enable thinking per request. The separate
multi-slot RAM cache is disabled. After five idle
minutes llama-server unloads the model; the next inference request reloads it.
Use `systemctl --user stop llama-server` to stop the server completely.

Both model files declare a native 262,144-token context. The configured 64K
window includes instructions, conversation, tool results, reasoning and the
final answer. Increasing it reserves more KV-cache memory and can move more
model layers onto the CPU. Long prompts also take longer to process. The
discovery plugin reads the actual serving limit, so OpenCode picks up context
changes after a restart without a separate model-list edit.

The 128K setting exceeded RADV's available memory during startup on the laptop.
On main it loaded, but short-response generation fell from roughly 15 to 6
tokens/sec with automatic GPU fitting. The 64K setting is the daily-use target
for this hardware rather than the models' theoretical maximum.

## OpenCode

OpenCode does not natively discover arbitrary llama.cpp model lists. The
chezmoi-owned `plugins/local-llama.ts` config hook requests `/v1/models` at
startup and registers the returned IDs and runtime context sizes under
`llama.cpp`. There is no model list to maintain in `opencode.jsonc`. Chezmoi
installs the plugin only on `main` and `laptop`.

Start llama-server and wait for `/health` to return HTTP 200 before starting
OpenCode. Select the discovered model with `/models`, or start a fresh session:

```sh
# On main:
opencode --model llama.cpp/qwen3.6-27b-ablit
# On laptop:
opencode --model llama.cpp/qwen3.6-35b-a3b-abliterated-v4
```

Discovery has a two-second timeout. A stopped or loading server produces an
OpenCode log warning while remote providers remain available. Restart OpenCode
after the server becomes ready or its selected model changes; discovery runs
at startup, not continuously. `--pure` disables the plugin and therefore local
discovery. Listing a sleeping server's model does not reload its weights.

The local model accepts text and tool calls. The plugin caps output, including
thinking tokens, at one quarter of the serving context, up to 8,192 tokens. Start a fresh conversation
when switching from a much larger-context model.

### Thinking

After restarting OpenCode to load the plugin, select the local model and press
`Ctrl+T` to toggle between `thinking` and the default. `thinking` enables
reasoning; the default leaves it off using the server's non-thinking setting.
OpenCode remembers the selected variant for each model.

For a terminal request:

```sh
# Laptop; substitute qwen3.6-27b-ablit on main.
opencode run --model llama.cpp/qwen3.6-35b-a3b-abliterated-v4 \
  --variant thinking --thinking "Explain how you would diagnose this bug."
```

`--variant thinking` enables generation of reasoning. `--thinking` only makes
the terminal display those reasoning blocks; it does not enable reasoning by
itself. The plugin sends Qwen's `chat_template_kwargs.enable_thinking` setting
and requests separate `reasoning_content` output. No llama-server restart or
model reload is needed to switch modes. Thinking consumes output tokens and
usually takes longer to finish.

## Verify and benchmark

```sh
curl --fail http://127.0.0.1:8080/health
curl --fail http://127.0.0.1:8080/v1/models
```

For a benchmark, stop the service first to avoid loading two copies. Substitute
the downloaded GGUF path and the server's fitted GPU-layer count:

```sh
systemctl --user stop llama-server
env VK_DRIVER_FILES=/usr/share/vulkan/icd.d/radeon_icd.json \
  llama-bench -m /path/to/model.gguf -ngl GPU_LAYER_COUNT
systemctl --user start llama-server
```

Forcing all layers with `-ngl 999` may exceed available VRAM. The Framework
laptop has a Radeon 780M with shared RAM; main has a 16 GiB RX 7800 XT.
