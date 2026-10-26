// Extract T3's own default settings so t3-settings can omit values the user
// never changed. T3 keeps no defaults API: the defaults are module constants in
// the server bundle, computed from the settings schemas. We import a patched
// copy of that bundle from the Electron app's asar, once per app build.
//
// Usage: node t3-defaults.mjs <appimage> <cache-dir>
// Prints {"version": "...", "client": {...}, "server": {...}}. The caller only
// uses the defaults when the version matches the running server.
// Exits nonzero when the bundle layout or the constant names changed; callers
// must treat that as "no defaults available" and keep capturing everything.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';

const STALE_EXTRACT_MS = 60 * 60 * 1000;
const COMMAND_TIMEOUT_MS = 60_000;

const [appImage, cacheArg] = process.argv.slice(2);
if (!appImage || !cacheArg) {
  process.stderr.write('usage: t3-defaults.mjs <appimage> <cache-dir>\n');
  process.exit(2);
}

try {
  process.stdout.write(readDefaults(path.resolve(appImage), path.resolve(cacheArg)));
} catch (error) {
  process.stderr.write(`t3-defaults: ${error.message}\n`);
  process.exitCode = 3;
}

function readDefaults(appImage, cacheDir) {
  fs.mkdirSync(cacheDir, { recursive: true });
  pruneStaleExtracts(cacheDir);
  const cacheFile = path.join(cacheDir, `defaults-${scriptHash()}-${imageKey(appImage)}.json`);
  if (fs.existsSync(cacheFile)) return fs.readFileSync(cacheFile, 'utf8');

  const tmp = fs.mkdtempSync(path.join(cacheDir, 'extract-'));
  try {
    const tree = path.join(tmp, 'tree');
    const version = extractAsar(unpackImage(appImage, tmp), tree);
    const json = JSON.stringify({ version, ...importDefaults(path.join(tree, 'dist')) });
    const staging = `${cacheFile}.tmp`;
    fs.writeFileSync(staging, json);
    fs.renameSync(staging, cacheFile);
    pruneCache(cacheDir);
    return json;
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
}

function scriptHash() {
  const source = fs.readFileSync(new URL(import.meta.url));
  return createHash('sha256').update(source).digest('hex').slice(0, 12);
}

function imageKey(file) {
  const { size, mtimeMs, ctimeMs } = fs.statSync(file);
  return `${size}-${Math.round(mtimeMs)}-${Math.round(ctimeMs)}`;
}

function unpackImage(appImage, dir) {
  const result = spawnSync(appImage, ['--appimage-extract', 'resources/app.asar*'], {
    cwd: dir,
    stdio: ['ignore', 'ignore', 'pipe'],
    timeout: COMMAND_TIMEOUT_MS,
  });
  const asar = path.join(dir, 'squashfs-root', 'resources', 'app.asar');
  if (!fs.existsSync(asar)) {
    const reason =
      result.error?.message ?? (lastLine(result.stderr) || `exit ${result.status}`);
    throw new Error(`cannot unpack ${appImage}: ${reason}`);
  }
  return asar;
}

function extractAsar(asarPath, tree) {
  const raw = fs.readFileSync(asarPath);
  if (raw.length < 16) throw new Error('app.asar is truncated');
  const headerSize = raw.readUInt32LE(4);
  const header = JSON.parse(raw.subarray(16, 16 + raw.readUInt32LE(12)).toString('utf8'));
  const dataStart = 8 + headerSize;

  const unpack = (entry, dest) => {
    if (entry.unpacked) return;
    const offset = Number(entry.offset);
    fs.mkdirSync(path.dirname(dest), { recursive: true });
    fs.writeFileSync(dest, raw.subarray(dataStart + offset, dataStart + offset + Number(entry.size)));
  };

  const dist = find(header, ['apps', 'server', 'dist']);
  if (!dist) throw new Error('app.asar has no server dist directory');
  copyTree(dist, path.join(tree, 'dist'), unpack);

  const modules = find(header, ['node_modules']);
  if (!modules) throw new Error('app.asar has no node_modules');
  copyTree(modules, path.join(tree, 'node_modules'), unpack);

  const unpacked = path.join(path.dirname(asarPath), 'app.asar.unpacked');
  if (fs.existsSync(unpacked)) {
    fs.cpSync(unpacked, tree, { recursive: true, force: true });
  }
  return packageVersion(raw, header, dataStart);
}

function packageVersion(raw, header, dataStart) {
  const entry = header.files?.['package.json'];
  if (!entry) throw new Error('app.asar has no package.json');
  const start = dataStart + Number(entry.offset);
  const pkg = JSON.parse(raw.subarray(start, start + Number(entry.size)).toString('utf8'));
  if (typeof pkg.version !== 'string' || !pkg.version) {
    throw new Error('app.asar package.json has no version');
  }
  return pkg.version;
}

function find(node, parts) {
  return parts.reduce((current, part) => current?.files?.[part], node);
}

function copyTree(node, dest, write) {
  for (const [name, entry] of Object.entries(node.files ?? {})) {
    const target = path.join(dest, name);
    if (entry.files) copyTree(entry, target, write);
    else write(entry, target);
  }
}

function findBundle(dist) {
  const name = fs.readdirSync(dist).find((file) => /^binCli-.*\.mjs$/.test(file));
  if (!name) throw new Error('server dist has no binCli bundle');
  return path.join(dist, name);
}

function importDefaults(dist) {
  const source = fs.readFileSync(findBundle(dist), 'utf8');
  const decoded = [...source.matchAll(/const\s+(\w+)\s*=\s*decodeSync\((\w+)\)\(\{\}\)/g)].map(
    ([, name]) => name,
  );
  const pick = (pattern) => {
    const names = decoded.filter((name) => pattern.test(name));
    if (names.length !== 1) throw new Error(`cannot identify the ${pattern} default constant`);
    return names[0];
  };
  const client = pick(/CLIENT/);
  const server = pick(/SERVER/);
  // The bundle defines the CLI but only runs it from bin.mjs, so a copy with
  // exported constants can be imported for its schema defaults.
  const probe = path.join(dist, 'defaults-probe.mjs');
  fs.writeFileSync(probe, `${source}\nexport { ${client}, ${server} };\n`);
  const runner = path.join(dist, 'defaults-run.mjs');
  fs.writeFileSync(
    runner,
    `import { ${client}, ${server} } from './${path.basename(probe)}';\n` +
      `process.stdout.write(JSON.stringify({ client: ${client}, server: ${server} }));\n`,
  );
  const run = spawnSync(process.execPath, [runner], {
    cwd: dist,
    encoding: 'utf8',
    timeout: COMMAND_TIMEOUT_MS,
  });
  if (run.status !== 0) {
    const reason = run.error?.message ?? lastLine(run.stderr);
    throw new Error(`server bundle refused to load: ${reason}`);
  }
  return JSON.parse(run.stdout);
}

function lastLine(text) {
  return (text ?? '').trim().split('\n').at(-1) || 'no output';
}

function pruneStaleExtracts(dir) {
  for (const name of fs.readdirSync(dir)) {
    if (!name.startsWith('extract-')) continue;
    const target = path.join(dir, name);
    try {
      if (Date.now() - fs.statSync(target).mtimeMs > STALE_EXTRACT_MS) {
        fs.rmSync(target, { recursive: true, force: true });
      }
    } catch {
      // A concurrent run may remove it first.
    }
  }
}

function pruneCache(dir) {
  const entries = fs
    .readdirSync(dir)
    .filter((name) => name.startsWith('defaults-') && name.endsWith('.json'))
    .map((name) => ({ name, time: fs.statSync(path.join(dir, name)).mtimeMs }))
    .sort((a, b) => b.time - a.time);
  for (const entry of entries.slice(2)) fs.rmSync(path.join(dir, entry.name), { force: true });
}
