import { execFileSync } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { saveWindowHtml } from "../../shared/snapshot.mjs";
import { captureScreenshot } from "../../shared/screenshot.mjs";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { chromium } from "playwright-core";
import { suggestionScrollOffset } from "./animation.mjs";
import { resolveAlecaWebRoot } from "./source.mjs";
import { captureVideo } from "../../shared/video.mjs";
import { repositoryRoot } from "../../shared/paths.mjs";

const sourcePath = path.join(
  await resolveAlecaWebRoot(repositoryRoot),
  "relicRecommendation.html",
);
const outputDirectory =
  process.env.ALECA_LAYOUT_OUTPUT_DIR ?? path.join(repositoryRoot, "previews/warframe/reference");
const screenshotPath = path.join(outputDirectory, "alecaframe-relic-suggestions.png");
const geometryPath = path.join(outputDirectory, "alecaframe-relic-suggestions.json");
const videoPath = path.join(outputDirectory, "alecaframe-relic-suggestions.webm");
const dimensions = displayDimensions();
const bounds = { width: 480, height: 220, left: dimensions.width - 500, top: 20 };
const media = new Set((process.env.ALECA_LAYOUT_MEDIA ?? "image").split(/\s+/).filter(Boolean));
for (const value of media) {
  if (value !== "image" && value !== "video") {
    throw new Error(`unknown Aleca reference media: ${value}`);
  }
}

await mkdir(outputDirectory, { recursive: true });

const browser = await chromium.launch({
  headless: true,
  args: ["--allow-file-access-from-files"],
});
try {
  const page = await browser.newPage({
    viewport: { width: bounds.width, height: bounds.height },
    deviceScaleFactor: 1,
  });
  page.on("pageerror", (error) => process.stderr.write(`[browser] ${error.message}\n`));
  await page.route("**/assets/css/fonts/*", (route) => {
    const filename = path.basename(new URL(route.request().url()).pathname);
    return route.fulfill({ path: path.join(path.dirname(sourcePath), "assets/fonts", filename) });
  });
  await page.route(/^https?:\/\//, (route) =>
    route.fulfill({ status: 200, contentType: "text/javascript", body: "" }),
  );
  await page.addInitScript(
    ({ data, traces, screen }) => {
      const update = event();
      const close = event();
      const plugin = {
        get() {
          return {
            onRelicRecommendationUpdate: update,
            onCloseRelicRecommendation: close,
            relicRecommendationReady() {
              setTimeout(() => {
                update.emit("traces", traces);
                update.emit("data", JSON.stringify(data));
              });
            },
            SendRelicRecommendationMetrics() {},
            getAnalyticsName(callback) {
              callback(true, "reference");
            },
          };
        },
      };
      const ok = { success: true, status: "success" };
      const noop = (...args) => {
        const callback = args.at(-1);
        if (typeof callback === "function") callback(ok);
      };
      globalThis.overwolf = {
        profile: {
          getCurrentUser: (callback) =>
            callback({ ...ok, username: "reference", userId: "reference" }),
        },
        windows: {
          getMainWindow: () => ({ plugin }),
          getCurrentWindow: (callback) =>
            callback({
              ...ok,
              window: {
                id: "relic-suggestion-reference",
                width: 480,
                height: 220,
                dpiScale: 1,
              },
            }),
          changeSize: (_options, callback) => callback(ok),
          changePosition: noop,
          setZoom: noop,
          close: noop,
        },
        games: {
          getRunningGameInfo2: (callback) =>
            callback({
              ...ok,
              gameInfo: { logicalWidth: screen.width, logicalHeight: screen.height },
            }),
        },
      };

      function event() {
        const listeners = new Set();
        return {
          addListener(listener) {
            listeners.add(listener);
          },
          removeListener(listener) {
            listeners.delete(listener);
          },
          emit(...args) {
            for (const listener of listeners) listener(...args);
          },
        };
      }
    },
    { data: suggestionData(), traces: 1842, screen: dimensions },
  );

  await page.goto(pathToFileURL(sourcePath).href, { waitUntil: "load" });
  await page.waitForSelector(".recommendedRelic:nth-child(8)", { timeout: 10_000 });
  await page.evaluate(() => document.fonts.ready);

  const selectors = [
    ".content",
    ".recommendedRelicsTitle",
    ".recommendedRelicsTitleTraces",
    ".recommendedRelicsTitleTracesIcon",
    ".recommendedRelicsTitleText",
    ".closeIcon",
    ".recommendedRelicsContainer",
    ".recommendedRelic",
    ".relicTop",
    ".relicVaulted",
    ".relicCount",
    ".relicTitle",
    ".relicBottom",
    ".relicDetailsBLTopExpectedTitle",
    ".relicDetailsBLTopExpectedCoinsCoin",
    ".relicDetailsBLTopExpectedCoinsCoinIcon",
    ".footer",
  ];
  const elements = await page.evaluate((wanted) => {
    const rect = (value) => ({
      x: value.x,
      y: value.y,
      width: value.width,
      height: value.height,
      top: value.top,
      right: value.right,
      bottom: value.bottom,
      left: value.left,
    });
    return Object.fromEntries(
      wanted.map((selector) => [
        selector,
        [...document.querySelectorAll(selector)].map((element) => {
          const style = getComputedStyle(element);
          return {
            text: element.textContent.trim().replace(/\s+/g, " "),
            rect: rect(element.getBoundingClientRect()),
            style: {
              display: style.display,
              position: style.position,
              width: style.width,
              height: style.height,
              margin: style.margin,
              padding: style.padding,
              gap: style.gap,
              fontFamily: style.fontFamily,
              fontSize: style.fontSize,
              fontWeight: style.fontWeight,
              alignItems: style.alignItems,
              justifyContent: style.justifyContent,
              gridTemplateColumns: style.gridTemplateColumns,
              backgroundColor: style.backgroundColor,
              color: style.color,
              border: style.border,
              borderRadius: style.borderRadius,
              boxShadow: style.boxShadow,
            },
          };
        }),
      ]),
    );
  }, selectors);

  const outputs = [];
  if (media.has("video")) {
    const maximum = await page.locator(".recommendedRelicsContainer").evaluate(
      (element) => element.scrollHeight - element.clientHeight,
    );
    await captureVideo({
      page,
      path: videoPath,
      dimensions,
      bounds,
      duration: 4,
      fps: 10,
      update: (elapsed) =>
        page.locator(".recommendedRelicsContainer").evaluate(
          (element, scrollTop) => {
            element.scrollTop = scrollTop;
          },
          suggestionScrollOffset(elapsed, maximum),
        ),
    });
    outputs.push(videoPath);
  }

  if (media.has("image")) {
    await page.locator(".recommendedRelicsContainer").evaluate((element) => {
      element.scrollTop = 0;
    });
    await saveWindowHtml(page, screenshotPath.replace(/\.png$/, ".html"));
    const overlayImage = await captureScreenshot(page, { omitBackground: true });
    await page.setViewportSize(dimensions);
    await page.setContent(`
      <!doctype html>
      <html style="margin:0;background:transparent">
        <body style="margin:0;background:transparent;overflow:hidden">
          <img src="data:image/png;base64,${overlayImage.toString("base64")}"
            width="${bounds.width}" height="${bounds.height}"
            style="position:absolute;left:${bounds.left}px;top:${bounds.top}px">
        </body>
      </html>
    `);
    await captureScreenshot(page, { path: screenshotPath, omitBackground: true });
    await writeFile(
      geometryPath,
      `${JSON.stringify(
        {
          source: path.relative(repositoryRoot, sourcePath),
          screen: dimensions,
          bounds,
          elements: offsetElements(elements, bounds),
        },
        null,
        2,
      )}\n`,
    );
    outputs.push(screenshotPath, geometryPath);
  }
  process.stdout.write(`${outputs.join("\n")}\n`);
} finally {
  await browser.close();
}

function displayDimensions() {
  const override = process.env.ALECA_LAYOUT_SIZE;
  if (override) {
    const match = /^(\d+)x(\d+)$/i.exec(override);
    if (!match) throw new Error("ALECA_LAYOUT_SIZE must be WIDTHxHEIGHT");
    return { width: Number(match[1]), height: Number(match[2]) };
  }
  const value = JSON.parse(execFileSync("kscreen-doctor", ["-j"], { encoding: "utf8" }));
  const outputs = value.outputs.filter((output) => output.enabled);
  const output = outputs.find((candidate) => candidate.priority === 1) ?? outputs[0];
  if (!output) throw new Error("no enabled display found");
  const mode = output.modes.find((candidate) => candidate.id === output.currentModeId);
  if (!mode) throw new Error(`current mode ${output.currentModeId} not found for ${output.name}`);
  return {
    width: Math.round(mode.size.width * output.scale),
    height: Math.round(mode.size.height * output.scale),
  };
}

function offsetElements(elements, origin) {
  return Object.fromEntries(
    Object.entries(elements).map(([selector, matches]) => [
      selector,
      matches.map((match) => ({
        ...match,
        screenRect: {
          ...match.rect,
          x: match.rect.x + origin.left,
          y: match.rect.y + origin.top,
          top: match.rect.top + origin.top,
          right: match.rect.right + origin.left,
          bottom: match.rect.bottom + origin.top,
          left: match.rect.left + origin.left,
        },
      })),
    ]),
  );
}

function suggestionData() {
  const item = (name, amountOwned, expectedPlat, expectedDucats, options = {}) => ({
    normalData: {
      name,
      amountOwned,
      vaulted: options.vaulted ?? false,
    },
    custom: {
      isFav: options.favorite ?? false,
      expectedPlat,
      expectedDucats,
    },
  });
  return [
    item("Axi A18", 7, 18, 31, { favorite: true }),
    item("Axi G15", 3, 16, 42, { vaulted: true }),
    item("Axi L7", 12, 14, 36),
    item("Axi S17", 2, 13, 48, { vaulted: true, favorite: true }),
    item("Axi V10", 6, 12, 38),
    item("Axi P7", 4, 11, 34, { vaulted: true }),
    item("Axi N10", 9, 10, 29, { favorite: true }),
    item("Axi O6", 1, 9, 26, { vaulted: true }),
  ];
}
