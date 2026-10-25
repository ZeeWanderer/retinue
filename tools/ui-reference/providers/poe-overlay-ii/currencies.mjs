import {createHash} from "node:crypto";
import {mkdir, readFile, realpath, rename, writeFile} from "node:fs/promises";
import path from "node:path";
import {pathToFileURL} from "node:url";
import {repositoryRoot} from "../../shared/paths.mjs";

export async function prepareCurrencies({root = path.join(repositoryRoot, ".cache/ui-reference/poe2/currencies"), fetchImage = fetch} = {}) {
  const entries = JSON.parse(await readFile(new URL("./fixtures/currencies.json", import.meta.url), "utf8"));
  await mkdir(root, {recursive: true});
  const assets = new Map();
  for (const entry of entries) {
    const url = `https://web.poecdn.com${entry.image}`;
    const file = path.join(root, `${entry.digest}.png`);
    let body;
    try { body = await readFile(file); } catch (error) { if (error.code !== "ENOENT") throw error; }
    const valid = bytes => bytes && createHash("sha256").update(bytes).digest("hex") === entry.digest;
    if (!valid(body)) {
      const response = await fetchImage(url, {signal: AbortSignal.timeout(15000)});
      if (!response.ok) throw new Error(`Currency fixture download failed: ${response.status} ${url}`);
      body = Buffer.from(await response.arrayBuffer());
      if (!valid(body)) throw new Error(`Currency fixture digest changed: ${entry.id}`);
      const temporary = `${file}.${process.pid}.tmp`;
      await writeFile(temporary, body);
      await rename(temporary, file);
    }
    assets.set(url, body);
  }
  const manifest = path.join(root, "manifest.json");
  const temporary = `${manifest}.${process.pid}.tmp`;
  await writeFile(temporary, JSON.stringify(entries.map(entry => ({...entry, path: `${entry.digest}.png`})), null, 2));
  await rename(temporary, manifest);
  return {manifest, assets, groups: [{id: "Currency", label: "Currency", entries}]};
}

if (process.argv[1] && import.meta.url === pathToFileURL(await realpath(process.argv[1])).href) {
  console.log((await prepareCurrencies()).manifest);
}
