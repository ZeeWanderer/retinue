import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";

export default async function exercise(page) {
  const cases = JSON.parse(await readFile(new URL(
    "../../../../apps/retinue_cli/test/fixtures/poe2_trade_properties.json", import.meta.url), "utf8"));
  const results = await page.evaluate(async cases => {
    const {require, store} = window.reference;
    require(46443).O.instance.set(require(42712).Bj, {});
    const types = require(29924).lr;
    const build = require(71116).gQ, pathFor = require(71116).u2;
    const results = [];
    for (const test of cases) {
      const item = structuredClone(store.getState().evaluate.item);
      Object.assign(item, {category: test.category, rarity: test.rarity,
        frameTypeId: require(47746).y9[test.rarity], corrupted: !!test.corrupted,
        properties: test.properties.map(p => ({name: p.name, type: types[p.type],
          values: p.values.map(value => [value, 0])})),
        extended: {hashes: {}, mods: {}}});
      for (const row of test.rows) {
        const section = row.section ?? "explicit", hash = `${section}.${row.key}`;
        const mods = item.extended.mods[section] ??= [];
        (item.extended.hashes[section] ??= []).push([hash, [mods.length]]);
        mods.push({tier: "P1", name: row.key,
          magnitudes: [{hash, id: row.internal_ids.join(" "), value: 10, min: 10, max: 10}],
          enriched: {tags: row.decorator?.tags ?? [], roll: {value: 10, min: 10, max: 10, percentage: 1}}});
      }
      await require(37687).rU(item, {types: {quality: true, damage: true, modRanking: true, pseudoMods: true}});
      const summary = {};
      for (const [profile, margin] of [["nomods", 0], ["allmods", 0], ["fuzzy", .1]]) {
        const request = build(item, {mods: {minRange: margin, selectRanked: profile !== "allmods", selectAll: profile === "allmods"},
          props: {minRange: margin, selectRanked: true}});
        const get = key => pathFor(request)(key, false)?.split(/[.[\]]/).filter(Boolean)
          .reduce((node, key) => node?.[key], request);
        summary[profile] = {
          properties: item.properties.map(p => ({name: p.name, value: p.values[0][0],
            score: p.enriched?.ranking?.score ?? null, filter: get({property: p.type}) ?? null})),
          selected: test.rows.filter(row => !get({stat: `${row.section ?? "explicit"}.${row.key}`})?.disabled).map(row => row.key),
        };
      }
      results.push({name: test.name, properties: summary.nomods.properties.map((p, i) => ({
        name: p.name, value: p.value, score: p.score,
        quick: p.filter ? !p.filter.disabled : null,
        exact: summary.allmods.properties[i].filter ? !summary.allmods.properties[i].filter.disabled : null,
        min: p.filter?.min ?? null, broad_min: summary.fuzzy.properties[i].filter?.min ?? null,
      })), quick: summary.nomods.selected, exact: summary.allmods.selected});
    }
    return results;
  }, cases);
  for (const [index, test] of cases.entries()) {
    if (test.expected) assert.deepEqual(results[index], {name: test.name, ...test.expected}, test.name);
  }
  return results;
}
