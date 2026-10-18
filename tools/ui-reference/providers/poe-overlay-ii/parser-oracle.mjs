import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import path from "node:path";
import {repositoryRoot} from "../../shared/paths.mjs";

export default async function exercise(page) {
  const source = path.join(repositoryRoot, "research/retinue/providers/extracted/app-poe-overlay-ii-1.68.0/dist");
  for (const chunk of [5040, 7663, 3046, 7605, 4982, 3992]) {
    await page.addScriptTag({path: path.join(source, `${chunk}.bundle.js`)});
  }
  const samples = [];
  for (const name of ["unique_spear", "rare_body_armour", "magic_charm"]) {
    samples.push({name, text: await readFile(path.join(repositoryRoot,
      `apps/retinue_cli/test/fixtures/poe2_${name}.txt`), "utf8")});
  }
  const result = await page.evaluate(async samples => {
    const {require} = window.reference;
    const data = id => {const value = require(id); return value.default ?? value;};
    const stats = data(15555), translations = data(21362), types = data(10277);
    const categories = data(19268), tags = data(55493), words = data(77707);
    const container = require(46443).O.instance, services = require(42712);
    const lookup = (rows, id, languages) => languages.map(language => rows[language]?.[id]).find(Boolean);
    const identity = (rows, text, languages, whitelist = [], blacklist = []) => {
      for (const language of languages) {
        const exact = Object.entries(rows[language] ?? {}).find(([, value]) => value === text);
        if (exact) return exact[0];
      }
      return languages.flatMap(language => Object.entries(rows[language] ?? {}))
        .filter(([id, value]) => text.includes(value) && (!whitelist.length || whitelist.includes(categories[id])) && !blacklist.includes(categories[id]))
        .sort((a, b) => b[1].length - a[1].length)[0]?.[0];
    };
    container.set(services.r5, {get: async type => stats[type] ?? {}});
    container.set(services.oK, {
      get: async (id, language) => translations[language]?.[id],
      entries: async language => Object.entries(translations[language] ?? {}),
    });
    container.set(services.Jh, {
      getId: async (name, ...languages) => identity(words, name, languages),
      getName: async (id, ...languages) => lookup(words, id, languages),
    });
    container.set(services.mD, {
      getTypeLineId: async ({typeLine, languages, whitelistedCategories, blacklistedCategories}) =>
        identity(types, typeLine, languages, whitelistedCategories, blacklistedCategories),
      getTypeLine: async (id, ...languages) => lookup(types, id, languages),
      getCategory: async id => categories[id],
      getTags: async id => tags[id] ?? [],
    });
    const results = [];
    for (const sample of samples) {
      const item = await require(37687).b8(sample.text);
      const query = require(71116).G2(require(71116).gQ(item, {mods: {minRange: 0, selectAll: true}}));
      results.push({name: sample.name, item, query});
    }
    return {source: "PoE Overlay II 1.68.0 parser and bundled English stat data",
      lookup: "Local data adapters; partial-name lookup uses longest match", results};
  }, samples);
  const expected = JSON.parse(await readFile(path.join(repositoryRoot,
    "apps/retinue_cli/test/fixtures/poe2_modifier_cases.json"), "utf8"));
  for (const row of result.results) {
    const golden = expected.cases.find(test => test.name === row.name);
    assert.equal(row.item.category, golden.category, row.name);
    assert.equal(row.item.typeLine, golden.base, row.name);
    assert.deepEqual(row.item.extended.hashes, golden.hashes, row.name);
    const stats = row.query.query.stats.map(({disabled, filters, ...group}) => ({...group,
      filters: filters.map(({disabled, value, ...filter}) => ({...filter, value: value ?? {}}))}));
    assert.deepEqual(stats, golden.stats, row.name);
  }
  return result;
}
