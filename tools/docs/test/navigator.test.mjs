import assert from 'node:assert/strict';
import {test} from 'node:test';
import {load} from 'cheerio';
import {navigator, outline} from '../navigation.mjs';
import {url} from '../paths.mjs';

test('guides, crates and symbols share roots and the selected branch is expanded', () => {
  const root = 'reference/rust/example/index.html',
    module = 'reference/rust/example/layout/index.html';
  const fn = 'reference/rust/example/layout/fn.Column.html';
  const catalog = [
    {
      label: 'UI',
      items: [
        {label: 'Layout', link: url('guide/')},
        {label: 'SDK', link: url(root)},
      ],
    },
    {label: 'Elsewhere', link: url('elsewhere/')},
  ];
  const symbols = [
    [root, 'example', 'module'],
    [module, 'layout', 'module'],
    [fn, 'Column', 'function'],
  ].map(([route, title, role]) => ({route, title, role}));
  const tree = outline([], symbols, catalog),
    nav = navigator(tree);
  for (const route of ['guide/', root, fn]) {
    const $ = load(nav.forPage(url(route)), null, false);
    assert.deepEqual(
      $('ul')
        .first()
        .children()
        .toArray()
        .map((n) => $(n).children('details').children('summary').text() || $(n).children('a').text()),
      ['UI', 'Elsewhere'],
    );
    const selected = $('a[aria-current="page"]');
    assert.equal(selected.attr('href'), url(route));
    assert.equal(selected.parents('details:not([open])').length, 0);
  }
  const guide = load(nav.forPage(url('guide/')), null, false);
  const sdk = guide('summary a')
    .filter((_, a) => guide(a).attr('href') === url(root))
    .parent()
    .parent();
  assert.ok(sdk.children('.outline-fallback').length);
  const payload = nav.render(nav.branches.get(sdk.attr('data-branch')).items);
  assert.equal(load(payload)('summary a').attr('href'), url(module));
});

test('curated symbols and guides have one home under visible topic headings', () => {
  const root = 'reference/rust/sdk/index.html', mod = 'reference/rust/sdk/authoring/index.html';
  const run = 'reference/rust/sdk/authoring/fn.run.html';
  const symbols = [{route:root, title:'sdk', role:'module', topicGroups:[{title:'Getting started', links:[url(run)]}]},
    {route:mod, title:'authoring', role:'module'}, {route:run, title:'run', role:'function'}];
  const tree = outline([], symbols, [{marker:'Frameworks'}, {label:'sdk', link:url(root),
    items:[{label:'First steps', link:url('guide/')}] }]);
  const nav = navigator(tree);
  const $ = load(nav.forPage(url(run)));
  assert.deepEqual($('.outline-marker').map((_,n)=>$(n).text()).get(), ['Frameworks','Topics','Getting started','Modules']);
  const selected = $('[aria-current=page]');
  assert.equal(selected.parents('details').length, 1);
  assert.equal(selected.parents('details').find('a[href="'+url('guide/')+'"]').text(), 'First steps');
  assert.deepEqual(nav.breadcrumbs(url(run)).map(n=>n.label), ['sdk']);
  assert.equal(load(nav.render(tree,'',new Set(),'all'))('a[href="'+url(run)+'"]').length, 1);
});
