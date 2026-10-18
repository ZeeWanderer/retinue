import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { parseArgs } from "node:util";
import { chromium } from "playwright-core";
import { composeComparison } from "./shared/comparison.mjs";
import { repositoryRoot } from "./shared/paths.mjs";

const { values } = parseArgs({ options: {
  reference: { type: "string" }, companion: { type: "string" }, output: { type: "string" },
} });
if (!values.output || !values.reference || !values.companion) {
  throw new Error("Usage: node tools/ui-reference/compare.mjs --output BASE --reference PNG --companion PNG");
}
const sources = [];
for (const [side, label] of [["reference", "Original"], ["companion", "Retinue"]]) {
  const file = path.resolve(values[side]);
  sources.push({ label, file: path.relative(repositoryRoot, file),
    image: `data:image/png;base64,${(await readFile(file)).toString("base64")}` });
}
const font = `data:font/ttf;base64,${(await readFile(path.join(repositoryRoot,
  "assets/fonts/roboto/Roboto-Regular.ttf"))).toString("base64")}`;
const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage({ deviceScaleFactor: 1 });
  const { png, report } = await page.evaluate(composeComparison, { sources, font });
  await mkdir(path.dirname(values.output), { recursive: true });
  await writeFile(`${values.output}.png`, Buffer.from(png, "base64"));
  await writeFile(`${values.output}.json`, JSON.stringify(report, null, 2) + "\n");
  console.log(`${values.output}.png`);
} finally {
  await browser.close();
}
