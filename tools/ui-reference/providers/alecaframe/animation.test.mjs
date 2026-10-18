import test from "node:test";
import assert from "node:assert/strict";

import { suggestionScrollOffset } from "./animation.mjs";

test("suggestion scroll animation holds both ends and returns", () => {
  assert.equal(suggestionScrollOffset(0.25, 136), 0);
  assert.equal(suggestionScrollOffset(1, 136), 68);
  assert.equal(suggestionScrollOffset(2, 136), 136);
  assert.equal(suggestionScrollOffset(2.75, 136), 68);
  assert.equal(suggestionScrollOffset(3.5, 136), 0);
  assert.equal(suggestionScrollOffset(4.25, 136), 0);
});
