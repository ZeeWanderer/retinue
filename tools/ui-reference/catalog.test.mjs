import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { access, chmod, copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

import { providers, renderer } from "./catalog.mjs";
import { repositoryRoot } from "./shared/paths.mjs";
import { states as evaluateStates } from "./providers/poe-overlay-ii/states.mjs";

test("every provider scene resolves independently of the working directory", async () => {
  for (const [provider, scenes] of Object.entries(providers)) {
    for (const scene of Object.keys(scenes)) await access(renderer(provider, scene));
  }
  await access(new URL("../../Makefile", import.meta.url));
  assert.equal(repositoryRoot, fileURLToPath(new URL("../../", import.meta.url)));
});

test("unknown providers and scenes cannot escape the provider directory", () => {
  for (const provider of ["warframe", "../alecaframe", "constructor"]) {
    assert.throws(() => renderer(provider, "relic-rewards"), /Unknown provider/);
  }
  assert.throws(() => renderer("alecaframe", "../poe2"), /Unknown alecaframe scene/);
  assert.throws(() => renderer("poe-overlay-ii", "relic-rewards"), /Unknown poe-overlay-ii scene/);
});

test("help and input errors do not launch a browser or need provider packages", () => {
  const script = fileURLToPath(new URL("render.mjs", import.meta.url));
  const help = spawnSync(process.execPath, [script, "--help"], { cwd: "/tmp", encoding: "utf8" });
  assert.equal(help.status, 0);
  assert.match(help.stdout, /alecaframe: desktop, foundry, relic-rewards/);
  assert.match(help.stdout, /poe-overlay-ii: evaluate/);
  const error = spawnSync(process.execPath, [script, "alecaframe", "evaluate"], { encoding: "utf8" });
  assert.equal(error.status, 2);
  assert.match(error.stderr, /Unknown alecaframe scene/);
});

test("make defaults to all games and has explicit per-game targets", () => {
  const preview = target => spawnSync("make", ["-n", target, "PREVIEW_SETS=reference"], {
    cwd: repositoryRoot, encoding: "utf8",
  });
  for (const target of ["previews", "previews-all"]) {
    const result = preview(target);
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /generate-previews all/);
  }
  assert.doesNotMatch(preview("previews-wf").stdout, /generate-previews poe2/);
  assert.doesNotMatch(preview("previews-poe2").stdout, /generate-previews warframe/);
});

test("preview failures do not stop later resolutions or games and remain failures", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "retinue-preview-driver-"));
  try {
    await mkdir(path.join(directory, "scripts"));
    const script = path.join(directory, "scripts/generate-previews");
    await copyFile(`${repositoryRoot}/scripts/generate-previews`, script);
    await chmod(script, 0o755);
    await mkdir(path.join(directory, "dev/bin"), { recursive: true });
    await writeFile(path.join(directory, "dev/bin/wfcompanion"), `#!/usr/bin/env bash
case "$*" in
    'preview list') printf '%s\\n' relic-rewards relic-suggestions notification; exit 0 ;;
    'preview list --animated') exit 0 ;;
esac
`, { mode: 0o755 });
    await writeFile(path.join(directory, "dev/bin/poe2companion"), `#!/usr/bin/env bash
[[ "$*" != *'--state results'* ]]
`, { mode: 0o755 });
    await writeFile(path.join(directory, "node"), `#!/usr/bin/env bash
if [[ "$1" == */currencies.mjs ]]; then
    printf '%s\\n' /tmp/currency-manifest.json
    exit 0
fi
if [[ "$1" == */compare.mjs ]]; then
    printf 'comparison:%s\\n' "$3" >> "$PREVIEW_TEST_LOG"
    exit 0
fi
printf '%s:%s:%s\\n' "$2" "$3" "\${ALECA_LAYOUT_SIZE:-}" >> "$PREVIEW_TEST_LOG"
if [[ ( "$3" == market-orders || "$3" == relic-suggestions ) && "\${ALECA_LAYOUT_SIZE:-}" == 1920x1080 ]]; then
    exit 1
fi
`, { mode: 0o755 });
    for (const sets of ["reference", "companion reference"]) {
      const log = path.join(directory, `calls-${sets}`);
      const result = spawnSync("bash", [script, "all"], {
        encoding: "utf8",
        env: { ...process.env, PATH: `${directory}:${process.env.PATH}`, PREVIEW_TEST_LOG: log,
          PREVIEW_SETS: sets, PREVIEW_MEDIA: "image", PREVIEW_SCENES: "all",
          PREVIEW_RESOLUTIONS: "1920x1080 2560x1440", PREVIEW_LAYOUT: "comparison" },
      });
      assert.equal(result.status, 1);
      const allCalls = (await readFile(log, "utf8")).trim().split("\n");
      const calls = allCalls.filter(call => !call.startsWith("comparison:"));
      const comparisons = allCalls.filter(call => call.startsWith("comparison:"));
      assert.equal(calls.length, 8 + 2 * evaluateStates.length);
      const expectedPairs = sets === "reference" ? [] : [
        "warframe/1920x1080/side-by-side/relic-rewards",
        "warframe/2560x1440/side-by-side/relic-rewards",
        "warframe/2560x1440/side-by-side/relic-suggestions",
        ...["1920x1080", "2560x1440"].flatMap(resolution =>
          ["evaluate", "evaluate-predicting", "evaluate-estimate", "evaluate-estimate-error",
            "evaluate-searching", "evaluate-fetching", "evaluate-listing-preview", "evaluate-empty",
            "evaluate-search-error", "evaluate-fetch-error", "evaluate-cooldown"].map(scene =>
            `poe2/${resolution}/side-by-side/${scene}`)),
      ];
      assert.deepEqual(comparisons, expectedPairs.map(pair => `comparison:${directory}/previews/${pair}`));
      assert.equal(calls[3], "alecaframe:market-orders:1920x1080");
      assert.equal(calls[7], "alecaframe:market-orders:2560x1440");
      assert.equal(calls[8], "poe-overlay-ii:evaluate:");
      assert.match(result.stderr, /Preview failures:\n/);
      assert.match(result.stderr, /warframe\/1920x1080\/reference\/market-orders \(exit 1\)/);
    }
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("comparison command requires both providers before reading images", () => {
  for (const side of ["reference", "companion"]) {
    const result = spawnSync(process.execPath, [`${repositoryRoot}/tools/ui-reference/compare.mjs`,
      "--output", "/tmp/unused-comparison", `--${side}`, "/missing.png"], { encoding: "utf8" });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /Usage:.*--reference PNG --companion PNG/);
  }
});

test("preview driver rejects invalid selections before rendering", () => {
  for (const [game, settings, message] of [
    ["missing", {}, /unknown preview game/],
    ["warframe", { PREVIEW_SCENES: "missing" }, /unknown Warframe preview scene/],
    ["poe2", { PREVIEW_SCENES: "relic-rewards" }, /unknown PoE2 preview scene/],
    ["poe2", { PREVIEW_SCENES: "price-check" }, /unknown PoE2 preview scene/],
    ["poe2", { PREVIEW_SETS: "companion", PREVIEW_SCENES: "evaluate-gem-auto" }, /requires PREVIEW_SETS=reference/],
    ["poe2", { PREVIEW_RESOLUTIONS: "bad" }, /invalid preview resolution/],
    ["warframe", { PREVIEW_RESOLUTIONS: "../../" }, /invalid preview resolution/],
    ["poe2", { PREVIEW_LAYOUT: "missing" }, /unknown preview layout/],
  ]) {
    const result = spawnSync("bash", [`${repositoryRoot}/scripts/generate-previews`, game], {
      encoding: "utf8",
      env: { ...process.env, PREVIEW_SETS: "reference", PREVIEW_MEDIA: "image", ...settings },
    });
    assert.equal(result.status, 2);
    assert.match(result.stderr, message);
  }
});
