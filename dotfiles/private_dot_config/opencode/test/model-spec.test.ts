import { expect, test } from "bun:test";
import { asModelSpec, formatModel, parseModel, sameModel } from "../lib/model-spec";

test("parses and formats provider/model[#variant] specs", () => {
  const specs = [
    { providerID: "opencode-go", modelID: "deepseek-v4.1-flash", variant: "max" },
    { providerID: "openai", modelID: "gpt-6-luna" },
  ];
  for (const spec of specs) expect(parseModel(formatModel(spec))).toEqual(spec);
  expect(formatModel({ providerID: "openai", modelID: "gpt-6-luna", variant: "max" })).toBe(
    "openai/gpt-6-luna#max",
  );
  expect(parseModel("  openai/gpt-6-luna#max  ")).toEqual({
    providerID: "openai",
    modelID: "gpt-6-luna",
    variant: "max",
  });
});

test("rejects malformed specs", () => {
  for (const value of ["", "openai", "openai/", "/luna", "openai/luna#", "openai/luna#a#b", "a b/luna"]) {
    expect(parseModel(value)).toBeUndefined();
  }
  for (const value of [
    undefined,
    null,
    1,
    "openai/luna",
    {},
    { providerID: "openai" },
    { providerID: "", modelID: "luna" },
    { providerID: "openai", modelID: "luna", variant: 7 },
  ]) {
    expect(asModelSpec(value)).toBeUndefined();
  }
});

test("accepts complete JSON shapes", () => {
  expect(asModelSpec({ providerID: "openai", modelID: "gpt-6-sol-fast", variant: "high" })).toEqual({
    providerID: "openai",
    modelID: "gpt-6-sol-fast",
    variant: "high",
  });
  expect(asModelSpec({ providerID: "opencode-go", modelID: "kimi-k3" })).toEqual({
    providerID: "opencode-go",
    modelID: "kimi-k3",
  });
});

test("compares models by provider and model, ignoring variant", () => {
  expect(
    sameModel(
      { providerID: "openai", modelID: "gpt-6-sol-fast", variant: "high" },
      { providerID: "openai", modelID: "gpt-6-sol-fast", variant: "low" },
    ),
  ).toBe(true);
  expect(sameModel({ providerID: "openai", modelID: "gpt-6-luna" }, { providerID: "openai", modelID: "gpt-6-astra" })).toBe(
    false,
  );
  expect(
    sameModel({ providerID: "openai", modelID: "gpt-6-luna" }, { providerID: "opencode-go", modelID: "gpt-6-luna" }),
  ).toBe(false);
});
