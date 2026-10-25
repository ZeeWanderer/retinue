import assert from "node:assert/strict";

export default async function exercise(page) {
  await page.waitForFunction(() => [...document.querySelectorAll('img[src^="https://web.poecdn.com/"]')]
    .some(image => image.complete && image.naturalWidth > 0));
  const images = await page.evaluate(() => [...document.querySelectorAll('img[src^="https://web.poecdn.com/"]')]
    .map(image => ({alt: image.alt, width: image.width, height: image.height,
      naturalWidth: image.naturalWidth, bounds: image.getBoundingClientRect().toJSON(),
      parent: image.parentElement.textContent, src: image.src})));
  assert.ok(images.length > 0);
  for (const image of images) {
    assert.equal(image.alt, "Exalted Orb");
    assert.equal(image.width, 20);
    assert.equal(image.height, 20);
    assert.ok(image.naturalWidth > 20);
  }
  if (!await page.locator('[role="grid"]').count()) return {images};
  await page.evaluate(() => {
    const {store} = window.reference;
    const ids = ["divine", "perfect-exalted-orb", "future-coin"];
    const fetches = Object.values(store.getState().webTrade.searchFetches).map((row, i) => ({
      ...row, listing: {...row.listing, price: {...row.listing.price, currency: ids[i]}},
    }));
    store.dispatch({type: "webTrade/fetch/fulfilled", payload: {type: "search", fetches}});
  });
  await page.locator('img[alt="Divine Orb"]').waitFor();
  await page.locator('img[alt="Perfect Exalted Orb"]').waitFor();
  await page.evaluate(() => Promise.all([...document.images].map(image => image.decode().catch(() => {}))));
  const rows = await page.locator('.MuiDataGrid-row').allTextContents();
  assert.match(rows[1], /III/);
  assert.match(rows[2], /future-coin/);
  const tier = await page.locator('img[alt="Perfect Exalted Orb"]').evaluate(image => {
    const label = image.nextElementSibling;
    const style = getComputedStyle(label);
    return {text: label.textContent, size: style.fontSize, right: style.right,
      bottom: style.bottom, bounds: label.getBoundingClientRect().toJSON()};
  });
  assert.equal(tier.size, "8px");
  assert.equal(tier.bottom, "-1px");
  return {images, tier, rows};
}
