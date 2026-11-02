import {defineCollection} from 'astro:content';
import {glob} from 'astro/loaders';
import {docsSchema} from '@astrojs/starlight/schema';
import {content} from '../paths.mjs';

export const collections = {
  docs: defineCollection({loader: glob({pattern: '**/*.md', base: content}), schema: docsSchema()}),
};
