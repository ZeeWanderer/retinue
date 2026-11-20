import {plot, line, dot, text, tip, pointerX} from '@observablehq/plot';

const panel = document.querySelector('#history');
const [suite, group, workload, metric, axis] = panel.querySelectorAll('select');
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
  series,
  points,
  values,
  revision = 0,
  chosen,
  chartWidth;
const pretty = (n) => format.format(n) + (unit(metric.value) ? ' ' + unit(metric.value) : '');
const detail = (d) =>
  'Session ' +
  d.ordinal +
  ' · ' +
  pretty(d.value) +
  ' · ' +
  utc(d.date) +
  ' UTC · ' +
  d.runs +
  ' runs · ' +
  d.revision.slice(0, 8) +
  (d.dirty ? ' + local changes' : ' (clean)') +
  ' · binary ' +
  (d.binary?.slice(0, 8) ?? 'unknown');
function link(href, label) {
  const a = document.createElement('a');
  a.href = href;
  a.textContent = label;
  return a;
}

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
  const labels = !timed && valid.length <= Math.max(1, Math.floor((width - 80) / 80)) ? valid : [];
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
      line(rows, {...mark, stroke: 'var(--sl-color-text-accent)'}),
      dot(valid, {...mark, r: 4, fill: 'var(--sl-color-text-accent)'}),
      text(labels, {...mark, text: (d) => pretty(d.value), dy: -13}),
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
  const data = values[workload.value][metric.value];
  const rows = points.map((p, i) => ({...p, ordinal: i + 1, at: new Date(p.date), value: data[i]}));
  panel.querySelector('#metric-heading').textContent = label(metric.value);
  graph(rows);
  let previous;
  const rendered = [];
  for (const d of rows) {
    if (d.value == null) {
      previous = undefined;
      continue;
    }
    const row = document.createElement('tr');
    const delta = previous == null || previous === 0 ? null : (d.value / previous - 1) * 100;
    for (const v of [
      '#' + d.ordinal + ' · ' + utc(d.date),
      pretty(d.value),
      delta == null ? '—' : (delta > 0 ? '+' : '') + delta.toFixed(1) + '%',
      d.runs,
    ])
      row.insertCell().textContent = v;
    row
      .insertCell()
      .append(
        link(panel.dataset.repository + '/commit/' + d.revision, d.revision.slice(0, 8)),
        d.dirty ? ' + local changes' : ' (clean)',
      );
    row.insertCell().textContent = d.binary?.slice(0, 8) ?? 'Unknown';
    row
      .insertCell()
      .append(
        link(
          new URL('records/' + d.id + '.json', base),
          d.id.slice(0, 8) + (d.id === series.baseline ? ' · baseline' : ''),
        ),
      );
    rendered.unshift(row);
    previous = d.value;
  }
  panel.querySelector('tbody').replaceChildren(...rendered);
  const bins = new Set(rows.filter((d) => d.value != null && d.binary).map((d) => d.binary));
  status.textContent =
    rendered.length +
    ' measured session' +
    (rendered.length !== 1 ? 's' : '') +
    (bins.size ? ' · ' + bins.size + ' distinct ' + (bins.size === 1 ? 'binary' : 'binaries') : '') +
    ' · ' +
    series.cpu;
  const locationURL = new URL(location.href);
  for (const key of ['series', 'workload', 'metric', 'axis'])
    locationURL.searchParams.set(
      key,
      {series: series.series, workload: workload.value, metric: metric.value, axis: axis.value}[key],
    );
  history.replaceState(null, '', locationURL);
}
function cases(preferred) {
  const names = Object.keys(values[workload.value]).sort();
  options(
    metric,
    names.map((k) => [k, k]),
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
    s = index.find((s) => s.series === group.value);
  values = null;
  chosen = null;
  chart.replaceChildren();
  panel.querySelector('tbody').replaceChildren();
  reading.textContent = '';
  status.textContent = 'Loading measurements…';
  workload.disabled = metric.disabled = true;
  try {
    const data = await get(new URL(s.series + '.json', base));
    if (seq !== revision) return;
    series = s;
    ({points, values} = data);
    const names = Object.keys(values).sort();
    options(
      workload,
      names.map((k) => [k, k]),
    );
    if (names.includes(requested?.get('workload'))) workload.value = requested.get('workload');
    workload.disabled = metric.disabled = false;
    cases(requested?.get('metric'));
  } catch (e) {
    if (seq === revision) status.textContent = 'Could not load measurements: ' + e.message;
  }
}
function groups(requested) {
  const sets = index
    .filter((s) => s.suite === suite.value)
    .sort((a, b) => Number(b.count > 1) - Number(a.count > 1) || b.last.localeCompare(a.last));
  options(
    group,
    sets.map((s) => [
      s.series,
      [
        s.last.slice(0, 10),
        s.threads && s.threads + ' threads',
        s.samples && s.samples + ' samples',
        s.count + ' sessions',
        s.series.slice(0, 8),
      ]
        .filter(Boolean)
        .join(' · '),
    ]),
  );
  if (sets.some((s) => s.series === requested?.get('series'))) group.value = requested.get('series');
  loadSeries(requested);
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
    index.find((s) => s.series === initial.get('series'))?.suite ??
    (suites.includes('gui-foundry') ? 'gui-foundry' : suites[0]);
  if (initial.get('axis') === 'time') axis.value = 'time';
  groups(initial);
  suite.onchange = () => groups();
  group.onchange = () => loadSeries();
  workload.onchange = () => cases(metric.value);
  metric.onchange = axis.onchange = () => {
    if (values) draw();
  };
  new ResizeObserver(() => {
    if (values && Math.round(chart.getBoundingClientRect().width) !== chartWidth) draw();
  }).observe(chart);
} catch (e) {
  status.textContent = 'Could not load measurements: ' + e.message;
}
