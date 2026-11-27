import { Plugin } from "@opencode/plugin";
import type { SessionContext } from "@opencode/plugin/promise/session";

const modelID = "inkling-2-mini-uncensored";
const scope = { providerID: "llama.cpp" } as const;
const allowedTools = new Set(["read", "glob", "grep", "edit", "write", "shell", "websearch", "webfetch"]);
const integrationInstructions = new Set(["core/codemode", "core/mcp-guidance", "core/skill-guidance"]);
const maxResultChars = 12_000;
type ToolResult = Extract<SessionContext["messages"][number], { role: "tool" }>["content"][number];

const checkpointSections = [
  ["Objective", 160],
  ["Requirements", 240],
  ["Decisions", 240],
  ["Work State", 380],
  ["Next Move", 300],
  ["Relevant Files", 260],
  ["Important Context", 380],
] as const;

// Inkling generates its own message delimiters. Constrain those too, otherwise
// the chat parser loses the boundary between thinking and the visible summary.
const checkpointGrammar = [
  "root ::= thinking? <|message_model|> <|content_text|> summary <|end_message|> <|content_model_end_sampling|>?",
  "thinking ::= <|message_model|> <|content_thinking|> [^<]{0,420} <|end_message|>",
  `summary ::= ${checkpointSections.map(([name, limit], index) =>
    `${JSON.stringify(`${index ? "\n\n" : ""}## ${name}\n- `)} text{1,${limit}}`).join(" ")} "\\n"?`,
  String.raw`text ::= [^\r\n<]`,
].join("\n");
const checkpointInstructions =
  "\n\nUse exactly one short line per section, with no subsections. " +
  "Replace the old checkpoint rather than copying it. Prioritize unresolved user requests, decisions, " +
  "and exact references needed to resume. Write (none) for inapplicable sections.";

function excerpt(text: string) {
  if (text.length <= maxResultChars) return text;
  const head = maxResultChars / 2;
  return `${text.slice(0, head).toWellFormed()}\n[Local: ${text.length - maxResultChars} characters omitted. Use a narrower query or file range.]\n${text.slice(-head).toWellFormed()}`;
}

function trimResult(part: ToolResult): ToolResult {
  const result = part.result;
  if (result.type === "text" && result.value.length > maxResultChars) {
    return { ...part, result: { ...result, value: excerpt(result.value) } };
  }
  if (result.type === "content" && result.value.some((item) => item.type === "text" && item.text.length > maxResultChars)) {
    const value = result.value.map((item) => item.type === "text" && item.text.length > maxResultChars
      ? { ...item, text: excerpt(item.text) }
      : item);
    return { ...part, result: { ...result, value } };
  }
  return part;
}

function shapeLocalRequest(event: SessionContext, kind: "context" | "generate" | "compaction") {
  if (event.model.id !== modelID || event.model.providerID !== scope.providerID) return;
  for (const name of Object.keys(event.tools)) {
    if (kind === "compaction" || !allowedTools.has(name)) delete event.tools[name];
  }

  event.system = event.system.map((part) => {
    const sources = (part.metadata as { experimental?: { instructions?: unknown } } | undefined)
      ?.experimental?.instructions;
    if (!Array.isArray(sources)) return part;
    // Only remove exact source blocks, preserving project instructions and other hooks' edits.
    const delimited = sources.reduce((text, source) => {
      if (!source || !integrationInstructions.has(source.key) || typeof source.text !== "string") return text;
      return text.replace(`\n\n${source.text}\n\n`, "\n\n");
    }, `\n\n${part.text}\n\n`);
    return { ...part, text: delimited.slice(2, -2) };
  }).filter((part) => part.text.length > 0);

  // Replayed instruction updates can mix old catalogs with project rules and lack source keys.
  if (kind === "context" && event.messages.some((message) => message.role === "system")) {
    event.system.push({ text: "Use only tools available in this request. Disregard older guidance for unavailable tools." });
  }

  // Keep call IDs, attachment entries, and full stored output. Only the model sees excerpts.
  event.messages = event.messages.map((message) => {
    if (message.role === "tool") return { ...message, content: message.content.map(trimResult) };
    if (message.role === "assistant" && message.content.some((part) => part.type === "tool-result")) return {
      ...message,
      content: message.content.map((part) => part.type === "tool-result" ? trimResult(part) : part),
    };
    return message;
  });
}

export default Plugin.define({
  id: "local.inkling-compaction",
  async setup(ctx) {
    for (const kind of ["context", "generate", "compaction"] as const) {
      await ctx.session.hook(kind, async (event) => shapeLocalRequest(event, kind), scope);
    }
    await ctx.session.hook("http.request", async (event) => {
      if (event.kind !== "compaction" || event.model.id !== modelID) return;
      const request = event.request;
      const url = new URL(request.url);
      if (url.origin !== "http://127.0.0.1:8080" || url.pathname !== "/v1/chat/completions") return;

      const body = await request.clone().json();
      // OpenCode adds its summary instructions after the context hook runs.
      const prompt = Array.isArray(body.messages) ? body.messages.at(-1) : undefined;
      if (prompt?.role !== "user") throw new Error("Local Inkling compaction request has no final user prompt");
      if (typeof prompt.content === "string") {
        prompt.content += checkpointInstructions;
      } else if (Array.isArray(prompt.content)) {
        prompt.content.push({ type: "text", text: checkpointInstructions });
      } else {
        throw new Error("Local Inkling compaction prompt must contain text or content parts");
      }
      const headers = new Headers(request.headers);
      headers.delete("content-length");
      event.request = new Request(request, {
        headers,
        body: JSON.stringify({ ...body, reasoning_effort: "low", grammar: checkpointGrammar }),
      });
    }, scope);
  },
});
