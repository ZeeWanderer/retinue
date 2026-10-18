import assert from "node:assert/strict";

export default async function exercise(page) {
  const observations = [];
  const check = async (step, expected) => {
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(resolve)));
    const requests = await page.evaluate(() => window.reference.actions
      .filter(action => action.meta?.trigger === "webTrade/search").length);
    observations.push({step, requests});
    assert.equal(requests, expected, step);
  };
  await check("rare item opened", 0);
  const life = page.locator('[data-type="explicit"] > div').filter({hasText: "+80 to maximum Life"});
  await life.getByText("+80 to maximum Life", {exact: true}).click();
  await check("modifier included", 0);
  await life.locator("input").first().fill("70");
  await check("numeric bound edited", 0);
  await page.keyboard.press("Enter");
  await check("Enter in numeric field", 0);
  await page.getByText("Broad (-10%)", {exact: true}).click();
  await check("profile selected", 0);
  await page.evaluate(() => document.activeElement.blur());
  assert.equal(await page.evaluate(() => document.activeElement === document.body), true);
  await page.keyboard.press("Enter");
  await check("Enter on body", 1);
  return observations;
}
