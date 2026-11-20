import {plot, line, dot, text, tip, pointerX, ruleX} from '@observablehq/plot';
import {rowsFor, timeline, trendsFor} from './history-series.mjs';
import {sessionRows} from './history-table.js';

const panel = document.querySelector('#history');
const control = (name) => panel.querySelector(`select[name="${name}"]`);
const [suite, group, workload, metric, axis, reference, configuration] =
  ['suite', 'series', 'workload', 'metric', 'axis', 'reference', 'configuration'].map(control);
const status = panel.querySelector('[role=status]'),
  chart = panel.querySelector('.plot'),
  reading = panel.querySelector('.reading');
const base = new URL(panel.dataset.index, location.href);
const initial = new URLSearchParams(location.search);
const format = new Intl.NumberFormat(undefined, {maximumSignificantDigits: 6});
const options = (select, pairs) =>
  select.replaceChildren(...pairs.map(([value, label]) => new Option(label, value)));
const utc = (date) => date.slice(0, 19).replace('T', ' ');
const label = (key) =>
  ({
    frame_p50_ms: 'Median frame time',
    frame_p95_ms: '95th percentile frame time',
    frame_max_ms: 'Longest frame',
    frame_mean_ms: 'Mean frame time',
  })[key] ?? key.replaceAll('_', ' ');
const unit = (key) =>
  key.split('_').includes('ms') ? 'ms' : key.split('_').includes('bytes') ? 'bytes' : '';
const get = async (url) => {
  const r = await fetch(url);
  if (!r.ok) throw Error('HTTP ' + r.status);
  return r.json();
};
let index,
  trends,
  series,
  points,
  values,
  revision = 0,
  chosen,
  chartRows,
  chartWidth;
const pretty = (n) => format.format(n) + (unit(metric.value) ? ' ' + unit(metric.value) : '');
const middle = (d) => d.spread ? d.spread.map(pretty).join(' – ') : 'Not available';
const detail = (d) =>
  'Session ' +
  d.ordinal +
  ' · ' +
  pretty(d.value) +
  ' · ' +
  utc(d.date) +
  ' UTC · ' +
  d.runs +
  ' runs · middle 50%: ' + middle(d) + ' · ' +
  d.revision.slice(0, 8) +
  (d.dirty ? ' + local changes' : ' (clean)') +
  ' · binary ' +
  (d.binary?.slice(0, 8) ?? 'unknown') + (d.change ? ' · Changed: ' + d.change : '');
function graph(rows) {
  const valid = rows.filter((d) => d.value != null),
    timed = axis.value === 'time';
  const width = Math.max(260, Math.round(chart.getBoundingClientRect().width));
  const x = timed ? 'at' : 'ordinal',
    indices = valid.map((d) => d.ordinal);
  const ticks = Math.max(2, Math.floor((width - 85) / 70));
  const mark = {x, y: 'value'};
  const shortDates = +rows.at(-1).at - +rows[0].at < 86400000;
  const xAxis = timed
    ? {
        type: 'utc',
        ticks,
        label: 'Measurement time (UTC)',
        tickFormat: shortDates ? '%H:%M:%S' : '%b %d',
      }
    : {
        type: 'point',
        domain: rows.map((d) => d.ordinal),
        ticks: indices.filter((_, i) => i % Math.ceil(valid.length / ticks) === 0),
        label: 'Session (chronological order)',
      };
  const stride = Math.max(1, Math.ceil(valid.length / Math.max(1, Math.floor((width - 85) / 100))));
  const labels = timed ? [] : valid.filter((_, i) => (valid.length - 1 - i) % stride === 0);
  const svg = plot({
    width,
    height: 230,
    marginLeft: 60,
    marginRight: 25,
    marginTop: 30,
    marginBottom: 46,
    style: {background: 'transparent', fontFamily: 'inherit', fontSize: '12px'},
    ariaLabel: label(metric.value) + ' by session',
    x: xAxis,
    y: {grid: true, nice: true, label: unit(metric.value) || label(metric.value)},
    marks: [
      ruleX(valid.filter((d) => d.change), {x, stroke: 'var(--sl-color-gray-3)',
        strokeDasharray: '3,5', ariaLabel: (d) => 'Session ' + d.ordinal + ': changed ' + d.change}),
      line(rows, {...mark, stroke: 'var(--sl-color-text-accent)'}),
      ruleX(valid.filter((d) => d.spread), {x, y1: (d) => d.spread[0], y2: (d) => d.spread[1],
        stroke: 'var(--sl-color-text)', strokeWidth: 2, ariaLabel: (d) => 'Middle 50%: ' + middle(d)}),
      dot(valid, {...mark, r: 4, fill: 'var(--sl-color-text-accent)'}),
      text(labels, {...mark, text: (d) => pretty(d.value), dy: -13,
        fill: 'currentColor', stroke: 'var(--sl-color-bg)', strokeWidth: 5, paintOrder: 'stroke',
      }),
      tip(
        valid,
        pointerX({
          ...mark,
          title: d=>'Session '+d.ordinal+' · '+pretty(d.value)+'\n'+utc(d.date)+' UTC · '+d.runs+' runs\n'+
            d.revision.slice(0,8)+(d.dirty?' + local changes':' (clean)')+'\nBinary '+(d.binary?.slice(0,8)??'unknown'),
          fill: 'var(--sl-color-bg)',
          stroke: 'currentColor',
          maxRadius: Infinity,
        }),
      ),
    ],
  });
  chart.replaceChildren(svg);
  const dots = [...svg.querySelectorAll('circle')];
  const show = (d) => {
    chosen = d.id;
    reading.textContent = detail(d);
  };
  for (const [i, circle] of dots.entries()) {
    const d = valid[i];
    circle.setAttribute('tabindex', '0');
    circle.setAttribute('role', 'button');
    circle.setAttribute('aria-label', detail(d));
    circle.addEventListener('focus', () => show(d));
    circle.addEventListener('click', () => show(d));
    circle.addEventListener('keydown', (e) => {
      if (['ArrowRight', 'ArrowLeft', 'Home', 'End', 'Enter', ' '].includes(e.key)) {
        e.preventDefault();
        const next = {
          ArrowRight: Math.min(i + 1, dots.length - 1),
          ArrowLeft: Math.max(i - 1, 0),
          Home: 0,
          End: dots.length - 1,
        }[e.key];
        if (next != null) dots[next].focus();
        else show(d);
      }
    });
  }
  svg.addEventListener('input', () => {
    if (svg.value) show(svg.value);
  });
  if (valid.length) show(valid.find((d) => d.id === chosen) ?? valid.at(-1));
  chartWidth = width;
}

function draw() {
  const rows = rowsFor(points, workload.value, metric.value);
  chartRows = rows;
  panel.querySelector('#metric-heading').textContent = label(metric.value);
  graph(rows);
  let previous, previousClean;
  const fixed = rows.find((r) => r.id === reference.value);
  const rendered = [];
  for (const d of rows) {
    if (d.value == null) {
      previous = undefined;
      continue;
    }
    const ref = fixed ?? (reference.value === 'previous-clean' ? previousClean : previous);
    let change = '—', percent;
    if (ref?.id === d.id) change = 'Reference';
    else if (ref?.value != null) {
      const delta = d.value - ref.value;
      change = (delta > 0 ? '+' : '') + pretty(delta);
      if (ref.value !== 0) {
        const pct = 100 * delta / Math.abs(ref.value);
        percent = (pct > 0 ? '+' : '') + pct.toFixed(1) + '%';
      }
    }
    rendered.push(sessionRows(d, {median: pretty(d.value), spread: middle(d), change, percent,
      timestamp: utc(d.date), commitURL: panel.dataset.repository + '/commit/' + d.revision,
      recordURL: new URL('records/' + d.id + '.json', base)}));
    previous = d;
    if (!d.dirty) previousClean = d;
  }
  panel.querySelector('tbody').replaceChildren(...rendered.reverse().flat());
  const bins = new Set(rows.filter((d) => d.value != null && d.binary).map((d) => d.binary));
  status.textContent =
    rendered.length +
    ' measured session' +
    (rendered.length !== 1 ? 's' : '') +
    (bins.size ? ' · ' + bins.size + ' distinct ' + (bins.size === 1 ? 'binary' : 'binaries') : '') +
    ' · ' +
    series.cpu;
  const locationURL = new URL(location.href);
  for (const select of [group, configuration, workload, metric, axis, reference])
    locationURL.searchParams.set(select.name, select.value);
  history.replaceState(null, '', locationURL);
}
function cases(preferred) {
  const names = [...values.get(workload.value)].sort();
  options(
    metric,
    names.map((k) => [k, label(k)]),
  );
  metric.value = names.includes(preferred)
    ? preferred
    : names.includes('frame_p50_ms')
      ? 'frame_p50_ms'
      : names[0];
  draw();
}
async function loadSeries(requested) {
  const seq = ++revision,
    s = trends.find((s) => s.series === group.value);
  values = null;
  chosen = null;
  chart.replaceChildren();
  panel.querySelector('tbody').replaceChildren();
  reading.textContent = '';
  status.textContent = 'Loading measurements…';
  panel.querySelector('.suite-description').textContent = s.description ?? '';
  workload.disabled = metric.disabled = reference.disabled = true;
  try {
    const members = s.members.filter(c => configuration.value === 'all' || c.series === configuration.value);
    const parts = await Promise.all(members.map(async config =>
      ({config, data: await get(new URL(config.series + '.json', base))})));
    if (seq !== revision) return;
    series = s;
    ({points, metrics: values} = timeline(parts));
    options(reference, [['previous-clean', 'Previous clean session'], ['previous', 'Previous session'], ...points.map((p, i) =>
      [p.id, '#' + (i + 1) + ' · ' + p.revision.slice(0, 8) +
        (p.dirty ? ' + local changes' : ' (clean)') + (p.id === p.config.baseline ? ' · accepted baseline' : '')])]);
    const preferred = requested?.get('reference') ?? (points.some(p => !p.dirty) ? 'previous-clean' : 'previous');
    if ([...reference.options].some(o => o.value === preferred)) reference.value = preferred;
    const names = [...values.keys()].sort();
    options(
      workload,
      names.map((k) => [k, k]),
    );
    if (names.includes(requested?.get('workload'))) workload.value = requested.get('workload');
    workload.disabled = metric.disabled = reference.disabled = false;
    cases(requested?.get('metric'));
  } catch (e) {
    if (seq === revision) status.textContent = 'Could not load measurements: ' + e.message;
  }
}
function configurations(requested) {
  const {members} = trends.find(s => s.series === group.value);
  options(configuration, [['all', 'All configurations'], ...members.map(c => [c.series,
    [c.compiler?.split(' (')[0], c.last.slice(0, 10), c.series.slice(0, 8)].filter(Boolean).join(' · ')])]);
  const preferred = requested?.get('configuration') ?? requested?.get('series');
  if (members.some(c => c.series === preferred)) configuration.value = preferred;
  configuration.disabled = members.length === 1;
  loadSeries(requested);
}
function groups(requested) {
  trends = trendsFor(index.filter((s) => s.suite === suite.value));
  options(
    group,
    trends.map((s) => [
      s.series,
      [
        s.host,
        'workload v' + s.workload,
        s.threads && s.threads + ' threads',
        s.hz && s.hz + ' Hz',
        (s.adapter === 'overlay' || s.adapter === 'quality') && s.samples && s.samples + ' samples',
        s.count + ' sessions',
      ]
        .filter(Boolean)
        .join(' · '),
    ]),
  );
  const preferred = trends.find(s => s.series === requested?.get('series') ||
    s.members.some(c => c.series === requested?.get('series')));
  if (preferred) group.value = preferred.series;
  configurations(requested);
}
try {
  index = await get(base);
  if (!index.length) throw Error('No archived measurements');
  const suites = [...new Set(index.map((s) => s.suite))].sort();
  options(
    suite,
    suites.map((k) => [k, k]),
  );
  suite.value =
    index.find((s) => [s.series, s.trend].includes(initial.get('series')))?.suite ??
    (suites.includes('gui-foundry') ? 'gui-foundry' : suites[0]);
  if (initial.get('axis') === 'time') axis.value = 'time';
  groups(initial);
  suite.onchange = () => groups();
  group.onchange = () => configurations();
  configuration.onchange = () => loadSeries(new URLSearchParams(location.search));
  workload.onchange = () => cases(metric.value);
  metric.onchange = axis.onchange = reference.onchange = () => {
    if (values) draw();
  };
  new ResizeObserver(() => {
    if (values && Math.round(chart.getBoundingClientRect().width) !== chartWidth) graph(chartRows);
  }).observe(chart);
} catch (e) {
  status.textContent = 'Could not load measurements: ' + e.message;
}
