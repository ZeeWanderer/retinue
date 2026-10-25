import assert from "node:assert/strict";

const settle = page => page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
const counts = page => page.evaluate(() => ({
  search: window.reference.actions.filter(a => a.meta?.trigger === "webTrade/search").length,
  fetch: window.reference.actions.filter(a => a.meta?.trigger === "webTrade/fetch").length,
  rows: document.querySelectorAll(".MuiDataGrid-row").length,
}));

export default async function exercise(page) {
  const original = await page.evaluate(() => window.reference.store.getState().evaluate.item);
  const before = await counts(page);
  assert.equal(before.rows, 3, "run with --state results");
  const show = async item => {
    await page.evaluate(item => window.reference.store.dispatch({type: "evaluate/setItem", payload: {item}}), item);
    await settle(page);
  };
  await show({...original, category: "weapon.bow", raw: `${original.raw}\nbow`});
  assert.equal((await counts(page)).rows, 0);
  await page.clock.setFixedTime(new Date("2026-10-14T10:00:00Z"));
  await show(original);
  assert.deepEqual(await counts(page), before, "revisit restores listings without network or time expiry");
  await show({...original, name: "Another Ring", raw: `${original.raw}\nanother ring`});
  assert.deepEqual(await counts(page), before, "identity is the canonical query, not the copied item");

  const reducer = await page.evaluate(() => {
    const {store, require} = window.reference;
    const slice = require(62859).z;
    const key = require(98185).Yj;
    const request = {request: {query: {type: "Gold Ring"}}, league: "Standard", language: "en"};
    const keys = [key(request), key({...request, league: "Hardcore"}), key({...request, language: "de"})];
    let state = slice.reducer(undefined, {type: "@@init"});
    const put = (id, hit = `hit-${id}`) => {
      state = slice.reducer(state, {type: "webTrade/search/fulfilled", payload: {
        key: id, search: {id, hits: [hit]},
      }});
      state = slice.reducer(state, {type: "webTrade/fetch/fulfilled", payload: {
        type: "search", fetches: [{id: hit}],
      }});
    };
    for (let i = 0; i < 10; ++i) put(String(i));
    const ten = state.searchOrder.slice();
    state = slice.reducer(state, {type: "unrelated/read"});
    put("10");
    const evicted = {order: state.searchOrder.slice(), search: !!state.searches["0"], row: !!state.searchFetches["hit-0"]};
    put("5", "replacement-5");
    const refreshed = {order: state.searchOrder.slice(), oldRow: !!state.searchFetches["hit-5"], row: !!state.searchFetches["replacement-5"]};
    const current = store.getState().webTrade;
    state = slice.reducer(state, slice.actions.clearSearches());
    return {keys, ten, evicted, refreshed, cleared: [Object.keys(state.searches).length, Object.keys(state.searchFetches).length],
      revisitedOrder: current.searchOrder.slice()};
  });
  assert.equal(new Set(reducer.keys).size, 3);
  assert.equal(reducer.ten.length, 10);
  assert.deepEqual(reducer.evicted, {order: Array.from({length: 10}, (_, i) => String(i + 1)), search: false, row: false});
  assert.equal(reducer.refreshed.order.at(-1), "5");
  assert.equal(reducer.refreshed.oldRow, false);
  assert.equal(reducer.refreshed.row, true);
  assert.deepEqual(reducer.cleared, [0, 0]);
  assert.equal(reducer.revisitedOrder.length, 1);

  const base = await page.evaluate(original => ({...original,
    rarity: "Normal", frameTypeId: window.reference.require(47746).y9.Normal,
    name: "", explicitMods: [], extended: {hashes: {}, mods: {}},
    raw: "Item Class: Rings\nRarity: Normal\nGold Ring\n--------\nItem Level: 82",
  }), original);
  const reject = async () => {
    await page.evaluate(() => window.reference.store.dispatch({type: "webTrade/search/rejected", payload: {code: 503, message: "Fixture failure"}}));
    await settle(page);
  };
  await show(base);
  const attempted = (await counts(page)).search;
  assert.equal(attempted, before.search + 1);
  await reject();
  await show(base);
  assert.equal((await counts(page)).search, attempted, "same failed item is not automatically retried");
  await show({...base, raw: `${base.raw}\nsecond base`});
  assert.equal((await counts(page)).search, attempted + 1);
  await reject();
  await show(base);
  assert.equal((await counts(page)).search, attempted + 2, "only last attempted item is remembered");
  await reject();
  await show(original);
  assert.equal((await counts(page)).rows, 3);
  return {revisit: before, expiry: "none", identity: "query + league + language", reducer,
    autoAttempt: "last item only; failure is not retried until another item is attempted"};
}
