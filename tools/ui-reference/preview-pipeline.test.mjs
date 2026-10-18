import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { copyFile, mkdir, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { repositoryRoot } from "./shared/paths.mjs";

test("single-provider previews retain source renders and remove stale comparisons", {
  skip: !existsSync(path.join(repositoryRoot, "dev/bin/poe2companion")),
}, async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "retinue-preview-pipeline-"));
  try {
    await mkdir(path.join(directory, "scripts"));
    const script = path.join(directory, "scripts/generate-previews");
    await copyFile(path.join(repositoryRoot, "scripts/generate-previews"), script);
    for (const name of ["apps", "dev", "tools"]) {
      await symlink(path.join(repositoryRoot, name), path.join(directory, name));
    }
    const resolution = path.join(directory, "previews/poe2/2560x1440");
    const output = path.join(resolution, "side-by-side/evaluate");
    await mkdir(path.dirname(output), { recursive: true });
    const seedComparison = async () => {
      for (const extension of ["png", "json"]) await writeFile(`${output}.${extension}`, "stale");
    };
    const run = extra => spawnSync("bash", [script, "poe2"], {
      cwd: repositoryRoot, encoding: "utf8", timeout: 45000,
      env: { ...process.env, PREVIEW_MEDIA: "image", PREVIEW_SETS: "companion",
        PREVIEW_SCENES: "evaluate", PREVIEW_RESOLUTIONS: "2560x1440",
        PREVIEW_SCALE: "1", PREVIEW_LAYOUT: "comparison", ...extra },
    });
    await seedComparison();
    const single = run({});
    assert.equal(single.status, 0, single.stderr);
    assert.equal(existsSync(`${output}.png`), false);
    assert.equal(existsSync(`${output}.json`), false);
    assert.equal(existsSync(path.join(resolution, "evaluate.png")), false);
    assert.equal(existsSync(path.join(resolution, "evaluate.json")), false);
    assert.equal(existsSync(path.join(resolution, "companion/evaluate.png")), true);

    await seedComparison();
    const failure = run({ PREVIEW_SCALE: "99" });
    assert.notEqual(failure.status, 0);
    assert.match(failure.stderr, /Preview failures:/);
    assert.equal(existsSync(`${output}.png`), false);
    assert.equal(existsSync(`${output}.json`), false);

    await seedComparison();
    const screen = run({ PREVIEW_LAYOUT: "screen" });
    assert.equal(screen.status, 0, screen.stderr);
    const png = await readFile(path.join(resolution, "companion/evaluate.png"));
    assert.deepEqual([png.readUInt32BE(16), png.readUInt32BE(20)], [2560, 1440]);
    assert.equal(existsSync(`${output}.png`), false);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
