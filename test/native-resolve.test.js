import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { pathToFileURL } from 'node:url';

for (const [platform, arch, libc] of [['linux','x64','gnu'], ['linux','arm64','gnu'], ['linux','x64','musl'], ['linux','arm64','musl'], ['darwin','x64',''], ['darwin','arm64',''], ['win32','x64',''], ['win32','arm64','']]) {
  const binary = platform === 'win32' ? 'inheriteame-native.exe' : 'inheriteame-native';
  test(`${platform} ${arch} ${libc} resolves installed and locally staged binaries`, (t) => {
    const root = mkdtempSync(join(tmpdir(), 'inheriteame-resolve-'));
    t.after(() => rmSync(root, { recursive: true, force: true }));
    mkdirSync(join(root, 'lib/native'), { recursive: true });
    writeFileSync(join(root, 'package.json'), JSON.stringify({ type: 'module' }));
    const module = join(root, 'lib/native/resolve.js');
    copyFileSync('lib/native/resolve.js', module);
    const name = `${platform === 'win32' ? '@carlosiribar/' : ''}inheriteame-${platform}-${arch}${libc ? '-'+libc : ''}`;
    const installed = join(root, 'node_modules', name);
    mkdirSync(join(installed, 'bin'), { recursive: true });
    writeFileSync(join(installed, 'package.json'), JSON.stringify({ name, version: '1.0.0' }));
    writeFileSync(join(installed, 'bin', binary), 'fixture');
    const resolve = () => execFileSync(process.execPath, ['--input-type=module', '-e', `
      Object.defineProperty(process, 'platform', { value: ${JSON.stringify(platform)} });
      Object.defineProperty(process, 'arch', { value: ${JSON.stringify(arch)} });
      Object.defineProperty(process.report, 'getReport', { value: () => ({header: {glibcVersionRuntime: ${JSON.stringify(libc === 'gnu' ? '2.35' : null)}}}) });
      const { resolveNativeBinary } = await import(${JSON.stringify(pathToFileURL(module).href)});
      console.log(resolveNativeBinary());
    `], { encoding: 'utf8' }).trim();
    assert.equal(resolve(), join(installed, 'bin', binary));
    const staged = join(root, 'npm', name, 'bin');
    mkdirSync(staged, { recursive: true });
    writeFileSync(join(staged, binary), 'fixture');
    assert.equal(resolve(), join(staged, binary));
  });
}
