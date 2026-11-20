import fs from 'node:fs/promises';
import path from 'node:path';
import {root, work, content, repository, basePath, url, sourceRevision} from './paths.mjs';
import {topic, linkResolver} from './markdown.mjs';
import {protocolPages} from './protocol.mjs';
import {relatedAPI} from './catalog.mjs';
import {publicFiles} from './public-files.mjs';

const researchTopics = new Set([
  'alecaframe-overlay-catalog', 'overwolf-alecaframe-overlay-research', 'wfcd-parser-audit',
  'ui-composition-prototype', 'blend2d-issues',
]);

const toolTopics = new Map([
  ['tools/benchmarks/README.md', 'developer/benchmarks'],
  ['tools/ui-reference/README.md', 'developer/ui-previews'],
  ['tools/ghidra/README.md', 'research/native-analysis'],
  ['tools/rendering-docs/README.md', 'developer/rendering-reference'],
]);

export function route(source) {
  if (toolTopics.has(source)) return toolTopics.get(source);
  const name = source.replace(/\.md$/, '');
  if (source === 'README.md') return '';
  if (source === 'docs/index.md') return '';
  if (source === 'docs/search.md') return 'search';
  if (source === 'docs/DEVELOPER.md') return 'developer';
  if (source.startsWith('docs/ui/')) return name.slice(5);
  if (researchTopics.has(path.basename(name))) return 'research/' + path.basename(name);
  if (source.startsWith('docs/developer/') || source.startsWith('docs/research/')) return name.slice(5).replace(/\/index$/, '');
  return 'guides/' + name.slice(5);
}

export async function prepare() {
  await fs.rm(content, {recursive: true, force: true});
  await fs.mkdir(content, {recursive: true});
  const ref = sourceRevision();
  const published = publicFiles();
  const sources = [...published].filter(p => p.startsWith('docs/') && p.endsWith('.md')).sort();
  sources.push(...toolTopics.keys());
  const routes = new Map(sources.map(source => [source, route(source)]));
  if (new Set(routes.values()).size !== routes.size) throw new Error('Duplicate documentation routes');
  routes.set('README.md', '');
  const assets = new Set([...published].filter(p=>p.startsWith('docs/images/')));
  for(const file of [...published]) for(let dir=path.dirname(file);dir!=='.';dir=path.dirname(dir)) published.add(dir);
  const resolve = linkResolver(routes, {base: basePath, repository, ref, assets, published});
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
    const page = {...topic(markdown, source, resolve), source, route: routes.get(source)};
    const related = relatedAPI(page.route);
    if (related.length) page.body += '\n## Related API\n\n' + related.map(item => `- [${item.label}](${item.link})`).join('\n') + '\n';
    await write(page);
  }
  const cli = JSON.parse(await fs.readFile(path.join(work, 'cli.json'), 'utf8'));
  for (const command of cli) {
    const name = ['retinue', ...command.path].join(' ');
    const commandRoute = 'reference/cli' + (command.path.length ? '/' + command.path.join('/') : '');
    const children = cli.filter(child => child.path.length === command.path.length + 1 && command.path.every((value, i) => child.path[i] === value));
    const links = children.map(child => `- [${child.path.at(-1)}](${url('reference/cli/' + child.path.join('/') + '/')})`).join('\n');
    await write({route: commandRoute, title: name,
      body: `${command.summary}\n\n[Command guide](${url('guides/cli/')})${command.path.includes('query') ? ` | [Queries and watches](${url('guides/query/')})` : ''}\n\n~~~text\n${command.help}\n~~~\n\n${children.length ? '## Subcommands\n\n' : ''}${links}\n`});
  }
  for (const page of await protocolPages(root, url)) {
    page.body = page.body.replace('SOURCE_URL', `${repository}/blob/${ref}/${page.source}`);
    await write(page);
  }
  const rustLink = name => `- [${name}](${url(`reference/rust/${name}/index.html`)})`;
  const erlangLink = name => `- [${name}](${url(`reference/erlang/${name}/api-reference.html`)})`;
  await write({route: 'reference', title: 'API Catalog', body: `Generated declarations and source links. Start with each subsystem's guide for concepts and examples.\n
## UI And Rendering\n\n[UI framework guide](${url('developer/ui-sdk/')}) | [Render pipeline](${url('developer/render-pipeline/')})\n\n${['retinue_ui', 'retinue_ui_macros', 'retinue_render'].map(rustLink).join('\n')}
## Shared Services\n\n[Architecture](${url('developer/structure/')}) | [Daemon lifecycle](${url('developer/daemon/')})\n\n${['retinue_core', 'retinue_daemon', 'retinue_cli'].map(erlangLink).join('\n')}\n${['retinue_client', 'retinue_native'].map(rustLink).join('\n')}
## Game Integrations\n\n${['retinue_warframe', 'wfcompanion', 'wfgui', 'retinue_poe2', 'poe2companion', 'arccompanion', 'retinue_inspect'].map(rustLink).join('\n')}\n${['wfcore', 'poe2core'].map(erlangLink).join('\n')}
## Commands And Protocol\n\n- [Retinue commands](${url('reference/cli/')})\n${(await protocolPages(root, url)).map(page => `- [${page.title}](${url(page.route + '/')})`).join('\n')}
`});
  pages.push({route:'research/benchmarks',title:'Performance history'});
  await fs.writeFile(path.join(work, 'pages.json'), JSON.stringify(pages, null, 2));
  return {pages, assets};
}
