import { spawnSync } from "node:child_process";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { repositoryRoot } from "./shared/paths.mjs";
import { compareLayout } from "./gui-layout.mjs";

const pages = (process.env.GUI_PREVIEW_PAGES || "foundry mastery inventory relics market builds settings").split(/\s+/);
const scales = (process.env.GUI_PREVIEW_SCALES || "100 125 200").split(/\s+/).map(Number);
const resolutions = (process.env.GUI_PREVIEW_RESOLUTIONS || "2560x1440").split(/\s+/);
const supported = ["foundry", "mastery", "inventory", "relics", "market", "builds", "settings"];
if (pages.some(p => !supported.includes(p)) || scales.some(s => !Number.isInteger(s) || s < 75 || s > 200) || resolutions.some(r => !/^\d+x\d+$/.test(r))) throw new Error("Invalid GUI preview selection");
const failures = [];
function run(label, command, args) {
  const result = spawnSync(command, args, { cwd: repositoryRoot, stdio: "inherit" });
  if (result.status !== 0) { failures.push(label); return false; }
  return true;
}
for (const size of resolutions) for (const scale of scales) {
  const root = path.join(repositoryRoot, "previews/warframe/gui", size, `${scale}`);
  for (const folder of ["native", "reference", "side-by-side", "layout"]) await mkdir(path.join(root, folder), { recursive: true });
  for (const page of pages) {
    const native = path.join(root, "native", page);
    const reference = path.join(root, "reference", page);
    const pair = path.join(root, "side-by-side", page);
    await rm(`${pair}.png`, { force: true });
    await rm(`${pair}.json`, { force: true });
    const ok = run(`${size}/${scale}/${page}`, path.join(repositoryRoot, "dev/bin/wfgui"), [
      "--page", page, "--size", size, "--ui-scale", `${scale}`, "--screenshot", `${native}.png`,
      "--capture-target", "center-rail", "--report", `${native}.json`,
      ...(process.env.GUI_PREVIEW_LIVE === "1" ? [] : ["--fixture"]),
    ]);
    if (!ok) continue;
    const nativeReport = JSON.parse(await readFile(`${native}.json`, "utf8"));
    let referenceReport;
    if (["foundry", "mastery", "inventory", "relics"].includes(page)) {
    const { center_size: [width, height] } = nativeReport;
    if (run(`${size}/${scale}/${page}/reference`, process.execPath, ["tools/ui-reference/render.mjs", "alecaframe", "desktop",
      "--page", page, "--width", `${width}`, "--height", `${height}`, "--scale", `${scale / 100}`, "--output", reference])) {
      run(`${size}/${scale}/${page}/comparison`, process.execPath, ["tools/ui-reference/compare.mjs", "--reference", `${reference}.png`, "--companion", `${native}.png`, "--output", pair]);
      referenceReport = JSON.parse(await readFile(`${reference}.json`, "utf8"));
    }
    }
    await writeFile(path.join(root,"layout",`${page}.json`), JSON.stringify(compareLayout(page,nativeReport,referenceReport),null,2)+"\n");
  }
}
if (failures.length) { console.error(`GUI preview failures:\n${failures.join("\n")}`); process.exitCode = 1; }
