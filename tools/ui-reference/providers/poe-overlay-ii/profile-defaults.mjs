import assert from "node:assert/strict";

export default async function exercise(page) {
  const cases = [
    {name: "rare", rarity: "Rare", category: "accessory.ring", profiles: ["nomods", "fuzzy"]},
    {name: "magic", rarity: "Magic", category: "accessory.ring", profiles: ["nomods", "fuzzy"]},
    {name: "fractured", fractured: true, profiles: ["bases", "nomods", "fuzzy"]},
    {name: "normal", rarity: "Normal", category: "accessory.ring", profiles: ["bases", "nomods", "fuzzy"]},
  ];
  const original = await page.evaluate(() => window.reference.store.getState().evaluate.item);
  for (const test of cases) {
    await page.evaluate(({test, original}) => {
      const {require, store} = window.reference;
      const item = {...original, rarity: test.rarity ?? "Rare", category: test.category ?? "accessory.ring",
        frameTypeId: require(47746).y9[test.rarity ?? "Rare"],
        nameAlt: original.name, typeLineAlt: original.typeLine,
        corrupted: !!test.corrupted, fractured: !!test.fractured, raw: `${original.raw}\n${test.name}`};
      if (test.rarity === "Normal") {
        item.name = "";
        item.nameAlt = "";
        item.explicitMods = [];
        item.extended = {hashes: {}, mods: {}};
      }
      store.dispatch({type: "evaluate/setItem", payload: {item}});
    }, {test, original});
    await page.waitForFunction(selected => document.querySelector('input[type="radio"]:checked')?.value === selected, test.profiles[0]);
    assert.deepEqual(await page.locator('input[type="radio"]').evaluateAll(inputs => inputs.map(i => i.value)), test.profiles, test.name);
  }
  return {source: "PoE Overlay II 1.68.0 category defaults", cases};
}
