function element(tag, text, className) {
  const el = document.createElement(tag);
  if (text != null) el.textContent = text;
  if (className) el.className = className;
  return el;
}

function link(url, text) {
  const a = element('a', text);
  a.href = url;
  return a;
}

export function sessionRows(d, {median, spread, change, percent, timestamp, commitURL, recordURL}) {
  const config = d.config;
  const source = link(commitURL, d.revision.slice(0, 8));
  const record = link(recordURL, d.id.slice(0, 8));
  const state = d.dirty ? 'Local changes' : 'Clean';
  const baseline = d.id === config.baseline;
  const row = element('tr');
  function cell(className, ...content) {
    const td = row.insertCell();
    td.className = className;
    td.append(...content);
  }
  const toggle = element('button', '#' + d.ordinal, 'session-toggle');
  toggle.type = 'button';
  toggle.setAttribute('aria-label', 'Details for session ' + d.ordinal);
  const time = element('time', timestamp, 'secondary');
  time.dateTime = d.date;
  cell('session', toggle, time);
  cell('median', median);
  cell('spread', spread);
  cell('change', change);
  if (percent) row.lastElementChild.append(element('span', percent, 'secondary'));
  cell('runs', d.runs);
  cell('source', source, element('span', state, 'secondary'));
  cell('record', record);
  if (baseline) row.lastElementChild.append(element('span', 'Baseline', 'secondary'));

  const detail = element('tr');
  detail.id = 'session-' + d.id;
  detail.hidden = true;
  toggle.setAttribute('aria-controls', detail.id);
  toggle.setAttribute('aria-expanded', 'false');
  toggle.addEventListener('click', () => {
    detail.hidden = !detail.hidden;
    toggle.setAttribute('aria-expanded', String(!detail.hidden));
  });
  const facts = element('dl', null, 'session-facts');
  const fields = [
    ['Middle 50%', spread], ['Runs', d.runs],
    ['Source', [source.cloneNode(true), ' · ' + state]],
    ['Data', [record.cloneNode(true), baseline ? ' · Baseline' : '']],
    ['Binary', d.binary], ['Changed', d.change], ['Rust', config.compiler],
    ['Native compiler', config.nativeCompiler], ['Kernel', config.kernel],
    ['CPU affinity', config.affinity?.join(', ')], ['Build features', config.features?.join(', ')],
    ['Configuration', config.series],
  ];
  for (const [label, value] of fields) {
    if (value == null || value === '') continue;
    const group = element('div'), dd = element('dd');
    dd.append(...(Array.isArray(value) ? value : [value]));
    group.append(element('dt', label), dd);
    facts.append(group);
  }
  const body = detail.insertCell();
  body.colSpan = row.cells.length;
  body.append(facts);
  return [row, detail];
}
