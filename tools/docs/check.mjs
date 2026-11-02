import fs from 'node:fs/promises';
import path from 'node:path';
import {load} from 'cheerio';
import {basePath, output, work, rustCrates, erlangApps} from './paths.mjs';

export async function check(directory) {
  const pages = JSON.parse(await fs.readFile(path.join(work, 'pages.json'), 'utf8'));
  const errors = [];
  const targets = new Map();
  const targetInfo = async file => {
    if (!targets.has(file)) targets.set(file, (async () => {
      if ((await fs.stat(file)).isDirectory()) file = path.join(file, 'index.html');
      if (!file.endsWith('.html')) return {file};
      const document = load(await fs.readFile(file, 'utf8'));
      return {file, ids: new Set(document('[id]').toArray().map(node => document(node).attr('id')))};
    })());
    return targets.get(file);
  };
  for (const page of pages) {
    const file = path.join(directory, page.route, 'index.html');
    const $ = load(await fs.readFile(file, 'utf8'));
    const refs = $('main a[href], main img[src]').toArray().map(node => $(node).attr(node.name === 'a' ? 'href' : 'src'));
    for (const href of refs) {
      if (!href || /^(?:[a-z][a-z\d+.-]*:|\/\/)/i.test(href)) continue;
      const location = new URL(href, 'https://docs.invalid' + basePath + (page.route ? page.route + '/' : ''));
      if (!location.pathname.startsWith(basePath)) {
        errors.push(`${page.route}: link escapes site base: ${href}`);
        continue;
      }
      const requested = path.join(directory, decodeURI(location.pathname.slice(basePath.length)));
      try {
        const target = await targetInfo(requested);
        if (location.hash && target.ids && !target.file.includes('/reference/rust/')) {
          const id = decodeURIComponent(location.hash.slice(1));
          if (!target.ids.has(id)) errors.push(`${page.route}: missing anchor ${href}`);
        }
      } catch { errors.push(`${page.route}: missing ${href}`); }
    }
  }
  for (const [kind, names, leaf] of [['rust', rustCrates, 'index.html'], ['erlang', erlangApps, 'api-reference.html']]) {
    for (const name of names) {
      const file = path.join(directory, 'reference', kind, name, leaf);
      const $ = load(await fs.readFile(file, 'utf8'));
      if (!$('.retinue-portal').length || !$('[data-pagefind-body]').length) errors.push(`${kind}/${name}: missing portal/search integration`);
    }
  }
  await fs.writeFile(path.join(work, 'link-report.json'), JSON.stringify({pages: pages.length, errors}, null, 2));
  if (errors.length) throw new Error(`${errors.length} documentation errors; see _build/documentation/link-report.json\n${errors.slice(0, 8).join('\n')}`);
  console.log(`Documentation verified: ${pages.length} topics, ${rustCrates.length + erlangApps.length} API roots, no broken topic links.`);
}

if (process.argv[1]?.endsWith('/check.mjs')) await check(output);
