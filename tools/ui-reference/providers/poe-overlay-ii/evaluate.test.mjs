import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { repositoryRoot } from "../../shared/paths.mjs";
import { states } from "./states.mjs";

const source = path.join(repositoryRoot, "research/retinue/providers/extracted/app-poe-overlay-ii-1.68.0/dist/renderer/ingame-evaluate.html");

test("reference listings and estimates use CDN currency icons", {skip: !existsSync(source)}, async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "retinue-currencies-"));
  try {
    for (const state of ["results", "estimate"]) {
      const result = spawnSync(process.execPath, ["tools/ui-reference/render.mjs", "poe-overlay-ii", "evaluate",
        "--state", state, "--exercise", "tools/ui-reference/providers/poe-overlay-ii/currency-layout.mjs",
        "--output", path.join(directory, state)], {cwd: repositoryRoot, encoding: "utf8", timeout: 45000});
      assert.equal(result.status, 0, result.stderr);
    }
  } finally { await rm(directory, {recursive: true, force: true}); }
});

test("original renderer states and search interactions produce named full-resolution artifacts", {skip: !existsSync(source)}, async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "retinue-evaluate-"));
  try {
    for (const state of states) {
      const output = path.join(directory, state);
      await writeFile(`${output}.error.json`, JSON.stringify({error: "previous failed capture"}));
      const scale = state === "results" ? "1.25" : "1";
      const args = ["tools/ui-reference/render.mjs", "poe-overlay-ii", "evaluate", "--state", state,
        "--width", "2560", "--height", "1440", "--scale", scale, "--output", output];
      if (state === "ready") args.push("--exercise", "tools/ui-reference/providers/poe-overlay-ii/search-interactions.mjs");
      if (state === "results") args.push("--exercise", "tools/ui-reference/providers/poe-overlay-ii/results-controls.mjs");
      const result = spawnSync(process.execPath, args, {cwd: repositoryRoot, encoding: "utf8", timeout: 45000});
      assert.equal(result.status, 0, `${state}: ${result.stderr}`);
      assert.equal(existsSync(`${output}.error.json`), false);
      const png = await readFile(`${output}.png`);
      assert.equal(png.readUInt32BE(16), 2560);
      assert.equal(png.readUInt32BE(20), 1440);
      const report = JSON.parse(await readFile(`${output}.json`, "utf8"));
      assert.deepEqual(report.errors, []);
      assert.equal(report.scenario, state);
      assert.equal(report.scale, Number(scale));
      assert.equal(report.state.appGame.resolution.height, 1440);
      assert.ok(report.transitions.length > 0);
      assert.match(await readFile(`${output}.html`, "utf8"), /data-tooltip-anchor/);
    }
    assert.ok((await readdir(directory, {withFileTypes: true})).every(entry => entry.isFile()));
  } finally {
    await rm(directory, {recursive: true, force: true});
  }
});

test("profile rules match the original ranking, query builder and UI", {skip: !existsSync(source)}, async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "retinue-profile-"));
  try {
    for (const exercise of ["profile-ranking", "property-ranking", "profile-ranges", "profile-defaults", "profile-interactions", "motion", "plaque-layout", "item-layout", "evaluation-behavior", "parser-oracle"]) {
      const output = path.join(directory, exercise);
      const result = spawnSync(process.execPath, ["tools/ui-reference/render.mjs", "poe-overlay-ii", "evaluate",
        "--exercise", `tools/ui-reference/providers/poe-overlay-ii/${exercise}.mjs`, "--output", output],
        {cwd: repositoryRoot, encoding: "utf8", timeout: 45000});
      assert.equal(result.status, 0, `${exercise}: ${result.stderr}`);
      const report = JSON.parse(await readFile(`${output}.json`, "utf8"));
      assert.deepEqual(report.errors, []);
    }
  } finally {
    await rm(directory, {recursive: true, force: true});
  }
});

test("result refresh preserves the original table while searching", {skip: !existsSync(source)}, async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "retinue-results-refresh-"));
  try {
    const result = spawnSync(process.execPath, ["tools/ui-reference/render.mjs", "poe-overlay-ii", "evaluate",
      "--state", "results", "--exercise", "tools/ui-reference/providers/poe-overlay-ii/results-refresh.mjs",
      "--output", path.join(directory, "refresh")],
      {cwd: repositoryRoot, encoding: "utf8", timeout: 45000});
    assert.equal(result.status, 0, result.stderr);
  } finally {
    await rm(directory, {recursive: true, force: true});
  }
});
