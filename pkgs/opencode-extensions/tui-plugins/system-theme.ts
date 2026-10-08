import type { TuiPluginModule } from "@opencode-ai/plugin/tui";
import { Database } from "bun:sqlite";
import { execFile, spawn } from "node:child_process";
import { mkdirSync, watch } from "node:fs";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { setTimeout } from "node:timers/promises";
import { promisify } from "node:util";

const execute = promisify(execFile);
const portal = [
  "--session",
  "--dest", "org.freedesktop.portal.Desktop",
  "--object-path", "/org/freedesktop/portal/desktop",
];

export default {
  id: "local.system-theme",
  async tui(api) {
    const controller = new AbortController();
    const { signal } = controller;
    const report = (error: unknown) => {
      if (signal.aborted) return;
      console.error("[system-theme]", error);
      api.ui.toast({
        title: "System theme",
        message: error instanceof Error ? error.message : String(error),
        variant: "error",
      });
    };

    const directory = api.state.path.state;
    mkdirSync(directory, { recursive: true });
    const file = join(directory, "system-theme.sqlite");
    const db = new Database(file);
    api.lifecycle.onDispose(() => {
      controller.abort();
      db.close();
    });
    db.run("PRAGMA busy_timeout = 1000");
    db.run(`CREATE TABLE IF NOT EXISTS theme (
      id INTEGER PRIMARY KEY CHECK (id = 1),
      system INTEGER NOT NULL CHECK (system IN (0, 1)),
      dark INTEGER NOT NULL CHECK (dark IN (0, 1))
    )`);
    const selection = db.query<{ dark: number }, []>("SELECT dark FROM theme WHERE id = 1");
    const toggle = db.query("UPDATE theme SET dark = 1 - dark WHERE id = 1");
    // The conditional upsert preserves manual choices on startup and duplicate
    // signals, even when other TUIs are updating the same row concurrently.
    const followSystem = db.query(`INSERT INTO theme (id, system, dark) VALUES (1, ?1, ?1)
      ON CONFLICT (id) DO UPDATE SET system = excluded.system, dark = excluded.dark
      WHERE theme.system <> excluded.system`);
    const apply = () => {
      if (signal.aborted || !api.theme.ready) return;
      try {
        const current = selection.get();
        if (!current) return; // No portal reading yet.
        const theme = current.dark ? "rosepine-moon" : "rosepine-dawn";
        if (api.theme.selected !== theme && !api.theme.set(theme)) {
          throw new Error(`Theme is not available: ${theme}`);
        }
      } catch (error) {
        report(error);
      }
    };

    // SQLite's default rollback journal writes this file without replacing it.
    const watcher = watch(file, apply);
    watcher.on("error", report);
    api.lifecycle.onDispose(() => watcher.close());
    void (async () => {
      // A single startup wait applies the latest selection after theme discovery.
      while (!api.theme.ready) await setTimeout(25, undefined, { signal });
      apply();
    })().catch(report);
    if (api.command) {
      api.lifecycle.onDispose(api.command.register(() => [{
        title: "Toggle theme in all TUIs",
        value: "theme.toggle-all",
        slash: { name: "theme-toggle" },
        onSelect: () => {
          api.ui.dialog.clear();
          try {
            if (!toggle.run().changes) throw new Error("System theme is still initializing; try again shortly.");
            apply();
          } catch (error) {
            report(error);
          }
        },
      }]));
    } else {
      report(new Error("This OpenCode build has no TUI command API; /theme-toggle is unavailable."));
    }

    const env = { ...process.env, LC_ALL: "C" };
    const monitor = spawn("gdbus", ["monitor", ...portal], {
      env,
      stdio: ["ignore", "pipe", "pipe"],
    });
    const lines = createInterface({ input: monitor.stdout });
    let stderr = "";
    let monitorError: Error | undefined;
    monitor.stderr.setEncoding("utf8");
    monitor.stderr.on("data", (chunk) => { stderr += chunk; });
    monitor.on("error", (error) => { monitorError = error; });
    monitor.on("close", (code, exitSignal) => {
      report(monitorError ?? new Error(`Color-scheme monitor stopped (${exitSignal ?? code}): ${stderr.trim()}`));
    });
    api.lifecycle.onDispose(() => {
      lines.close();
      monitor.kill();
    });

    // Subscribe before reading, and consume notifications in their bus order.
    void (async () => {
      for await (const line of lines) {
        if (signal.aborted) break;
        try {
          let payload = line;
          if (line.startsWith("The name org.freedesktop.portal.Desktop ")) {
            // Read at startup and when the portal restarts. A read also activates
            // the portal if the monitor reports that it has no owner yet.
            const { stdout } = await execute("gdbus", [
              "call", ...portal,
              "--method", "org.freedesktop.portal.Settings.Read",
              "org.freedesktop.appearance", "color-scheme",
            ], { env, signal, timeout: 5000 });
            payload = stdout.trim();
          } else if (!line.includes("org.freedesktop.portal.Settings.SettingChanged ('org.freedesktop.appearance', 'color-scheme',")) {
            continue;
          }

          const value = payload.match(/\buint32 (\d+)\b/)?.[1];
          if (value !== "0" && value !== "1" && value !== "2") {
            throw new Error(`Unexpected portal color scheme: ${payload}`);
          }
          // XDG: 1 = dark, 2 = light, 0 = no preference (use Dawn).
          followSystem.run(value === "1" ? 1 : 0);
          apply();
        } catch (error) {
          report(error);
        }
      }
    })().catch(report);
  },
} satisfies TuiPluginModule;
