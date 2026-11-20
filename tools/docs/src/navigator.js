const outline = document.querySelector('.doc-outline');
const scroller = outline.closest('#starlight__sidebar');
const base = outline.dataset.fragments;
const key = 'retinue:outline:' + base;
let saved = {};
try {
  saved = JSON.parse(sessionStorage.getItem(key)) || {};
} catch {}
const expanded = new Set(saved.open);
for (const d of outline.querySelectorAll('details[open]')) expanded.add(d.dataset.branch);
const loading = new WeakMap();
const form = document.querySelector('.outline-filter'),
  field = form.querySelector('input'),
  matches = document.querySelector('#outline-matches');
form.hidden = false;
let entries;
async function filter() {
  const q = field.value.trim().toLowerCase();
  outline.hidden = !!q;
  matches.hidden = !q;
  const status = form.querySelector('output');
  if (!q) {
    status.textContent = '';
    matches.replaceChildren();
    return;
  }
  try {
    const result = await (entries ??= fetch(base + 'filter.json').then((r) => {
      if (!r.ok) throw Error(r.status);
      return r.json();
    }));
    if (q !== field.value.trim().toLowerCase()) return;
    const found = result
      .filter(([name]) => name.toLowerCase().includes(q))
      .sort((a, b) => Number(b[0].toLowerCase() === q) - Number(a[0].toLowerCase() === q));
    const list = document.createElement('ul');
    for (const [name, href, kind, path] of found.slice(0, 60)) {
      const li = document.createElement('li'),
        a = document.createElement('a'),
        small = document.createElement('small');
      a.href = href;
      a.textContent = name;
      small.textContent = [kind, path].filter(Boolean).join(' · ');
      if (location.pathname === href) a.setAttribute('aria-current', 'page');
      a.append(small);
      li.append(a);
      list.append(li);
    }
    matches.replaceChildren(list);
    status.textContent =
      found.length > 60 ? 'First 60 of ' + found.length + ' matches' : found.length + ' matches';
  } catch {
    entries = null;
    status.textContent = 'Could not load navigator index.';
  }
}
field.value = saved.filter ?? '';
if (field.value) filter();
field.addEventListener('input', filter);
field.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') {
    e.preventDefault();
    field.value = '';
    filter();
  }
  if (e.key === 'ArrowDown' && field.value) {
    e.preventDefault();
    matches.querySelector('a')?.focus();
  }
});

function reveal(branch) {
  const fallback = branch.querySelector(':scope > .outline-fallback');
  if (!fallback) return restore(branch.querySelector(':scope > ul'));
  if (!loading.has(branch)) {
    const task = fetch(base + branch.dataset.branch + '.frag')
      .then(async (response) => {
        if (!response.ok) throw Error(response.status);
        const template = document.createElement('template');
        template.innerHTML = await response.text();
        fallback.replaceWith(template.content);
        await restore(branch.querySelector(':scope > ul'));
      })
      .catch(() => {})
      .finally(() => loading.delete(branch));
    loading.set(branch, task);
  }
  return loading.get(branch);
}

async function restore(list) {
  if (!list) return;
  const jobs = [];
  for (const d of list.querySelectorAll(':scope > li > details')) {
    if (d.open || expanded.has(d.dataset.branch)) {
      d.open = true;
      jobs.push(reveal(d));
    }
  }
  await Promise.all(jobs);
}

outline.addEventListener(
  'toggle',
  (event) => {
    const d = event.target;
    if (!d.matches('details[data-branch]')) return;
    if (d.open) {
      expanded.add(d.dataset.branch);
      if (d.querySelector(':scope > .outline-fallback')) reveal(d);
    } else expanded.delete(d.dataset.branch);
  },
  true,
);
addEventListener('pagehide', () => {
  try {
    sessionStorage.setItem(
      key,
      JSON.stringify({open: [...expanded], scroll: scroller.scrollTop, filter: field.value}),
    );
  } catch {}
});
restore(outline.firstElementChild).then(() => {
  if (saved.scroll) scroller.scrollTop = saved.scroll;
  const selected = outline.querySelector('[aria-current=page]');
  if (!selected || !scroller.clientHeight) return;
  const box = selected.getBoundingClientRect(),
    pane = scroller.getBoundingClientRect();
  if (box.bottom > pane.bottom || box.top < pane.top)
    scroller.scrollTop += box.top - pane.top - pane.height / 2;
});
