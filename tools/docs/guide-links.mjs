import path from 'node:path';
import {url} from './paths.mjs';

export function guideLinks(pages, files, source, directory) {
  const targets = new Map(), modules = new Map();
  const canonical = new Map(pages.filter(p => !p.redirect).map(p => [url(p.route), p]));
  function add(map, key, href) {
    if (!map.has(key)) map.set(key, href);
    else if (map.get(key) !== href) map.set(key, null);
  }
  function rustName(parts, href, bare = false) {
    for (let i = 0; i < parts.length - 1; ++i) add(targets, parts.slice(i).join('::'), href);
    if (bare) add(targets, parts.at(-1), href);
  }
  for (const p of pages) {
    const href = p.redirect ?? url(p.route);
    if (p.moduleFile) {
      // Inline Rust modules share their parent's file but do not own it.
      const file = p.moduleFile, stem = path.basename(file, '.rs');
      if (p.kind === 'Erlang' || p.title === stem ||
          (stem === 'mod' && p.title === path.basename(path.dirname(file))) ||
          (['lib','main'].includes(stem) && p.route.split('/').length === 4))
        add(modules, file, href);
    }
    if (p.kind === 'Erlang' && p.role === 'Application') {
      add(targets, p.title, href);
      add(targets, 'app:' + p.title, href);
    }
    if (p.kind === 'Erlang' && p.role === 'Module') {
      add(targets, p.title, href);
      add(targets, 'm:' + p.title, href);
      for (const [name, target] of p.privateFunctions ?? []) add(targets, p.title + ':' + name, target);
      for (const group of p.groups ?? [])
        for (const item of group.items) {
          if (!item.anchor) continue;
          const prefix = item.anchor.slice(0, -item.name.length);
          add(targets, prefix + p.title + ':' + item.name, href + '#' + item.anchor);
        }
    }
    if (p.kind !== 'Rust') continue;
    const parts = p.route.slice('reference/rust/'.length, -'.html'.length).split('/');
    const leaf = parts.pop();
    if (leaf !== 'index') parts.push(leaf.slice(leaf.indexOf('.') + 1));
    rustName(parts, href, parts.length === 1 || /^[A-Z][A-Za-z0-9_]*$/.test(parts.at(-1)));
    if (leaf !== 'index') add(targets, leaf.split('.')[0] + '@' + parts.join('::'), href);
    if (leaf.startsWith('fn.')) rustName([...parts.slice(0,-1), parts.at(-1) + '()'], href, true);
    if (leaf.startsWith('macro.')) rustName([...parts.slice(0,-1), parts.at(-1) + '!'], href, true);
    for (const group of (p.redirect ? canonical.get(href) : p)?.groups ?? [])
      for (const item of group.items)
        if (item.anchor) {
          rustName([...parts, item.name], href + '#' + item.anchor);
          if (/(?:^|-)(?:ty)?method\./.test(item.anchor))
            rustName([...parts, item.name + '()'], href + '#' + item.anchor);
        }
  }
  const dirs = new Set();
  for (const file of files) {
    const href = modules.get(file) ?? source(file);
    const parts = file.split('/');
    if (parts.length > 1 || !targets.has(file)) add(targets, file, href);
    if (directory) for (let i = 1; i < parts.length; ++i) dirs.add(parts.slice(0,i).join('/'));
    if (/\.(rs|erl|hrl|proto|cpp|hpp|mjs|escript)$/.test(file))
      for (let i = 1; i < parts.length; ++i) add(targets, parts.slice(i).join('/'), href);
  }
  for (const dir of dirs) {
    const href = directory(dir);
    add(targets, dir + '/', href);
    if (dir.includes('/')) add(targets, dir, href);
  }
  const resolve = (code, owner) => {
    if (owner?.kind === 'Erlang' && owner.role === 'Module') {
      const local = /^([tc]:)?([a-z][\w@]*\/\d+)$/.exec(code);
      if (local) return targets.get((local[1] ?? '') + owner.title + ':' + local[2]) ?? undefined;
    }
    if (owner?.kind === 'Rust') {
      const scope = path.posix.dirname(owner.route.slice('reference/rust/'.length)).split('/');
      const member = code.startsWith('Self::') ? owner.title + code.slice(4) : code;
      while (scope.length) {
        const local = [...scope, member].join('::');
        if (targets.has(local)) return targets.get(local) ?? undefined;
        scope.pop();
      }
    }
    if (targets.has(code)) return targets.get(code) ?? undefined;
    const key = code.replace(/^([\w-]+)(?=::)/, name => name.replaceAll('-', '_'))
      .replace(/^(\w+(?:::\w+)+)<[\w ,:]+>$/, '$1')
      .replace(/^(\w+(?:::\w+)+)\([^()]*\)$/, '$1()');
    return targets.get(key) ?? undefined;
  };
  resolve.knows = key => targets.has(key);
  resolve.ambiguous = key => targets.get(key) === null;
  return resolve;
}

export function topicLinks(resolveCode, pages, schemas, commands) {
  const names = new Map(), cli = new Map(), missing = new Map(), imports = new Map();
  function add(name, href) {
    if (names.has(name) && names.get(name) !== href) names.set(name, null);
    else names.set(name, href);
  }
  for (const p of [...pages, ...schemas]) {
    const href = url(p.route ? p.route + '/' : '');
    add(p.source, href);
    add(path.basename(p.source), href);
    for (const [name, anchor, short = name.split('.').at(-1)] of p.symbols ?? []) {
      add(name, href + '#' + anchor);
      if (!resolveCode.knows(short)) add(short, href + '#' + anchor);
    }
  }
  const declarations = new Map(names);
  const declaration = key => declarations.get(key) ?? resolveCode(key);
  for (const p of pages) {
    const href = url(p.route ? p.route + '/' : '');
    const local = new Map();
    for (const [alias, symbol] of p.imports ?? []) {
      const target = declaration(symbol);
      if (!target || local.has(alias)) throw Error(p.source + ': invalid reference import ' + alias + ': ' + symbol);
      local.set(alias, target);
    }
    imports.set(p.source, local);
    for (const [term, target] of p.terms ?? []) {
      const destination = target.startsWith('#') ? href + target :
        target.startsWith('https://') ? target : declaration(target);
      if (!destination) throw Error(p.source + ': invalid term target: ' + target);
      if (resolveCode.knows(term) || names.has(term)) throw Error(p.source + ': ambiguous shared term ' + term);
      add(term, destination);
    }
  }
  for (const {path: parts} of commands) {
    const href = url(['reference/cli', ...parts].join('/') + '/');
    cli.set(['retinue', ...parts].join(' '), href);
    const alias = {warframe:'wfcli', poe2:'poe2cli', arc:'arccli'}[parts[0]];
    if (alias) cli.set([alias, ...parts.slice(1)].join(' '), href);
  }
  function resolve(code, source) {
    const name = code.split(/\s+--?/)[0];
    const href = imports.get(source)?.get(code) ?? cli.get(name) ?? names.get(code) ?? resolveCode(code);
    if (!href) {
      if (!missing.has(source)) missing.set(source, new Set());
      missing.get(source).add(code);
    }
    return href;
  }
  return {resolve, missing};
}

export function unresolvedReferences(missing, ambiguous = () => false) {
  return [...missing].flatMap(([file, names]) => [...names]
    .filter(name => (/^(?:[A-Z]\w*|retinue_\w+|wfcore|poe2core)$/.test(name) && ambiguous(name)) ||
      /^retinue\.(?:\w+\.)+v\d+\./.test(name) ||
      /^(?:(?:retinue[-_]\w+|wfcompanion|wfgui|poe2companion|arccompanion)::|(?:apps|crates|scripts|tools)\/[\w./-]+$|(?:[tc]:)?[a-z][\w@]*:[a-z][\w@]*\/\d+$)/.test(name))
    .map(name => file + ': ' + name));
}
