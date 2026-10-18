import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import path from "node:path";
import {repositoryRoot} from "../../shared/paths.mjs";

export default async function exercise(page) {
  const link = page.getByText("pathofexile.com/trade", {exact: true});
  const result = await link.evaluate(link => {
    const row = link.parentElement.parentElement;
    const button = row.querySelector("button");
    const svg = button.querySelector("svg");
    const style = element => ({bounds: element.getBoundingClientRect().toJSON(),
      color: getComputedStyle(element).color, size: getComputedStyle(element).fontSize,
      decoration: getComputedStyle(element).textDecorationLine});
    return {link: style(link), prefix: style(link.parentElement), button: style(button),
      icon: {...style(svg), viewBox: svg.getAttribute("viewBox"),
        paths: [...svg.querySelectorAll("path")].map(path => path.getAttribute("d"))}};
  });
  await link.click();
  result.calls = await page.evaluate(() => window.reference.calls.filter(call => call.key === "openUrl"));
  assert.equal(result.calls.length, 1);
  assert.match(result.calls[0].args[0], /^https:\/\/www.pathofexile.com\/trade2\/search\/poe2\//);
  assert.equal(result.link.decoration, "underline");
  const asset = await readFile(path.join(repositoryRoot, "apps/poe2companion/assets/frames/refresh.svg"), "utf8");
  assert.deepEqual(result.icon.paths, [...asset.matchAll(/\bd="([^"]+)"/g)].map(match => match[1]));
  assert.equal(result.icon.bounds.width, 16);
  return result;
}
