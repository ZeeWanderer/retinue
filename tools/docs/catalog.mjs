import {url} from './paths.mjs';

const page = (label, route) => ({label, link: url(route + (route && !route.endsWith('.html') ? '/' : ''))});
const section = (label, ...items) => [{marker: label}, ...items];
const collection = (landing, ...items) => ({...landing, items});
const rust = (name, label = name) => page(label, `reference/rust/${name}/index.html`);
const erlang = (name, label = name) => page(label, `reference/erlang/${name}/api-reference.html`);

export const sidebar = [
  page('Documentation', ''),
  ...section('Using Retinue',
    page('Install and connect', 'guides/installation'),
    collection(page('Commands and diagnostics', 'guides/cli'),
      page('Queries and watches', 'guides/query'), page('Daemon service', 'guides/daemon'),
      page('MCP integration', 'guides/mcp'), page('Inspect games', 'guides/wfinspect'),
      collection(page('Command reference', 'reference/cli'), {autogenerate: {directory: 'reference/cli'}}))),
  ...section('Games',
    collection(page('Warframe', 'guides/companion'), page('Desktop app', 'guides/gui'),
      page('Forma planning', 'guides/forma-plan'), page('Build calculator', 'guides/build-stats')),
    page('Path of Exile 2', 'guides/poe2'), page('ARC Raiders', 'guides/arc')),
  ...section('Frameworks and services',
    collection(rust('retinue_ui'),
      ...section('Essentials', page('Framework overview', 'developer/ui-sdk'), page('Build a view', 'ui/first-view'),
        page('Views and identity', 'ui/views'), page('State and bindings', 'ui/state'),
        page('Inherited environment', 'ui/environment'), page('Layout', 'developer/declarative-layout'),
        page('Text, images and drawing', 'developer/ui-drawing'), page('Animation and clocks', 'ui/animation'),
        page('Input and focus', 'ui/input'), page('Window and overlay hosts', 'ui/hosts'),
        page('Run the view demo', 'research/ui-composition-prototype')),
      ...section('Implementation', page('Composition', 'developer/ui-composition'), page('State and ownership', 'developer/ui-contracts'),
        page('Motion scheduling', 'developer/ui-motion'), page('Custom drawing', 'developer/ui-drawing-design')),
      ...section('Macros', rust('retinue_ui_macros'))),
    collection(rust('retinue_render'), page('Pipeline and representation', 'developer/render-pipeline'),
      page('Resources and residency', 'developer/render-residency'), page('Relocation and repair', 'developer/render-relocate'),
      page('Invalidation', 'developer/ui-invalidation'), page('Assets', 'developer/assets'),
      page('Profile and measure', 'developer/profiling'), page('Optimizer quality', 'developer/optimizer-quality'),
      page('Benchmark workflows', 'developer/benchmarks'), page('Performance history', 'research/benchmarks'),
      page('Blend2D reference tools', 'developer/rendering-reference')),
    collection(erlang('retinue_daemon'), page('Daemon lifecycle', 'developer/daemon'),
      page('Health reporting', 'developer/health'), page('Query implementation', 'developer/query-language'),
      ...section('Warframe data', page('Canonical entities', 'developer/entities'), page('Data sources', 'guides/data-sources'),
        page('World state', 'developer/worldstate'), page('Public exports', 'developer/exports'),
        page('Knowledge catalog', 'developer/knowledge'), page('Forma solver', 'developer/forma_plan'))),
    collection(erlang('retinue_cli'), page('CLI implementation', 'developer/cli'), page('Terminal tables', 'developer/table_layout')),
    collection(erlang('retinue_core'), page('Shared contracts', 'developer/shared_utilities'),
      ...section('Wire protocol', {autogenerate: {directory: 'reference/protocol'}})),
    rust('retinue_client'), rust('retinue_native'), rust('retinue_inspect')),
  ...section('Game integrations',
    collection(rust('wfcompanion'), page('Companion lifecycle', 'developer/companion'), page('Overlay implementation', 'developer/overlay-rendering')),
    collection(rust('wfgui'), page('Desktop implementation', 'developer/native-ui'), page('GUI source organization', 'developer/gui-organization')),
    collection(rust('retinue_warframe'), page('Player inventory', 'developer/player-data')),
    erlang('wfcore'), rust('poe2companion'), rust('retinue_poe2'), erlang('poe2core'), rust('arccompanion')),
  ...section('Project',
    collection(page('Contributing', 'developer'), page('System architecture', 'developer/structure'), page('Build workflows', 'developer/workflows'),
      page('Testing', 'developer/testing'), page('Adding features', 'developer/adding_features'),
      page('Overlay source organization', 'developer/companion-presentation'),
      page('Development tools', 'developer/tools'), page('Plan visualization', 'developer/visualize'),
      page('Documentation authoring', 'developer/documentation'), page('UI previews', 'developer/ui-previews')),
    page('API catalog', 'reference'),
    collection(page('Research', 'research'),
      page('Native analysis tools', 'research/native-analysis'),
      page('Warframe provider data flow', 'research/overwolf-alecaframe-overlay-research'),
      page('AlecaFrame overlay catalog', 'research/alecaframe-overlay-catalog'),
      page('WFCD parser audit', 'research/wfcd-parser-audit'), page('Blend2D findings', 'research/blend2d-issues'))),
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

const erlangGuides = {retinue_core: 'developer/shared_utilities', retinue_daemon: 'developer/daemon',
  retinue_cli: 'developer/cli', wfcore: 'developer/knowledge', poe2core: 'guides/poe2'};

export function relatedTopics(route) {
  if (route.startsWith('reference/rust/')) {
    const symbol = route.slice('reference/rust/'.length);
    return relations.filter(([, , prefixes]) => prefixes.some(prefix => symbol.startsWith(prefix)))
      .map(([guide, label]) => page(label, guide));
  }
  const app = route.split('/')[2];
  if(route==='reference/erlang/retinue_daemon/wf_query_parse.html')
    return [page('Query syntax and examples','guides/query'),page('Query implementation','developer/query-language')];
  const guide = erlangGuides[app];
  return guide ? [page('Concepts and usage', guide)] : [];
}

export function relatedAPI(route) {
  return relations.filter(([guide]) => guide === route).flatMap(([, , symbols]) => symbols.map(symbol => {
    const module = symbol.endsWith('/');
    return page(module ? symbol.replace(/\/$/, '').replaceAll('/', '::') : symbol.split('/').at(-1).split('.').slice(1).join('.'),
      `reference/rust/${symbol}${module ? 'index' : ''}.html`);
  })).concat(Object.entries(erlangGuides).filter(([, guide]) => guide === route)
    .map(([app]) => erlang(app)));
}
