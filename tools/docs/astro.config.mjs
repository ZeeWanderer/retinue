import {defineConfig} from 'astro/config';
import starlight from '@astrojs/starlight';
import {basePath, output, work, repository, publishedSite} from './paths.mjs';

export default defineConfig({
  site: new URL(publishedSite).origin,
  base: basePath,
  outDir: output,
  publicDir: work + '/public',
  integrations: [starlight({
    title: 'Retinue',
    description: 'Companions, tools and developer reference.',
    social: [{icon: 'github', label: 'GitHub', href: repository}],
    sidebar: [],
    customCss: ['./src/styles/docs.css'],
    components: {PageTitle: './src/components/PageTitle.astro', Sidebar: './src/components/Navigator.astro'},
    head: [{tag: 'script', attrs: {src: basePath + 'portal/search.js', defer: true}}],
    pagination: false,
  })],
});
