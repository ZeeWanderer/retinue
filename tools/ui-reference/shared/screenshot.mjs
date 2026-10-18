import { setTimeout } from "node:timers/promises";

export async function captureScreenshot(target, options = {}) {
  for (let attempt = 0; ; attempt += 1) {
    try {
      return await target.screenshot(options);
    } catch (error) {
      if (attempt === 2 || !error.message.includes(
        "Protocol error (Page.captureScreenshot): Unable to capture screenshot",
      )) throw error;
      console.error(`Screenshot capture failed; retry ${attempt + 1}/2${options.path ? `: ${options.path}` : ""}`);
      await setTimeout(150 * (attempt + 1));
    }
  }
}
