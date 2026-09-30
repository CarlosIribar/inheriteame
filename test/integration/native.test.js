import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, symlinkSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { pathToFileURL } from 'node:url';
import { runNative } from '../../lib/native/run.js';

function fixture(t, nested = false) {
  const root = mkdtempSync(join(tmpdir(), 'inheriteame-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const project = nested ? join(root, 'salesforce') : root;
  mkdirSync(project, { recursive: true });
  const put = (path, content) => { mkdirSync(join(project, path, '..'), { recursive: true }); writeFileSync(join(project, path), content); };
  put('sfdx-project.json', JSON.stringify({ packageDirectories: [{ path: 'force-app' }, { path: 'other' }] }));
  mkdirSync(join(project, 'force-app')); mkdirSync(join(project, 'other'));
  const git = (...args) => execFileSync('git', ['-C', root, ...args], { encoding: 'utf8' });
  git('init', '-q'); git('config', 'core.autocrlf', 'false');
  const config = (value) => put('.inheriteame.json', JSON.stringify(value));
  const run = (options = {}) => runNative({ protocolVersion: 1, projectDir: project, all: true, ...options });
  return { root, project, put, git, config, run, read: (path) => readFileSync(join(project, path), 'utf8') };
}

test('dry-run diff, check, write and idempotence across packages', async t => {
  const f = fixture(t);
  f.put('force-app/A.cls', '// José\r\npublic class A {}\r\n');
  f.put('other/B.cls', 'public virtual class B {}');
  f.put('other/event.trigger', 'trigger Event on Account (before insert) {}');
  const dry = await f.run({ dryRun: true });
  assert.equal(dry.exitCode, 0); assert.equal(dry.result.proposedEdits, 2);
  assert.equal(dry.result.changedFiles, 0); assert.equal(dry.result.selectedFiles, 2);
  assert.match(dry.result.files[0].diff, /\+public inherited sharing class A/);
  assert.equal(f.read('force-app/A.cls'), '// José\r\npublic class A {}\r\n');
  assert.equal((await f.run({ check: true })).exitCode, 1);
  assert.equal((await f.run()).result.changedFiles, 2);
  assert.equal(f.read('force-app/A.cls'), '// José\r\npublic inherited sharing class A {}\r\n');
  assert.equal((await f.run({ check: true })).exitCode, 0);
  assert.equal((await f.run()).result.changedFiles, 0);
});

test('exclusions combine class names, paths and header markers without defaults', async t => {
  const f = fixture(t);
  f.put('force-app/A.cls', 'public class AccountCONTROLLER {}');
  f.put('force-app/generated/B.cls', 'public class B {}');
  f.put('other/Legacy.cls', 'public class Legacy {}');
  f.put('other/Header.cls', '/** apex-inherited-sharing-ignore: external */\npublic class Header {}');
  f.put('force-app/controllers/C.cls', 'public class C {}');
  f.config({ version: 1, ignoreClassPatterns: ['*Controller'], excludeFiles: ['**/generated/**', 'other/Legacy.cls'] });
  const result = await f.run({ check: true });
  assert.equal(result.result.excludedFiles, 4); assert.equal(result.result.proposedEdits, 1);
  assert.deepEqual(new Set(result.result.files.filter(x => x.status === 'excluded').map(x => x.reason)), new Set(['excludeFiles', 'ignoreClassPatterns', 'header-marker']));
});

test('missing configuration has no automatic test or controller exclusions', async t => {
  const f = fixture(t);
  f.put('force-app/controllers/A.cls', 'public class AController {}');
  f.put('force-app/tests/B.cls', '@IsTest public class BTest {}');
  assert.equal((await f.run()).result.changedFiles, 2);
});

for (const invalid of [{ version: 2 }, { version: 1, unknown: true }, { version: 1, excludeFiles: ['['] }, { version: 1, ignoreClassPatterns: [42] }, { version: 1, excludeFiles: ['../outside.cls'] }, { version: 1, excludeFiles: ['/absolute.cls'] }, { version: 1, ignoreClassPatterns: ['dir/A'] }]) {
  test(`invalid configuration makes no edits: ${JSON.stringify(invalid)}`, async t => {
    const f = fixture(t); f.put('force-app/A.cls', 'public class A {}'); f.config(invalid);
    const r = await f.run(); assert.equal(r.exitCode, 2); assert.equal(f.read('force-app/A.cls'), 'public class A {}');
  });
}

test('malformed JSON is an execution error before writes', async t => {
  const f = fixture(t); f.put('force-app/A.cls', 'public class A {}'); f.put('.inheriteame.json', '{');
  assert.equal((await f.run()).exitCode, 2); assert.equal(f.read('force-app/A.cls'), 'public class A {}');
});

test('staged selection works when project is nested within Git root and preserves index', async t => {
  const f = fixture(t, true);
  f.put('force-app/A.cls', 'public class A {}'); f.put('other/B.cls', 'public class B {}');
  f.git('add', 'salesforce/force-app/A.cls');
  const index = f.git('diff', '--cached', '--binary');
  const result = await f.run({ all: false });
  assert.equal(result.result.selectedFiles, 1); assert.equal(result.result.changedFiles, 1);
  assert.equal(f.git('diff', '--cached', '--binary'), index);
  assert.equal(f.read('other/B.cls'), 'public class B {}');
});

test('partial staging aborts before any writes; override processes working tree', async t => {
  const f = fixture(t);
  f.put('force-app/A.cls', 'public class A {}'); f.put('force-app/Z.cls', 'public class Z {}');
  f.git('add', 'force-app'); const index = f.git('diff', '--cached', '--binary');
  f.put('force-app/Z.cls', 'public class Z { Integer value; }');
  assert.equal((await f.run({ all: false })).exitCode, 2);
  assert.equal(f.read('force-app/A.cls'), 'public class A {}');
  assert.equal((await f.run({ all: false, allowUnstaged: true })).result.changedFiles, 2);
  assert.equal(f.git('diff', '--cached', '--binary'), index);
});

test('excluded staged files do not fail partial staging protection', async t => {
  const f = fixture(t); f.put('force-app/A.cls', 'public class A {}'); f.git('add', 'force-app');
  f.put('force-app/A.cls', '// apex-inherited-sharing-ignore\npublic class A {}');
  const result = await f.run({ all: false }); assert.equal(result.exitCode, 0); assert.equal(result.result.excludedFiles, 1);
});

test('parse and encoding errors preserve affected files and report paths', async t => {
  const f = fixture(t); f.put('force-app/A.cls', 'public class {'); f.put('other/Bad.cls', Buffer.from([0xff]));
  const r = await f.run(); assert.equal(r.exitCode, 1); assert.equal(r.result.errors, 2);
  assert.equal(r.result.changedFiles, 0); assert.equal(f.read('force-app/A.cls'), 'public class {');
  assert.match(r.result.diagnostics.join('\n'), /force-app\/A.cls/);
});

test('overlapping package directories are deduplicated', async t => {
  const f = fixture(t); f.put('force-app/A.cls', 'public class A {}');
  f.put('sfdx-project.json', JSON.stringify({ packageDirectories: [{ path: 'force-app' }, { path: 'force-app' }] }));
  assert.equal((await f.run()).result.changedFiles, 1);
});

test('single-star excludes only one directory level', async t => {
  const f = fixture(t); f.put('force-app/A.cls', 'public class A {}'); f.put('force-app/nested/B.cls', 'public class B {}');
  f.config({ version: 1, excludeFiles: ['force-app/*.cls'] });
  const r = await f.run(); assert.equal(r.result.excludedFiles, 1); assert.equal(r.result.changedFiles, 1);
});

test('symlinks are skipped during both discovery modes', { skip: process.platform === 'win32' }, async t => {
  const f = fixture(t); f.put('outside/A.cls', 'public class A {}');
  symlinkSync(join(f.project, 'outside/A.cls'), join(f.project, 'force-app/A.cls'));
  f.git('add', 'force-app/A.cls');
  assert.equal((await f.run()).result.selectedFiles, 0);
  assert.equal((await f.run({ all: false })).result.selectedFiles, 0);
});

function cli(f, args) {
  const url = pathToFileURL(join(process.cwd(), 'lib/commands/apex/inherited-sharing/fix.js')).href;
  return spawnSync(process.execPath, ['--input-type=module', '-e', `const {default: Fix} = await import(${JSON.stringify(url)}); const {handle} = await import("@oclif/core"); await Fix.run(process.argv.slice(1)).catch(handle);`, '--', '--project-dir', f.project, ...args], { encoding: 'utf8', env: { ...process.env, SF_DISABLE_TELEMETRY: 'true' } });
}
test('CLI exposes JSON, diff, verbose, quiet and logical exits', t => {
  const f = fixture(t); f.put('force-app/A.cls', 'public class A {}');
  const check = cli(f, ['--all', '--check', '--json']);
  assert.equal(check.status, 1, check.stderr);
  const json = JSON.parse(check.stdout); assert.equal(json.result.result.proposedEdits, 1); assert.equal(typeof json.result.result.durationSeconds, 'number');
  const dry = cli(f, ['--all', '--dry-run', '--verbose']); assert.equal(dry.status, 0, dry.stderr);
  assert.match(dry.stdout, /missing-sharing/); assert.match(dry.stdout, /\+public inherited sharing/);
  const quiet = cli(f, ['--all', '--dry-run', '--quiet']); assert.equal(quiet.stdout.trim(), '');
  assert.equal(cli(f, ['--check', '--dry-run']).status, 2);
  assert.equal(cli(f, ['--all', '--allow-unstaged']).status, 2);
  assert.equal(cli(f, ['--quiet', '--verbose']).status, 2);
  f.config({ version: 20 }); assert.equal(cli(f, ['--all', '--json']).status, 2);
});
