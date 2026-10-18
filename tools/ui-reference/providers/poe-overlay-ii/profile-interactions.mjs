import assert from "node:assert/strict";

export default async function exercise(page) {
  const profiles = () => page.locator('input[type="radio"]');
  const selected = () => page.locator('input[type="radio"]:checked').inputValue();
  const defaults = () => page.evaluate(() => window.reference.store.getState().evaluateConfig.defaultProfiles);
  const initialDefaults = await defaults();
  assert.deepEqual(await profiles().evaluateAll(inputs => inputs.map(input => input.value)), ["nomods", "fuzzy"]);
  assert.equal(await selected(), "nomods");
  const life = page.locator('[data-type="explicit"] > div').filter({hasText: "+80 to maximum Life"});
  await life.getByText("+80 to maximum Life", {exact: true}).click();
  assert.equal(await selected(), "nomods");
  await page.locator('input[type="radio"][value="fuzzy"]').check();
  assert.equal(await selected(), "fuzzy");
  assert.equal(await life.locator("input").first().isDisabled(), false);
  assert.equal(await life.locator("input").first().inputValue(), "80");
  await life.getByText("+80 to maximum Life", {exact: true}).click();
  assert.equal(await selected(), "fuzzy");
  assert.deepEqual(await defaults(), initialDefaults);
  return {
    category: "nonunique",
    available: ["nomods", "fuzzy"],
    initial: "nomods",
    afterFilterEdit: "nomods",
    afterSwitchAndFilterEdit: "fuzzy",
    broadPreservesFixedRoll: true,
    profileSwitchWritesCategoryDefault: false,
  };
}
