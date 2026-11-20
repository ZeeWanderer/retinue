import {url, basePath} from './paths.mjs';

const page = (label, route) => ({label, link: url(route + (route && !route.endsWith('.html') ? '/' : ''))});
const group = (label, items) => ({label, items, collapsed: true});
const rust = (name, label = name) => page(label, `reference/rust/${name}/index.html`);
const erlang = name => page(name, `reference/erlang/${name}/api-reference.html`);

export const sidebar = [
  page('Documentation', ''),
  group('Using Retinue', [
    page('Install and connect', 'guides/installation'), page('Commands and diagnostics', 'guides/cli'),
    page('Queries and watches', 'guides/query'), page('Daemon service', 'guides/daemon'),
    page('MCP integration', 'guides/mcp'), page('Inspect games', 'guides/wfinspect'),
    group('Command reference', [page('retinue', 'reference/cli'), {autogenerate: {directory: 'reference/cli'}}]),
  ]),
  group('Games', [
    group('Warframe', [page('In-game companion', 'guides/companion'), page('Desktop app', 'guides/gui'),
      page('Forma planning', 'guides/forma-plan'), page('Build calculator', 'guides/build-stats')]),
    page('Path of Exile 2', 'guides/poe2'), page('ARC Raiders status', 'guides/arc'),
    page('Data sources', 'guides/data-sources'),
  ]),
  group('UI Framework', [
    page('Overview', 'developer/ui-sdk'), page('Build a view', 'ui/first-view'),
    page('Views and identity', 'ui/views'), page('State and bindings', 'ui/state'),
    page('Inherited environment', 'ui/environment'), page('Layout', 'developer/declarative-layout'),
    page('Text, images and drawing', 'developer/ui-drawing'), page('Animation and clocks', 'ui/animation'),
    page('Input and focus', 'ui/input'), page('Window and overlay hosts', 'ui/hosts'),
    page('Run the view demo', 'research/ui-composition-prototype'),
    group('Implementation contracts', [page('Composition', 'developer/ui-composition'), page('State and ownership', 'developer/ui-contracts'),
      page('Motion scheduling', 'developer/ui-motion'), page('Custom drawing', 'developer/ui-drawing-design')]),
    group('API', [rust('retinue_ui'), rust('retinue_ui_macros')]),
  ]),
  group('Rendering', [
    page('Pipeline and representation', 'developer/render-pipeline'),
    page('Resources and residency', 'developer/render-residency'),
    page('Relocation and repair', 'developer/render-relocate'),
    page('Invalidation', 'developer/ui-invalidation'), page('Assets', 'developer/assets'),
    page('Profile and measure', 'developer/profiling'), page('Optimizer quality', 'developer/optimizer-quality'),
    page('Benchmark workflows', 'developer/benchmarks'), page('Performance history', 'research/benchmarks'),
    page('Blend2D reference tools', 'developer/rendering-reference'),
    rust('retinue_render', 'Rendering API'),
  ]),
  group('Services And Data', [
    page('System architecture', 'developer/structure'), page('Daemon lifecycle', 'developer/daemon'),
    page('Health reporting', 'developer/health'), page('CLI implementation', 'developer/cli'),
    page('Query implementation', 'developer/query-language'), page('Canonical entities', 'developer/entities'),
    page('Shared contracts', 'developer/shared_utilities'), page('Terminal tables', 'developer/table_layout'),
    group('Service API', [erlang('retinue_core'), erlang('retinue_daemon'), erlang('retinue_cli'), rust('retinue_client')]),
    group('Wire protocol', [{autogenerate: {directory: 'reference/protocol'}}]),
  ]),
  group('Game Integrations', [
    page('Companion lifecycle', 'developer/companion'), page('Player inventory', 'developer/player-data'),
    page('World state', 'developer/worldstate'), page('Public exports', 'developer/exports'),
    page('Knowledge catalog', 'developer/knowledge'), page('Forma solver', 'developer/forma_plan'),
    page('Plan visualization', 'developer/visualize'),
    group('Domain and application API', [rust('retinue_native'), rust('retinue_warframe'), erlang('wfcore'),
      rust('wfcompanion'), rust('wfgui'), rust('retinue_poe2'), erlang('poe2core'),
      rust('poe2companion'), rust('arccompanion'), rust('retinue_inspect')]),
  ]),
  group('Contributing', [
    page('Start here', 'developer'), page('Build workflows', 'developer/workflows'),
    page('Testing', 'developer/testing'), page('Adding features', 'developer/adding_features'),
    page('GUI source organization', 'developer/gui-organization'),
    page('Overlay source organization', 'developer/companion-presentation'),
    page('Desktop implementation', 'developer/native-ui'), page('Overlay implementation', 'developer/overlay-rendering'),
    page('Development tools', 'developer/tools'), page('Documentation authoring', 'developer/documentation'),
    page('UI previews', 'developer/ui-previews'),
    page('API catalog', 'reference'),
  ]),
  group('Research', [
    page('Evidence and findings', 'research'),
    page('Native analysis tools', 'research/native-analysis'),
    page('Warframe provider data flow', 'research/overwolf-alecaframe-overlay-research'),
    page('AlecaFrame overlay catalog', 'research/alecaframe-overlay-catalog'),
    page('WFCD parser audit', 'research/wfcd-parser-audit'), page('Blend2D findings', 'research/blend2d-issues'),
  ]),
];

export const relations = [
  ['developer/ui-sdk', 'UI framework', ['retinue_ui/', 'retinue_ui_macros/']],
  ['ui/first-view', 'Build a view', ['retinue_ui/authoring/trait.View', 'retinue_ui/view/struct.ViewContext', 'retinue_ui/ir/struct.InstanceGraph']],
  ['ui/views', 'Views and identity', ['retinue_ui/view/', 'retinue_ui/runtime/']],
  ['ui/state', 'State and bindings', ['retinue_ui/state/', 'retinue_ui_macros/attr.observable', 'retinue_ui_macros/derive.ViewProperties']],
  ['ui/environment', 'Inherited environment', ['retinue_ui/environment/']],
  ['developer/declarative-layout', 'Layout', ['retinue_ui/layout/', ...['HStack','VStack','ZStack','ScrollViewport'].map(n=>'retinue_ui/authoring/fn.'+n)]],
  ['developer/ui-drawing', 'Text, images and drawing', ['retinue_ui/drawing/', 'retinue_ui/image/', 'retinue_ui/view/primitive/']],
  ['ui/animation', 'Animation and clocks', ['retinue_ui/motion/', 'retinue_ui/transition/', 'retinue_ui/timeline/']],
  ['ui/input', 'Input and focus', ['retinue_ui/input/', 'retinue_ui/semantics/']],
  ['ui/hosts', 'Window and overlay hosts', ['retinue_ui/desktop/', 'retinue_ui/surface/', 'retinue_ui/presentation_buffers/']],
  ['developer/render-pipeline', 'Render pipeline', ['retinue_render/']],
  ['developer/companion', 'Companion lifecycle', ['retinue_native/', 'wfcompanion/', 'poe2companion/', 'arccompanion/']],
  ['developer/gui-organization', 'GUI organization', ['wfgui/']],
  ['guides/wfinspect', 'Inspect games', ['retinue_inspect/']],
  ['developer/player-data', 'Player inventory', ['retinue_warframe/']],
  ['guides/poe2', 'Path of Exile 2', ['retinue_poe2/']],
  ['developer/daemon', 'Daemon lifecycle', ['retinue_client/']],
];

export function relatedTopics(route) {
  if (route.startsWith('reference/rust/')) {
    const symbol = route.slice('reference/rust/'.length);
    return relations.filter(([, , prefixes]) => prefixes.some(prefix => symbol.startsWith(prefix)))
      .map(([guide, label]) => page(label, guide));
  }
  const app = route.split('/')[2];
  if(route==='reference/erlang/retinue_daemon/wf_query_parse.html')
    return [page('Query syntax and examples','guides/query'),page('Query implementation','developer/query-language')];
  const guide = ({retinue_core: 'developer/shared_utilities', retinue_daemon: 'developer/daemon',
    retinue_cli: 'developer/cli', wfcore: 'developer/knowledge', poe2core: 'guides/poe2'})[app];
  return guide ? [page('Concepts and usage', guide)] : [];
}

export function relatedAPI(route) {
  return relations.filter(([guide]) => guide === route).flatMap(([, , symbols]) => symbols.map(symbol => {
    const module = symbol.endsWith('/');
    return page(module ? symbol.replace(/\/$/, '').replaceAll('/', '::') : symbol.split('/').at(-1).split('.').slice(1).join('.'),
      `reference/rust/${symbol}${module ? 'index' : ''}.html`);
  }));
}

export function breadcrumbs(route) {
  function find(items, parents, href) {
    for (const item of items) {
      if (item.link === href) return [...parents, item];
      if (item.items) {
        const result = find(item.items, [...parents, {label: item.label, link: item.items[0]?.link}], href);
        if (result) return result;
      }
    }
  }
  const meaningful = items => items.filter(item => !item.label.endsWith('API'));
  const found = find(sidebar, [], url(route + (route && !route.endsWith('.html') ? '/' : '')));
  if (found) return meaningful(found.slice(0, -1));
  if (route.startsWith('reference/rust/')) {
    const parts = route.slice('reference/rust/'.length).split('/');
    const crate = parts[0] === 'src' ? parts[1] : parts[0];
    const owner = meaningful(find(sidebar, [], rust(crate).link) ?? [page('API catalog', 'reference'), rust(crate)]);
    if (parts[0] === 'src') return [...owner, {label: 'Source'}];
    return [...owner, ...parts.slice(1, -1).filter(p => p !== crate).map((label, i) =>
      page(label, 'reference/rust/' + parts.slice(0, i + 2).join('/') + '/index.html'))];
  }
  if (route.startsWith('reference/erlang/')) {
    const root = erlang(route.split('/')[2]);
    return meaningful(find(sidebar, [], root.link) ?? [page('API catalog', 'reference'), root]);
  }
  if (route.startsWith('reference/cli')) return [page('Commands', 'guides/cli'), page('Command reference', 'reference/cli')];
  return [page('Documentation', '')];
}

export const routeFromPath = pathname => pathname.slice(basePath.length).replace(/\/$/, '');
