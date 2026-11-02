import fs from 'node:fs/promises';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {prepare} from './prepare.mjs';
import {root, work, output, erlangApps} from './paths.mjs';
import {integrateDirectory} from './native.mjs';
import {check} from './check.mjs';

console.log('Preparing guides, commands and protocols');
const {assets} = await prepare();
const publicDir = path.join(work, 'public');
await fs.rm(publicDir, {recursive: true, force: true});
await fs.mkdir(publicDir, {recursive: true});
await fs.cp(path.join(root, 'tools/docs/public'), publicDir, {recursive: true});
for (const asset of assets) {
  const target = path.join(publicDir, asset);
  await fs.mkdir(path.dirname(target), {recursive: true});
  await fs.copyFile(path.join(root, asset), target);
}
const rust = path.join(publicDir, 'reference/rust');
console.log('Integrating native API navigation and search');
await fs.cp(path.join(process.env.CARGO_TARGET_DIR ?? path.join(root, '_build/cargo'), 'doc'), rust, {recursive: true});
await integrateDirectory(rust, 'Rust');
for (const app of erlangApps) {
  const target = path.join(publicDir, 'reference/erlang', app);
  await fs.cp(path.join(work, 'erlang', app), target, {recursive: true});
  await integrateDirectory(target, 'Erlang');
}
const result = spawnSync(path.join(root, 'tools/docs/node_modules/.bin/astro'), ['build'], {cwd: path.join(root, 'tools/docs'), stdio: 'inherit'});
if (result.error) throw result.error;
if (result.status) process.exit(result.status);
await check(output);
