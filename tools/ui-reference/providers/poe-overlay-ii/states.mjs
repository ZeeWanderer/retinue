import assert from "node:assert/strict";

export const states = ["ready", "predicting", "estimate", "estimate-error", "searching", "fetching", "results", "listing-preview", "empty", "search-error", "fetch-error", "cooldown", "gem-auto"];

export async function exerciseState(page, state, sample) {
  const transitions = [];
  const record = async action => {
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    const snapshot = await page.evaluate(() => {
      const {store, actions} = window.reference;
      const data = store.getState();
      const rect = document.querySelector("[data-tooltip-anchor]")?.getBoundingClientRect();
      const tooltip = document.querySelector(".MuiTooltip-tooltip");
      const tooltipStyle = tooltip && getComputedStyle(tooltip);
      return {
        text: document.body.innerText,
        requests: actions.filter(action => action.type === "ALIASED").map(action => action.meta.trigger),
        searchStatus: data.webTrade.searchState.status, fetchStatus: data.webTrade.fetchState.status,
        table: !!document.querySelector('[role="grid"]'),
        rows: document.querySelectorAll('.MuiDataGrid-row').length,
        content: rect ? {x: rect.x, y: rect.y, width: rect.width, height: rect.height} : null,
        preview: tooltip ? {
          bounds: tooltip.getBoundingClientRect().toJSON(),
          padding: tooltipStyle.padding, border: tooltipStyle.border,
          radius: tooltipStyle.borderRadius, background: tooltipStyle.backgroundColor,
        } : null,
      };
    });
    transitions.push({action, ...snapshot});
    return snapshot;
  };
  const initial = await record("open");
  const searchCount = snapshot => snapshot.requests.filter(request => request === "webTrade/search").length;
  if (state === "gem-auto") {
    assert.equal(searchCount(initial), 1, "gem should search on opening");
    assert.equal(initial.searchStatus, "pending");
    return transitions;
  }
  assert.equal(searchCount(initial), 0, "ordinary rare equipment must not search on opening");
  assert.equal(initial.table, false);
  if (state === "ready") return transitions;
  if (state === "predicting" || state === "estimate" || state === "estimate-error") {
    await page.evaluate(() => {
      const {store, require, actions} = window.reference;
      const action = actions.findLast(action => action.meta?.trigger === "exile-prediction/predict-prices");
      if (!action) throw new Error("Price prediction was not requested on opening");
      store.dispatch({type: "exile-prediction/predict-prices/pending",
        meta: {arg: action.payload[0], requestId: require(56901).X(action.payload[0])}});
    });
    const pending = await record("price prediction pending");
    assert.equal(searchCount(pending), 0);
    assert.ok(pending.content.height > initial.content.height);
    if (state !== "predicting") {
      await page.evaluate(() => {
        const {store, require, actions} = window.reference;
        const action = actions.findLast(action => action.meta?.trigger === "exile-prediction/predict-prices");
        store.dispatch({type: "exile-prediction/predict-prices/fulfilled",
          meta: {arg: action.payload[0], requestId: require(56901).X(action.payload[0])},
          payload: {entries: [{kind: "success", amount: 40, currency: "exalted",
            uncertainty: {low: 30, high: 50, confidence: 75}}]}});
      });
      const estimated = await record("price prediction returned estimate");
      assert.match(estimated.text, /Reliability/);
      assert.equal(searchCount(estimated), 0);
      if (state === "estimate-error") {
        await page.evaluate(() => window.reference.store.dispatch({
          type: "webTrade/search/rejected", payload: {code: 503, message: "Trade service unavailable"},
        }));
        const combined = await record("estimate remains after search error");
        assert.match(combined.text, /Estimated Value/);
        assert.match(combined.text, /Trade service unavailable/);
      }
    }
    return transitions;
  }
  if (state === "cooldown") {
    assert.equal(await page.getByRole("button", {name: /Search.*10s/}).isDisabled(), true);
    return transitions;
  }
  await page.getByRole("button", {name: "Search", exact: true}).click();
  const searching = await record("click Search");
  assert.equal(searchCount(searching), 1);
  assert.equal(searching.searchStatus, "pending");
  assert.equal(searching.table, false);
  if (state === "searching") return transitions;
  if (state === "search-error") {
    await page.evaluate(() => window.reference.store.dispatch({
      type: "webTrade/search/rejected", payload: {code: 503, message: "Trade service unavailable"},
    }));
    const failed = await record("search rejected");
    assert.match(failed.text, /Trade service unavailable/);
    assert.equal(failed.table, false);
    return transitions;
  }
  await page.evaluate(({sample, empty}) => {
    const {store, require, actions} = window.reference;
    const request = actions.findLast(action => action.meta?.trigger === "webTrade/search").payload[0];
    store.dispatch({type: "webTrade/search/fulfilled", payload: {
      key: require(98185).Yj(request),
      search: {id: sample.results.search_id, url: sample.results.url, language: request.language,
        league: request.league, hits: empty ? [] : sample.results.listings.map(row => row.id), pseudos: []},
    }});
  }, {sample, empty: state === "empty"});
  const fetchedIds = await record("search returned IDs");
  assert.equal(fetchedIds.table, true);
  if (state === "empty") {
    assert.match(fetchedIds.text, /No results/);
    assert.equal(fetchedIds.rows, 0);
    return transitions;
  }
  assert.equal(fetchedIds.fetchStatus, "pending");
  assert.match(fetchedIds.text, /Fetching/);
  if (state === "fetching") return transitions;
  if (state === "fetch-error") {
    await page.evaluate(() => window.reference.store.dispatch({
      type: "webTrade/fetch/rejected", payload: {code: 503, message: "Trade service unavailable"},
    }));
    const failed = await record("fetch rejected");
    assert.match(failed.text, /Unable to fetch data/);
    return transitions;
  }
  await page.evaluate(sample => {
    const {store, require} = window.reference;
    const frameTypes = require(47746).y9;
    const propertyTypes = require(29924).lr;
    const fetches = sample.results.listings.map(row => {
      if (!row.item || !row.type) throw new Error("Listing fixture must include its own item and trade type");
      const item = row.item;
      const mods = Object.groupBy(item.modifiers, mod => mod.section);
      return {id: row.id, item: {
        name: item.name, type: item.base, typeLine: item.base, baseType: item.base,
        nameAlt: "", category: "accessory.ring", rarity: item.rarity, frameTypeId: frameTypes[item.rarity],
        frameType: {Normal: 0, Magic: 1, Rare: 2, Unique: 3, Gem: 4, Currency: 5}[item.rarity],
        itemLevel: item.item_level, ilvl: item.item_level, identified: !item.unidentified,
        corrupted: !!item.corrupted, variants: [], sockets: [], requirements: [],
        properties: item.properties.map(property => {
          const [name, value] = property.text.split(": ", 2);
          return {name, values: value ? [[value, 0]] : [], displayMode: 0,
            ...(name === "Quality" ? {type: propertyTypes.Quality} : {})};
        }),
        ...Object.fromEntries(Object.entries(mods).map(([section, modifiers]) => [`${section}Mods`, modifiers.map(mod => mod.text)])),
        extended: {hashes: {}, mods: {}},
      },
      listing: {type: row.type, indexed: row.indexed, price: {amount: row.amount, currency: row.currency},
        account: {name: row.account, online: row.online ? {league: sample.evaluation.league} : undefined}},
      };
    });
    store.dispatch({type: "webTrade/fetch/fulfilled", payload: {type: "search", fetches}});
  }, sample);
  const completed = await record("fetch returned listings");
  assert.equal(completed.rows, sample.results.listings.length);
  assert.match(completed.text, /ExampleOne/);
  const quality = page.locator('.MuiDataGrid-row').nth(1).locator('[data-field="quality"]');
  assert.equal((await quality.innerText()).trim(), "20%");
  if (state === "listing-preview") {
    await page.locator('.MuiDataGrid-row').nth(1).locator('[data-field="showItem"] button').hover();
    await page.getByRole("tooltip").waitFor({state: "visible"});
    await page.getByRole("tooltip").getByText("Gold Ring", {exact: true}).waitFor({state: "visible"});
    assert.doesNotMatch(await page.getByRole("tooltip").innerText(), /Beast Circle/);
    assert.match(await page.getByRole("tooltip").innerText(), /\+90 to maximum Life/);
    assert.equal(await page.getByRole("tooltip").locator('input:not(:disabled)').count(), 0);
    await record("hover second listing");
  }
  return transitions;
}
