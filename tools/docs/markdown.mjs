import path from 'node:path';
import {unified} from 'unified';
import parse from 'remark-parse';
import gfm from 'remark-gfm';
import stringify from 'remark-stringify';
import {visit} from 'unist-util-visit';
import {toString} from 'mdast-util-to-string';
import {load} from 'cheerio';

const processor = unified().use(parse).use(gfm).use(stringify, {bullet: '-', fences: true});

export function topic(markdown, source, resolve) {
  const tree = processor.parse(markdown);
  const heading = tree.children.find(node => node.type === 'heading' && node.depth === 1);
  if (!heading) throw new Error(`Missing title: ${source}`);
  tree.children.splice(tree.children.indexOf(heading), 1);
  visit(tree, node => {
    if (['link', 'image', 'definition'].includes(node.type)) node.url = resolve(node.url, source);
    if (node.type === 'html') {
      const $ = load(node.value, null, false);
      $('[href], [src]').each((_, element) => {
        for (const attribute of ['href', 'src']) {
          const value = $(element).attr(attribute);
          if (value) $(element).attr(attribute, resolve(value, source));
        }
      });
      node.value = $.html();
    }
  });
  return {title: toString(heading), body: processor.stringify(tree)};
}

export function linkResolver(routes, {base, repository, ref, assets}) {
  return (value, source) => {
    if (/^(?:[a-z][a-z\d+.-]*:|#|\/\/)/i.test(value)) return value;
    const [file, fragment] = value.split(/#(.*)/s);
    const [pathname, query] = file.split(/\?(.*)/s);
    const target = path.posix.normalize(path.posix.join(path.posix.dirname(source), decodeURI(pathname)));
    const suffix = (query ? `?${query}` : '') + (fragment ? `#${fragment}` : '');
    if (routes.has(target)) return base + routes.get(target) + (routes.get(target) ? '/' : '') + suffix;
    if (target.startsWith('docs/images/') && assets.has(target)) return base + target + suffix;
    if (target.startsWith('research/') || target.startsWith('.context/')) {
      throw new Error(`Unpublished evidence link in ${source}: ${value}`);
    }
    return `${repository}/blob/${ref}/${target}${suffix}`;
  };
}
