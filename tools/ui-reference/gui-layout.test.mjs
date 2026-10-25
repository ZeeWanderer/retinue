import test from "node:test";
import assert from "node:assert/strict";
import { compareLayout } from "./gui-layout.mjs";

test("crosswalk preserves measured units and distinguishes Qt source constants", () => {
  const bounds = { x: 10, y: 20, width: 37, height: 37 };
  const report = compareLayout("relics", { layout: {
    scale: 1.25, regions: { "relic.card.reward": bounds }, elements: [
      { kind: "component_glow", bounds, color: "#46d234ff" },
      { kind: "text", bounds: { ...bounds, x: 70 }, text: "Other" },
    ],
  } }, { elements: { ".foundryObjectComponentsComponent": [{ bounds }] } });
  const reward = report.components.find(c => c.name === "Reward");
  assert.deepEqual(reward.native.bounds, bounds);
  assert.equal(reward.native.elements.length, 1);
  assert.equal(reward.qt.diameter, 37);
  assert.match(reward.qt.source, /relic_card_layout.cpp$/);
  assert.equal(reward.aleca.elements.length, 1);
  assert.equal(report.units, "logical_pixels");
});

test("absent elements remain absent, not a claimed match", () => {
  const report = compareLayout("mastery", {}, undefined);
  assert.equal(report.components[0].native.bounds, null);
  assert.deepEqual(report.components[0].aleca.elements, []);
});

test("summary plaques pair with their corresponding reference, not all three", () => {
  const plaques = ["Game content", "Star Chart", "Intrinsics"].map(text => ({ text }));
  const report = compareLayout("mastery", {}, { elements: { ".masteryHelperMidRect": plaques } });
  assert.deepEqual(report.components.slice(0, 3).map(c => c.aleca.elements), plaques.map(p => [p]));
});
