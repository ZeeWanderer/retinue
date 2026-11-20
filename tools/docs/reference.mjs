import fs from 'node:fs/promises';
import path from 'node:path';
import {unified} from 'unified';
import markdown from 'remark-parse';
import gfm from 'remark-gfm';
import rehype from 'remark-rehype';
import stringify from 'rehype-stringify';
import {visit} from 'unist-util-visit';
import {
  root,
  work,
  repository,
  sourceRevision,
  rustCrates,
  erlangApps,
  url,
  publicRoute,
} from './paths.mjs';
import {publicFiles} from './public-files.mjs';
import {rustPages} from './rust-symbols.mjs';
import {relatedTopics} from './catalog.mjs';

const parser = unified().use(markdown).use(gfm);
const html = unified().use(rehype).use(stringify);
export function prose(text, links = {}) {
  if (!text) return '';
  // Definitions must exist while parsing shortcut references, not only during rendering.
  const body = Object.keys(links).length
    ? parser.parse(
        text +
          '\n\n' +
          Object.entries(links)
            .filter(([key]) => !/[\n\]]/.test(key))
            .map(([key, href]) => '[' + key + ']: ' + href)
            .join('\n'),
      )
    : parser.parse(text);
  visit(body, 'link', (node) => {
    if (links[node.url]) node.url = links[node.url];
    else if (!/^(?:https?:\/\/|mailto:|#)/.test(node.url)) {
      // External-crate names without local metadata are text, never guessed URLs.
      node.type = 'emphasis';
      delete node.url;
    }
  });
  return html.stringify(html.runSync(body));
}

export function erlangPages(modules, source) {
  const pages = [],
    mods = new Map(modules.map((m) => [m.name, m]));
  const route = (m) => 'reference/erlang/' + m.app + '/' + m.name + '.html';
  for (const app of erlangApps) {
    const own = modules.filter((m) => m.app === app);
    if (!own.length) throw Error('No Erlang metadata for ' + app);
    pages.push({
      kind: 'Erlang',
      role: 'Application',
      route: 'reference/erlang/' + app + '/api-reference.html',
      title: app,
      groups: [
        {
          title: 'Modules',
          items: own.map((m) => ({
            name: m.name,
            href: url(route(m)),
            summary: m.description.split('\n')[0],
          })),
        },
      ],
    });
    for (const m of own) {
      const groups = [];
      for (const [kind, title] of [
        ['function', 'Functions'],
        ['type', 'Types'],
        ['callback', 'Callbacks'],
      ]) {
        const members = m.members
          .filter((v) => v.kind === kind)
          .map((v) => ({
            name: v.name + '/' + v.arity,
            signature: v.signature,
            anchor: (kind === 'function' ? '' : kind === 'type' ? 't:' : 'c:') + v.name + '/' + v.arity,
            description: v.description,
            source: source(m.source, v.line),
          }));
        if (members.length) groups.push({title, items: members});
      }
      for (const [title, list] of [
        ['Behaviours', m.behaviours],
        ['Implementing modules', modules.filter((v) => v.behaviours.includes(m.name)).map((v) => v.name)],
      ]) {
        if (list.length)
          groups.push({
            title,
            items: list.map((n) => ({name: n, href: mods.has(n) ? url(route(mods.get(n))) : undefined})),
          });
      }
      pages.push({
        route: route(m),
        kind: 'Erlang',
        role: 'Module',
        title: m.name,
        signature: '-module(' + m.name + ').',
        description: m.description,
        source: source(m.source),
        groups,
      });
    }
  }
  return pages;
}

export async function prepareReference() {
  const files = publicFiles(),
    ref = sourceRevision();
  const source = (filename, line) => {
    if (!filename) return;
    const rel = path.isAbsolute(filename) ? path.relative(root, filename) : filename;
    return files.has(rel) ? repository + '/blob/' + ref + '/' + rel + (line ? '#L' + line : '') : undefined;
  };
  const pages = [],
    crates = new Set(rustCrates);
  for (const crate of crates) {
    const doc = JSON.parse(
      await fs.readFile(path.join(root, '_build/doc-toolchain/doc', crate + '.json'), 'utf8'),
    );
    pages.push(...rustPages(doc, files, source, crates));
  }
  const modules = JSON.parse(await fs.readFile(path.join(work, 'erlang.json'), 'utf8'));
  pages.push(
    ...erlangPages(
      modules.filter((m) => source(m.source)),
      source,
    ),
  );
  const byUrl = new Map();
  for (const page of pages) {
    const key = url(page.route);
    if (byUrl.has(key)) throw Error('Duplicate API page ' + key);
    byUrl.set(key, page);
  }
  for (const p of pages)
    for (const g of p.groups ?? [])
      if (g.title === 'Trait implementations') {
        for (const impl of g.items) {
          const trait = byUrl.get(impl.href);
          if (!trait || trait.role !== 'trait') continue;
          let inverse = trait.groups.find((g) => g.title === 'Implementing types');
          if (!inverse) trait.groups.push((inverse = {title: 'Implementing types', items: []}));
          if (!inverse.items.some((r) => r.href === url(p.route)))
            inverse.items.push({name: p.title, href: url(p.route)});
        }
      }
  for (const [owner, groups] of Object.entries(
    JSON.parse(await fs.readFile(new URL('./topic-groups.json', import.meta.url), 'utf8')),
  )) {
    const lookup = (route) => {
      const page = byUrl.get(url('reference/rust/' + route + '.html'));
      if (!page) throw Error('Unknown topic-group symbol: ' + route);
      return page.redirect ? byUrl.get(page.redirect) : page;
    };
    const parent = lookup(owner);
    parent.groups.unshift(
      ...Object.entries(groups).map(([title, items]) => ({
        title,
        items: items.map((id) => {
          const p = lookup(id);
          if (!p.description?.trim()) throw Error('Featured symbol needs a source doc comment: ' + id);
          return {name: p.title, href: url(p.route), summary: p.description?.split(/\n\s*\n/)[0]};
        }),
      })),
    );
  }
  const dir = path.join(work, 'api');
  await fs.rm(dir, {recursive: true, force: true});
  for (const p of pages) {
    p.related = relatedTopics(p.route);
    for (const item of [p, ...(p.groups ?? []).flatMap((g) => g.items)]) {
      if (item.description && item === p && !/^[#`|*-]/.test(item.description)) {
        const [first, ...rest] = item.description.split(/\n\s*\n/);
        p.abstract = prose(first, item.links);
        item.description = rest.join('\n\n');
      }
      if (item.description) item.html = prose(item.description, item.links);
      delete item.description;
      delete item.links;
    }
    const file = path.join(dir, p.route + '.json');
    await fs.mkdir(path.dirname(file), {recursive: true});
    await fs.writeFile(file, JSON.stringify(p));
  }
  const list = pages.map(({route, kind, role, title, redirect}) => ({route, kind, role, title, redirect}));
  await fs.writeFile(path.join(work, 'api-pages.json'), JSON.stringify(list));
  return list;
}

export async function preserveReferenceLinks(directory, pages) {
  for (const page of pages) {
    const file = path.join(directory, page.route);
    if (file === path.join(directory, publicRoute(page.route), 'index.html')) continue;
    const dest = url(page.route);
    await fs.writeFile(
      file,
      `<!doctype html><meta charset="utf-8"><title>Reference moved</title><meta http-equiv="refresh" content="0;url=${dest}"><a href="${dest}">Open reference</a><script>location.replace(${JSON.stringify(dest)} + location.hash)</script>`,
    );
  }
}
