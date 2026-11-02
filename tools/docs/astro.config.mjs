import {defineConfig} from 'astro/config';
import starlight from '@astrojs/starlight';
import {basePath, output, work, repository} from './paths.mjs';

export default defineConfig({
  site: 'https://zeewanderer.github.io',
  base: basePath,
  outDir: output,
  publicDir: work + '/public',
  integrations: [starlight({
    title: 'Retinue',
    description: 'Companions, tools and developer reference.',
    social: [{icon: 'github', label: 'GitHub', href: repository}],
    sidebar: [
      {label: 'Overview', link: '/'},
      {label: 'Using Retinue', items: [{autogenerate: {directory: 'guides'}}], collapsed: true},
      {label: 'Development', items: ['developer', {autogenerate: {directory: 'developer'}}], collapsed: true},
      {label: 'Reference', items: ['reference', {autogenerate: {directory: 'reference'}}], collapsed: true},
      {label: 'Research', items: ['research', {autogenerate: {directory: 'research'}}], collapsed: true},
    ],
    head: [{tag: 'script', attrs: {src: basePath + 'portal/search.js', defer: true}}],
    pagination: false,
  })],
});
