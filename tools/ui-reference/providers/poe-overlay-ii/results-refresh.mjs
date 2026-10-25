import assert from "node:assert/strict";

export default async function exercise(page) {
  const before = await page.locator('[role="grid"]').boundingBox();
  const link = page.getByText("pathofexile.com/trade", {exact: true});
  await link.evaluate(link => link.parentElement.parentElement.querySelector("button").click());
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  const result = await page.evaluate(() => {
    const {store, actions} = window.reference;
    return {
      status: store.getState().webTrade.searchState.status,
      searches: actions.filter(action => action.meta?.trigger === "webTrade/search").length,
      table: !!document.querySelector('[role="grid"]'),
      rows: document.querySelectorAll(".MuiDataGrid-row").length,
      text: document.body.innerText,
    };
  });
  assert.equal(result.status, "pending");
  assert.equal(result.searches, 2);
  assert.equal(result.table, true);
  assert.equal(result.rows, 3);
  return {before, ...result};
}
