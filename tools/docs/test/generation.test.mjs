import {test} from 'node:test';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {mkdtempSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {load} from 'cheerio';
import {topic, linkResolver} from '../markdown.mjs';
import {protocolPages} from '../protocol.mjs';
import {root, basePath, erlangApps} from '../paths.mjs';
import {prose, erlangPages} from '../reference.mjs';
import {publicFiles} from '../public-files.mjs';
import {route} from '../prepare.mjs';
import {navigationErrors, hasAnchor} from '../check.mjs';
import {relatedAPI, relatedTopics, breadcrumbs} from '../catalog.mjs';

test('research documentation is publishable while raw evidence remains ignored', () => {
  const ignored = (file) =>
    spawnSync('git', ['check-ignore', '--no-index', file], {cwd: root, encoding: 'utf8'});
  assert.equal(ignored('docs/research/index.md').status, 1);
  assert.equal(ignored('research/capture.bin').status, 0);
});

test('compiler-resolved shortcuts link; unresolved prose does not invent a target', () => {
  assert.match(
    prose('Read [State].', {State: '/retinue/reference/state/'}),
    /href="\/retinue\/reference\/state\/"/,
  );
  assert.doesNotMatch(prose('Read [Foreign](missing-type).'), /href=/);
});

test('Erlang distinguishes arity and member kind and connects a behaviour in both directions', () => {
  const modules = erlangApps.map((app) => ({
    app,
    name: app,
    description: '',
    members: [],
    behaviours: [],
  }));
  const [contract, impl] = modules;
  contract.members = ['type', 'function', 'callback'].map((kind) => ({kind, name: 'open', arity: 1}));
  impl.behaviours = [contract.name];
  const pages = erlangPages(modules, () => undefined);
  const find = (m) => pages.find((p) => p.role === 'Module' && p.title === m.name);
  assert.deepEqual(
    find(contract)
      .groups.flatMap((g) => g.items.map((i) => i.anchor))
      .filter(Boolean),
    ['open/1', 't:open/1', 'c:open/1'],
  );
  assert.equal(find(contract).groups.at(-1).items[0].name, impl.name);
  assert.equal(find(impl).groups.at(-1).items[0].name, contract.name);
});

test('public repository links must resolve even if the target is outside the usual ignore directories', () => {
  const resolve = linkResolver(new Map(), {
    base: '/',
    assets: new Set(),
    repository: 'https://example.org',
    ref: 'abc',
    published: new Set(['src/ok.rs']),
  });
  assert.match(resolve('../src/ok.rs', 'docs/topic.md'), /src\/ok.rs$/);
  assert.throws(() => resolve('../dev/local.log', 'docs/topic.md'), /Not a public repository path/);
});

test('publishing excludes untracked drafts while previews still exclude ignored evidence', (t) => {
  const dir = mkdtempSync(join(tmpdir(), 'retinue-doc-files-'));
  t.after(() => rmSync(dir, {recursive: true, force: true}));
  assert.equal(spawnSync('git', ['init', '-q'], {cwd: dir}).status, 0);
  for (const [name, body] of [
    ['.gitignore', 'secret.log\n'],
    ['published.rs', ''],
    ['draft.rs', ''],
    ['secret.log', ''],
  ])
    writeFileSync(join(dir, name), body);
  assert.equal(spawnSync('git', ['add', '.gitignore', 'published.rs'], {cwd: dir}).status, 0);
  assert.deepEqual([...publicFiles(dir, true)].sort(), ['.gitignore', 'published.rs']);
  assert.deepEqual([...publicFiles(dir, false)].sort(), ['.gitignore', 'draft.rs', 'published.rs']);
});

test('navigation validation rejects empty groups and unlinked topics', () => {
  const $ = load(`<nav id="starlight__sidebar"><a href="${basePath}">Overview</a>
    <details><summary>Development</summary><ul></ul></details></nav>`);
  assert.deepEqual(navigationErrors($, [{route: ''}, {route: 'developer/testing'}]), [
    'Empty sidebar group: Development',
    `Missing sidebar link: ${basePath}developer/testing/`,
  ]);
  assert.deepEqual(navigationErrors(load('<main>Overview</main>'), []), [
    'Missing documentation sidebar',
  ]);
});

test('navigation validation accepts collapsed nested links and a separate search page', () => {
  const $ = load(`<nav id="starlight__sidebar"><a href="${basePath}">Overview</a>
    <details><summary>Reference</summary><ul><li><details><summary>CLI</summary>
      <a href="${basePath}reference/cli/">Commands</a>
    </details></li></ul></details></nav>`);
  assert.deepEqual(navigationErrors($, [{route: ''}, {route: 'reference/cli'}, {route: 'search'}]), []);
});

test('topic index routes match the static site directory routes', () => {
  assert.equal(route('docs/index.md'), '');
  assert.equal(route('README.md'), '');
  assert.equal(route('docs/installation.md'), 'guides/installation');
  assert.equal(route('docs/research/index.md'), 'research');
  assert.equal(route('docs/DEVELOPER.md'), 'developer');
  assert.equal(route('docs/developer/ui-sdk.md'), 'developer/ui-sdk');
  assert.equal(route('docs/ui/state.md'), 'ui/state');
  assert.equal(route('tools/benchmarks/README.md'), 'developer/benchmarks');
});

test('Markdown conversion preserves code while resolving real links and images', () => {
  const resolve = linkResolver(
    new Map([
      ['docs/gui.md', 'guides/gui'],
      ['docs/index.md', ''],
    ]),
    {
      base: '/retinue/',
      repository: 'https://github.com/owner/repo',
      ref: 'abc',
      assets: new Set(['docs/images/gui.webp']),
    },
  );
  const result = topic(
    '# Example\n\n[GUI](gui.md#capture)\n\n[Home](index.md)\n\n![GUI](images/gui.webp)\n\n`[not a link](gui.md)`\n\n<img src="images/gui.webp" width="200">',
    'docs/test.md',
    resolve,
  );
  assert.equal(result.title, 'Example');
  assert.match(result.body, /\/retinue\/guides\/gui\/#capture/);
  assert.match(result.body, /\[Home\]\(\/retinue\/\)/);
  assert.match(result.body, /`\[not a link\]\(gui.md\)`/);
  assert.match(result.body, /src="\/retinue\/docs\/images\/gui.webp"/);
  assert.equal(
    resolve('../crates/file.rs', 'docs/test.md'),
    'https://github.com/owner/repo/blob/abc/crates/file.rs',
  );
  assert.throws(() => resolve('../research/capture.bin', 'docs/test.md'), /Unpublished evidence/);
  assert.equal(
    resolve('https://zeewanderer.github.io/retinue/ui/state/#bindings', 'docs/test.md'),
    '/retinue/ui/state/#bindings',
  );
});

test('Protobuf reflection keeps field numbers, oneofs and cross-schema type links', async () => {
  const pages = await protocolPages(root, (route) => '/retinue/' + route);
  assert.equal(pages.length, 4);
  const envelope = pages.find((page) => page.title === 'retinue_wire.proto');
  assert.match(envelope.body, /\| `request` \| 1 \|/);
  assert.match(envelope.body, /oneof body/);
  assert.match(envelope.body, /\[retinue.wire.v1.Request\]/);
  const game = pages.find((page) => page.title === 'wf_native.proto');
  assert.match(game.body, /reference\/protocol\/retinue_wire\/#/);
  assert.match(game.body, /\| optional \|/);
});

test('guide and API relationships keep navigation in its subsystem', () => {
  assert(
    relatedAPI('ui/state').some((link) => link.link === basePath + 'reference/rust/retinue_ui/state/'),
  );
  assert(
    relatedTopics('reference/rust/retinue_ui/state/struct.State.html').some(
      (link) => link.link === basePath + 'ui/state/',
    ),
  );
  assert.equal(breadcrumbs('ui/state')[0].label, 'UI Framework');
  assert.equal(breadcrumbs('reference/rust/retinue_ui/state/struct.State')[0].label, 'UI Framework');
  assert.equal(
    breadcrumbs('reference/rust/retinue_ui/state/struct.State')[0].link,
    basePath + 'developer/ui-sdk/',
  );
});

test('anchor checks handle Rust source ranges and escaped implementation IDs', () => {
  const ids = new Set(['1', '8', 'impl%3CFoo%3E']);
  assert(hasAnchor(ids, '#1-8'));
  assert(hasAnchor(ids, '#impl%3CFoo%3E'));
  assert(!hasAnchor(ids, '#1-9'));
  assert(!hasAnchor(ids, '#missing'));
});
