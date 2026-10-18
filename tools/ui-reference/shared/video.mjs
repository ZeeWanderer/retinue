import { once } from "node:events";
import { spawn } from "node:child_process";
import { captureScreenshot } from "./screenshot.mjs";

export async function captureVideo({
  page,
  path,
  dimensions,
  bounds,
  duration,
  fps,
  update,
}) {
  const executable = process.env.UI_REFERENCE_FFMPEG ?? "ffmpeg";
  const filter =
    `format=rgba,pad=${dimensions.width}:${dimensions.height}:` +
    `${bounds.left}:${bounds.top}:color=black@0`;
  const child = spawn(
    executable,
    [
      "-hide_banner",
      "-loglevel",
      "error",
      "-y",
      "-f",
      "image2pipe",
      "-framerate",
      String(fps),
      "-vcodec",
      "png",
      "-i",
      "pipe:0",
      "-an",
      "-vf",
      filter,
      "-c:v",
      "libvpx-vp9",
      "-lossless",
      "1",
      "-pix_fmt",
      "yuva420p",
      "-auto-alt-ref",
      "0",
      "-deadline",
      "good",
      "-cpu-used",
      "4",
      "-metadata:s:v:0",
      "alpha_mode=1",
      "-f",
      "webm",
      path,
    ],
    { stdio: ["pipe", "ignore", "pipe"] },
  );
  let stderr = "";
  child.stderr.setEncoding("utf8");
  child.stderr.on("data", (chunk) => {
    stderr += chunk;
  });
  const closed = new Promise((resolve, reject) => {
    child.once("error", reject);
    child.once("close", (code) => {
      if (code === 0) resolve();
      else reject(new Error(`ffmpeg exited with ${code}: ${stderr.trim()}`));
    });
  });

  try {
    const frameCount = Math.ceil(duration * fps);
    for (let frame = 0; frame < frameCount; frame += 1) {
      await update(frame / fps);
      const image = await captureScreenshot(page, {
        omitBackground: true,
        animations: "allow",
      });
      if (!child.stdin.write(image)) await once(child.stdin, "drain");
    }
    child.stdin.end();
    await closed;
  } catch (error) {
    child.stdin.destroy();
    child.kill();
    await closed.catch(() => {});
    throw error;
  }
}
