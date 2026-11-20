import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';

export const root = process.env.DOCS_REPO_ROOT ?? path.resolve(fileURLToPath(new URL('../..', import.meta.url)));
export const work = path.join(root, '_build/documentation');
export const content = path.join(root, 'tools/docs/src/content/docs');
export const output = path.join(root, '_build/docs');
export const repository = 'https://github.com/ZeeWanderer/retinue';
export const publishedSite = 'https://zeewanderer.github.io/retinue/';
export const sourceRevision = () => process.env.DOCS_SOURCE_REF ?? execFileSync('git', ['rev-parse', 'HEAD'], {cwd: root, encoding: 'utf8'}).trim();
export const base = '/' + (process.env.DOCS_BASE ?? '/retinue/').split('/').filter(Boolean).join('/') + '/';
export const basePath = base === '//' ? '/' : base;
export const publicRoute = route => route.startsWith('reference/rust/') || route.startsWith('reference/erlang/')
  ? route.replace(/\/index\.html$/, '').replace(/\.html$/, '').replace(/\/$/, '')
  : route.replace(/\/$/, '');
export const url = (route = '') => {
  const [pathname, suffix = ''] = route.replace(/^\//, '').split(/([?#].*)/s);
  return basePath + (pathname.startsWith('reference/rust/') || pathname.startsWith('reference/erlang/') ? publicRoute(pathname) + '/' : pathname) + suffix;
};
export const erlangApps = ['retinue_core', 'wfcore', 'poe2core', 'retinue_cli', 'retinue_daemon'];
export const rustCrates = [
  'retinue_ui', 'retinue_ui_macros', 'retinue_render', 'retinue_client',
  'retinue_native', 'retinue_warframe', 'retinue_poe2',
  'retinue_inspect', 'wfcompanion', 'poe2companion', 'arccompanion', 'wfgui',
];
