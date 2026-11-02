import path from 'node:path';
import {fileURLToPath} from 'node:url';

export const root = path.resolve(fileURLToPath(new URL('../..', import.meta.url)));
export const work = path.join(root, '_build/documentation');
export const content = path.join(work, 'content');
export const output = path.join(root, '_build/docs');
export const repository = 'https://github.com/ZeeWanderer/retinue';
export const base = '/' + (process.env.DOCS_BASE ?? '/retinue/').split('/').filter(Boolean).join('/') + '/';
export const basePath = base === '//' ? '/' : base;
export const url = (route = '') => basePath + route.replace(/^\//, '');
export const erlangApps = ['retinue_core', 'wfcore', 'poe2core', 'retinue_cli', 'retinue_daemon'];
export const rustCrates = [
  'retinue_ui', 'retinue_ui_macros', 'retinue_render', 'retinue_client',
  'retinue_native', 'retinue_warframe', 'retinue_poe2',
  'retinue_inspect', 'wfcompanion', 'poe2companion', 'arccompanion', 'wfgui',
];
