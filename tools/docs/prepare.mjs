import fs from 'node:fs/promises';
import path from 'node:path';
import {execFileSync} from 'node:child_process';
import {root, work, content, repository, basePath, url, erlangApps, rustCrates} from './paths.mjs';
import {topic, linkResolver} from './markdown.mjs';
import {protocolPages} from './protocol.mjs';

const researchTopics = new Set([
  'alecaframe-overlay-catalog', 'overwolf-alecaframe-overlay-research', 'wfcd-parser-audit',
  'ui-composition-prototype', 'blend2d-issues',
]);

export function route(source) {
  const name = source.replace(/\.md$/, '');
  if (source === 'README.md') return 'guides/overview';
  if (source === 'docs/index.md') return '';
  if (source === 'docs/search.md') return 'search';
  if (source === 'docs/DEVELOPER.md') return 'developer';
  if (researchTopics.has(path.basename(name))) return 'research/' + path.basename(name);
  if (source.startsWith('docs/developer/') || source.startsWith('docs/research/')) return name.slice(5).replace(/\/index$/, '');
  return 'guides/' + name.slice(5);
}

async function files(directory) {
  const entries = await fs.readdir(directory, {withFileTypes: true});
  const nested = await Promise.all(entries.map(async entry => {
    const name = path.join(directory, entry.name);
    return entry.isDirectory() ? files(name) : [name];
  }));
  return nested.flat();
}

export async function prepare() {
  await fs.rm(content, {recursive: true, force: true});
  await fs.mkdir(content, {recursive: true});
  const ref = process.env.DOCS_SOURCE_REF ?? execFileSync('git', ['rev-parse', 'HEAD'], {cwd: root, encoding: 'utf8'}).trim();
  const sources = ['README.md', ...(await files(path.join(root, 'docs'))).filter(p => p.endsWith('.md')).map(p => path.relative(root, p))];
  const routes = new Map(sources.map(source => [source, route(source)]));
  if (new Set(routes.values()).size !== routes.size) throw new Error('Duplicate documentation routes');
  const assets = new Set((await files(path.join(root, 'docs/images'))).map(p => path.relative(root, p)));
  const resolve = linkResolver(routes, {base: basePath, repository, ref, assets});
  const pages = [];
  async function write(page) {
    const target = path.join(content, (page.route || 'index') + '.md');
    await fs.mkdir(path.dirname(target), {recursive: true});
    const frontmatter = {title: page.title, editUrl: page.source ? `${repository}/edit/master/${page.source}` : false};
    const header = Object.entries(frontmatter).map(([key, value]) => `${key}: ${JSON.stringify(value)}`).join('\n');
    await fs.writeFile(target, `---\n${header}\n---\n\n${page.body}`);
    pages.push({route: page.route, title: page.title, source: page.source});
  }
  for (const source of sources) {
    const markdown = await fs.readFile(path.join(root, source), 'utf8');
    await write({...topic(markdown, source, resolve), source, route: routes.get(source)});
  }
  const cli = JSON.parse(await fs.readFile(path.join(work, 'cli.json'), 'utf8'));
  for (const command of cli) {
    const name = ['retinue', ...command.path].join(' ');
    const commandRoute = 'reference/cli' + (command.path.length ? '/' + command.path.join('/') : '');
    const children = cli.filter(child => child.path.length === command.path.length + 1 && command.path.every((value, i) => child.path[i] === value));
    const links = children.map(child => `- [${child.path.at(-1)}](${url('reference/cli/' + child.path.join('/') + '/')})`).join('\n');
    await write({route: commandRoute, title: name,
      body: `${command.summary}\n\nGenerated from the same command tree as help and completion.\n\n~~~text\n${command.help}\n~~~\n\n${links}\n`});
  }
  for (const page of await protocolPages(root, url)) {
    page.body = page.body.replace('SOURCE_URL', `${repository}/blob/${ref}/${page.source}`);
    await write(page);
  }
  await write({route: 'reference', title: 'Reference', body: `API pages come from the current checkout. Guides explain contracts and workflows; signatures are generated.\n
## Rust\n\n${rustCrates.map(name => `- [${name}](${url(`reference/rust/${name}/index.html`)})`).join('\n')}
## Erlang\n\n${erlangApps.map(name => `- [${name}](${url(`reference/erlang/${name}/api-reference.html`)})`).join('\n')}
## Commands And Protocol\n\n- [Retinue commands](${url('reference/cli/')})\n${(await protocolPages(root, url)).map(page => `- [${page.title}](${url(page.route + '/')})`).join('\n')}
`});
  await fs.writeFile(path.join(work, 'pages.json'), JSON.stringify(pages, null, 2));
  return {pages, assets};
}
