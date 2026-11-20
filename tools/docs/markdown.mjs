import path from 'node:path';
import {unified} from 'unified';
import parse from 'remark-parse';
import gfm from 'remark-gfm';
import stringify from 'remark-stringify';
import {visit} from 'unist-util-visit';
import {toString} from 'mdast-util-to-string';
import {load} from 'cheerio';
import {publishedSite} from './paths.mjs';

const processor = unified().use(parse).use(gfm).use(stringify, {bullet: '-', fences: true});

export function topic(markdown, source, resolve) {
  if (/(?:\/var\/(?:mnt|home)\/|\/home\/[\w.-]+\/|(?:^|[\s(`])\.context\/)/m.test(markdown)) {
    throw new Error(`Private workspace reference in ${source}`);
  }
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

export function linkResolver(routes, {base, repository, ref, assets, published}) {
  return (value, source) => {
    if (value.startsWith(publishedSite)) return base + value.slice(publishedSite.length);
    if (published && value.startsWith(repository+'/')) {
      const match = value.slice(repository.length).match(/^\/(?:tree|blob|raw)\/[^/]+\/(.+?)(?:[?#]|$)/);
      if(match && !published.has(decodeURI(match[1]).replace(/\/$/,'')))throw new Error(`Unpublished repository link in ${source}: ${value}`);
    }
    if (/^(?:[a-z][a-z\d+.-]*:|#|\/\/)/i.test(value)) return value;
    const [file, fragment] = value.split(/#(.*)/s);
    const [pathname, query] = file.split(/\?(.*)/s);
    const target = path.posix.normalize(path.posix.join(path.posix.dirname(source), decodeURI(pathname))).replace(/\/$/, '');
    const suffix = (query ? `?${query}` : '') + (fragment ? `#${fragment}` : '');
    if (routes.has(target)) return base + routes.get(target) + (routes.get(target) ? '/' : '') + suffix;
    if (target.startsWith('docs/images/') && assets.has(target)) return base + target + suffix;
    if (target.startsWith('research/') || target.startsWith('.context/')) {
      throw new Error(`Unpublished evidence link in ${source}: ${value}`);
    }
    if (published && !published.has(target)) throw new Error(`Not a public repository path in ${source}: ${value}`);
    return `${repository}/blob/${ref}/${target}${suffix}`;
  };
}
