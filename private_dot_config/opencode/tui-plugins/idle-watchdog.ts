import { mkdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import type { TuiPluginModule } from "@opencode-ai/plugin/tui";

export default {
	id: "local.idle-watchdog",
	async tui(api) {
		const runtime = process.env.XDG_RUNTIME_DIR;
		if (!runtime) throw new Error("XDG_RUNTIME_DIR is required");
		const directory = join(runtime, "opencode-idle-watchdog");
		const target = join(directory, `${process.pid}.json`);
		const temporary = `${target}.tmp`;
		const stat = await readFile(`/proc/${process.pid}/stat`, "utf8");
		const started = stat.slice(stat.lastIndexOf(")") + 2).split(" ")[19];
		if (!started) throw new Error("could not read process start time");
		await mkdir(directory, { mode: 0o700, recursive: true });

		let disposed = false;
		let pending = false;
		let refreshing = false;
		let writing = Promise.resolve();
		const refresh = () => {
			if (disposed) return;
			if (refreshing) {
				pending = true;
				return;
			}
			refreshing = true;
			writing = (async () => {
				const route = api.route.current;
				const selected = route.name === "session" && route.params && "sessionID" in route.params
					? route.params.sessionID as string
					: undefined;
				const response = await api.client.session.status();
				if (response.error) throw new Error("could not read session status");
				const status = { ...response.data };
				if (selected && !status[selected]) status[selected] = { type: "idle" };
				const snapshots = new Map<string, {
					status: Record<string, unknown>;
					sessions: Record<string, unknown>;
					assistants: Record<string, unknown>;
					questions: unknown[];
					permissions: unknown[];
				}>();
				for (const [sessionID, sessionStatus] of Object.entries(status)) {
					const session = api.state.session.get(sessionID);
					if (!session) throw new Error(`missing session metadata for ${sessionID}`);
					const workspace = session.directory;
					const snapshot = snapshots.get(workspace) ?? {
						status: {}, sessions: {}, assistants: {}, questions: [], permissions: [],
					};
					snapshot.status[sessionID] = sessionStatus;
					snapshot.sessions[sessionID] = session;
					const assistant = api.state.session.messages(sessionID).findLast((message) => message.role === "assistant");
					if (assistant) snapshot.assistants[sessionID] = { info: assistant, parts: api.state.part(assistant.id) };
					snapshot.questions.push(...api.state.session.question(sessionID));
					snapshot.permissions.push(...api.state.session.permission(sessionID));
					snapshots.set(workspace, snapshot);
				}
				await writeFile(temporary, JSON.stringify({
					schema: 1,
					pid: process.pid,
					started,
					updatedAt: Date.now(),
					snapshots: [...snapshots].map(([workspace, snapshot]) => ({ workspace, ...snapshot })),
				}), { mode: 0o600 });
				await rename(temporary, target);
			})().catch(async (error) => {
				await rm(target, { force: true });
				console.error("[idle-watchdog]", error);
			}).finally(() => {
				refreshing = false;
				if (pending) {
					pending = false;
					refresh();
				}
			});
		};
		refresh();
		const timer = setInterval(refresh, 1_000);
		const unsubscribe = [
			api.event.on("session.status", refresh),
			api.event.on("question.asked", refresh),
			api.event.on("question.replied", refresh),
			api.event.on("permission.asked", refresh),
			api.event.on("permission.replied", refresh),
			api.event.on("tui.session.select", refresh),
		];
		api.lifecycle.onDispose(async () => {
			disposed = true;
			clearInterval(timer);
			unsubscribe.forEach((stop) => { stop(); });
			await writing;
			await Promise.all([rm(target, { force: true }), rm(temporary, { force: true })]);
		});
	},
} satisfies TuiPluginModule;
