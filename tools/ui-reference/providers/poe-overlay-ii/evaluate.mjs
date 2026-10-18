import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { saveWindowHtml } from "../../shared/snapshot.mjs";
import { captureScreenshot } from "../../shared/screenshot.mjs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import { chromium } from "playwright-core";
import { repositoryRoot as root } from "../../shared/paths.mjs";
import { states, exerciseState } from "./states.mjs";

const { values } = parseArgs({ options: {
  source: { type: "string", default: path.join(root, "research/retinue/providers/extracted/app-poe-overlay-ii-1.68.0/dist") },
  output: { type: "string" },
  scale: { type: "string", default: "1" },
  width: { type: "string", default: "1920" },
  height: { type: "string", default: "1080" },
  state: { type: "string", default: "ready" },
  fixture: { type: "string" },
  selector: { type: "string" },
  exercise: { type: "string" },
} });
const fixture = values.fixture ? JSON.parse(await readFile(values.fixture, "utf8")) : {};
const scale = Number(values.scale);
if (!Number.isFinite(scale) || scale < 0.5 || scale > 3) throw new Error("Invalid scale");
const screen = { width: Number(values.width), height: Number(values.height) };
if (!Number.isInteger(screen.width) || !Number.isInteger(screen.height) ||
    screen.width < 640 || screen.width > 7680 || screen.height < 640 || screen.height > 4320) {
  throw new Error("Invalid resolution");
}
if (!states.includes(values.state)) throw new Error(`Unknown state: ${values.state}`);
const output = values.output ?? path.join(root, `previews/poe2/${screen.width}x${screen.height}/reference/evaluate`);
const sample = JSON.parse(await readFile(path.join(root, "apps/poe2companion/tests/fixtures/evaluate-reference.json"), "utf8"));
await mkdir(path.dirname(output), { recursive: true });
const browser = await chromium.launch({ headless: true, args: ["--allow-file-access-from-files"] });
const errors = [];
try {
  const page = await browser.newPage({ viewport: { width: 840, height: Math.round(screen.height / scale) }, deviceScaleFactor: scale });
  await page.clock.setFixedTime(new Date("2026-10-13T10:00:00Z"));
  page.on("pageerror", error => errors.push(error.stack));
  page.on("console", message => { if (message.type() === "error") errors.push(message.text()); });
  page.on("requestfailed", request => errors.push(`${request.url()}: ${request.failure()?.errorText}`));
  await page.route(/^https?:\/\//, route => route.abort());
  await page.addInitScript(({fixture, sample, screen, scenario}) => {
    const noop = () => {};
    const event = { addListener: noop, removeListener: noop };
    window.overwolf = { games: { inputTracking: { onMouseDown: event, onKeyDown: event } } };
    const chunks = window.rspackChunk_apps_poe2 = [];
    let push = Array.prototype.push;
    Object.defineProperty(chunks, "push", {
      get: () => { const forward = push; return chunk => {
        if (chunk[1]?.[94846]) {
          const initialize = chunk[1][94846];
          chunk[1][94846] = (module, exports, require) => {
            initialize(module, exports, require);
            const render = exports.C;
            module.exports = { C: async (...args) => {
              window.renderPhase = "initializing";
              await render(...args);
              window.renderPhase = "mounted";
            } };
          };
        }
        if (chunk[1]?.[2133]) {
          chunk[1][2133] = (module, exports, require) => {
            exports.bootstrap = async options => {
              const reducer = options.redux.reducer;
              let state = structuredClone(reducer(undefined, { type: "@@init" }));
              state.userSettings.locale = require(6489).h.English;
              state.userSettings.zoom = 1;
              state.app.windowStates[options.name] = "normal";
              state.appGame.resolution = { ...screen, logicalWidth: screen.width, logicalHeight: screen.height };
              state.webTradeConfig.league = sample.evaluation.league;
              state.webTradeConfig.language = "en";
              state.webTradeData.leagues = sample.evaluation.leagues;
              state.evaluateConfig.profiles = [
                { id: "bases", name: "Crafting Base", categories: ["base"], modsConfig: { minRange: 0, selectedTypes: ["implicit", "fractured", "skill"], copySelectedFromPrevious: false }, preset: true },
                { id: "nomods", name: "Quick Price", categories: ["nonunique", "base"], modsConfig: { minRange: 0, selectedTypes: [], copySelectedFromPrevious: false }, preset: true },
                { id: "allmods", name: "Exact Match", categories: ["unique", "unmodifiable", "relic-flask"], modsConfig: { minRange: 0, selectedTypes: [], copySelectedFromPrevious: false }, preset: true },
                { id: "fuzzy", name: "Broad (-10%)", categories: ["nonunique", "unique", "unmodifiable", "relic-flask", "base"], modsConfig: { minRange: .1, selectedTypes: [], copySelectedFromPrevious: true } },
              ];
              const item = sample.evaluation.item;
              state.evaluate.item = {
                name: item.name, type: item.base, typeLine: item.base, baseType: item.base,
                nameAlt: "", category: "accessory.ring",
                rarity: "Rare", frameTypeId: require(47746).y9.Rare, itemLevel: 82, ilvl: 82, identified: true,
                variants: [], sockets: [], properties: [], requirements: [],
                explicitMods: item.modifiers.map(mod => mod.text),
                extended: {
                  hashes: { explicit: item.modifiers.map((mod, i) => [mod.id, [i]]) },
                  mods: { explicit: item.modifiers.map(mod => ({ magnitudes: [
                    { hash: mod.id, value: mod.min, min: mod.min, max: mod.min },
                  ] })) },
                },
                raw: "Item Class: Rings\nRarity: Rare\nGloom Finger\nGold Ring\n--------\nItem Level: 82\n--------\n+80 to maximum Life\n+35% to Fire Resistance\n+28% to Cold Resistance",
              };
              if (scenario === "gem-auto") {
                const property = require(29924);
                state.evaluate.item = {
                  name: "", type: "Tame Beast", typeLine: "Tame Beast", baseType: "Tame Beast",
                  nameAlt: "", category: "gem.activegem", frameTypeId: require(47746).y9.Gem,
                  identified: true, variants: [], sockets: [], requirements: [], explicitMods: [],
                  properties: [{name: "Level", type: property.lr.GemLevel, values: [["13", 0]], displayMode: 0}],
                  extended: {hashes: {}, mods: {}},
                  raw: "Item Class: Skill Gems\nRarity: Gem\nTame Beast\n--------\nLevel: 13",
                };
              }
              if (scenario === "cooldown") {
                state.webRateLimit.policies.preview = {name: "preview", rules: [], retryAfterTimestamp: Date.now() + 10000};
                state.webRateLimit.resourceToPolicyMap["/api/trade2/search"] = "preview";
              }
              for (const [slice, value] of Object.entries(fixture)) {
                if (!(slice in state)) throw new Error(`Unknown vendor state slice: ${slice}`);
                state[slice] = { ...state[slice], ...value };
              }
              const listeners = new Set();
              const actions = [];
              const store = {
                getState: () => state,
                subscribe: fn => { listeners.add(fn); return () => listeners.delete(fn); },
                dispatch: action => {
                  actions.push(action);
                  const trigger = action.meta?.trigger;
                  state = reducer(state, action.type === "ALIASED" && ["webTrade/search", "webTrade/fetch"].includes(trigger)
                    ? {type: `${trigger}/pending`, meta: {arg: action.payload[0]}} : action);
                  listeners.forEach(fn => fn());
                  return action;
                },
              };
              window.reference = { store, require, calls: [], actions };
              const api = new Proxy({}, { get: (_, key) => {
                if (key === "then") return undefined;
                return (...args) => {
                  window.reference.calls.push({ key, args });
                  return key === "setZoom" ? Promise.resolve() : noop;
                };
              } });
              return { name: options.name, store, ipc: api, app: api, browser: api,
                hotkeys: api, inputSystem: api, updater: api };
            };
          };
        }
        return forward.call(chunks, chunk);
      }; },
      set: value => { push = value; },
    });
  }, {fixture, sample, screen, scenario: values.state});
  await page.goto(pathToFileURL(path.join(values.source, "renderer/ingame-evaluate.html")).href);
  await page.waitForFunction(() => window.renderPhase === "mounted" && window.reference?.calls.some(call => call.key === "setSize"));
  const requested = await page.evaluate(() => window.reference.calls.filter(call => call.key === "setSize").at(-1).args[0]);
  if (requested?.width > 0 && requested?.height > 0) {
    await page.setViewportSize({ width: Math.round(requested.width / scale), height: Math.round(requested.height / scale) });
  }
  await page.mouse.move(800, 1000);
  await page.waitForTimeout(350);
  await page.evaluate(() => document.fonts.ready);
  const transitions = await exerciseState(page, values.state, sample);
  if (values.exercise) {
    try {
      const { default: exercise } = await import(pathToFileURL(path.resolve(values.exercise)).href);
      const result = await exercise(page);
      await writeFile(`${output}.interactions.json`, JSON.stringify(result, null, 2));
    } catch (error) {
      errors.push(`Exercise: ${error.stack}`);
    }
  }
  if (values.selector) {
    await captureScreenshot(page.locator(values.selector).first(), { path: `${output}-element.png`, omitBackground: true });
  }
  await saveWindowHtml(page, `${output}.html`);
  const report = await page.evaluate(() => ({
    state: window.reference?.store.getState(), calls: window.reference?.calls, actions: window.reference?.actions,
    text: document.body.innerText, phase: window.renderPhase,
    scripts: [...document.scripts].map(script => script.src),
    elements: [...document.querySelectorAll("#app *")].flatMap(element => {
      const rect = element.getBoundingClientRect();
      if (!rect.width || !rect.height) return [];
      const style = getComputedStyle(element);
      return [{ tag: element.tagName, class: element.className, text: element.children.length ? undefined : element.textContent,
        bounds: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
        style: Object.fromEntries(["display", "position", "font", "color", "background", "padding", "gap", "border", "borderRadius", "gridTemplateColumns", "flexDirection", "webkitAppRegion"].map(key => [key, style[key]])) }];
    }),
  }));
  const position = {x: Math.max(0, Math.round((screen.width - 556 * scale) / 2)), y: Math.round(32 * scale)};
  await writeFile(`${output}.json`, JSON.stringify({ ...report, screen, scale, scenario: values.state, requested,
    position: {...position, source: "fixture; the provider retains the user's window position"}, transitions, errors }, null, 2));
  const windowImage = await captureScreenshot(page, { omitBackground: true });
  const frame = await browser.newPage({viewport: screen, deviceScaleFactor: 1});
  await frame.setContent(`<html style="background:transparent"><body style="margin:0;overflow:hidden">
    <img style="position:absolute;left:${position.x}px;top:${position.y}px" src="data:image/png;base64,${windowImage.toString("base64")}">
    </body></html>`);
  await frame.locator("img").evaluate(image => image.decode());
  await captureScreenshot(frame, { path: `${output}.png`, omitBackground: true });
  console.log(`${output}.png\n${output}.json\n${output}.html`);
  const itemName = report.state?.evaluate.item?.name || report.state?.evaluate.item?.typeLine;
  if (errors.length || !itemName || !report.text.includes(itemName)) process.exitCode = 1;
  else await rm(`${output}.error.json`, {force: true});
} catch (error) {
  await writeFile(`${output}.error.json`, JSON.stringify({error: error.stack, errors}, null, 2));
  throw error;
} finally {
  await browser.close();
}
