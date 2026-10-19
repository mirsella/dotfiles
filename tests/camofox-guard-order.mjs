// Guards the credential-fill protections patched into the pinned Camofox
// server. Presence alone is not enough: a guard that runs after the fill, or an
// error log that runs before the redaction, silently weakens the bridge.
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import vm from "node:vm";

const source = await fs.readFile(process.argv[2], "utf8");

// A duplicate guard would mean the patch also reached another handler.
const uniqueIndex = (marker) => {
  const at = source.indexOf(marker);
  assert.notEqual(at, -1, `patched server.js is missing: ${marker}`);
  assert.equal(source.indexOf(marker, at + marker.length), -1, `patched server.js repeats: ${marker}`);
  return at;
};

for (const marker of ["page_origin_changed", "target_origin_changed", "Credential fill failed"]) {
  uniqueIndex(marker);
}

// Execute the native handler with mocked browser I/O, not a copy of its guards.
const prefix = "app.post('/tabs/:tabId/type',";
const start = uniqueIndex(prefix);
const end = source.indexOf("\n});", start);
assert(end > start, "the native type handler must have a complete route body");
const handlerScript = new vm.Script(source.slice(start + prefix.length, end + "\n}".length));
const origin = "https://login.example.com";
const secret = "synthetic-credential-fill-test";

async function attempt({ pageOrigin = origin, targetOrigin = origin, fillError, ...body } = {}) {
  const result = { writes: [], events: [], logs: [] };
  let checkedTarget;
  const locator = {
    async evaluate(callback) {
      checkedTarget = this;
      return callback({ ownerDocument: { location: { origin: targetOrigin } } });
    },
  };
  const tabState = {
    page: { url: () => pageOrigin, locator: () => ({ ...locator }) },
    refs: new Map(),
    toolCalls: 0,
  };
  const handler = handlerScript.runInNewContext({
    URL,
    sessions: new Map([["test-user", {}]]),
    normalizeUserId: (user) => user,
    findTab: () => ({ tabState }),
    selectorValidationError: () => null,
    withTabLock: (tab, callback) => {
      assert.equal(tab, "test-tab");
      return callback();
    },
    refToLocator: () => locator,
    fillLocator: async (target, text) => {
      if (checkedTarget) assert.equal(target, checkedTarget);
      result.writes.push(text);
      if (fillError) throw new Error(fillError);
    },
    pluginEvents: { emit: (event, payload) => result.events.push({ event, ...payload }) },
    typeEventPayload: (payload) => payload,
    log: (level, message, fields) => result.logs.push({ level, message, ...fields }),
    handleRouteError: (error) => { result.error = error; },
  });
  await handler({
    params: { tabId: "test-tab" },
    reqId: "test-request",
    body: {
      userId: "test-user", ref: "e1", selector: "#credential", text: secret,
      mode: "fill", expectedOrigin: origin, ...body,
    },
  }, { json(value) { result.body = value; } });
  return result;
}

for (const ref of ["e1", null]) {
  const success = await attempt({ ref });
  assert.equal(success.body?.ok, true);
  assert.deepEqual(success.writes, [secret]);
  assert.equal(success.events.length, 1);
  assert.equal(success.events[0].event, "tab:type");
  assert.equal(success.events[0].text, "");
  for (const [change, code] of [
    [{ pageOrigin: "https://other.example.com" }, "page_origin_changed"],
    [{ targetOrigin: "https://other.example.com" }, "target_origin_changed"],
  ]) {
    const rejected = await attempt({ ...change, ref });
    assert.equal(rejected.error.statusCode, 409);
    assert.equal(rejected.error.code, code);
    assert.deepEqual(rejected.writes, []);
    assert.deepEqual(rejected.events, []);
  }
}
const failed = await attempt({ fillError: `Cannot fill ${secret}` });
assert.equal(failed.error.stack, undefined);
assert.equal(failed.error.message, "Credential fill failed");
assert(!JSON.stringify(failed.logs).includes(secret));
assert.deepEqual(failed.events, []);
const ordinary = await attempt({ expectedOrigin: undefined, pageOrigin: "https://other.example.com" });
assert.equal(ordinary.body?.ok, true);
assert.deepEqual(ordinary.writes, [secret]);
assert.equal(ordinary.events[0].text, secret);

console.log("camofox credential-fill guards hold");
