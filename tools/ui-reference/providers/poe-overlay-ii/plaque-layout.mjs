import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export default async function exercise(page) {
  const result = await page.evaluate(async () => {
    const {store, require, actions} = window.reference;
    const action = actions.findLast(action => action.meta?.trigger === "exile-prediction/predict-prices");
    const meta = {arg: action.payload[0], requestId: require(56901).X(action.payload[0])};
    store.dispatch({type: "exile-prediction/predict-prices/fulfilled", meta,
      payload: {entries: [{kind: "success", amount: 40, currency: "exalted",
        uncertainty: {low: 30, high: 50, confidence: 75}}]}});
    store.dispatch({type: "webTrade/search/rejected", payload: {code: 503, message: "Trade service unavailable"}});
    await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const elements = [...document.querySelectorAll("[data-tooltip-anchor] *")].flatMap(element => {
      const style = getComputedStyle(element);
      if (style.borderBottomStyle === "none") return [];
      const rect = element.getBoundingClientRect();
      return [{tag: element.tagName, text: element.textContent.slice(0, 120),
        bounds: rect.toJSON(), border: style.border, radius: style.borderRadius,
        background: style.backgroundColor, padding: style.padding,
        sides: [style.borderTop, style.borderRight, style.borderBottom, style.borderLeft],
        before: {border: getComputedStyle(element, "::before").border,
          background: getComputedStyle(element, "::before").background,
          mask: getComputedStyle(element, "::before").mask},
        after: {border: getComputedStyle(element, "::after").border,
          background: getComputedStyle(element, "::after").background}}];
    });
    const typography = [...document.querySelectorAll("[data-tooltip-anchor] *")]
      .filter(e => !e.children.length && ["Estimated Value", "\u2248", "40", "exalted", "Range:", "Reliability:", "High"].includes(e.textContent))
      .map(e => { const s = getComputedStyle(e); return {text: e.textContent, bounds: e.getBoundingClientRect().toJSON(),
        color: s.color, size: s.fontSize, weight: s.fontWeight, lineHeight: s.lineHeight}; });
    const caption = [...document.querySelectorAll("[data-tooltip-anchor] *")]
      .find(element => !element.children.length && element.textContent === "Estimated Value");
    const help = caption.parentElement.querySelector("svg");
    const helpIcon = {viewBox: help.getAttribute("viewBox"), paths: [...help.querySelectorAll("path")].map(path => path.getAttribute("d")),
      bounds: help.getBoundingClientRect().toJSON(), color: getComputedStyle(help).color};
    const approximate = [...document.querySelectorAll("[data-tooltip-anchor] *")]
      .find(element => !element.children.length && element.textContent === "\u2248");
    approximate.setAttribute("data-estimate-approximation", "");
    return {elements, typography, helpIcon, types: require(29924).lr,
      predictionItem: action.payload[0].items[0], text: document.body.innerText};
  });
  const session = await page.context().newCDPSession(page);
  try {
    await session.send("DOM.enable");
    await session.send("CSS.enable");
    const {root} = await session.send("DOM.getDocument");
    const {nodeId} = await session.send("DOM.querySelector", {nodeId: root.nodeId, selector: "[data-estimate-approximation]"});
    result.approximationFonts = (await session.send("CSS.getPlatformFontsForNode", {nodeId})).fonts;
  } finally {
    await session.detach();
  }
  assert.match(result.text, /Estimated Value/);
  assert.match(result.text, /Trade service unavailable/);
  assert.deepEqual(result.elements[0].sides, ["0px none rgb(127, 127, 127)",
    "1px solid rgb(35, 34, 29)", "1px solid rgb(35, 34, 29)", "1px solid rgb(35, 34, 29)"]);
  assert.equal(result.helpIcon.viewBox, "0 0 16 16");
  assert.equal(result.helpIcon.bounds.width, 12);
  assert.equal(result.helpIcon.bounds.height, 12);
  assert.equal(result.helpIcon.color, "rgb(115, 113, 94)");
  const approximation = result.typography.find(element => element.text === "\u2248");
  assert.equal(approximation.size, "16px");
  assert.equal(approximation.weight, "700");
  assert.equal(approximation.lineHeight, "16px");
  const asset = await readFile(new URL("../../../../apps/poe2companion/assets/frames/estimate-help.svg", import.meta.url), "utf8");
  assert.ok(result.helpIcon.paths.every(path => asset.includes(`d="${path}"`)));
  return result;
}
