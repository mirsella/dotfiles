// Guards the credential-fill protections patched into the pinned Camofox
// server. Presence alone is not enough: a guard that runs after the fill, or an
// error log that runs before the redaction, silently weakens the bridge.
import assert from "node:assert/strict";
import fs from "node:fs/promises";

const source = await fs.readFile(process.argv[2], "utf8");

const index = (marker) => {
  const at = source.indexOf(marker);
  assert.notEqual(at, -1, `patched server.js is missing: ${marker}`);
  return at;
};

// The guard belongs to exactly one handler. A second copy would mean the
// patch anchor drifted, so a count of one is part of the contract.
const only = (marker) => {
  const count = source.split(marker).length - 1;
  assert.equal(count, 1, `patched server.js must contain one ${marker}, found ${count}`);
  return index(marker);
};

const pageCheck = only("page_origin_changed");
const targetCheck = only("target_origin_changed");
const targetOrigin = only("element.ownerDocument.location.origin");
const fill = index("await fillLocator(");
assert(pageCheck < fill, "the page origin must be checked before filling");
assert(targetCheck < fill, "the fill target's origin must be checked before filling");
assert(
  targetOrigin < targetCheck,
  "the resolved element's own document origin must be the checked value",
);

const redaction = only("err.message = 'Credential fill failed'");
const errorLog = index("log('error', 'type failed'");
assert(redaction < errorLog, "a credential-fill error must be redacted before it is logged");

const emit = index("pluginEvents.emit('tab:type'");
const emitEnd = source.indexOf("\n", emit);
assert(emitEnd > emit, "the tab:type emit must be a single line");
assert(
  source.slice(emit, emitEnd).includes('expectedOrigin !== undefined ? ""'),
  "a credential fill must emit an empty payload text",
);

// Only the check in front of upstream's fill branch is ours. The anchor below
// is the one the patch uses, so the untouched /act copy cannot satisfy it: a
// mangled replacement would leave typing broken while every marker still exists.
only("\n      if (mode === 'fill') {");

console.log("camofox guard order holds");
