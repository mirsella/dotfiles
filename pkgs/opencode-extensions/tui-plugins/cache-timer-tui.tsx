/** @jsxImportSource @opentui/solid */
import { Plugin } from "@opencode/plugin/tui";
import { maxStackFor, buildSeed } from "../lib/cache-timer";
import { createSignal, Show } from "solid-js";
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

interface Config {
  enableAutoPrompt?: boolean;
  defaultDuration?: number;
  durations?: Record<string, number>;
  enabledProviders?: string[];
}
function config(): Config {
  const global =
    process.env.OPENCODE_CONFIG_DIR ??
    join(process.env.XDG_CONFIG_HOME ?? join(homedir(), ".config"), "opencode");
  let merged: Config = {};
  for (const path of [
    join(global, "cache-timer.json"),
    join(process.cwd(), ".opencode/cache-timer.json"),
  ]) {
    let text;
    try {
      text = readFileSync(path, "utf8");
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") continue;
      throw error;
    }
    const value = JSON.parse(text) as Config;
    if (typeof value !== "object" || value === null)
      throw new Error(`Invalid cache timer config: ${path}`);
    for (const duration of [
      value.defaultDuration,
      ...Object.values(value.durations ?? {}),
    ]) {
      if (
        duration !== undefined &&
        (!Number.isFinite(duration) || duration <= 30)
      )
        throw new Error(`Cache duration must exceed 30 seconds: ${path}`);
    }
    if (
      value.enabledProviders &&
      !value.enabledProviders.every((provider) => typeof provider === "string")
    )
      throw new Error(`Invalid cache provider list: ${path}`);
    if (
      value.enableAutoPrompt !== undefined &&
      typeof value.enableAutoPrompt !== "boolean"
    )
      throw new Error(`Invalid enableAutoPrompt: ${path}`);
    merged = {
      ...merged,
      ...value,
      durations: { ...merged.durations, ...value.durations },
    };
  }
  return merged;
}
const refreshPrompt = "Reply with just: ok";
export default Plugin.define({
  id: "local.cache-timer",
  setup(ctx) {
    const settings = config();
    const bank = new Map<string, number>();
    const sending = new Set<string>();
    const creating = new Set<string>();
    const warned = new Map<string, string>();
    const users = new Map<string, string>();
    const [now, setNow] = createSignal(Date.now());
    const toast = (error: unknown) =>
      ctx.ui.toast.show({
        variant: "error",
        message: `Cache timer: ${String(error)}`,
      });
    const snapshot = (id: string) => {
      const messages = ctx.data.session.message.list(id);
      const message = messages.findLast(
        (message) => message.type === "assistant",
      );
      if (
        !message ||
        message.type !== "assistant" ||
        !(settings.enabledProviders ?? []).some((provider) =>
          message.model.providerID
            .toLowerCase()
            .includes(provider.toLowerCase()),
        )
      )
        return;
      const duration =
        Object.entries(
          settings.durations ?? { claude: 300, gemini: 300, gpt: 300 },
        ).find(([family]) =>
          message.model.id.toLowerCase().includes(family.toLowerCase()),
        )?.[1] ??
        settings.defaultDuration ??
        300;
      return {
        messages,
        duration,
        remaining: Math.round(duration - (now() - message.time.created) / 1000),
        tokens: message.tokens
          ? message.tokens.input +
            message.tokens.cache.read +
            message.tokens.cache.write
          : 0,
      };
    };
    const pending = (id: string) =>
      !!ctx.data.session.permission.list(id)?.length ||
      !!ctx.data.session.form.list(id)?.length;
    const refresh = async (id: string) => {
      if (
        sending.has(id) ||
        pending(id) ||
        ctx.data.session.status(id) !== "idle"
      )
        return;
      sending.add(id);
      try {
        await ctx.client.session.prompt({ sessionID: id, text: refreshPrompt });
        bank.set(id, Math.max(0, (bank.get(id) ?? 0) - 1));
      } catch (error) {
        bank.delete(id);
        toast(error);
      } finally {
        sending.delete(id);
      }
    };
    const tick = () => {
      setNow(Date.now());
      for (const session of ctx.data.session.list()) {
        const id = session.id;
        const state = snapshot(id);
        if (!state) continue;
        const user = state.messages.findLast(
          (message) => message.type === "user",
        );
        if (user?.type === "user") {
          if (
            users.has(id) &&
            users.get(id) !== user.id &&
            user.text !== refreshPrompt
          )
            bank.delete(id);
          users.set(id, user.id);
        }
        if (pending(id)) {
          const warning =
            state.remaining <= 0
              ? "cold"
              : state.remaining < 60
                ? "expiring"
                : undefined;
          if (warning && warned.get(id) !== warning) {
            warned.set(id, warning);
            ctx.ui.toast.show({
              variant: "warning",
              message: `Cache ${warning}: a permission or question is awaiting your reply.`,
              duration: 10_000,
            });
          }
        } else warned.delete(id);
        if (
          state.remaining > 0 &&
          state.remaining <= 60 &&
          ((bank.get(id) ?? 0) > 0 || settings.enableAutoPrompt)
        )
          void refresh(id);
      }
      const ids = new Set(ctx.data.session.list().map((session) => session.id));
      for (const map of [bank, users, warned])
        for (const id of map.keys()) if (!ids.has(id as never)) map.delete(id);
    };
    const timer = setInterval(tick, 1000);
    const dispose = ctx.ui.slot({
      append: "prompt.footer.status",
      render: (input) => {
        const id = () => input.sessionID;
        const state = () => {
          now();
          return id() ? snapshot(id()!) : undefined;
        };
        const label = () => {
          const value = state();
          if (!value) return "";
          const remaining = Math.max(0, value.remaining);
          const time = `${Math.floor(remaining / 60)
            .toString()
            .padStart(2, "0")}:${(remaining % 60).toString().padStart(2, "0")}`;
          const count = bank.get(id()!) ?? 0;
          return `Cache: ${value.remaining <= 0 ? "COLD" : "HOT"} ${time}${count ? ` (total ${Math.ceil((remaining + count * value.duration) / 60)}min)` : ""}`;
        };
        const queue = (button?: number) => {
          const value = state();
          if (!value || !id()) return;
          if (button === 2) {
            bank.delete(id()!);
            tick();
            return;
          }
          const count = bank.get(id()!) ?? 0;
          const cap = maxStackFor(value.tokens);
          if (count >= cap) {
            ctx.ui.toast.show({
              variant: "warning",
              message: `Refresh limit: ${cap} (cache break-even budget)`,
            });
            return;
          }
          bank.set(id()!, count + 1);
          tick();
        };
        const fresh = async () => {
          const sourceID = id();
          if (!sourceID || creating.has(sourceID)) return;
          creating.add(sourceID);
          try {
            const source = ctx.data.session.get(sourceID);
            const seed = buildSeed(ctx.data.session.message.list(sourceID));
            if (!seed || !source)
              throw new Error("No last exchange for a fresh chat");
            if (ctx.data.session.status(sourceID) !== "idle")
              await ctx.client.session.interrupt({ sessionID: sourceID });
            bank.delete(sourceID);
            const next = await ctx.client.session.create({
              location: source.location,
            });
            if (source.agent)
              await ctx.client.session.switchAgent({
                sessionID: next.id,
                agent: source.agent,
              });
            if (source.model)
              await ctx.client.session.switchModel({
                sessionID: next.id,
                model: source.model,
              });
            await ctx.client.session.prompt({ sessionID: next.id, text: seed });
            await ctx.ui.router.navigate({ type: "session", sessionID: next.id });
          } finally {
            creating.delete(sourceID);
          }
        };
        return (
          <Show when={state()}>
            <box gap={1} flexDirection="row">
              <text
                fg={
                  (state()?.remaining ?? 0) <= 0
                    ? "#22C55E"
                    : (state()?.remaining ?? 0) < 60
                      ? "#FBBF24"
                      : "#EF4444"
                }
              >
                {label()}
              </text>
              <Show when={(state()?.remaining ?? 0) > 0}>
                <text onMouseUp={(event) => queue(event.button)}>[+]</text>
              </Show>
              <Show
                when={id() && buildSeed(ctx.data.session.message.list(id()!))}
              >
                <Show
                  when={
                    (state()?.remaining ?? 0) <= 0 &&
                    ctx.data.session.status(id()!) === "idle"
                  }
                >
                  <text onMouseUp={() => void fresh().catch(toast)}>
                    [New chat]
                  </text>
                </Show>
              </Show>
              <Show
                when={
                  id() &&
                  (state()?.remaining ?? 0) <= 0 &&
                  ctx.data.session.status(id()!) !== "idle"
                }
              >
                <text onMouseUp={() => void fresh().catch(toast)}>
                  [Interrupt]
                </text>
              </Show>
            </box>
          </Show>
        );
      },
    });
    return () => {
      clearInterval(timer);
      dispose();
    };
  },
});
