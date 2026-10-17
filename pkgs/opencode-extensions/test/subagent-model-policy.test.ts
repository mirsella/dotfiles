import { expect, test } from "bun:test";
import plugin from "../plugins/subagent-model-policy";

async function hooks() {
  const handlers = new Map<string, (event: any) => void>();
  await plugin.setup({
    session: {
      hook: async (name: string, handler: (event: any) => void) => {
        handlers.set(name, handler);
      },
    },
  } as any);
  return handlers;
}

function context() {
  return {
    model: { providerID: "openai", id: "gpt-6-sol" },
    system: [{ type: "text", text: "Existing instructions" }],
    messages: [],
    tools: {
      subagent: {
        description: "Native delegation and completion instructions",
        input: {
          type: "object",
          properties: {
            agent: { type: "string" },
            description: { type: "string" },
            prompt: { type: "string" },
            model: {
              type: "string",
              description: "NEVER set this unless the user explicitly asks",
            },
            sessionID: { type: "string" },
            background: { type: "boolean" },
          },
          required: ["agent", "description", "prompt"],
          additionalProperties: false,
        },
      },
    },
  };
}

test("all native request phases permit model choice without changing execution or schema", async () => {
  const registered = await hooks();
  for (const handler of registered.values()) {
    const event = context();
    const original = structuredClone(event);
    handler(event);
    const model = event.tools.subagent.input.properties.model;
    expect(model.description).not.toContain("NEVER set");
    expect(model.description).toContain(
      "You may choose the model and reasoning variant",
    );
    expect(model.description).toContain("use the models tool");
    expect(model.description).toContain(
      "Astra only when the user explicitly requests it",
    );
    const applied = model.description;
    handler(event);
    expect(model.description).toBe(applied);
    model.description =
      original.tools.subagent.input.properties.model.description;
    expect(event).toEqual(original);
  }
  expect([...registered.keys()]).toEqual(["context", "compaction", "generate"]);
});

test("agents without delegation keep their request unchanged", async () => {
  const event = {
    tools: {},
    system: [{ type: "text", text: "Read-only request" }],
  };
  const original = structuredClone(event);
  for (const handler of (await hooks()).values()) handler(event);
  expect(event).toEqual(original);
});

test("an unexpected native model schema fails clearly", async () => {
  const handler = (await hooks()).get("context")!;
  const event = context();
  event.tools.subagent.input.properties.model.type = "integer";
  expect(() => handler(event)).toThrow("native string model parameter");
  delete (event.tools.subagent.input.properties as Record<string, unknown>).model;
  expect(() => handler(event)).toThrow("native string model parameter");
});
