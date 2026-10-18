import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";

export default async function exercise(page) {
  const cases = JSON.parse(await readFile(new URL(
    "../../../../apps/retinue_cli/test/fixtures/poe2_trade_profiles.json", import.meta.url), "utf8"));
  const actual = await page.evaluate(cases => {
    const {require, store} = window.reference;
    const build = require(71116).gQ;
    const properties = require(29924).lr;
    const propertyTypes = {
      "property.gem_level": properties.GemLevel,
      "property.gem_sockets": properties.GemSockets,
      "property.quality": properties.Quality,
    };
    return cases.map(({name, modifier}) => {
      const {id, section, min: value, magnitudes = []} = modifier;
      const item = structuredClone(store.getState().evaluate.item);
      item.extended = {hashes: {}, mods: {}};
      item.properties = [];
      if (section === "property") {
        item.frameTypeId = require(47746).y9.Gem;
        item.category = "gem.activegem";
        item.properties = [{type: propertyTypes[id], values: [[`${value}`, 0]]}];
      } else {
        item.extended.hashes[section] = [[id, [0]]];
        item.extended.mods[section] = [{magnitudes: magnitudes.map(m => ({hash: id, ...m}))}];
      }
      const ranges = {};
      for (const [profile, minRange] of [["allmods", 0], ["fuzzy", .1]]) {
        const request = build(item, {mods: {minRange, selectAll: true}, props: {minRange}});
        const field = require(71116).u2(request)(section === "property"
          ? {property: propertyTypes[id]} : {stat: id}, false);
        const filter = field.split(/[.[\]]/).filter(Boolean).reduce((node, key) => node[key], request);
        const range = section === "property" ? filter : filter.value;
        ranges[profile] = Object.fromEntries(["min", "max"].filter(k => Number.isFinite(range[k]))
          .map(k => [k, range[k]]));
      }
      return {name, ranges};
    });
  }, cases);
  for (const [index, test] of cases.entries()) {
    assert.deepEqual(actual[index].ranges, test.expected, test.name);
  }
  return {source: "PoE Overlay II 1.68.0 query builder", cases: actual};
}
