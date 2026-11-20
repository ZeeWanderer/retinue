import {execFileSync} from 'node:child_process';
import {existsSync} from 'node:fs';
import path from 'node:path';
import {root} from './paths.mjs';

export function publicFiles(cwd = root, trackedOnly = process.env.DOCS_TRACKED_ONLY === '1') {
  const options = trackedOnly ? [] : ['--others', '--exclude-standard'];
  const names = execFileSync('git', ['ls-files', '-z', '--cached', ...options], {
    cwd,
    encoding: 'utf8',
    maxBuffer: 32 * 1024 * 1024,
  })
    .split('\0')
    .filter(Boolean);
  return new Set(names.filter((name) => existsSync(path.join(cwd, name))));
}
