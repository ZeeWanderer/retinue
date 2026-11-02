const container = document.querySelector('[data-retinue-search]');
if (container) {
  const base = new URL('../', document.currentScript.src);
  const css = document.createElement('link');
  css.rel = 'stylesheet';
  css.href = new URL('pagefind/pagefind-ui.css', base);
  document.head.append(css);
  const script = document.createElement('script');
  script.src = new URL('pagefind/pagefind-ui.js', base);
  script.onload = () => {
    const search = new PagefindUI({element: '[data-retinue-search]', showSubResults: true, bundlePath: new URL('pagefind/', base).href});
    const query = new URL(location.href).searchParams.get('q');
    if (query) search.triggerSearch(query);
  };
  document.head.append(script);
}
