export type CreditStatus = {
  route: "promotional" | "subscription";
  organization_id: string;
};

export function parseCreditStatus(value: unknown, organization: string): CreditStatus {
  if (
    !value || typeof value !== "object" ||
    !("route" in value) || !["promotional", "subscription"].includes(String(value.route)) ||
    !("organization_id" in value) || value.organization_id !== organization
  ) throw new Error("Credit router returned an invalid status or a different organization");
  return value as CreditStatus;
}

export function creditEndpoint(settings: Record<string, unknown>, upstream: string, router: string): string {
  if (settings.baseURL !== upstream || settings.apiKey !== undefined || settings.authToken !== undefined) {
    throw new Error("Credit routing requires the existing Sleev subscription endpoint and native login; leaving provider unchanged");
  }
  return router;
}

export type Interruption = { at: string; native_session_id: string | null };

/** Promotional responses the router saw the vendor end for lack of credit. */
export function parseInterruptions(status: unknown): Interruption[] {
  const list = status && typeof status === "object" && "interrupted" in status ? status.interrupted : undefined;
  if (!Array.isArray(list)) throw new Error("Credit router status has no interruption list");
  return list.filter((entry): entry is Interruption =>
    !!entry && typeof entry === "object" && typeof entry.at === "string" && Number.isFinite(Date.parse(entry.at)) &&
    (entry.native_session_id === null || typeof entry.native_session_id === "string"));
}

/**
 * Decides whether a failed turn is one the router saw cut off when the
 * promotional credits ran out. Each cut is retried once per session. Every
 * other failure, including a subscription billing error, is left alone.
 */
export function creditCutRetries(window = 120_000, capacity = 4_096) {
  const native = new Map<string, string>();
  const retried = new Map<string, string>();
  const remember = (map: Map<string, string>, key: string, value: string) => {
    map.delete(key);
    map.set(key, value);
    if (map.size > capacity) map.delete(map.keys().next().value!);
  };
  return {
    /** Records which Claude Code session an OpenCode session runs in. */
    bind(sessionID: unknown, nativeSessionID: unknown) {
      if (typeof sessionID === "string" && typeof nativeSessionID === "string") remember(native, sessionID, nativeSessionID);
    },
    retry(input: { sessionID: string; message: string; interrupted: Interruption[]; now: number }) {
      const known = native.get(input.sessionID);
      // A session with no recorded Claude Code session can only be matched by
      // the failure itself, so require the CLI's balance error in that case.
      if (known === undefined && !/credit balance is too low/i.test(input.message)) return false;
      const cut = input.interrupted.findLast((entry) => {
        const age = input.now - Date.parse(entry.at);
        return age >= 0 && age <= window && (known === undefined || entry.native_session_id === known);
      });
      if (!cut || retried.get(input.sessionID) === cut.at) return false;
      remember(retried, input.sessionID, cut.at);
      return true;
    },
  };
}
