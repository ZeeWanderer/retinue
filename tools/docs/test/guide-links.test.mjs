import {test} from 'node:test';
import assert from 'node:assert/strict';
import {guideLinks, topicLinks, unresolvedReferences} from '../guide-links.mjs';
import {topic, guideDefinitions} from '../markdown.mjs';
import {prose} from '../reference.mjs';
import {url} from '../paths.mjs';

const host = app => ({kind: 'Rust', role: 'module', title: 'host',
  moduleFile: `apps/${app}/src/host.rs`, route: `reference/rust/${app}/host/index.html`});
const service = {kind: 'Erlang', role: 'Module', title: 'service', moduleFile: 'apps/daemon/src/service.erl',
  route: 'reference/erlang/daemon/service.html', groups: [{items: [
    {name: 'start/2', anchor: 'start/2'}, {name: 'state/0', anchor: 't:state/0'},
  ]}]};
const open = {kind: 'Rust', role: 'function', title: 'open', route: 'reference/rust/one/host/fn.open.html'};
const owner = {kind: 'Rust', role: 'struct', title: 'Window', route: 'reference/rust/one/host/struct.Window.html',
  groups: [{items: [{name: 'close', anchor: 'method.close'}]}]};
const files = new Set([host('one').moduleFile, host('two').moduleFile, service.moduleFile, 'scripts/fixture.erl']);
const lookup = guideLinks([host('one'), host('two'), service, open, owner], files, f => 'source/' + f);

test('guide references use module identity and actual member anchors', () => {
  assert.equal(lookup('apps/one/src/host.rs'), url(host('one').route));
  assert.equal(lookup('host.rs'), undefined);
  assert.equal(lookup('one::host'), url(host('one').route));
  assert.equal(lookup('host::open()'), url(open.route));
  assert.equal(lookup('Window::close()'), url(owner.route) + '#method.close');
  assert.equal(lookup('service.erl'), url(service.route));
  assert.equal(lookup('service:start/2'), url(service.route) + '#start/2');
  assert.equal(lookup('service:start/1'), undefined);
  assert.equal(lookup('service:state/0'), undefined);
  assert.equal(lookup('t:service:state/0'), url(service.route) + '#t:state/0');
  assert.equal(lookup('start/2'), undefined);
  assert.equal(lookup('start/2', service), url(service.route) + '#start/2');
  assert.equal(lookup('t:state/0', service), url(service.route) + '#t:state/0');
  assert.match(prose('Use `start/2`.', {}, code => lookup(code, service)), /href="[^"]+#start\/2"/);
  assert.equal(lookup('scripts/fixture.erl'), 'source/scripts/fixture.erl');
});

test('guide imports are local; glossary and schema anchors and command aliases work across guides', () => {
  const md = '# Usage\n\n[term:Updates]: #changes\n[api:Canvas]: struct@one::host::Window\n\n## Changes\n\n`Canvas`';
  const guide = {source:'docs/window.md',route:'guide', ...guideDefinitions(md)};
  const schema = {source:'schemas/example.proto',route:'reference/protocol/example',symbols:[['app.State','appstate'],['app.Window','appwindow']]};
  const links = topicLinks(lookup,[guide],[schema],[{path:['warframe','doctor']}]);
  assert.equal(links.resolve('Canvas',guide.source), url(owner.route));
  assert.equal(links.resolve('Canvas','other.md'), undefined);
  assert.equal(links.resolve('Window','other.md'), url(owner.route));
  assert.equal(links.resolve('Updates','other.md'), url('guide/#changes'));
  assert.equal(links.resolve('app.State','other.md'), url('reference/protocol/example/#appstate'));
  assert.equal(links.resolve('wfcli doctor --json','other.md'), url('reference/cli/warframe/doctor/'));
  assert.doesNotMatch(topic(md,guide.source,v=>v,c=>links.resolve(c,guide.source)).body,/term:|api:/);
  links.resolve('apps/gone/src.rs','other.md');
  assert.deepEqual(unresolvedReferences(links.missing), ['other.md: apps/gone/src.rs']);
  assert.throws(()=>topicLinks(lookup,[{...guide,imports:[['Canvas','Missing']]}],[],[]),/invalid reference import/);
  assert.throws(()=>topicLinks(lookup,[{...guide,terms:[['Window','#changes']]}],[],[]),/ambiguous shared term/);
});

test('member references survive re-exports, but homonyms must be qualified', () => {
  const field = {...owner, groups:[{items:[{name:'size',anchor:'structfield.size'}]}]};
  const alias = {kind:'Rust', title:'Pane', route:'reference/rust/two/struct.Pane.html', redirect:url(owner.route)};
  const other = {...owner, route:'reference/rust/three/struct.Window.html'};
  const lookup = guideLinks([field,alias,other],[],()=>undefined);
  assert.equal(lookup('Pane::size'), url(owner.route) + '#structfield.size');
  assert.equal(lookup('Self::size',field), url(owner.route) + '#structfield.size');
  assert.deepEqual(unresolvedReferences(new Map([['guide.md',new Set(['Window','data'])]]),lookup.ambiguous),['guide.md: Window']);
});

test('wire fields are addressable without claiming bare field names in unrelated guides', () => {
  const schema = {source:'poe.proto', route:'protocol/poe', symbols:[
    ['poe.Search.offset','poesearch-offset','Search.offset'],
    ['poe.Page.offset','poepage-offset','Page.offset'],
  ]};
  const guide = {source:'trade.md',route:'trade',imports:[['offset','poe.Page.offset']],
    terms:[['search_offset','poe.Search.offset']]};
  const {resolve} = topicLinks(lookup,[guide],[schema],[]);
  assert.equal(resolve('offset','trade.md'),url('protocol/poe/#poepage-offset'));
  assert.equal(resolve('offset','other.md'),undefined);
  assert.equal(resolve('search_offset','other.md'),url('protocol/poe/#poesearch-offset'));
  assert.equal(resolve('Page.offset','other.md'),url('protocol/poe/#poepage-offset'));
  assert.throws(()=>topicLinks(lookup,[{...guide,imports:[['offset','poe.Removed.offset']]}],[schema],[]),/invalid reference import/);
});

test('an Erlang application and module can share a name without silently choosing either', () => {
  const app = {kind:'Erlang', role:'Application', title:service.title,
    route:'reference/erlang/service/api-reference.html'};
  const resolve = guideLinks([service, app],[],()=>undefined);
  assert.equal(resolve('service'), undefined);
  assert.equal(resolve('app:service'), url(app.route));
  assert.equal(resolve('m:service'), url(service.route));
});

test('only prose code is linked; deliberate links, headings and examples survive', () => {
  const md = '# Title\n\n## `service`\n\n`service` [`service`](elsewhere) `retinue doctor`\n\n```erlang\nservice:start/2\n```';
  const {body} = topic(md, 'doc.md', v => v, lookup);
  assert.equal(body.split(url(service.route)).length - 1, 1);
  assert.match(body, /\[`service`\]\(elsewhere\)/);
  assert.match(body, /## `service`/);
  assert.match(body, /\nservice:start\/2\n/);
});
