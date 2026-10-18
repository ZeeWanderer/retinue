import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { chromium } from "playwright-core";
import { saveWindowHtml } from "./snapshot.mjs";

test("DOM snapshots retain generated styles and live controls without reexecuting scripts", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "retinue-dom-"));
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    await page.setContent('<head><base href="file:///tmp/reference/"><style></style></head><body><input><textarea></textarea><select><option>A</option><option>B</option></select><script>window.original = true;</script></body>');
    await page.evaluate(() => {
      document.styleSheets[0].insertRule("input { color: rgb(100, 110, 120); }");
      document.querySelector("input").value = "15";
      document.querySelector("textarea").value = "live text";
      document.querySelector("select").selectedIndex = 1;
    });
    const output = path.join(directory, "window.html");
    await saveWindowHtml(page, output);
    const html = await readFile(output, "utf8");
    assert.doesNotMatch(html, /<script/);
    const snapshot = await browser.newPage();
    await snapshot.setContent(html);
    assert.equal(await snapshot.locator("input").inputValue(), "15");
    assert.equal(await snapshot.locator("textarea").inputValue(), "live text");
    assert.equal(await snapshot.locator("select").inputValue(), "B");
    assert.equal(await snapshot.locator("input").evaluate(e => getComputedStyle(e).color), "rgb(100, 110, 120)");
    assert.equal(await snapshot.evaluate(() => document.baseURI), "file:///tmp/reference/");
    assert.equal(await snapshot.evaluate(() => window.original), undefined);
  } finally {
    await browser.close();
    await rm(directory, { recursive: true, force: true });
  }
});
