import type { SessionMessageInfo } from "@opencode/client";

export function maxStackFor(tokens: number): number {
  if (tokens <= 0) return 6;
  return Math.max(
    0,
    Math.floor((0.75 * 1.15 * tokens) / (0.1 * tokens + 12 * 6)),
  );
}
export function buildSeed(
  messages: readonly SessionMessageInfo[],
): string | undefined {
  const assistant = messages.findLast(
    (message) => message.type === "assistant",
  );
  if (!assistant || assistant.type !== "assistant") return;
  const user = messages
    .slice(0, messages.indexOf(assistant))
    .findLast((message) => message.type === "user");
  if (!user || user.type !== "user") return;
  const text = assistant.content
    .filter((part) => part.type === "text")
    .map((part) => part.text)
    .join("\n");
  const files: string[] = [];
  for (const message of messages.toReversed()) {
    if (message.type !== "assistant") continue;
    for (const part of message.content.toReversed()) {
      if (
        part.type !== "tool" ||
        part.name !== "read" ||
        part.state.status === "streaming"
      )
        continue;
      const path = part.state.input.filePath ?? part.state.input.path;
      if (typeof path === "string" && !files.includes(path) && files.length < 5)
        files.push(path);
    }
  }
  return `We are continuing from a previous work session. We do not yet know what the user wants to do next.\n\nThe last user request was:\n${user.text}\n\nThe agent responded:\n${text || "(no text content recorded)"}\n\nThe most recently referenced documents were:\n${files.map((path) => `- ${path}`).join("\n") || "(none recorded)"}\n\nAsk the user what they would like to work on next given this context.`;
}
