import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { chromium } from "playwright-core";
import { repositoryRoot } from "../../shared/paths.mjs";
import { captureScreenshot } from "../../shared/screenshot.mjs";
import { saveWindowHtml } from "../../shared/snapshot.mjs";
import { resolveAlecaWebRoot } from "./source.mjs";

const root = await resolveAlecaWebRoot(repositoryRoot);
const output = process.env.ALECA_LAYOUT_OUTPUT_DIR ?? path.join(repositoryRoot, "previews/warframe/reference");
const scale = Number(process.env.ALECA_LAYOUT_DPI ?? 1);
const template = await readFile(path.join(root, "main_parts/foundry.html"), "utf8");
const styles = ["base", "color", "main/foundry", "main/inventory", "main/relicPlanner"];
const names = ["Ash Prime", "Ember Prime", "Frost"];
const items = names.map((name, index) => ({
  name, internalName: name, type: "warframe", picture: "", owned: index === 0,
  mastered: index !== 1, pendingInFoundry: index === 0, helminthDone: index === 0,
  extraVaultedInfo: "fixture", vualtedMakesSense: index !== 2, vaulted: index === 0,
  components: ["Blueprint", "Neuroptics", "Chassis", "Systems", "Orokin Cell"].map((name, part) => ({
    uniqueName: name, name, picture: "", quantityOwned: (index + part) % 3 === 0 ? 0 : 3,
    recipeNeccessaryComponents: (index + part) % 3 !== 0 || part === 2,
    anyRelicsOwned: part === 1,
  })),
}));

await mkdir(output, { recursive: true });
const document = path.join(output, "alecaframe-foundry.source.html");
await writeFile(document, `<!doctype html><html><head>
  <base href="${pathToFileURL(root + path.sep).href}">
  ${styles.map(name => `<link rel="stylesheet" href="assets/css/${name}.css">`).join("\n")}
  <style>html,body{width:100%;height:100%;background:var(--color-bg-2)}
  #app{padding:12px}.foundryMainBody{height:220px}.foundryObjectContainer{overflow:hidden}
  </style></head><body><div id="app">${template}</div></body></html>`);
const browser = await chromium.launch({ headless: true, args: ["--allow-file-access-from-files"] });
try {
  const page = await browser.newPage({ viewport: { width: 900, height: 290 }, deviceScaleFactor: scale });
  const errors = [];
  page.on("pageerror", error => errors.push(error.message));
  page.on("console", message => { if (message.type() === "error") errors.push(message.text()); });
  await page.route(/^https?:/, route => route.abort());
  await page.goto(pathToFileURL(document).href);
  await page.evaluate(() => {
    const app = document.querySelector("#app");
    window.filterTemplate = app.querySelector("#tooltipFiltersFoundryTemplate").innerHTML;
    app.replaceChildren(app.querySelector(".foundryTopSettings"), app.querySelector(".foundryMainBody"));
  });
  await page.addScriptTag({ path: path.join(root, "assets/js/external/vue.global.prod.js") });
  await page.evaluate(items => {
    Vue.createApp({
      data: () => ({ items, topLeftCategories: ["All", "Warframe", "Primary", "Secondary", "Melee", "Modular", "Arch", "Companion"], selectedCategory: "Warframe",
        selectedSearch: "", yesnoFilters: {}, noItems: false, loading: false }),
      methods: { changeSetting() {}, refresh() {}, setFavStatus() {}, openDetails() {}, openCraftingTree() {} },
    }).mount("#app");
  }, items);
  if (errors.length) throw new Error(errors.join("\n"));
  await page.evaluate(() => document.fonts.ready);
  const selectors = [".foundryObject", ".foundryObjectComponentsComponent", ".foundryObjectComponentsComponentCount",
    ".relicPLannerMiniRewardParentOwned.relic.foundry", ".foundryObjectTopIcon.helminth"];
  const elements = await page.evaluate(selectors => Object.fromEntries(selectors.map(selector => [selector,
    [...document.querySelectorAll(selector)].map(element => {
      const r = element.getBoundingClientRect(), s = getComputedStyle(element);
      return { text: element.textContent.trim(), bounds: { x: r.x, y: r.y, width: r.width, height: r.height },
        style: Object.fromEntries(["backgroundColor", "backgroundSize", "boxShadow", "border", "borderRadius",
          "padding", "fontFamily", "fontSize", "lineHeight", "boxSizing"].map(key => [key, s[key]])) };
    }),
  ])), selectors);
  const prefix = path.join(output, "alecaframe-foundry");
  await saveWindowHtml(page, `${prefix}.html`);
  await captureScreenshot(page, { path: `${prefix}.png` });
  for (const [name, selector] of [["owned", ".foundryObject.owned"], ["unowned", ".foundryObject:not(.owned)"],
    ["components", ".foundryObjectComponents"]]) {
    const bounds = await page.locator(selector).first().boundingBox();
    await captureScreenshot(page, { path: `${prefix}-${name}.png`, clip: {
      x: bounds.x - 4, y: bounds.y - 4, width: bounds.width + 8, height: bounds.height + 8,
    } });
  }
  await writeFile(`${prefix}.json`, JSON.stringify({ source: "main_parts/foundry.html", scale, elements }, null, 2) + "\n");
  const searchStates = [];
  for (const state of ["idle", "open", "query"]) {
    if (state === "open") await page.locator(".topSettingIcon.search").hover();
    if (state === "query") {
      await page.locator("#foundrySearch").fill("Ash Prime Neuroptics");
      await page.mouse.move(0, 280);
    }
    await page.waitForTimeout(300);
    const bounds = await page.locator(".foundryTopSettings").boundingBox();
    await captureScreenshot(page, { path: `${prefix}-search-${state}.png`, clip: bounds });
    await saveWindowHtml(page, `${prefix}-search-${state}.html`);
    searchStates.push({ state, bounds, search: await page.locator("#foundrySearch").evaluate(input => {
      const r = input.parentElement.getBoundingClientRect(), s = getComputedStyle(input);
      return { x: r.x, y: r.y, width: r.width, height: r.height,
        font: s.font, color: s.color, padding: s.padding };
    }) });
  }
  await writeFile(`${prefix}-search.json`, JSON.stringify({ source: "#foundrySearch", scale, states: searchStates }, null, 2) + "\n");
  await page.setViewportSize({ width: 900, height: 700 });
  await page.evaluate(() => {
    const popup = document.createElement("div");
    popup.className = "tippy-box";
    popup.dataset.theme = "extraVaultedInfoTheme";
    popup.style.cssText = "position:absolute;right:20px;top:70px";
    const content = document.createElement("div");
    content.className = "tippy-content";
    content.style.padding = "5px 9px";
    content.innerHTML = window.filterTemplate;
    popup.append(content);
    document.body.append(popup);
  });
  const filterBounds = await page.locator(".tippy-box").boundingBox();
  await saveWindowHtml(page, `${prefix}-filters.html`);
  await captureScreenshot(page, { path: `${prefix}-filters.png`, clip: filterBounds });
  const filterStyles = await page.locator(".filterOptions").first().evaluate(element => {
    const option = element.firstElementChild;
    const s = getComputedStyle(option), r = option.getBoundingClientRect();
    return { bounds: { width: r.width, height: r.height }, style: Object.fromEntries(
      ["backgroundColor", "border", "borderRadius", "padding", "fontFamily", "fontSize", "fontWeight"]
        .map(key => [key, s[key]])) };
  });
  await page.locator(".filterOption").first().evaluate(element => element.classList.add("pulsed"));
  await captureScreenshot(page, { path: `${prefix}-filters-selected.png`, clip: filterBounds });
  await writeFile(`${prefix}-filters.json`, JSON.stringify({ source: "#tooltipFiltersFoundryTemplate", scale,
    bounds: filterBounds, option: filterStyles }, null, 2) + "\n");
  console.log(`${prefix}.png`);
} finally {
  await browser.close();
}
