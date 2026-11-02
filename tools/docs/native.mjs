import fs from 'node:fs/promises';
import path from 'node:path';
import {load} from 'cheerio';
import {url, erlangApps} from './paths.mjs';

export function integrate(html, kind) {
  const $ = load(html);
  const main = kind === 'Rust' ? $('main .main-heading').parent() : $('#content');
  const body = main.length ? main : $('main');
  if (!body.length) return html;
  $('body').addClass('retinue-api');
  body.first().attr('data-pagefind-body', '');
  $('head').append($('<meta>').attr({name: 'retinue-reference', content: kind}));
  $('head').append($('<link>').attr({rel: 'stylesheet', href: url('portal/api.css')}));
  const navigation = $('<nav>').attr({'class': 'retinue-portal', 'aria-label': 'Retinue documentation', 'data-pagefind-ignore': ''});
  const home = $('<strong>').append($('<a>').attr('href', url()).text('Retinue'));
  navigation.append(home);
  for (const [label, href] of [['Guides', 'guides/installation/'], ['Development', 'developer/'], ['Reference', 'reference/'], ['Research', 'research/'], ['Search all docs', 'search/']]) {
    navigation.append($('<a>').attr('href', url(href)).text(label));
  }
  $('body').prepend(navigation);
  $('a[href]').each((_, element) => {
    const href = $(element).attr('href');
    for (const app of erlangApps) {
      const prefix = `https://hexdocs.pm/${app}/`;
      if (href.startsWith(prefix)) {
        const relative = href.slice(prefix.length).replace(/^\d+[^/]*\//, '');
        $(element).attr('href', url(`reference/erlang/${app}/${relative}`));
      }
    }
  });
  return $.html();
}

export async function integrateDirectory(directory, kind) {
  for (const entry of await fs.readdir(directory, {withFileTypes: true})) {
    const file = path.join(directory, entry.name);
    if (entry.isDirectory()) await integrateDirectory(file, kind);
    else if (entry.name.endsWith('.html')) {
      const text = await fs.readFile(file, 'utf8');
      await fs.writeFile(file, integrate(text, kind));
    }
  }
}
