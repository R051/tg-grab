import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';

const json = (path) => JSON.parse(readFileSync(new URL(`../${path}`, import.meta.url), 'utf8'));
const text = (path) => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
const version = json('package.json').version;
assert.match(version, /^\d+\.\d+\.\d+(?:-[\da-zA-Z.-]+)?$/);
const versions = {
  'package-lock.json': json('package-lock.json').version,
  'package-lock root': json('package-lock.json').packages[''].version,
  'tauri.conf.json': json('src-tauri/tauri.conf.json').version,
  'Cargo.toml': text('src-tauri/Cargo.toml').match(/\[package\][\s\S]*?^version\s*=\s*"([^"]+)"/m)?.[1],
  'Cargo.lock': text('src-tauri/Cargo.lock').match(/\[\[package\]\]\s+name = "tg-grab"\s+version = "([^"]+)"/)?.[1],
};
for (const [file, value] of Object.entries(versions)) assert.equal(value, version, `${file} version differs`);
if (process.argv[2]) assert.equal(process.argv[2], `v${version}`, 'Release tag must match the app version');
console.log(`Versions match: ${version}`);
