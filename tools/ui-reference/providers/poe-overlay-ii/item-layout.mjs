import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";

export default async function exercise(page) {
  const result = await page.evaluate(async () => {
    const describe = element => {
      const style = getComputedStyle(element);
      const canvas = document.createElement("canvas");
      const context = canvas.getContext("2d");
      context.font = style.font;
      context.letterSpacing = style.letterSpacing;
      const metrics = context.measureText(element.textContent);
      const range = document.createRange();
      range.selectNodeContents(element);
      return {tag: element.tagName, text: element.textContent.slice(0, 80),
        bounds: element.getBoundingClientRect().toJSON(),
        font: style.font, padding: style.padding, margin: style.margin,
        boxShadow: style.boxShadow, outline: style.outline, transform: style.transform,
        before: {content: getComputedStyle(element, "::before").content,
          background: getComputedStyle(element, "::before").background},
        letterSpacing: style.letterSpacing, fontKerning: style.fontKerning,
        textRange: range.getBoundingClientRect().toJSON(),
        canvasFont: context.font,
        textMetrics: Object.fromEntries(["width", "actualBoundingBoxLeft", "actualBoundingBoxRight",
          "actualBoundingBoxAscent", "actualBoundingBoxDescent", "fontBoundingBoxAscent",
          "fontBoundingBoxDescent"].map(key => [key, metrics[key]])),
        background: style.backgroundColor, color: style.color, lineHeight: style.lineHeight};
    };
    const labels = ["Modifiable", "Item Rarity:", "+80 to maximum Life", "Quick Price"];
    const rows = labels.map(text => {
      const element = [...document.querySelectorAll("*")].find(e =>
        !e.children.length && e.textContent.trim().toLowerCase() === text.toLowerCase());
      return {label: text, elements: element
        ? [element, element.parentElement, element.parentElement.parentElement].map(describe) : []};
    });
    window.reference.store.dispatch({type: "webTrade/search/rejected",
      payload: {code: 503, message: "Trade service unavailable"}});
    await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const caption = [...document.querySelectorAll("*")].find(e =>
      !e.children.length && e.textContent === "Trade service unavailable");
    let parent = caption.parentElement;
    for (let depth = 0; depth < 3 && !parent.querySelector("svg"); depth++) {
      parent = parent.parentElement;
    }
    const icon = parent.querySelector("svg");
    return {rows, errorIcon: icon ? {viewBox: icon.getAttribute("viewBox"),
      paths: [...icon.querySelectorAll("path")].map(path => path.getAttribute("d")),
      ...describe(icon)} : null};
  });
  const [text, selection, row] = result.rows[0].elements;
  assert.equal(text.bounds.height, 17);
  assert.equal(selection.bounds.width, text.bounds.width);
  assert.equal(selection.background, "rgba(255, 255, 255, 0.16)");
  assert.equal(selection.boxShadow,
    "rgba(255, 255, 255, 0.16) 4px 0px 0px 0px, rgba(255, 255, 255, 0.16) -4px 0px 0px 0px");
  assert.equal(selection.bounds.y - row.bounds.y, 1);
  assert.equal(row.bounds.height, 20);
  assert.equal(result.rows[2].elements[2].bounds.y, 150);
  assert.equal(result.rows[3].elements[2].bounds.y, 250);
  assert.equal(result.errorIcon.viewBox, "0 0 16 16");
  assert.equal(result.errorIcon.bounds.width, 16);
  assert.equal(result.errorIcon.color, "rgb(232, 13, 13)");
  const asset = await readFile(new URL("../../../../apps/poe2companion/assets/frames/search-error.svg", import.meta.url), "utf8");
  assert.ok(result.errorIcon.paths.every(path => asset.includes(`d="${path}"`)));
  return result;
}
