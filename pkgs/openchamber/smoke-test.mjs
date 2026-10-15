import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const packageRoot = process.argv[2];
const require = createRequire(path.join(packageRoot, 'package.json'));
const braces = require('braces');
const { simpleGit, GitPluginError } = require('simple-git');

assert.deepEqual(braces.expand('src/{a,b}/{1..2}.js'), [
  'src/a/1.js', 'src/a/2.js', 'src/b/1.js', 'src/b/2.js',
]);
assert.equal(braces.compile('src/{a,b}.js'), 'src/(a|b).js');
for (const operation of ['compile', 'expand', 'stringify']) {
  const pattern = '{'.repeat(4000) + 'a,b' + '}'.repeat(4000);
  assert.throws(() => braces[operation](pattern), {
    name: 'SyntaxError', message: /maximum AST depth/,
  });
  // The parser flattens incomplete braces into literal text; preserve that.
  assert.doesNotThrow(() => braces[operation]('{'.repeat(4000) + 'a,b'));
  // Public operations also accept an AST, bypassing the string parser.
  assert.throws(() => braces[operation](braces.parse('{'.repeat(4000) + 'a,b' + '}'.repeat(4000))), {
    name: 'SyntaxError', message: /maximum AST depth/,
  });
}

fs.mkdirSync('.agents-local', { recursive: true });
const temporary = fs.mkdtempSync(path.resolve('.agents-local/openchamber-'));
try {
  process.env.HOME = path.join(temporary, 'home');
  fs.mkdirSync(process.env.HOME);
  process.env.XDG_CONFIG_HOME = path.join(temporary, 'config');
  process.env.GIT_CONFIG_NOSYSTEM = '1';
  process.env.GIT_CONFIG_GLOBAL = path.join(temporary, 'gitconfig');
  fs.writeFileSync(process.env.GIT_CONFIG_GLOBAL, '[user]\nname = Test\nemail = test@example.invalid\n');
  fs.mkdirSync(path.join(process.env.XDG_CONFIG_HOME, 'git'), { recursive: true });
  fs.writeFileSync(path.join(process.env.XDG_CONFIG_HOME, 'git/config'), '[user]\nname = Test\nemail = test@example.invalid\n');
  execFileSync('git', ['init', '--initial-branch=main', temporary]);
  execFileSync('git', ['-C', temporary, '-c', 'user.name=Test', '-c', 'user.email=test@example.invalid', 'commit', '--allow-empty', '-m', 'initial']);

  const git = simpleGit(temporary);
  assert.equal((await git.status()).current, 'main');
  for (const args of [
    ['-c', 'include.path=/does-not-exist', 'version'],
    ['-c', 'trailer.test.command=echo unused', 'version'],
  ]) {
    await assert.rejects(git.raw(args), GitPluginError);
  }
  for (const flag of ['--receive-p=/does-not-exist', '--exe=/does-not-exist']) {
    await assert.rejects(git.push([temporary, 'HEAD:refs/heads/blocked', flag]), {
      reason: 'DISALLOWED_ABBREVIATED',
    });
  }
  await assert.rejects(simpleGit(temporary).env({ PATH: process.env.PATH, VISUAL: 'unused-editor' }).status(), GitPluginError);

  const service = await import(pathToFileURL(path.join(packageRoot, 'server/lib/git/service.js')));
  service.configureGitEnvironment({ forDirectory: async () => ({ VISUAL: 'unused-editor' }) });
  await service.getStatus(temporary);
  await service.getBranches(temporary);
  fs.writeFileSync(path.join(temporary, 'test.txt'), 'Git compatibility test\n');
  await service.stageFiles(temporary, ['test.txt']);
  assert.match(await service.getDiff(temporary, { path: 'test.txt', staged: true }), /Git compatibility test/);
  await service.commit(temporary, 'Git compatibility test');
  assert.equal(await git.show(['HEAD:test.txt']), 'Git compatibility test\n');

  const pty = require('node-pty');
  await new Promise((resolve, reject) => {
    const child = pty.spawn('bash', ['-c', 'printf pty-ok'], { cwd: temporary, env: process.env });
    let output = '';
    const timeout = setTimeout(() => { child.kill(); reject(new Error('PTY timed out')); }, 5000);
    child.onData((data) => { output += data; });
    child.onExit(({ exitCode }) => {
      clearTimeout(timeout);
      try { assert.equal(exitCode, 0); assert.equal(output, 'pty-ok'); resolve(); }
      catch (error) { reject(error); }
    });
  });
  console.log('OpenChamber Git compatibility, security guards, brace depth limits and native PTY passed');
} finally {
  fs.rmSync(temporary, { recursive: true, force: true });
}
