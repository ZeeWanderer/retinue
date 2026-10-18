import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";

export default async function exercise(page) {
  const cases = JSON.parse(await readFile(new URL(
    "../../../../apps/retinue_cli/test/fixtures/poe2_trade_ranking.json", import.meta.url), "utf8"));
  const actual = await page.evaluate(async cases => {
    const {require, store} = window.reference;
    require(46443).O.instance.set(require(42712).Bj, {});
    const enrich = require(37687).rU;
    const build = require(71116).gQ;
    const results = [];
    for (const test of cases) {
      const item = structuredClone(store.getState().evaluate.item);
      item.category = test.category;
      item.frameTypeId = require(47746).y9[test.rarity ?? "Rare"];
      item.extended = {hashes: {explicit: []}, mods: {explicit: []}};
      const groups = new Map();
      for (const [index, row] of test.rows.entries()) {
        const group = row.decorator?.group ?? row.key;
        let mod = groups.get(group);
        if (!mod) {
          mod = {magnitudes: [], enriched: {tags: row.decorator?.tags ?? [], tier: row.tier_info}};
          groups.set(group, mod);
          item.extended.mods.explicit.push(mod);
        }
        const hash = `explicit.stat_${index + 1}`;
        item.extended.hashes.explicit.push([hash, [item.extended.mods.explicit.indexOf(mod)]]);
        for (const magnitude of row.magnitudes ?? [{value: 10, min: 10, max: 10}]) {
          mod.magnitudes.push({hash, id: (row.internal_ids ?? ["base_maximum_life"]).join(" "), ...magnitude});
        }
      }
      for (const mod of groups.values()) {
        const sum = key => mod.magnitudes.reduce((sum, m) => sum + m[key], 0);
        mod.enriched.roll = {min: sum("min"), max: sum("max"),
          percentage: (sum("value") - sum("min")) / (sum("max") - sum("min") || 1)};
      }
      await enrich(item, {types: {modRanking: true}});
      const query = build(item, {mods: {minRange: 0, selectRanked: true}});
      const scores = {}, selected = [];
      for (const [index, row] of test.rows.entries()) {
        const mod = groups.get(row.decorator?.group ?? row.key);
        if (mod.enriched.ranking) scores[row.key] = mod.enriched.ranking.score;
        const path = require(71116).u2(query)({stat: `explicit.stat_${index + 1}`}, false);
        const filter = path?.split(/[.[\]]/).filter(Boolean).reduce((node, key) => node?.[key], query);
        if (filter && !filter.disabled) selected.push(row.key);
      }
      results.push({name: test.name, scores, selected});
    }
    return results;
  }, cases);
  for (const [index, test] of cases.entries()) {
    assert.deepEqual(Object.keys(actual[index].scores), Object.keys(test.scores), test.name);
    for (const [key, score] of Object.entries(test.scores)) {
      assert.ok(Math.abs(actual[index].scores[key] - score) < 1e-9, `${test.name}: ${key}`);
    }
    assert.deepEqual(actual[index].selected, test.selected, test.name);
  }
  return {source: "PoE Overlay II 1.68.0 ranking and query builder", cases: actual};
}
