import assert from "node:assert/strict";
import test from "node:test";
import { captureScreenshot } from "./screenshot.mjs";

const transient = "page.screenshot: Protocol error (Page.captureScreenshot): Unable to capture screenshot";

test("screenshot capture retries transient failures with unchanged options", async () => {
  const options = { omitBackground: true, animations: "allow" };
  const bytes = Buffer.from("frame");
  let attempts = 0;
  const result = await captureScreenshot({ screenshot: async received => {
    assert.equal(received, options);
    if (++attempts < 3) throw new Error(transient);
    return bytes;
  } }, options);
  assert.equal(result, bytes);
  assert.equal(attempts, 3);
});

test("persistent capture failures stop after two retries", async () => {
  let attempts = 0;
  const error = new Error(transient);
  await assert.rejects(captureScreenshot({ screenshot: async () => {
    attempts += 1;
    throw error;
  } }), actual => actual === error);
  assert.equal(attempts, 3);
});

test("other screenshot errors fail without retrying", async () => {
  for (const message of ["Target closed", "Timeout 30000ms exceeded", "EACCES: permission denied"]) {
    let attempts = 0;
    const error = new Error(message);
    await assert.rejects(captureScreenshot({ screenshot: async () => {
      attempts += 1;
      throw error;
    } }), actual => actual === error);
    assert.equal(attempts, 1);
  }
});
