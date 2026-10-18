import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

import { providers, renderer } from "./catalog.mjs";

const [provider, scene, ...args] = process.argv.slice(2);
if (!provider || provider === "--help" || provider === "--list") {
  console.log("Usage: node tools/ui-reference/render.mjs PROVIDER SCENE [renderer options]\n");
  for (const [name, scenes] of Object.entries(providers)) {
    console.log(`${name}: ${Object.keys(scenes).join(", ")}`);
  }
  console.log("\nSee tools/ui-reference/README.md for setup and per-provider options.");
} else {
  try {
    const child = spawn(process.execPath, [fileURLToPath(renderer(provider, scene)), ...args], {
      stdio: "inherit",
    });
    child.on("error", error => { console.error(error.message); process.exitCode = 1; });
    child.on("exit", (code, signal) => {
      if (signal) process.kill(process.pid, signal);
      else process.exitCode = code;
    });
  } catch (error) {
    console.error(error.message);
    process.exitCode = 2;
  }
}
