import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const root = JSON.parse(readFileSync('package.json', 'utf8'));
test('all eight optional platform packages have consistent distribution metadata', () => {
  assert.equal(Object.keys(root.optionalDependencies).length, 8);
  for (const [name, version] of Object.entries(root.optionalDependencies)) {
    const manifest = JSON.parse(readFileSync(`npm/${name}/package.json`, 'utf8'));
    const [os, cpu, libc] = name.replace('@carlosiribar/', '').replace('inheriteame-', '').split('-');
    assert.equal(manifest.name, name); assert.equal(version, root.version); assert.equal(manifest.version, version);
    assert.deepEqual(manifest.os, [os]); assert.deepEqual(manifest.cpu, [cpu]);
    if (os === 'linux') assert.deepEqual(manifest.libc, [libc === 'gnu' ? 'glibc' : 'musl']);
    assert.equal(manifest.bin['inheriteame-native'], `bin/inheriteame-native${os === 'win32' ? '.exe' : ''}`);
    assert.equal(manifest.scripts?.postinstall, undefined);
  }
  assert.equal(root.scripts.postinstall, undefined);
  assert(!root.files.includes('npm')); assert(!root.files.includes('target'));
});
