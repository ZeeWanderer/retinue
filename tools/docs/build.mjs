import fs from 'node:fs/promises';
import path from 'node:path';
import {execFileSync, spawnSync} from 'node:child_process';
import {prepare} from './prepare.mjs';
import {root, work, output} from './paths.mjs';
import {prepareReference, preserveReferenceLinks} from './reference.mjs';
import {check} from './check.mjs';
import {publishNavigator} from './navigation.mjs';

console.log('Preparing API pages and reference links');
const {api, resolveCode} = await prepareReference();
console.log('Preparing guides, commands and protocols');
const {assets} = await prepare(resolveCode);
const publicDir = path.join(work, 'public');
await fs.rm(publicDir, {recursive: true, force: true});
await fs.mkdir(publicDir, {recursive: true});
await fs.cp(path.join(root, 'tools/docs/public'), publicDir, {recursive: true});
execFileSync('python3', ['tools/benchmarks/timeline.py', 'site', path.join(publicDir, 'metrics')], {cwd: root});
for (const asset of assets) {
  const target = path.join(publicDir, asset);
  await fs.mkdir(path.dirname(target), {recursive: true});
  await fs.copyFile(path.join(root, asset), target);
}
await publishNavigator(JSON.parse(await fs.readFile(path.join(work, 'pages.json'), 'utf8')), api, publicDir);
const result = spawnSync(path.join(root, 'tools/docs/node_modules/.bin/astro'), ['build'], {
  cwd: path.join(root, 'tools/docs'), stdio: 'inherit', env: {...process.env, DOCS_REPO_ROOT: root},
});
if (result.error) throw result.error;
if (result.status) process.exit(result.status);
await preserveReferenceLinks(output, api);
await check(output);
