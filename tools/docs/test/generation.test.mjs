import {test} from 'node:test';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {load} from 'cheerio';
import {topic, linkResolver} from '../markdown.mjs';
import {protocolPages} from '../protocol.mjs';
import {integrate} from '../native.mjs';
import {root, basePath} from '../paths.mjs';
import {route} from '../prepare.mjs';
import {navigationErrors} from '../check.mjs';

test('research documentation is publishable while raw evidence remains ignored', () => {
  const ignored = file => spawnSync('git', ['check-ignore', '--no-index', file], {cwd: root, encoding: 'utf8'});
  assert.equal(ignored('docs/research/index.md').status, 1);
  assert.equal(ignored('research/capture.bin').status, 0);
});

test('navigation validation rejects empty groups and unlinked topics', () => {
  const $ = load(`<nav id="starlight__sidebar"><a href="${basePath}">Overview</a>
    <details><summary>Development</summary><ul></ul></details></nav>`);
  assert.deepEqual(navigationErrors($, [{route: ''}, {route: 'developer/testing'}]), [
    'Empty sidebar group: Development',
    `Missing sidebar link: ${basePath}developer/testing/`,
  ]);
  assert.deepEqual(navigationErrors(load('<main>Overview</main>'), []), ['Missing documentation sidebar']);
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
  assert.equal(route('README.md'), 'guides/overview');
  assert.equal(route('docs/installation.md'), 'guides/installation');
  assert.equal(route('docs/research/index.md'), 'research');
  assert.equal(route('docs/DEVELOPER.md'), 'developer');
  assert.equal(route('docs/developer/ui-sdk.md'), 'developer/ui-sdk');
});

test('Markdown conversion preserves code while resolving real links and images', () => {
  const resolve = linkResolver(new Map([['docs/gui.md', 'guides/gui'], ['docs/index.md', '']]), {
    base: '/retinue/', repository: 'https://github.com/owner/repo', ref: 'abc',
    assets: new Set(['docs/images/gui.webp']),
  });
  const result = topic('# Example\n\n[GUI](gui.md#capture)\n\n[Home](index.md)\n\n![GUI](images/gui.webp)\n\n`[not a link](gui.md)`\n\n<img src="images/gui.webp" width="200">', 'docs/test.md', resolve);
  assert.equal(result.title, 'Example');
  assert.match(result.body, /\/retinue\/guides\/gui\/#capture/);
  assert.match(result.body, /\[Home\]\(\/retinue\/\)/);
  assert.match(result.body, /`\[not a link\]\(gui.md\)`/);
  assert.match(result.body, /src="\/retinue\/docs\/images\/gui.webp"/);
  assert.equal(resolve('../crates/file.rs', 'docs/test.md'), 'https://github.com/owner/repo/blob/abc/crates/file.rs');
  assert.throws(() => resolve('../research/capture.bin', 'docs/test.md'), /Unpublished evidence/);
});

test('Protobuf reflection keeps field numbers, oneofs and cross-schema type links', async () => {
  const pages = await protocolPages(root, route => '/retinue/' + route);
  assert.equal(pages.length, 4);
  const envelope = pages.find(page => page.title === 'retinue_wire.proto');
  assert.match(envelope.body, /\| `request` \| 1 \|/);
  assert.match(envelope.body, /oneof body/);
  assert.match(envelope.body, /\[retinue.wire.v1.Request\]/);
  const game = pages.find(page => page.title === 'wf_native.proto');
  assert.match(game.body, /reference\/protocol\/retinue_wire\/#/);
  assert.match(game.body, /\| optional \|/);
});

test('Native API integration preserves item IDs, scripts and language navigation', () => {
  const html = '<html><head><title>View</title></head><body><nav class="sidebar">Local</nav><main><section><h1 class="main-heading">View</h1><div id="tymethod.body">Body</div></section></main><script src="impl.js"></script></body></html>';
  const $ = load(integrate(html, 'Rust'));
  assert.equal($('#tymethod\\.body').text(), 'Body');
  assert.equal($('script').attr('src'), 'impl.js');
  assert.equal($('.sidebar').text(), 'Local');
  assert.equal($('[data-pagefind-body]').length, 1);
  assert.equal($('.retinue-portal a').last().attr('href'), basePath + 'search/');
  const exdoc = load(integrate('<main id="content"><a href="https://hexdocs.pm/wfcore/wf_contract.html">contract</a></main>', 'Erlang'));
  assert.equal(exdoc('#content a').attr('href'), basePath + 'reference/erlang/wfcore/wf_contract.html');
});
