import fs from 'node:fs/promises';
import path from 'node:path';
import {load} from 'cheerio';
import {basePath, output, work, rustCrates, erlangApps, publicRoute} from './paths.mjs';

export function hasAnchor(ids, hash) {
  const raw = hash.replace(/^#/, '');
  const decoded = decodeURIComponent(raw);
  if (ids.has(raw) || ids.has(decoded)) return true;
  const range = decoded.match(/^(\d+)-(\d+)$/);
  return Boolean(range && ids.has(range[1]) && ids.has(range[2]));
}

export function navigationErrors($, pages) {
  const sidebar = $('#starlight__sidebar');
  if (!sidebar.length) return ['Missing documentation sidebar'];
  const errors = [];
  for (const group of sidebar.find('details').toArray()) {
    if (!$(group).find('a[href]').length) {
      errors.push(`Empty sidebar group: ${$(group).children('summary').text().trim()}`);
    }
  }
  const links = new Set(sidebar.find('a[href]').toArray().map(node => $(node).attr('href')));
  for (const page of pages) {
    if (page.route === 'search') continue;
    const href = basePath + (page.route ? page.route + '/' : '');
    if (!links.has(href)) errors.push(`Missing sidebar link: ${href}`);
  }
  return errors;
}

export async function check(directory) {
  const topics = JSON.parse(await fs.readFile(path.join(work, 'pages.json'), 'utf8'));
  const api = JSON.parse(await fs.readFile(path.join(work, 'api-pages.json'), 'utf8'));
  const pages = [...topics, ...api];
  const errors = [];
  const targets = new Map();
  const validated = new Set();
  const outlineRoots = $ => $('.doc-outline > ul > li').toArray().map(li =>
    $(li).children('details').children('summary').text().trim() || $(li).children('a').text().trim());
  const roots=outlineRoots(load(await fs.readFile(path.join(directory,'index.html'),'utf8')));
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
    const file = path.join(directory, publicRoute(page.route), 'index.html');
    const $ = load(await fs.readFile(file, 'utf8'));
    if (!page.route) errors.push(...navigationErrors($, topics));
    if (!roots.length || JSON.stringify(outlineRoots($))!==JSON.stringify(roots))
      errors.push(page.route+': navigation roots changed');
    if (!page.redirect && page.route!=='search') {
      const current=$('.doc-outline a[aria-current=page]');
      if (current.length!==1 || current.attr('href')!==basePath+(page.route?publicRoute(page.route)+'/':'') ||
          current.parents('details:not([open])').length) errors.push(page.route+': current navigation row not visible');
    }
    if (page.kind && (!$('#starlight__sidebar').length || !$('#_top').length)) {
      errors.push(`${page.route}: missing shared layout/search integration`);
    }
    const refs = $('main a[href], main img[src], .topic-breadcrumbs a[href], #starlight__sidebar a[href]').toArray().map(node => $(node).attr(node.name === 'a' ? 'href' : 'src'));
    for (const href of new Set(refs)) {
      if (!href || /^(?:[a-z][a-z\d+.-]*:|\/\/)/i.test(href)) continue;
      const location = new URL(href, 'https://docs.invalid' + basePath + publicRoute(page.route) + '/');
      if (validated.has(location.href)) continue;
      validated.add(location.href);
      if (!location.pathname.startsWith(basePath)) {
        errors.push(`${page.route}: link escapes site base: ${href}`);
        continue;
      }
      const requested = path.join(directory, decodeURI(location.pathname.slice(basePath.length)));
      try {
        const target = await targetInfo(requested);
        if (location.hash && target.ids && !hasAnchor(target.ids, location.hash)) {
          errors.push(`${page.route}: missing anchor ${href}`);
        }
      } catch { errors.push(`${page.route}: missing ${href}`); }
    }
  }
  for (const [kind, names, leaf] of [['rust', rustCrates, 'index.html'], ['erlang', erlangApps, 'api-reference.html']]) {
    for (const name of names) {
      const file = path.join(directory, publicRoute(`reference/${kind}/${name}/${leaf}`), 'index.html');
      const $ = load(await fs.readFile(file, 'utf8'));
      if (!$('#starlight__sidebar').length || !$('[data-pagefind-body]').length) errors.push(`${kind}/${name}: missing shared layout/search integration`);
    }
  }
  await fs.writeFile(path.join(work, 'link-report.json'), JSON.stringify({pages: pages.length, errors}, null, 2));
  if (errors.length) throw new Error(`${errors.length} documentation errors; see _build/documentation/link-report.json\n${errors.slice(0, 8).join('\n')}`);
  console.log(`Documentation verified: ${topics.length} topics, ${api.length} API pages, complete sidebar and no broken links.`);
}

if (process.argv[1]?.endsWith('/check.mjs')) await check(output);
