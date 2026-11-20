const panel = document.querySelector('#history');
const [suite, group, workload, metric] = panel.querySelectorAll('select');
const status = panel.querySelector('[role=status]');
const base = new URL(panel.dataset.index, location.href);
const options = (select, pairs) =>
  select.replaceChildren(...pairs.map(([value, label]) => new Option(label, value)));
const get = async (url) => {
  const res = await fetch(url);
  if (!res.ok) throw Error(`HTTP ${res.status}`);
  return res.json();
};
let index,
  series,
  points,
  values,
  revision = 0;
function draw() {
  const data = values[workload.value][metric.value],
    svg = panel.querySelector('svg');
  const nums = Object.values(data),
    lo = Math.min(...nums),
    hi = Math.max(...nums);
  const dates = points.map((p) => Date.parse(p.date));
  const x = (i) => 10 + (740 * (dates[i] - dates[0])) / (dates.at(-1) - dates[0] || 1);
  const y = (n) => (hi === lo ? 90 : 167 - (154 * (n - lo)) / (hi - lo));
  const node = (tag, attrs) => {
    const e = document.createElementNS(svg.namespaceURI, tag);
    for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
    return e;
  };
  svg.replaceChildren();
  const rows = [];
  let prev;
  for (const [i, p] of points.entries()) {
    const n = data[i];
    if (n == null) {
      prev = undefined;
      continue;
    }
    if (prev != null)
      svg.append(
        node('line', {
          x1: x(i - 1),
          y1: y(prev),
          x2: x(i),
          y2: y(n),
          stroke: 'currentColor',
          'stroke-width': 2,
        }),
      );
    const dot = node('circle', {cx: x(i), cy: y(n), r: 4, fill: 'currentColor'});
    const title = node('title', {});
    title.textContent = p.date + ': ' + n;
    dot.append(title);
    svg.append(dot);
    const row = document.createElement('tr');
    for (const text of [
      p.date.slice(0, 19).replace('T', ' '),
      n.toLocaleString(undefined, {maximumSignificantDigits: 6}),
      prev == null || prev === 0 ? '—' : ((n / prev - 1) * 100).toFixed(1) + '%',
      p.runs,
    ])
      row.insertCell().textContent = text;
    const a = document.createElement('a');
    a.href = new URL('records/' + p.id + '.json', base);
    a.textContent =
      p.revision.slice(0, 8) +
      (p.dirty ? ' (modified)' : '') +
      (p.id === series.baseline ? ' · baseline' : '');
    row.insertCell().append(a);
    rows.unshift(row);
    prev = n;
  }
  panel.querySelector('tbody').replaceChildren(...rows);
  status.textContent =
    metric.value +
    ' · ' +
    rows.length +
    ' measured sessions · range ' +
    lo.toPrecision(4) +
    ' to ' +
    hi.toPrecision(4);
}
function cases() {
  const names = Object.keys(values[workload.value]).sort();
  options(
    metric,
    names.map((k) => [k, k]),
  );
  if (names.includes('frame_p50_ms')) metric.value = 'frame_p50_ms';
  draw();
}
async function selectGroup() {
  const seq = ++revision,
    s = index.find((s) => s.series === group.value);
  status.textContent = 'Loading measurements…';
  try {
    const data = await get(new URL(s.series + '.json', base));
    if (revision !== seq) return;
    series = s;
    ({points, values} = data);
    options(
      workload,
      Object.keys(values)
        .sort()
        .map((k) => [k, k]),
    );
    cases();
  } catch (e) {
    if (revision === seq) status.textContent = 'Could not load measurements: ' + e.message;
  }
}
function groups() {
  const sets = index
    .filter((s) => s.suite === suite.value)
    .sort((a, b) => Number(b.count > 1) - Number(a.count > 1) || b.last.localeCompare(a.last));
  options(
    group,
    sets.map((s) => [s.series, [s.last.slice(0, 10), s.threads && s.threads+' threads',
      s.samples && s.samples+' samples', s.count+' sessions', s.series.slice(0, 8)].filter(Boolean).join(' · ')]),
  );
  selectGroup();
}
try {
  index = await get(base);
  if (!index.length) throw Error('No archived measurements');
  options(
    suite,
    [...new Set(index.map((s) => s.suite))].sort().map((k) => [k, k]),
  );
  if (index.some((s) => s.suite === 'gui-foundry')) suite.value = 'gui-foundry';
  groups();
  suite.onchange = groups;
  group.onchange = selectGroup;
  workload.onchange = cases;
  metric.onchange = draw;
} catch (e) {
  status.textContent = 'Could not load measurements: ' + e.message;
}
