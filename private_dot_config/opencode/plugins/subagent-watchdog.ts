import type { Plugin } from "@opencode-ai/plugin";
import type {
	Event as RuntimeEvent,
	SessionStatus as OpenCodeSessionStatus,
} from "@opencode-ai/sdk/v2";
import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, join } from "node:path";

export interface WatchdogConfig {
	enabled: boolean;
	suspectAfterMs: number;
	recoverAfterMs: number;
	toolRecoverAfterMs: number;
	pollIntervalMs: number;
	abortWaitMs: number;
	maxRecoveriesPerChild: number;
	recoverParallelChildren: boolean;
	mode: "report" | "auto";
}

export const DEFAULT_CONFIG: WatchdogConfig = {
	enabled: true,
	suspectAfterMs: 60_000,
	recoverAfterMs: 180_000,
	toolRecoverAfterMs: 10 * 60_000,
	pollIntervalMs: 5_000,
	abortWaitMs: 15_000,
	maxRecoveriesPerChild: 1,
	recoverParallelChildren: false,
	mode: "auto",
};

export type SessionStatus = OpenCodeSessionStatus;

export interface SessionInfo {
	id: string;
	parentID?: string;
	time: { updated: number };
}

type WatchdogSnapshot = {
	sessions: SessionInfo[];
	statuses: Record<string, SessionStatus>;
	pending: Set<string>;
};
type TaskState = {
	callID: string;
	status: "running" | "completed" | "error";
	latest: boolean;
};

type PromptContext = {
	agent?: string;
	model?: { providerID: string; modelID: string };
	variant?: string;
};

export interface WatchdogApi {
	snapshot(): Promise<WatchdogSnapshot>;
	getStatuses(): Promise<Record<string, SessionStatus>>;
	getTaskState(parentSessionID: string, childSessionID: string): Promise<TaskState | undefined>;
	abort(sessionID: string): Promise<void>;
	promptAsync(
		sessionID: string,
		prompt: string,
		context?: PromptContext,
	): Promise<void>;
}

type RecoveryReservation = { attempt: number; rollback(): Promise<void> };
export interface RecoveryCounter {
	count(childSessionID: string): number;
	reserve(
		childSessionID: string,
		limit: number,
		now: number,
	): Promise<RecoveryReservation | undefined>;
	cleanup(now: number): Promise<void>;
}

type PersistedRecovery = { recoveryCount: number; lastRecoveryAt: number };
type Log = (
	level: "debug" | "info" | "warn" | "error",
	event: string,
	extra?: Record<string, unknown>,
) => Promise<void>;
type Notify = (
	message: string,
	variant?: "info" | "success" | "warning" | "error",
) => Promise<void>;

const STATE_RETENTION_MS = 30 * 24 * 60 * 60_000;
const STATE_CLEANUP_INTERVAL_MS = 24 * 60 * 60_000;
const TASK_RETENTION_MS = 60 * 60_000;
const WAIT_POLL_MS = 250;
const IDLE_STATUS: SessionStatus = { type: "idle" };

const isRecord = (value: unknown): value is Record<string, unknown> =>
	typeof value === "object" && value !== null && !Array.isArray(value);
const errorText = (error: unknown): string =>
	error instanceof Error ? error.message : String(error);
const statusOf = (
	statuses: Record<string, SessionStatus>,
	sessionID: string,
): SessionStatus => statuses[sessionID] ?? IDLE_STATUS;
const isActive = (status: SessionStatus): boolean =>
	status.type === "busy" || status.type === "retry";
const statusKey = (status: SessionStatus): string =>
	status.type === "retry"
		? `retry:${status.attempt}:${status.message}:${status.next}`
		: status.type;

export function normalizeConfig(value: unknown): {
	config: WatchdogConfig;
	warnings: string[];
} {
	const config = { ...DEFAULT_CONFIG };
	const warnings: string[] = [];
	if (!isRecord(value))
		return { config, warnings: ["configuration must be a JSON object"] };

	for (const key of ["enabled", "recoverParallelChildren"] as const) {
		if (!(key in value)) continue;
		if (typeof value[key] === "boolean") config[key] = value[key];
		else warnings.push(`${key} must be a boolean; using the default`);
	}
	for (const key of [
		"suspectAfterMs",
		"recoverAfterMs",
		"toolRecoverAfterMs",
		"pollIntervalMs",
		"abortWaitMs",
	] as const) {
		if (!(key in value)) continue;
		const candidate = value[key];
		if (typeof candidate === "number" && Number.isInteger(candidate) && candidate > 0)
			config[key] = candidate;
		else warnings.push(`${key} must be a positive integer; using the default`);
	}
	if ("maxRecoveriesPerChild" in value) {
		const candidate = value.maxRecoveriesPerChild;
		if (typeof candidate === "number" && Number.isInteger(candidate) && candidate >= 0)
			config.maxRecoveriesPerChild = candidate;
		else
			warnings.push(
				"maxRecoveriesPerChild must be a non-negative integer; using the default",
			);
	}
	if ("mode" in value) {
		if (value.mode === "report" || value.mode === "auto") config.mode = value.mode;
		else warnings.push('mode must be "report" or "auto"; using the default');
	}
	if (config.recoverAfterMs < config.suspectAfterMs) {
		warnings.push(
			"recoverAfterMs is lower than suspectAfterMs; using suspectAfterMs for recovery",
		);
		config.recoverAfterMs = config.suspectAfterMs;
	}
	if (config.toolRecoverAfterMs < config.recoverAfterMs) {
		warnings.push(
			"toolRecoverAfterMs is lower than recoverAfterMs; using recoverAfterMs for tools",
		);
		config.toolRecoverAfterMs = config.recoverAfterMs;
	}
	return { config, warnings };
}

async function loadConfig(path: string) {
	try {
		return normalizeConfig(JSON.parse(await readFile(path, "utf8")));
	} catch (error) {
		if (isRecord(error) && error.code === "ENOENT")
			return { config: { ...DEFAULT_CONFIG }, warnings: [] };
		return {
			config: { ...DEFAULT_CONFIG },
			warnings: [`failed to read ${path}: ${errorText(error)}; using defaults`],
		};
	}
}

export class RecoveryStore implements RecoveryCounter {
	private readonly records = new Map<string, PersistedRecovery>();
	private writes = Promise.resolve();

	private constructor(
		private readonly path: string,
		private readonly warn: (message: string) => Promise<void>,
	) {}

	static async open(
		path: string,
		now = Date.now(),
		warn: (message: string) => Promise<void> = async () => {},
	): Promise<RecoveryStore> {
		const store = new RecoveryStore(path, warn);
		try {
			const parsed: unknown = JSON.parse(await readFile(path, "utf8"));
			if (!isRecord(parsed)) throw new Error("state must be a JSON object");
			for (const [child, entry] of Object.entries(parsed)) {
				if (
					!isRecord(entry) ||
					typeof entry.recoveryCount !== "number" ||
					!Number.isInteger(entry.recoveryCount) ||
					entry.recoveryCount < 0 ||
					typeof entry.lastRecoveryAt !== "number"
				)
					throw new Error(`invalid recovery record for ${child}`);
				if (now - entry.lastRecoveryAt <= STATE_RETENTION_MS)
					store.records.set(child, {
						recoveryCount: entry.recoveryCount,
						lastRecoveryAt: entry.lastRecoveryAt,
					});
			}
		} catch (error) {
			if (isRecord(error) && error.code === "ENOENT") return store;
			await warn(`failed to load ${path}: ${errorText(error)}; recovery is disabled`);
			throw error;
		}
		return store;
	}

	count(childSessionID: string): number {
		return this.records.get(childSessionID)?.recoveryCount ?? 0;
	}

	async reserve(
		childSessionID: string,
		limit: number,
		now: number,
	): Promise<RecoveryReservation | undefined> {
		let reservation: RecoveryReservation | undefined;
		await this.update(() => {
			const previous = this.records.get(childSessionID);
			const recoveryCount = (previous?.recoveryCount ?? 0) + 1;
			if (recoveryCount > limit) return false;
			const current = { recoveryCount, lastRecoveryAt: now };
			this.records.set(childSessionID, current);
			reservation = {
				attempt: recoveryCount,
				rollback: () =>
					this.update(() => {
						if (this.records.get(childSessionID) !== current) return false;
						if (previous) this.records.set(childSessionID, previous);
						else this.records.delete(childSessionID);
						return true;
					}),
			};
			return true;
		});
		return reservation;
	}

	async cleanup(now: number): Promise<void> {
		await this.update(() => {
			let changed = false;
			for (const [child, entry] of this.records) {
				if (now - entry.lastRecoveryAt <= STATE_RETENTION_MS) continue;
				this.records.delete(child);
				changed = true;
			}
			return changed;
		});
	}

	private async update(change: () => boolean): Promise<void> {
		const operation = this.writes.then(async () => {
			const previous = new Map(this.records);
			try {
				if (!change()) return;
				await mkdir(dirname(this.path), { recursive: true });
				const temporary = `${this.path}.tmp-${process.pid}`;
				await writeFile(
					temporary,
					`${JSON.stringify(Object.fromEntries(this.records), null, 2)}\n`,
					{ mode: 0o600 },
				);
				await rename(temporary, this.path);
			} catch (error) {
				this.records.clear();
				for (const entry of previous) this.records.set(...entry);
				throw error;
			}
		});
		this.writes = operation.catch(() => undefined);
		try {
			await operation;
		} catch (error) {
			await this.warn(`failed to persist ${this.path}: ${errorText(error)}`);
			throw error;
		}
	}
}

type TaskInvocation = {
	parentID: string;
	callID: string;
	createdAt: number;
	childID?: string;
	subagentType?: string;
	description?: string;
	model?: string;
	variant?: string;
	background: boolean;
};

type Notice = "suspect" | "parallel" | "limit" | "report";
type Child = {
	id: string;
	parentID: string;
	updatedAt: number;
	activityAt: number;
	activeTools: Set<string>;
	notices: Set<Notice>;
	task?: TaskInvocation;
};
type Recovery = {
	childID: string;
	decidedAt: number;
	statusKey: string;
	blockedCallID?: string;
	promptedAt?: number;
	callID?: string;
};
type RecoverySnapshot = {
	childStatus: SessionStatus;
	activeChildren: string[];
};

function makeRecoveryPrompt(child: Child): string {
	return [
		"A subagent used by your previous turn became stuck, so that turn was interrupted automatically.",
		"",
		"Resume the EXISTING subagent task below and then continue the work you were doing.",
		"",
		"Existing task_id:",
		child.id,
		...(child.task?.subagentType
			? ["", "Subagent type:", child.task.subagentType]
			: ["", "Reuse the original subagent type associated with this task."]),
		...(child.task?.description
			? ["", "Original task:", child.task.description]
			: []),
		"",
		"You MUST reuse this exact task_id when calling the task tool.",
		"Do not create a replacement subagent.",
		"Do not restart the task from scratch.",
		"Continue from the existing subagent context.",
		"",
		"After the resumed subagent finishes, continue the original parent task normally.",
	].join("\n");
}

export class SubagentWatchdog {
	private readonly children = new Map<string, Child>();
	private readonly tasks = new Map<string, TaskInvocation>();
	private readonly promptContexts = new Map<string, PromptContext>();
	private readonly recoveries = new Map<string, Recovery>();
	private statuses: Record<string, SessionStatus> = {};
	private pending = new Set<string>();
	private timer?: ReturnType<typeof setInterval>;
	private ticking = false;
	private pollFailed = false;
	private lastCleanupAt = 0;

	constructor(
		private readonly config: WatchdogConfig,
		private readonly api: WatchdogApi,
		private readonly store: RecoveryCounter,
		private readonly log: Log = async () => {},
		private readonly notify: Notify = async () => {},
		private readonly now: () => number = Date.now,
		private readonly sleep: (ms: number) => Promise<void> = (ms) =>
			new Promise((resolve) => setTimeout(resolve, ms)),
	) {}

	async start(): Promise<void> {
		await this.tick();
		this.timer = setInterval(() => void this.tick(), this.config.pollIntervalMs);
		this.timer.unref?.();
	}

	stop(): void {
		if (this.timer) clearInterval(this.timer);
		this.timer = undefined;
	}

	recordActivity(sessionID: string, at = this.now()): void {
		const child = this.children.get(sessionID);
		if (!child) return;
		child.activityAt = Math.max(child.activityAt, at);
		child.notices.clear();
	}

	recordPromptContext(sessionID: string, context: PromptContext): void {
		this.promptContexts.set(sessionID, context);
		this.recordActivity(sessionID);
	}

	async handleToolBefore(
		input: { tool: string; sessionID: string; callID: string },
		output: { args: unknown },
	): Promise<void> {
		this.recordActivity(input.sessionID);
		this.children.get(input.sessionID)?.activeTools.add(input.callID);
		if (input.tool !== "task" || !isRecord(output.args)) return;
		const args = output.args;

		const recovery = this.recoveries.get(input.sessionID);
		if (
			recovery?.promptedAt !== undefined &&
			(!recovery.callID || recovery.callID === input.callID)
		) {
			recovery.callID = input.callID;
			const child = this.children.get(recovery.childID);
			const original = child?.task;
			const previousTaskID = args.task_id;
			const previousType = args.subagent_type;
			args.task_id = recovery.childID;
			if (original?.subagentType) args.subagent_type = original.subagentType;
			if (original?.model) args.model = original.model;
			if (original?.variant) args.variant = original.variant;
			args.background = false;
			this.recordActivity(recovery.childID);
			await this.log("info", "watchdog.child.resume_started", {
				child: recovery.childID,
				parent: input.sessionID,
				callID: input.callID,
				taskIDCorrected: previousTaskID !== recovery.childID,
				subagentTypeCorrected:
					original?.subagentType !== undefined && previousType !== original.subagentType,
			});
		}

		const stringArg = (key: string) =>
			typeof args[key] === "string" ? args[key] : undefined;
		const childID = stringArg("task_id");
		const task: TaskInvocation = {
			parentID: input.sessionID,
			callID: input.callID,
			createdAt: this.now(),
			childID,
			subagentType: stringArg("subagent_type"),
			description: stringArg("description"),
			model: stringArg("model"),
			variant: stringArg("variant"),
			background: args.background === true,
		};
		this.tasks.set(this.taskKey(input.sessionID, input.callID), task);
		if (childID) this.attachTask(task, childID);
	}

	async handleToolAfter(
		input: { tool: string; sessionID: string; callID: string },
		output: { metadata?: unknown },
	): Promise<void> {
		this.recordActivity(input.sessionID);
		this.children.get(input.sessionID)?.activeTools.delete(input.callID);
		if (input.tool !== "task") return;

		try {
			const metadata = isRecord(output.metadata) ? output.metadata : {};
			const childID =
				typeof metadata.sessionId === "string" ? metadata.sessionId : undefined;
			if (childID)
				await this.associateTask(input.sessionID, input.callID, childID, metadata);
			else
				await this.failRecovery(
					input.sessionID,
					"task returned no child session ID",
					input.callID,
				);
		} finally {
			const key = this.taskKey(input.sessionID, input.callID);
			const task = this.tasks.get(key);
			if (!task?.childID || this.children.has(task.childID)) this.tasks.delete(key);
		}
	}

	async handleEvent(event: RuntimeEvent): Promise<void> {
		switch (event.type) {
			case "session.created":
			case "session.updated": {
				const { info } = event.properties;
				this.upsertChild(info, true);
				const parentID = info.parentID;
				const recovery = parentID ? this.recoveries.get(parentID) : undefined;
				if (
					event.type === "session.created" &&
					parentID &&
					recovery?.promptedAt !== undefined &&
					info.id !== recovery.childID
				)
					await this.failRecovery(
						parentID,
						`replacement child ${info.id} was created`,
						recovery.callID,
					);
				return;
			}
			case "session.deleted":
				await this.removeSession(event.properties.info.id);
				return;
			case "session.status":
				this.setStatus(event.properties.sessionID, event.properties.status);
				return;
			case "session.idle":
				this.setStatus(event.properties.sessionID, IDLE_STATUS);
				return;
			case "message.updated":
				this.recordActivity(event.properties.sessionID);
				return;
			case "message.part.updated": {
				const { part } = event.properties;
				this.recordActivity(part.sessionID);
				if (part.type !== "tool") return;
				const child = this.children.get(part.sessionID);
				if (child) {
					if (part.state.status === "running" || part.state.status === "pending")
						child.activeTools.add(part.callID);
					else child.activeTools.delete(part.callID);
				}
				const metadata =
					"metadata" in part.state && isRecord(part.state.metadata)
						? part.state.metadata
						: {};
				if (
					part.tool === "task" &&
					typeof metadata.sessionId === "string" &&
					(part.state.status === "running" || part.state.status === "completed")
				)
					await this.associateTask(
						part.sessionID,
						part.callID,
						metadata.sessionId,
						metadata,
						part.state.time.start,
					);
				return;
			}
			case "permission.asked":
			case "permission.v2.asked":
			case "question.asked":
			case "question.v2.asked": {
				const { sessionID } = event.properties;
				const wasPending = this.pending.has(sessionID);
				this.pending.add(sessionID);
				this.recordActivity(sessionID);
				const child = this.children.get(sessionID);
				if (!wasPending && child)
					await this.log("info", "watchdog.child.waiting_for_user", {
						child: sessionID,
						parent: child.parentID,
					});
				return;
			}
			case "permission.replied":
			case "permission.v2.replied":
			case "question.replied":
			case "question.rejected":
			case "question.v2.replied":
			case "question.v2.rejected":
				this.pending.delete(event.properties.sessionID);
				this.recordActivity(event.properties.sessionID);
				return;
			case "session.error":
				if (event.properties.sessionID) {
					this.recordActivity(event.properties.sessionID);
					if (this.recoveries.has(event.properties.sessionID))
						await this.failRecovery(
							event.properties.sessionID,
							"parent session errored before the saved child was resumed",
						);
				}
				return;
		}
	}

	async tick(): Promise<void> {
		if (this.ticking) return;
		this.ticking = true;
		try {
			await this.synchronize();
			await this.checkRecoveries();
			if (this.pollFailed) {
				this.pollFailed = false;
				await this.log("info", "watchdog.poll.recovered");
			}
			await this.evaluate();
			const now = this.now();
			if (now - this.lastCleanupAt >= STATE_CLEANUP_INTERVAL_MS) {
				this.lastCleanupAt = now;
				await this.store.cleanup(now);
			}
			for (const [key, task] of this.tasks)
				if (now - task.createdAt > TASK_RETENTION_MS) this.tasks.delete(key);
		} catch (error) {
			if (!this.pollFailed) {
				this.pollFailed = true;
				await this.log("error", "watchdog.poll.failed", { error: errorText(error) });
			}
		} finally {
			this.ticking = false;
		}
	}

	private async synchronize(): Promise<void> {
		const { sessions, statuses, pending } = await this.api.snapshot();
		const liveChildren = new Set<string>();
		for (const info of sessions) {
			if (info.parentID) liveChildren.add(info.id);
			this.upsertChild(info, false);
		}
		for (const childID of this.children.keys())
			if (!liveChildren.has(childID)) await this.removeSession(childID);
		for (const child of this.children.values()) {
			if (statusKey(statusOf(this.statuses, child.id)) !== statusKey(statusOf(statuses, child.id)))
				this.recordActivity(child.id);
			const waiting = pending.has(child.id);
			if (waiting !== this.pending.has(child.id)) {
				this.recordActivity(child.id);
				if (waiting)
					await this.logChild("info", "watchdog.child.waiting_for_user", child);
			}
		}
		this.statuses = statuses;
		this.pending = pending;
	}

	private async evaluate(): Promise<void> {
		const recoveries: Promise<void>[] = [];
		const now = this.now();
		for (const child of this.children.values()) {
			const status = statusOf(this.statuses, child.id);
			if (
				!isActive(status) ||
				this.pending.has(child.id) ||
				this.pending.has(child.parentID) ||
				child.task?.background
			)
				continue;
			if (this.recoveries.has(child.parentID)) continue;

			const activityAt = Math.max(
				child.activityAt,
				status.type === "retry" ? status.next : 0,
			);
			const idleForMs = now - activityAt;
			if (idleForMs < this.config.suspectAfterMs) continue;
			if (!child.notices.has("suspect")) {
				child.notices.add("suspect");
				await this.logChild("warn", "watchdog.child.suspect", child, {
					idleForMs,
					activeTools: [...child.activeTools],
				});
				await this.notify(
					`Subagent ${child.id} has made no progress for ${Math.round(idleForMs / 1_000)}s`,
					"warning",
				);
			}

			const threshold = child.activeTools.size
				? this.config.toolRecoverAfterMs
				: this.config.recoverAfterMs;
			if (idleForMs < threshold) continue;
			if (this.store.count(child.id) >= this.config.maxRecoveriesPerChild) {
				await this.reportLimit(child);
				continue;
			}
			if (this.config.mode === "report") {
				if (!child.notices.has("report")) {
					child.notices.add("report");
					await this.logChild("warn", "watchdog.child.recovery_required", child, {
						idleForMs,
					});
				}
				continue;
			}

			this.recoveries.set(child.parentID, {
				childID: child.id,
				decidedAt: child.activityAt,
				statusKey: statusKey(status),
				blockedCallID: child.task?.callID,
			});
			recoveries.push(this.recover(child));
		}
		await Promise.all(recoveries);
	}

	private async recover(child: Child): Promise<void> {
		const recovery = this.recoveries.get(child.parentID);
		if (!recovery || recovery.childID !== child.id) return;
		let reservation: RecoveryReservation | undefined;
		let abortRequested = false;
		try {
			let snapshot = await this.revalidate(child, recovery);
			if (!snapshot) return await this.cancelRecovery(child, recovery);
			if (await this.skipParallel(child, recovery, snapshot.activeChildren)) return;

			reservation = await this.store.reserve(
				child.id,
				this.config.maxRecoveriesPerChild,
				this.now(),
			);
			if (!reservation) {
				this.recoveries.delete(child.parentID);
				await this.reportLimit(child);
				return;
			}

			snapshot = await this.revalidate(child, recovery);
			if (!snapshot) {
				await reservation.rollback();
				return await this.cancelRecovery(child, recovery);
			}
			if (
				!this.config.recoverParallelChildren &&
				snapshot.activeChildren.length > 1
			) {
				await reservation.rollback();
				await this.skipParallel(child, recovery, snapshot.activeChildren);
				return;
			}

			await this.api.abort(child.parentID);
			abortRequested = true;
			await this.logChild("warn", "watchdog.child.recovery_started", child, {
				attempt: reservation.attempt,
				childStatusBeforeAbort: snapshot.childStatus.type,
				activeChildren: snapshot.activeChildren,
				lastActivityAt: child.activityAt,
				taskModel: child.task?.model,
				taskVariant: child.task?.variant,
			});
			await this.notify(`Recovering stalled subagent ${child.id}`, "warning");

			if (!(await this.waitForIdle(child.parentID)))
				return await this.failRecovery(child.parentID, "parent did not become idle");

			const [{ sessions, statuses, pending }, taskState] = await Promise.all([
				this.api.snapshot(),
				this.api.getTaskState(child.parentID, child.id),
			]);
			const freshChild = sessions.find(({ id }) => id === child.id);
			const freshParent = sessions.find(({ id }) => id === child.parentID);
			if (
				this.recoveries.get(child.parentID) !== recovery ||
				!freshChild ||
				!freshParent ||
				freshChild.parentID !== child.parentID ||
				isActive(statusOf(statuses, child.id)) ||
				statusOf(statuses, child.parentID).type !== "idle" ||
				!taskState ||
				taskState.callID !== recovery.blockedCallID ||
				taskState.status !== "error" ||
				!taskState.latest ||
				pending.has(child.id) ||
				pending.has(child.parentID)
			)
				return await this.failRecovery(
					child.parentID,
					"state changed after parent abort",
				);

			recovery.promptedAt = this.now();
			await this.api.promptAsync(
				child.parentID,
				makeRecoveryPrompt(child),
				this.promptContexts.get(child.parentID),
			);
			await this.logChild("info", "watchdog.child.recovery_prompted", child, {
				attempt: reservation.attempt,
			});
		} catch (error) {
			if (reservation && !abortRequested) {
				try {
					await reservation.rollback();
				} catch (rollbackError) {
					error = new Error(
						`${errorText(error)}; reservation rollback failed: ${errorText(rollbackError)}`,
					);
				}
			}
			await this.failRecovery(child.parentID, errorText(error));
		}
	}

	private async revalidate(
		child: Child,
		recovery: Recovery,
	): Promise<RecoverySnapshot | undefined> {
		const [{ sessions, statuses, pending }, taskState] = await Promise.all([
			this.api.snapshot(),
			this.api.getTaskState(child.parentID, child.id),
		]);
		const freshChild = sessions.find(({ id }) => id === child.id);
		const freshParent = sessions.find(({ id }) => id === child.parentID);
		if (this.recoveries.get(child.parentID) !== recovery) return;
		if (freshChild && freshChild.time.updated > child.updatedAt) {
			child.updatedAt = freshChild.time.updated;
			this.recordActivity(child.id, freshChild.time.updated);
			return;
		}
		const childStatus = statusOf(statuses, child.id);
		const parentStatus = statusOf(statuses, child.parentID);
		if (!recovery.blockedCallID) recovery.blockedCallID = taskState?.callID;
		if (
			!freshChild ||
			!freshParent ||
			freshChild.parentID !== child.parentID ||
			child.activityAt !== recovery.decidedAt ||
			statusKey(childStatus) !== recovery.statusKey ||
			!taskState ||
			taskState.callID !== recovery.blockedCallID ||
			!taskState.latest ||
			taskState.status !== "running" ||
			!isActive(childStatus) ||
			parentStatus.type !== "busy" ||
			(childStatus.type === "retry" && childStatus.next > this.now()) ||
			pending.has(child.id) ||
			pending.has(child.parentID) ||
			child.task?.background === true
		)
			return;
		return {
			childStatus,
			activeChildren: sessions
				.filter(
					(candidate) =>
						candidate.parentID === child.parentID &&
						isActive(statusOf(statuses, candidate.id)),
				)
				.map((candidate) => candidate.id),
		};
	}

	private async skipParallel(
		child: Child,
		recovery: Recovery,
		activeChildren: string[],
	): Promise<boolean> {
		if (this.config.recoverParallelChildren || activeChildren.length <= 1) return false;
		this.recoveries.delete(child.parentID);
		if (!child.notices.has("parallel")) {
			child.notices.add("parallel");
			await this.logChild("warn", "watchdog.child.parallel_skip", child, {
				activeChildren,
			});
			await this.notify(
				`Subagent ${child.id} stalled, but ${activeChildren.length} children are active`,
				"warning",
			);
		}
		return true;
	}

	private async reportLimit(child: Child): Promise<void> {
		if (child.notices.has("limit")) return;
		child.notices.add("limit");
		await this.logChild("error", "watchdog.child.recovery_limit", child, {
			recoveryCount: this.store.count(child.id),
		});
		await this.notify(
			`Subagent ${child.id} stalled again; automatic recovery is disabled`,
			"error",
		);
	}

	private async cancelRecovery(child: Child, recovery: Recovery): Promise<void> {
		if (this.recoveries.get(child.parentID) !== recovery) return;
		this.recoveries.delete(child.parentID);
		await this.logChild("info", "watchdog.child.recovery_cancelled", child, {
			reason: "state changed during revalidation",
		});
	}

	private async waitForIdle(parentID: string): Promise<boolean> {
		const deadline = this.now() + this.config.abortWaitMs;
		while (this.now() < deadline) {
			if (statusOf(await this.api.getStatuses(), parentID).type === "idle")
				return true;
			await this.sleep(Math.min(WAIT_POLL_MS, Math.max(0, deadline - this.now())));
		}
		return statusOf(await this.api.getStatuses(), parentID).type === "idle";
	}

	private async checkRecoveries(): Promise<void> {
		for (const [parentID, recovery] of [...this.recoveries]) {
			if (recovery.promptedAt === undefined) continue;
			const age = this.now() - recovery.promptedAt;
			if (age >= this.config.recoverAfterMs)
				await this.failRecovery(
					parentID,
					"parent did not resume the saved child before the recovery deadline",
					recovery.callID,
				);
		}
	}

	private async associateTask(
		parentID: string,
		callID: string,
		childID: string,
		metadata: Record<string, unknown>,
		startedAt?: number,
	): Promise<void> {
		const recovery = this.recoveries.get(parentID);
		const confirmsRecovery =
			recovery?.promptedAt !== undefined &&
			callID !== recovery.blockedCallID &&
			(recovery.callID === callID ||
				(!recovery.callID && startedAt !== undefined && startedAt >= recovery.promptedAt));
		if (confirmsRecovery && childID !== recovery.childID) {
			await this.failRecovery(
				parentID,
				`task resumed ${childID} instead of ${recovery.childID}`,
				callID,
			);
			return;
		}

		const task = this.tasks.get(this.taskKey(parentID, callID));
		if (task) {
			task.childID = childID;
			task.model ??= this.modelFrom(metadata.model);
			this.attachTask(task, childID);
		}
		if (!confirmsRecovery) return;

		recovery.callID = callID;
		this.recoveries.delete(parentID);
		this.recordActivity(childID);
		await this.log("info", "watchdog.child.recovered", {
			child: childID,
			parent: parentID,
			callID,
			reusedTaskID: true,
		});
		await this.notify(`Resumed existing subagent ${childID}`, "success");
	}

	private attachTask(task: TaskInvocation, childID: string): void {
		const child = this.children.get(childID);
		if (!child) return;
		child.task = task;
	}

	private async failRecovery(
		parentID: string,
		reason: string,
		callID?: string,
	): Promise<void> {
		const recovery = this.recoveries.get(parentID);
		if (!recovery || (callID && recovery.callID && callID !== recovery.callID)) return;
		this.recoveries.delete(parentID);
		await this.log("error", "watchdog.child.recovery_failed", {
			child: recovery.childID,
			parent: parentID,
			reason,
			callID,
		});
		await this.notify(`Failed to recover subagent ${recovery.childID}: ${reason}`, "error");
	}

	private async removeSession(sessionID: string): Promise<void> {
		this.children.delete(sessionID);
		this.pending.delete(sessionID);
		this.promptContexts.delete(sessionID);
		delete this.statuses[sessionID];
		if (this.recoveries.has(sessionID))
			await this.failRecovery(sessionID, "parent session was deleted");
		for (const [parentID, recovery] of [...this.recoveries])
			if (recovery.childID === sessionID)
				await this.failRecovery(parentID, "child session was deleted");
		for (const [key, task] of this.tasks)
			if (task.parentID === sessionID || task.childID === sessionID) this.tasks.delete(key);
	}

	private upsertChild(info: SessionInfo, eventActivity: boolean): Child | undefined {
		if (!info.parentID) return;
		const existing = this.children.get(info.id);
		if (!existing) {
			const child: Child = {
				id: info.id,
				parentID: info.parentID,
				updatedAt: info.time.updated,
				activityAt: eventActivity ? this.now() : info.time.updated,
				activeTools: new Set(),
				notices: new Set(),
				task: [...this.tasks.values()].find((task) => task.childID === info.id),
			};
			this.children.set(info.id, child);
			return child;
		}
		existing.parentID = info.parentID;
		if (eventActivity || info.time.updated > existing.updatedAt) {
			existing.updatedAt = Math.max(existing.updatedAt, info.time.updated);
			this.recordActivity(info.id, eventActivity ? this.now() : info.time.updated);
		}
		return existing;
	}

	private setStatus(sessionID: string, status: SessionStatus): void {
		if (statusKey(statusOf(this.statuses, sessionID)) !== statusKey(status))
			this.recordActivity(sessionID);
		this.statuses[sessionID] = status;
	}

	private modelFrom(value: unknown): string | undefined {
		if (typeof value === "string") return value;
		if (!isRecord(value)) return;
		return typeof value.providerID === "string" && typeof value.modelID === "string"
			? `${value.providerID}/${value.modelID}`
			: undefined;
	}

	private logChild(
		level: Parameters<Log>[0],
		event: string,
		child: Child,
		extra: Record<string, unknown> = {},
	): Promise<void> {
		return this.log(level, event, { child: child.id, parent: child.parentID, ...extra });
	}

	private taskKey(parentID: string, callID: string): string {
		return `${parentID}:${callID}`;
	}
}

type Client = Parameters<Plugin>[0]["client"];

function responseData<T>(
	response: { data?: T; error?: unknown },
	operation: string,
): T {
	if (response.error !== undefined) throw new Error(`${operation}: ${errorText(response.error)}`);
	if (response.data === undefined) throw new Error(`${operation}: response contained no data`);
	return response.data;
}

function createApi(client: Client, directory: string): WatchdogApi {
	const query = { directory };
	type RawClient = {
		get(options: {
			url: string;
			query: { directory: string };
		}): Promise<{ data?: unknown; error?: unknown }>;
	};
	const raw = (client as unknown as { _client?: RawClient })._client;
	if (!raw) throw new Error("OpenCode client does not expose its request client");
	const pendingInteractions = async () => {
		const responses = await Promise.all([
			raw.get({ url: "/permission", query }),
			raw.get({ url: "/question", query }),
		]);
		const pending = new Set<string>();
		for (const response of responses) {
			const entries = responseData(response, "list pending interactions");
			if (!Array.isArray(entries))
				throw new Error("pending interaction response was not an array");
			for (const entry of entries)
				if (isRecord(entry) && typeof entry.sessionID === "string")
					pending.add(entry.sessionID);
		}
		return pending;
	};
	const getStatuses = async () =>
		responseData(await client.session.status({ query }), "get session statuses") as Record<
			string,
			SessionStatus
		>;
	return {
		snapshot: async () => {
			const [sessions, statuses, pending] = await Promise.all([
				client.session.list({ query }),
				getStatuses(),
				pendingInteractions(),
			]);
			return {
				sessions: responseData(sessions, "list sessions") as SessionInfo[],
				statuses,
				pending,
			};
		},
		getStatuses,
		getTaskState: async (parentSessionID, childSessionID) => {
			const messages = responseData(
				await client.session.messages({
					path: { id: parentSessionID },
					query: { ...query, limit: 100 },
				}),
				"list parent messages",
			);
			for (let messageIndex = messages.length - 1; messageIndex >= 0; messageIndex--) {
				const parts = messages[messageIndex].parts;
				for (let partIndex = parts.length - 1; partIndex >= 0; partIndex--) {
					const part = parts[partIndex];
					if (
						part.type === "tool" &&
						part.tool === "task" &&
						"metadata" in part.state &&
						isRecord(part.state.metadata) &&
						part.state.metadata.sessionId === childSessionID
					)
						return {
							callID: part.callID,
							status: part.state.status,
							latest: messageIndex === messages.length - 1,
						};
				}
			}
		},
		abort: async (sessionID) => {
			const aborted = responseData(
				await client.session.abort({ path: { id: sessionID }, query }),
				"abort parent session",
			);
			if (aborted !== true) throw new Error("server did not confirm parent abort");
		},
		promptAsync: async (sessionID, prompt, context) => {
			const response = await client.session.promptAsync({
				path: { id: sessionID },
				query,
				body: {
					parts: [{ type: "text", text: prompt }],
					...(context?.agent ? { agent: context.agent } : {}),
					...(context?.model ? { model: context.model } : {}),
					...(context?.variant ? { variant: context.variant } : {}),
				},
			});
			if (response.error !== undefined)
				throw new Error(`prompt parent session: ${errorText(response.error)}`);
		},
	};
}

const configPath = join(
	process.env.XDG_CONFIG_HOME ?? join(homedir(), ".config"),
	"opencode/subagent-watchdog.json",
);
const statePath = join(
	process.env.XDG_STATE_HOME ?? join(homedir(), ".local/state"),
	"opencode/subagent-watchdog.json",
);
const globals = globalThis as typeof globalThis & {
	__opencodeSubagentWatchdogStores?: Map<string, Promise<RecoveryStore>>;
};
const sharedStores =
	globals.__opencodeSubagentWatchdogStores ?? new Map<string, Promise<RecoveryStore>>();
globals.__opencodeSubagentWatchdogStores = sharedStores;

export const SubagentWatchdogPlugin: Plugin = async ({ client, directory }) => {
	const log: Log = async (level, event, extra = {}) => {
		await client.app
			.log({
				query: { directory },
				body: {
					service: "subagent-watchdog",
					level,
					message: event,
					extra: { event, ...extra },
				},
			})
			.catch(() => undefined);
	};
	const notify: Notify = async (message, variant = "warning") => {
		await client.tui
			.showToast({
				query: { directory },
				body: { title: "Subagent watchdog", message, variant, duration: 6_000 },
			})
			.catch(() => undefined);
	};
	const loaded = await loadConfig(configPath);
	for (const warning of loaded.warnings)
		await log("warn", "watchdog.config.invalid", { warning });
	if (!loaded.config.enabled) {
		await log("info", "watchdog.disabled");
		return {};
	}

	let storePromise = sharedStores.get(statePath);
	if (!storePromise) {
		storePromise = RecoveryStore.open(statePath, Date.now(), (warning) =>
			log("error", "watchdog.state.invalid", { warning }),
		);
		sharedStores.set(statePath, storePromise);
	}
	const watchdog = new SubagentWatchdog(
		loaded.config,
		createApi(client, directory),
		await storePromise,
		log,
		notify,
	);
	await watchdog.start();
	await log("info", "watchdog.started", {
		mode: loaded.config.mode,
		suspectAfterMs: loaded.config.suspectAfterMs,
		recoverAfterMs: loaded.config.recoverAfterMs,
		toolRecoverAfterMs: loaded.config.toolRecoverAfterMs,
	});

	const guarded = async (operation: string, callback: () => Promise<void>) => {
		try {
			await callback();
		} catch (error) {
			await log("error", "watchdog.hook.failed", {
				operation,
				error: errorText(error),
			});
		}
	};
	return {
		dispose: async () => watchdog.stop(),
		event: async ({ event }) =>
			guarded("event", () => watchdog.handleEvent(event as unknown as RuntimeEvent)),
		"chat.message": async (input) => {
			watchdog.recordPromptContext(input.sessionID, {
				agent: input.agent,
				model: input.model,
				variant: input.variant,
			});
		},
		"tool.execute.before": async (input, output) =>
			guarded("tool.execute.before", () => watchdog.handleToolBefore(input, output)),
		"tool.execute.after": async (input, output) =>
			guarded("tool.execute.after", () => watchdog.handleToolAfter(input, output)),
	};
};
