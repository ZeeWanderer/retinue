import fs from 'node:fs/promises';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {sidebar} from './catalog.mjs';
import {basePath, work, url} from './paths.mjs';

export function outline(topics, symbols, catalog = sidebar) {
  const api = new Map(
    symbols
      .filter((s) => !s.redirect)
      .map((s) => [
        s.route,
        {
          label: s.title,
          link: url(s.route),
          kind: s.role,
        },
      ]),
  );
  const roots = new Map();
  const parents = new Map();
  for (const [route, node] of api) {
    const folder = path.posix.dirname(route);
    const owner = route.endsWith('/index.html')
      ? path.posix.dirname(folder) + '/index.html'
      : route.startsWith('reference/erlang/')
        ? folder + '/api-reference.html'
        : folder + '/index.html';
    const parent = api.get(owner);
    if (!parent || parent === node) roots.set(node.link, node);
    else {
      (parent.items ??= []).push(node);
      parents.set(node.link, parent);
    }
  }
  const curated = new Map();
  for (const {route, topicGroups} of symbols) {
    if (!topicGroups) continue;
    const owner = api.get(route);
    const rows = [];
    for (const {title, links} of topicGroups) {
      const placed = [];
      for (const href of links) {
        // Cross-framework recommendations stay in the article; each symbol has one navigator home.
        const parent = parents.get(href);
        let ancestor = parent;
        while (ancestor && ancestor !== owner) ancestor = parents.get(ancestor.link);
        if (!ancestor) continue;
        const index = parent?.items.findIndex(n => n.link === href) ?? -1;
        if (index >= 0) placed.push(...parent.items.splice(index, 1));
      }
      if (placed.length) rows.push({marker:title}, ...placed);
    }
    curated.set(owner, rows);
  }
  const sections = {module: 'Modules', Module: 'Modules', trait: 'Traits', struct: 'Structures',
    enum: 'Enumerations', 'type alias': 'Type aliases', function: 'Functions', constant: 'Constants',
    static: 'Statics', macro: 'Macros', 'proc attribute': 'Attributes', 'proc derive': 'Derives'};
  for (const node of api.values()) {
    if (!node.items) continue;
    const rows = node.items.sort((a,b) => a.label.localeCompare(b.label) || a.link.localeCompare(b.link));
    const groups = Map.groupBy(rows, row => sections[row.kind] ?? 'Declarations');
    node.items = [...(curated.get(node) ?? []), ...[...new Set([...Object.values(sections), 'Declarations'])].flatMap(label =>
      groups.has(label) ? [{marker: label}, ...groups.get(label)] : [])];
  }
  function automatic(dir) {
    const rows = new Map(
      topics
        .filter((p) => p.route.startsWith(dir + '/'))
        .map((p) => [
          p.route,
          {
            label: p.title,
            link: url(p.route + '/'),
          },
        ]),
    );
    const items = [];
    for (const [route, row] of rows) {
      const parent = rows.get(path.posix.dirname(route));
      if (dir === 'reference/cli') row.label = route.split('/').at(-1);
      (parent ? (parent.items ??= []) : items).push(row);
    }
    return items;
  }
  function visit(item) {
    if (item.marker) return [item];
    if (item.autogenerate) return automatic(item.autogenerate.directory);
    const root = roots.get(item.link);
    if (!item.items) return [root ?? item];
    return [{...root, ...item, curated: true, items: [
      ...(root && !item.items[0]?.marker ? [{marker:'Topics'}] : []), ...item.items.flatMap(visit), ...(root?.items ?? []),
    ]}];
  }
  return catalog.flatMap(visit);
}

const escape = (s) =>
  s.replace(/[&"<>]/g, (c) => ({'&': '&amp;', '"': '&quot;', '<': '&lt;', '>': '&gt;'})[c]);
const contents = url('portal/contents.html');
export function navigator(tree) {
  const trails = new Map(),
    branches = new Map();
  function index(rows, parents) {
    for (const n of rows) {
      if (n.items?.length) {
        n.id = createHash('sha256')
          .update(n.link ?? parents.join('/') + '/' + n.label)
          .digest('hex')
          .slice(0, 16);
        if (branches.has(n.id)) throw Error('Duplicate navigation branch: ' + n.label);
        branches.set(n.id, n);
      }
      if (n.link) {
        if (trails.has(n.link)) throw Error('Duplicate navigation link: ' + n.link);
        trails.set(n.link, n.id ? [...parents, n.id] : parents);
      }
      if (n.id) index(n.items, [...parents, n.id]);
    }
  }
  index(tree, []);
  function render(rows, current = '', open = new Set(), mode = 'compact') {
    return (
      '<ul>' +
      rows
        .map((n) => {
          if (n.marker) return '<li class="outline-marker">' + escape(n.marker) + '</li>';
          const name = escape(n.label);
          const type =
            n.kind && !['module', 'Application', 'Module'].includes(n.kind)
              ? ' <small>' + escape(n.kind) + '</small>'
              : '';
          const label = n.link
            ? '<a href="' +
              escape(n.link) +
              '"' +
              (n.link === current ? ' aria-current="page"' : '') +
              '>' +
              name +
              type +
              '</a>'
            : '<span>' + name + '</span>';
          if (!n.id) return '<li>' + label + '</li>';
          const show = open.has(n.id);
          const body =
            show || mode === 'all' || (mode === 'guides' && (!n.kind || n.curated))
              ? render(n.items, current, open, mode)
              : '<a class="outline-fallback" href="' +
                contents +
                '#' +
                n.id +
                '">Browse ' +
                name +
                '</a>';
          return (
            '<li><details id="b-' +
            n.id +
            '" data-branch="' +
            n.id +
            '"' +
            (show ? ' open' : '') +
            '><summary>' +
            label +
            '</summary>' +
            body +
            '</details></li>'
          );
        })
        .join('') +
      '</ul>'
    );
  }
  return {
    branches,
    trails,
    breadcrumbs: href => (trails.get(href) ?? []).map(id => branches.get(id)).filter(n => n.link !== href),
    render,
    forPage: (href) =>
      render(tree, href, new Set(trails.get(href)), href === basePath ? 'guides' : 'compact'),
  };
}

export async function publishNavigator(topics, symbols, directory) {
  const tree = outline(topics, symbols),
    nav = navigator(tree);
  for (const p of symbols)
    if (!p.redirect && !nav.trails.has(url(p.route))) throw Error('Unreachable API symbol: ' + p.route);
  const dir = path.join(directory, 'portal/navigation');
  await fs.mkdir(dir, {recursive: true});
  const entries = [];
  function collect(rows, parents = []) {
    for (const n of rows) {
      if (n.link) entries.push([n.label, n.link, n.kind ?? '', parents.join(' › ')]);
      if (n.items) collect(n.items, [...parents, n.label]);
    }
  }
  collect(tree);
  await fs.writeFile(path.join(dir, 'filter.json'), JSON.stringify(entries));
  for (const [id, n] of nav.branches)
    await fs.writeFile(path.join(dir, id + '.frag'), nav.render(n.items));
  await fs.writeFile(
    path.join(directory, 'portal/contents.html'),
    '<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="robots" content="noindex"><title>Documentation contents | Retinue</title><body data-pagefind-ignore><h1>Documentation contents</h1><nav>' +
      nav.render(tree, '', new Set(), 'all').replaceAll('<details id="b-', '<details open id="') +
      '</nav></html>',
  );
  await fs.writeFile(path.join(work, 'outline.json'), JSON.stringify(tree));
}

let cached;
export function siteNavigator() {
  return (cached ??= fs
    .readFile(path.join(work, 'outline.json'), 'utf8')
    .then(JSON.parse)
    .then(navigator));
}
