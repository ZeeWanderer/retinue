import fs from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';
import rust from '@ast-grep/lang-rust';
import {parse, registerDynamicLanguage} from '@ast-grep/napi';
import {root, url} from './paths.mjs';

registerDynamicLanguage({rust});
const kinds = {
  function: 'fn',
  trait: 'trait',
  struct: 'struct',
  enum: 'enum',
  union: 'union',
  static: 'static',
  constant: 'constant',
  macro: 'macro',
  type_alias: 'type',
  trait_alias: 'traitalias',
  proc_attribute: 'attr',
  proc_derive: 'derive',
};
const categories = {
  module: 'Modules',
  function: 'Functions',
  trait: 'Traits',
  struct: 'Structures',
  enum: 'Enumerations',
  type_alias: 'Type aliases',
  macro: 'Macros',
  proc_attribute: 'Attributes',
  proc_derive: 'Derives',
  constant: 'Constants',
  static: 'Statics',
};
function routeFor(info, parts = info?.path) {
  if (!info || !parts?.length) return;
  const base = 'reference/rust/' + parts.slice(0, -1).join('/');
  if (info.kind === 'module') return 'reference/rust/' + parts.join('/') + '/index.html';
  if (kinds[info.kind]) return base + '/' + kinds[info.kind] + '.' + parts.at(-1) + '.html';
}
const kindOf = (i) => Object.keys(i.inner)[0];
const brief = (text) =>
  text
    ?.split(/\n\s*\n/)[0]
    ?.replace(/\s+/g, ' ')
    .slice(0, 220) || '';
const sourceFiles = new Map();
export function declaration(code, kind) {
  if (!code.trim()) return '';
  if (kind === 'struct_field' || kind === 'variant' || kind === 'assoc_type' || kind === 'assoc_const')
    return code;
  const node = parse('rust', code)
    .root()
    .namedChildren()
    .find((n) => !['attribute_item', 'line_comment', 'block_comment'].includes(n.kind()));
  if (!node || node.kind() === 'ERROR') return '';
  const body =
    node.field('body') ??
    node
      .namedChildren()
      .find((n) =>
        [
          'declaration_list',
          'field_declaration_list',
          'enum_variant_list',
          'block',
          'macro_rule',
        ].includes(n.kind()),
      );
  if (kind === 'macro' || kind === 'proc_macro') return '';
  return (body ? code.slice(node.range().start.index, body.range().start.index) : node.text()).trimEnd();
}
function sourceDecl(item, files) {
  const s = item.span,
    kind = kindOf(item);
  if (!s || !files.has(s.filename) || kind === 'module' || kind === 'use') return '';
  if (!sourceFiles.has(s.filename))
    sourceFiles.set(s.filename, fs.readFileSync(path.join(root, s.filename), 'utf8').split('\n'));
  const lines = sourceFiles.get(s.filename).slice(s.begin[0] - 1, s.end[0]);
  if (!lines.length) return '';
  lines[lines.length - 1] = [...lines.at(-1)].slice(0, s.end[1] - 1).join('');
  lines[0] = [...lines[0]].slice(s.begin[1] - 1).join('');
  return declaration(lines.join('\n'), kind);
}

// Rustdoc IDs are file-local. Use canonical paths for links across crates.
export function rustPages(doc, files, source, crates) {
  if (doc.format_version !== 61) throw Error('Unsupported rustdoc JSON format ' + doc.format_version);
  const {index, paths} = doc,
    pages = new Map(),
    alias = new Map();
  const modules = new Set(
    Object.values(paths)
      .filter((p) => p.kind === 'module')
      .map((p) => p.path.join('/')),
  );
  for (const id of index[doc.root].inner.module.items) {
    const i = index[id];
    if (i?.inner.proc_macro?.kind === 'derive' && !paths[id])
      paths[id] = {crate_id: 0, path: [index[doc.root].name, i.name], kind: 'proc_derive'};
  }
  const targets = new Map();
  const target = (id) =>
    targets.get(id) ?? (crates.has(paths[id]?.path[0]) ? routeFor(paths[id]) : undefined);
  const section = (page, title) =>
    page.groups.find((s) => s.title === title) ??
    (page.groups.push({title, items: []}), page.groups.at(-1));
  const row = (page, title, item) => section(page, title).items.push(item);
  for (const [key, info] of Object.entries(paths)) {
    const id = Number(key),
      i = index[id],
      route = routeFor(info);
    if (info.crate_id !== 0 || !route || !i) continue;
    if (info.path.length > 1 && !modules.has(info.path.slice(0, -1).join('/'))) continue;
    targets.set(id, route);
    pages.set(route, {
      route,
      title: i.name,
      kind: 'Rust',
      role: info.kind.replaceAll('_', ' '),
      description: i.docs || '',
      signature: sourceDecl(i, files),
      source: source(i.span?.filename, i.span?.begin[0]),
      moduleFile: info.kind === 'module' ? i.span?.filename : undefined,
      groups: [],
      id,
    });
  }
  const usedAnchors = new WeakMap();
  function member(page, id, title, prefix) {
    const i = index[id];
    if (!i?.name) return;
    const k = kindOf(i),
      base =
        {
          function: i.inner.function?.has_body === false ? 'tymethod' : 'method',
          assoc_const: 'associatedconstant',
          assoc_type: 'associatedtype',
          struct_field: 'structfield',
          variant: 'variant',
        }[k] ?? k;
    const stem = (prefix ? prefix + '-' : '') + base + '.' + i.name;
    const counts = usedAnchors.get(page) ?? new Map();
    usedAnchors.set(page, counts);
    const n = counts.get(stem) ?? 0;
    counts.set(stem, n + 1);
    const anchor = n ? stem + '-' + (n + 1) : stem;
    if (!targets.has(id)) targets.set(id, page.route + '#' + anchor);
    row(page, title, {
      name: i.name,
      anchor,
      signature: sourceDecl(i, files),
      description: i.docs || '',
      id,
      source: source(i.span?.filename, i.span?.begin[0]),
    });
  }
  for (const p of pages.values()) {
    const i = index[p.id],
      k = kindOf(i),
      v = i.inner[k];
    if (k === 'module')
      for (const child of v.items) {
        const c = index[child],
          u = c?.inner.use,
          dst = target(u?.id ?? child);
        if (!dst) continue;
        if (u && !u.is_glob && c.visibility === 'public') {
          const info = paths[u.id],
            local = routeFor(info, [...paths[p.id].path, u.name]);
          if (local && local !== dst && !pages.has(local))
            alias.set(local, {route: local, title: u.name, kind: 'Rust', redirect: url(dst)});
        }
        row(p, u ? 'Re-exports' : (categories[paths[child]?.kind] ?? 'Other declarations'), {
          name: u ? (u.is_glob ? u.source + '::*' : u.name) : c.name,
          href: url(dst),
          summary: brief(index[u?.id ?? child]?.docs),
        });
      }
    for (const id of v.items ?? []) if (k === 'trait') member(p, id, 'Requirements');
    for (const id of v.variants ?? []) member(p, id, 'Variants');
    for (const id of v.kind?.plain?.fields ?? v.kind?.tuple ?? v.fields ?? [])
      if (id != null) member(p, id, 'Fields');
    const impls = (v.impls ?? []).map((id) => index[id]).filter(Boolean);
    const overloads = new Map();
    for (const {
      inner: {impl: a},
    } of impls)
      if (a.trait && !a.is_synthetic && !a.blanket_impl)
        overloads.set(a.trait.path, (overloads.get(a.trait.path) ?? 0) + 1);
    for (const impl of impls) {
      const a = impl.inner.impl,
        tr = a.trait;
      if (!tr) {
        for (const id of a.items) member(p, id, 'Methods');
        continue;
      }
      const signature = sourceDecl(impl, files);
      const group = a.is_synthetic
        ? 'Auto traits'
        : a.blanket_impl
          ? 'Blanket implementations'
          : 'Trait implementations';
      const r = target(tr.id),
        title = tr.path;
      row(p, group, {
        name: title,
        href: r && url(r),
        signature: signature.startsWith('impl') ? signature : undefined,
      });
      if (!a.is_synthetic && !a.blanket_impl) {
        let prefix = encodeURIComponent(title),
          label = title;
        if (overloads.get(title) > 1) {
          const key = JSON.stringify([tr, a.for, a.generics], (k, value) =>
            k === 'id' ? undefined : value,
          );
          prefix += '-' + createHash('sha256').update(key).digest('hex').slice(0, 10);
          if (signature.startsWith('impl')) label = signature.replace(/\s+/g, ' ');
        }
        for (const id of a.items) member(p, id, label, prefix);
      }
    }
    for (const id of v.implementations ?? []) {
      const a = index[id]?.inner.impl,
        ty = a?.for?.resolved_path,
        r = ty && target(ty.id);
      if (r) row(p, 'Implementing types', {name: ty.path, href: url(r)});
    }
    if (k === 'type_alias' && v.type?.resolved_path) {
      const ty = v.type.resolved_path,
        r = target(ty.id);
      row(p, 'Aliased type', {name: ty.path, href: r && url(r)});
    }
    p.deprecation = i.deprecation?.note;
  }
  for (const p of pages.values()) {
    const links = (id) =>
      Object.fromEntries(
        Object.entries(index[id]?.links ?? {}).flatMap(([key, dst]) => {
          const t = target(dst);
          return t ? [[key, url(t)]] : [];
        }),
      );
    p.links = links(p.id);
    // Only link type identifiers backed by an unambiguous compiler-resolved path.
    for (const r of [p, ...p.groups.flatMap((g) => g.items)]) {
      if (r.id == null) continue;
      if (r.signature) {
        const names = new Map();
        function collect(v) {
          if (!v || typeof v !== 'object') return;
          if (v.resolved_path) {
            const ty = v.resolved_path,
              name = ty.path.split('::').at(-1),
              dst = target(ty.id);
            names.set(name, names.has(name) && names.get(name) !== dst ? null : dst);
          }
          for (const child of Object.values(v)) collect(child);
        }
        collect(index[r.id].inner);
        if (names.size) {
          const tokens = [],
            tree = parse('rust', r.signature).root();
          let start = 0;
          for (const n of tree.findAll({rule: {kind: 'type_identifier'}})) {
            const dst = names.get(n.text());
            if (!dst) continue;
            const range = n.range();
            tokens.push(
              {text: r.signature.slice(start, range.start.index)},
              {text: n.text(), href: url(dst)},
            );
            start = range.end.index;
          }
          if (tokens.length) r.tokens = [...tokens, {text: r.signature.slice(start)}];
        }
      }
      r.links = links(r.id);
    }
    for (const g of p.groups) for (const r of g.items) delete r.id;
    delete p.id;
  }
  return [...pages.values(), ...alias.values()];
}
