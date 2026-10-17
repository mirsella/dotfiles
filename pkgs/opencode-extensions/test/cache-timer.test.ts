import { expect, test } from "bun:test";
import type { SessionMessageInfo } from "@opencode/client";
import { maxStackFor, buildSeed } from "../lib/cache-timer";
test("banked warm calls stay below the cold-resume budget", () => {
  for (const tokens of [1, 64, 1024, 16000, 100000, 500000]) {
    const count = maxStackFor(tokens);
    const callCost = 0.1 * tokens + 12 * 6;
    expect(count * callCost).toBeLessThanOrEqual(0.75 * 1.15 * tokens);
    expect((count + 1) * callCost).toBeGreaterThan(0.75 * 1.15 * tokens);
  }
});
test("fresh-chat seed uses a coherent last exchange and five distinct recent reads", () => {
  const messages = [
    { type: "user", id: "user-old", text: "Older request" },
    {
      type: "assistant",
      id: "assistant-old",
      content: [{ type: "text", text: "Older reply" }],
    },
    { type: "user", id: "user-current", text: "Current request" },
    {
      type: "assistant",
      id: "assistant-current",
      content: [
        ...["a", "b", "c", "d", "e", "f", "f"].map((path) => ({
          type: "tool",
          name: "read",
          state: { status: "completed", input: { filePath: path } },
        })),
        { type: "text", text: "Current reply" },
      ],
    },
    { type: "user", id: "unanswered", text: "Not yet answered" },
  ] as unknown as SessionMessageInfo[];
  const seed = buildSeed(messages)!;
  expect(seed).toContain("Current request");
  expect(seed).toContain("Current reply");
  expect(seed).not.toContain("Older reply");
  expect(seed).not.toContain("Not yet answered");
  expect(seed.match(/^- /gm)?.length).toBe(5);
  expect(seed).not.toContain("- a");
  expect(buildSeed(messages.slice(0, 1))).toBeUndefined();
});
