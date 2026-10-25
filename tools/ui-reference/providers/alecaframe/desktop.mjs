import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { parseArgs } from "node:util";
import { pathToFileURL } from "node:url";
import { chromium } from "playwright-core";
import { repositoryRoot } from "../../shared/paths.mjs";
import { captureScreenshot } from "../../shared/screenshot.mjs";
import { saveWindowHtml } from "../../shared/snapshot.mjs";
import { resolveAlecaWebRoot } from "./source.mjs";

const { values } = parseArgs({ options: {
  page: { type: "string", default: "mastery" }, width: { type: "string", default: "1200" },
  height: { type: "string", default: "800" }, scale: { type: "string", default: "1" },
  output: { type: "string" },
} });
const templates = { foundry: "foundry", mastery: "masteryHelper", inventory: "inventory", relics: "relicPlanner" };
if (!Object.hasOwn(templates, values.page) || !values.output) throw new Error("Use --page foundry|mastery|inventory|relics --output BASE");
const width = Number(values.width), height = Number(values.height), scale = Number(values.scale);
if (![width, height, scale].every(n => Number.isFinite(n) && n > 0) || width > 7680 || height > 4320 || scale > 3) throw new Error("Invalid capture dimensions");
const root = await resolveAlecaWebRoot(repositoryRoot);
const template = await readFile(path.join(root, `main_parts/${templates[values.page]}.html`), "utf8");
const styles = ["base", "color", "main/foundry", "main/inventory", "main/relicPlanner", "main/masteryHelper"];
const output = path.resolve(values.output);
await mkdir(path.dirname(output), { recursive: true });
const source = `${output}.source.html`;
await writeFile(source, `<!doctype html><html><head><base href="${pathToFileURL(root + path.sep)}">
${styles.map(s => `<link rel="stylesheet" href="assets/css/${s}.css">`).join("\n")}
<style>html,body{width:100%;height:100%;background:var(--color-bg-2)}
#app{display:flex;flex-direction:column;width:100%;height:100%;padding:12px;box-sizing:border-box}
.foundryMainBody{flex:1;min-height:0}.inventoryObjectContainer{height:100%;align-content:flex-start}
</style></head><body><div id="app">${template}</div></body></html>`);
const browser = await chromium.launch({ headless: true, args: ["--allow-file-access-from-files"] });
try {
  const page = await browser.newPage({ viewport: { width: Math.round(width / scale), height: Math.round(height / scale) }, deviceScaleFactor: scale });
  const errors = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.route(/^https?:/, route => route.abort());
  await page.goto(pathToFileURL(source).href);
  await page.addScriptTag({ path: path.join(root, "assets/js/external/vue.global.prod.js") });
  await page.evaluate(({ scene }) => {
    const components = ["Blueprint", "Neuroptics", "Chassis", "Systems"].map((name, i) => ({
      name, uniqueName: name, picture: "", quantityOwned: i ? 3 : 0, recipeNeccessaryComponents: i > 0, anyRelicsOwned: i === 1,
    }));
    const item = { name: "Ember Prime", internalName: "ember-prime", type: "warframe", picture: "", owned: false,
      mastered: false, isFav: false, vualtedMakesSense: true, vaulted: true, amountOwned: 3, currentModRank: 0,
      modRankMax: 3, modUsedBy: "", components, sellPrice: 15, buyPrice: 9, ducats: 45,
      masteryViewData: { potentialXPToGet: 6000, currentLevel: 0, maxLevel: 30, relicProbability: 0.0247, platinumNeededToBuyParts: 35 } };
    let items = [item, { ...item, name: "Ash Prime", internalName: "ash-prime", owned: true }];
    if (scene === "inventory") items = [{ ...item, name: "Ember Prime Neuroptics Blueprint", type: "part" },
      { ...item, name: "Ember Prime Set", type: "set", isReadyToSell: true }, { ...item, name: "Hush", type: "mod", modType: "common" }];
    if (scene === "relics") items = ["Lith A1", "Meso B2", "Axi A1"].map(name => ({ normalData: { ...item, name }, custom: {
      isFav: false, groupedDetails: [{ num: 3, tier: "Int", expectedPlat: 5.1, expectedDucats: 18 }, { num: 1, tier: "Rad", expectedPlat: 9.2, expectedDucats: 35 }],
      rewards: components.map(c => ({ ...c, parentOwned: false, recipeHighlightedComponent: false })),
    } }));
    const fraction = { current: 60, max: 100, percent: 60 };
    const summary = Object.fromEntries(["cont_warframes", "cont_weapons", "cont_companions", "star_normal", "star_junctions", "star_steel", "star_steel_junctions", "intrinsic_rail", "intrinsic_duviri"].map(k => [k, fraction]));
    Object.assign(summary, { contentPercent: 60, starPercent: 80, intrinsicPercent: 75 });
    Vue.createApp({ data: () => ({ items, data: { lvl: 25, percent: 96.9, currentLevelXp: 484500, nextLevelXp: 500000, summary, topItems: items },
      topLeftCategories: ["All", "Warframe", "Primary", "Secondary", "Melee", "Modular", "Arch", "Companion"], selectedCategory: "Warframe", selectedSearch: "",
      yesnoFilters: {}, loading: false, noItems: false, favExtraPercent: 0, hoveringExtraPercent: 0, orderingMode: "normal", useDucanator: false,
      onlyOwned: true, invFilterType: "allParts", summaryPlatinum: 135, summaryDucats: 135, summaryTraces: 725,
      inProgress: false, updatedRecommended: false, updateNeeded: false,
    }), methods: { changeSetting() {}, refresh() {}, setFavStatus() {}, openDetails() {}, openCraftingTree() {}, itemClicked() {}, componentClicked() {}, hoverStart() {}, hoverStop() {}, selectOrderingMode() {}, openRelic() {}, onBuySellItemClickedVUE() {} } }).mount("#app");
  }, { scene: values.page });
  await page.evaluate(() => document.fonts.ready);
  if (errors.length) throw new Error(errors.join("\n"));
  const selectors = [".foundryObject", ".foundryObjectTop", ".foundryObjectTopIcon", ".foundryObjectComponentsComponent",
    ".foundryComponentAmount", ".masteryHelperMidRect", ".masteryHelperMidTop", ".masteryHelperMidBottom", ".masteryHelperMidItemTitle",
    ".masteryHelperItem", ".masteryHelperItemImage", ".masteryHelperObjectName", ".masteryHelperPotentialXP", ".masteryHelperBottomTopPart",
    ".newEasyFilter", ".newEasyFilterItem", ".inventoryObject", ".inventoryItemImage", ".inventoryItemName", ".inventoryItemQuantity",
    ".inventoryComponentName", ".relicPlannerMiddle", ".relicDetailsTop", ".relicDetailsBLTopExpected", ".relicPlannerRewardContainer"];
  const elements = await page.evaluate(selectors => Object.fromEntries(selectors.map(selector => [selector,
    [...document.querySelectorAll(selector)].map(e => { const r = e.getBoundingClientRect(), s = getComputedStyle(e); return {
      text: e.textContent.trim(), bounds: { x: r.x, y: r.y, width: r.width, height: r.height },
      style: Object.fromEntries(["color", "backgroundColor", "borderRadius", "padding", "gap", "boxShadow", "justifyContent", "textAlign", "fontFamily", "fontSize", "fontWeight"].map(k => [k, s[k]])),
    }; }),
  ])), selectors);
  await saveWindowHtml(page, `${output}.html`);
  await captureScreenshot(page, { path: `${output}.png` });
  await writeFile(`${output}.json`, JSON.stringify({ template: `main_parts/${templates[values.page]}.html`, width, height, scale, elements }, null, 2) + "\n");
  console.log(`${output}.png`);
} finally { await browser.close(); }
