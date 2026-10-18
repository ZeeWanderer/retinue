import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { prepareAlecaSource, resolveAlecaWebRoot } from "./source.mjs";

async function packageAt(root, version) {
  const target = path.join(root, version, "package");
  await mkdir(path.join(target, "web"), { recursive: true });
  await writeFile(
    path.join(target, "manifest.json"),
    JSON.stringify({ meta: { name: "AlecaFrame", version } }),
  );
  await writeFile(path.join(target, "web/relicOverlay.html"), "");
  await writeFile(path.join(target, "web/relicRecommendation.html"), "");
  return target;
}

test("resolves newest compatible research package", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "wfcli-aleca-"));
  try {
    await packageAt(path.join(root, "research/alecaframe"), "2.6.9");
    const latest = await packageAt(path.join(root, "research/alecaframe"), "2.10.0");

    assert.equal(await resolveAlecaWebRoot(root), path.join(latest, "web"));
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("imports an extracted package by manifest version", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "wfcli-aleca-"));
  const source = await mkdtemp(path.join(os.tmpdir(), "aleca-package-"));
  try {
    await mkdir(path.join(source, "web"), { recursive: true });
    await writeFile(
      path.join(source, "manifest.json"),
      JSON.stringify({ meta: { name: "AlecaFrame", version: "3.1.4" } }),
    );
    await writeFile(path.join(source, "web/relicOverlay.html"), "");
    await writeFile(path.join(source, "web/relicRecommendation.html"), "");

    const prepared = await prepareAlecaSource(root, source);
    assert.equal(
      prepared.path,
      path.join(root, "research/alecaframe/3.1.4/package"),
    );
    assert.equal(await resolveAlecaWebRoot(root), path.join(prepared.path, "web"));
  } finally {
    await rm(root, { recursive: true, force: true });
    await rm(source, { recursive: true, force: true });
  }
});
