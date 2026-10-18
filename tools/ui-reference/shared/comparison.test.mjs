import assert from "node:assert/strict";
import test from "node:test";
import { chromium } from "playwright-core";
import { composeComparison } from "./comparison.mjs";

test("comparisons trim transparent margins without scaling or clipping painted controls", async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    const sources = await page.evaluate(() => [1, 1.25].map(scale => {
      const canvas = document.createElement("canvas");
      canvas.width = 2560;
      canvas.height = 1440;
      const context = canvas.getContext("2d");
      context.fillStyle = "#ff0000";
      context.fillRect(100, 100, 400 * scale, 300 * scale);
      context.fillStyle = "#00ff00";
      context.fillRect(100 + 420 * scale, 150, 116 * scale, 20);
      return { label: `Scale ${scale}`, image: canvas.toDataURL() };
    }));
    const { report, png } = await page.evaluate(composeComparison, { sources });
    assert.equal(report.resampled, false);
    assert.deepEqual(report.panels.map(panel => panel.crop), [
      { x: 100, y: 100, width: 536, height: 300 },
      { x: 100, y: 100, width: 670, height: 375 },
    ]);
    assert.equal(report.width, 32 + 536 + 24 + 670);
    assert.equal(report.height, 32 + 28 + 375);
    for (const { crop, destination } of report.panels) {
      assert.equal(crop.width, destination.width);
      assert.equal(crop.height, destination.height);
    }
    const pixels = await page.evaluate(async ({ png, report }) => {
      const image = new Image();
      image.src = `data:image/png;base64,${png}`;
      await image.decode();
      const canvas = document.createElement("canvas");
      canvas.width = image.width;
      canvas.height = image.height;
      const context = canvas.getContext("2d");
      context.drawImage(image, 0, 0);
      return report.panels.map(({ destination: { x, y } }) =>
        [...context.getImageData(x, y, 1, 1).data]);
    }, { png, report });
    assert.deepEqual(pixels, [[255, 0, 0, 255], [255, 0, 0, 255]]);

    await assert.rejects(page.evaluate(composeComparison, { sources: sources.slice(0, 1) }), /needs both renders/);
    const empty = await page.evaluate(() => {
      const canvas = document.createElement("canvas");
      return { label: "Empty", image: canvas.toDataURL() };
    });
    await assert.rejects(page.evaluate(composeComparison, { sources: [sources[0], empty] }), /render is empty/);
    await assert.rejects(page.evaluate(composeComparison, { sources: [] }), /needs both renders/);
  } finally {
    await browser.close();
  }
});
