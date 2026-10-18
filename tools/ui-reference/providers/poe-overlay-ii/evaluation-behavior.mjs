import assert from "node:assert/strict";

const settle = page => page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
const requests = (page, trigger) => page.evaluate(trigger => window.reference.actions
  .filter(action => action.meta?.trigger === trigger).map(action => action.payload[0]), trigger);

export default async function exercise(page) {
  const original = await page.evaluate(() => window.reference.store.getState().evaluate.item);
  const unknownText = "A modifier absent from trade metadata";
  await page.evaluate(({original, unknownText}) => {
    window.reference.store.dispatch({type: "evaluate/setItem", payload: {item: {
      ...original, explicitMods: [...original.explicitMods, unknownText], raw: `${original.raw}\n${unknownText}`,
    }}});
  }, {original, unknownText});
  await settle(page);
  const unknown = page.locator('[data-type="explicit"] > div').filter({hasText: unknownText});
  assert.equal(await unknown.count(), 1);
  assert.equal(await unknown.locator("input").count(), 0);
  assert.equal(await unknown.innerText(), unknownText);
  const predictionsBefore = await requests(page, "exile-prediction/predict-prices");
  assert.ok(predictionsBefore.at(-1).items[0].explicitMods.includes(unknownText));
  const life = page.locator('[data-type="explicit"] > div').filter({hasText: "+80 to maximum Life"});
  await life.getByText("+80 to maximum Life", {exact: true}).click();
  await life.locator("input").first().fill("70");
  await page.getByText("Broad (-10%)", {exact: true}).click();
  await settle(page);
  assert.deepEqual(await requests(page, "exile-prediction/predict-prices"), predictionsBefore);
  assert.equal((await requests(page, "webTrade/search")).length, 0);

  const cases = [
    {name: "rare ring", rarity: "Rare", category: "accessory.ring", auto: false},
    {name: "normal base", rarity: "Normal", category: "accessory.ring", auto: true},
    {name: "fractured rare", rarity: "Rare", category: "accessory.ring", fractured: true, auto: true},
    {name: "corrupted normal", rarity: "Normal", category: "accessory.ring", corrupted: true, auto: false},
    {name: "rare weapon", rarity: "Rare", category: "weapon.bow", auto: false},
    {name: "rare jewel", rarity: "Rare", category: "jewel", auto: false},
    {name: "rare map", rarity: "Rare", category: "map.waystone", auto: false},
    {name: "gem", rarity: "Gem", category: "gem.activegem", auto: true},
  ];
  for (const test of cases) {
    const before = (await requests(page, "webTrade/search")).length;
    await page.evaluate(({original, test}) => {
      const {store, require} = window.reference;
      store.dispatch({type: "webTrade/search/rejected", payload: {code: 503, message: "Fixture reset"}});
      store.dispatch({type: "evaluate/setItem", payload: {item: {
        ...original, rarity: ["Normal", "Magic", "Rare"].includes(test.rarity) ? test.rarity : undefined,
        frameTypeId: require(47746).y9[test.rarity],
        nameAlt: original.name, typeLineAlt: original.typeLine,
        category: test.category, fractured: !!test.fractured, corrupted: !!test.corrupted,
        raw: `${original.raw}\n${test.name}`,
      }}});
    }, {original, test});
    await settle(page);
    const count = (await requests(page, "webTrade/search")).length - before;
    assert.equal(count, test.auto ? 1 : 0, test.name);
  }
  return {unknownText, unknownControls: 0, predictionUsesFullItem: true,
    filterEditsRepeatPrediction: false, autoSearch: cases};
}
