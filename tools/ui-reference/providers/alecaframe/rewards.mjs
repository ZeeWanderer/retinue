import { execFileSync } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { saveWindowHtml } from "../../shared/snapshot.mjs";
import { captureScreenshot } from "../../shared/screenshot.mjs";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { chromium } from "playwright-core";
import { resolveAlecaWebRoot } from "./source.mjs";
import { repositoryRoot } from "../../shared/paths.mjs";

const sourcePath = path.join(await resolveAlecaWebRoot(repositoryRoot), "relicOverlay.html");
const outputDirectory =
  process.env.ALECA_LAYOUT_OUTPUT_DIR ?? path.join(repositoryRoot, "previews/warframe/reference");
const screenshotPath = path.join(outputDirectory, "alecaframe-relic-rewards.png");
const geometryPath = path.join(outputDirectory, "alecaframe-relic-rewards.json");

const dimensions = displayDimensions();
const dpi = Number.parseFloat(process.env.ALECA_LAYOUT_DPI ?? "1");
if (!Number.isFinite(dpi) || dpi <= 0) {
  throw new Error("ALECA_LAYOUT_DPI must be a positive number");
}

const selectedZoom = dpi > 1.4 ? -2 : dpi > 1.2 ? -1 : 0;
const bounds = calculateBounds(dimensions.width, dimensions.height, dpi, selectedZoom);
const mockData = relicData();

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

  page.on("console", (message) => {
    if (message.type() === "error") {
      process.stderr.write(`[browser] ${message.text()}\n`);
    }
  });
  page.on("pageerror", (error) => {
    process.stderr.write(`[browser] ${error.message}\n`);
  });

  await page.route("**/assets/css/fonts/*", (route) => {
    const filename = path.basename(new URL(route.request().url()).pathname);
    return route.fulfill({ path: path.join(path.dirname(sourcePath), "assets/fonts", filename) });
  });
  await page.route("**/assets/img/alecaframe.png", (route) =>
    route.fulfill({ path: path.join(path.dirname(sourcePath), "assets/img/AlecaFrame.png") }),
  );
  await page.route(/^https?:\/\//, (route) =>
    route.fulfill({ status: 200, contentType: "text/javascript", body: "" }),
  );
  await page.addInitScript(
    ({ data, screen, monitorDpi }) => {
      localStorage.setItem("subscriptionStatus", "None");
      localStorage.setItem("showPlatDucatsSetting", "true");
      globalThis.OwAd = null;

      const ok = { success: true, status: "success" };
      const plugin = {
        get() {
          return {
            GetRelicWindowData(_testing, callback) {
              callback(true, JSON.stringify(data), 600);
            },
            SendRelicRewardMetrics() {},
            getAnalyticsName(callback) {
              callback(true, "reference");
            },
          };
        },
      };

      const event = { addListener() {}, removeListener() {} };
      const noop = (...args) => {
        const callback = args.at(-1);
        if (typeof callback === "function") callback(ok);
      };

      globalThis.overwolf = {
        windows: {
          getMainWindow: () => ({ plugin }),
          getCurrentWindow: (callback) =>
            callback({ ...ok, window: { id: "relic-reference", monitorId: "display-1" } }),
          changeSize: (_options, callback) => callback(ok),
          changePosition: (_id, _left, _top, callback) => callback(ok),
          setZoom: noop,
          hide: noop,
          close: noop,
          restore: noop,
          maximize: noop,
          minimize: noop,
          obtainDeclaredWindow: noop,
          onStateChanged: event,
        },
        utils: {
          getMonitorsList: (callback) =>
            callback({
              ...ok,
              displays: [
                {
                  id: "display-1",
                  dpiX: 96 * monitorDpi,
                  dpiY: 96 * monitorDpi,
                  width: screen.width,
                  height: screen.height,
                  handle: { value: 1 },
                },
              ],
            }),
          placeOnClipboard() {},
          openUrlInDefaultBrowser() {},
        },
        games: {
          getRunningGameInfo2: (callback) =>
            callback({
              ...ok,
              gameInfo: {
                logicalWidth: screen.width,
                logicalHeight: screen.height,
                monitorHandle: { value: 1 },
                executionPath: "Warframe.x64.exe",
              },
            }),
        },
        extensions: {
          current: {
            getManifest: (callback) => callback({ ...ok, meta: { version: "reference" } }),
          },
        },
        settings: {
          getExtensionSettings: (callback) => callback({ ...ok, settings: {} }),
        },
        profile: {
          getCurrentUser: (callback) => callback({ ...ok, username: "reference" }),
          subscriptions: {
            inapp: { show: noop },
          },
        },
      };
    },
    { data: mockData, screen: dimensions, monitorDpi: dpi },
  );

  await page.goto(pathToFileURL(sourcePath).href, { waitUntil: "load" });
  await page.waitForSelector(".relic:nth-child(4)", { timeout: 10_000 });
  await page.waitForFunction(() =>
    [...document.querySelectorAll(".relicItemName")].every((element) => element.textContent.trim()),
  );
  await page.evaluate(() => document.fonts.ready);

  const sourceBounds = await page.evaluate(
    ({ width, height, zoom }) => calculateContentPixelBounds(width, height, zoom),
    { ...dimensions, zoom: selectedZoom },
  );
  if (JSON.stringify(sourceBounds) !== JSON.stringify(bounds)) {
    throw new Error(
      `AlecaFrame bounds disagree with bootstrap calculation: ${JSON.stringify(sourceBounds)} != ${JSON.stringify(bounds)}`,
    );
  }
  const elements = await page.evaluate(() => {
    const selectors = [
      "main",
      ".relicPart",
      ".relicHolder",
      ".relic",
      ".relicItemName",
      ".relicItemPrices",
      ".relicItemPricePlatinum",
      ".relicItemPricePlatinumIcon",
      ".itemIsVaulted",
      ".relicPLannerMiniRewardParentOwned.fav.foundry.big",
      ".relicItemPriceDucats",
      ".relicItemPriceDucatsIcon",
      ".relicItemOwnershipInfo",
      ".relicItemUnlocked",
      ".relicItemLocked",
      ".relicItemUnlockedCount",
      ".foundryObjectComponents",
      ".foundryObjectComponents > .components",
      ".foundryObjectComponentsComponent",
      ".foundryObjectComponentsComponentCount",
      ".foundryObjectComponents > .setPrice",
      ".relicItemSetPricePlatinumIcon",
      ".relicBottomAlecaframe",
      ".relicBottomAlecaFrameTitle",
      ".relicBottomOpenCloseMessage",
      ".relicItemPricePlatinumBOTTOM",
      ".relicItemPriceDucatsBOTTOM",
      ".staticRightColumn",
    ];

    function rect(value) {
      return {
        x: value.x,
        y: value.y,
        width: value.width,
        height: value.height,
        top: value.top,
        right: value.right,
        bottom: value.bottom,
        left: value.left,
      };
    }

    function textRect(element) {
      const node = [...element.childNodes].find(
        (child) => child.nodeType === Node.TEXT_NODE && child.textContent.trim(),
      );
      if (!node) return null;
      const range = document.createRange();
      range.selectNodeContents(node);
      return rect(range.getBoundingClientRect());
    }

    return Object.fromEntries(
      selectors.map((selector) => [
        selector,
        [...document.querySelectorAll(selector)].map((element) => {
          const style = getComputedStyle(element);
          return {
            text: element.textContent.trim().replace(/\s+/g, " "),
            rect: rect(element.getBoundingClientRect()),
            textRect: textRect(element),
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
              lineHeight: style.lineHeight,
              textAlign: style.textAlign,
              alignItems: style.alignItems,
              justifyContent: style.justifyContent,
              gridTemplateColumns: style.gridTemplateColumns,
              gridTemplateRows: style.gridTemplateRows,
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
  });
  const fonts = await page.evaluate(() =>
    [...document.fonts].map((font) => ({
      family: font.family,
      style: font.style,
      weight: font.weight,
      status: font.status,
    })),
  );
  const positionedElements = Object.fromEntries(
    Object.entries(elements).map(([selector, matches]) => [
      selector,
      matches.map((match) => ({
        ...match,
        screenRect: offsetRect(match.rect, bounds),
        screenTextRect: offsetRect(match.textRect, bounds),
      })),
    ]),
  );

  await saveWindowHtml(page, screenshotPath.replace(/\.png$/, ".html"));
  const overlayImage = await captureScreenshot(page, { omitBackground: true });
  await page.setViewportSize(dimensions);
  await page.setContent(`
    <!doctype html>
    <html style="margin:0;background:transparent">
      <body style="margin:0;background:transparent;overflow:hidden">
        <img src="data:image/png;base64,${overlayImage.toString("base64")}" width="${bounds.width}" height="${bounds.height}"
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
        dpi,
        selectedZoom,
        bounds,
        sourceBounds,
        fonts,
        elements: positionedElements,
      },
      null,
      2,
    )}\n`,
  );

  process.stdout.write(`${screenshotPath}\n${geometryPath}\n`);
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

function calculateBounds(screenWidth, screenHeight, monitorDpi, selectedZoom) {
  const zoomMultiplier = 1 / (0.175 * selectedZoom + 1);
  const extraWidth = 420 / zoomMultiplier;
  const width = 1000 * (screenHeight / 1080) + extraWidth * monitorDpi;
  return {
    width: Math.round(width),
    height: Math.round(((295 + 60) * monitorDpi) / zoomMultiplier),
    left: Math.round(screenWidth / 2 - width / 2 + (extraWidth * monitorDpi) / 2 - 15 * monitorDpi),
    top: Math.round(630 * (screenHeight / 1080)),
  };
}

function offsetRect(rect, origin) {
  if (!rect) return null;
  return {
    ...rect,
    x: rect.x + origin.left,
    y: rect.y + origin.top,
    top: rect.top + origin.top,
    right: rect.right + origin.left,
    bottom: rect.bottom + origin.top,
    left: rect.left + origin.left,
  };
}

function relicData() {
  const component = (quantityOwned, highlighted = false) => ({
    picture: "",
    quantityOwned,
    recipeNeccessaryComponents: quantityOwned > 0,
    recipeHighlightedComponent: highlighted,
    isFavOnlyPart: false,
    isFav: false,
  });
  const reward = (name, platinum, ducats, options = {}) => ({
    name,
    platinum,
    ducats,
    isItemVaulted: options.vaulted ?? false,
    isFav: options.favorite ?? false,
    isPartOfOwned: options.crafted ?? false,
    countOwned: options.owned ?? 0,
    totalToOwn: options.total ?? 1,
    componentData: options.components ?? [component(1), component(0), component(4, true)],
    setPlat: options.setPlat ?? platinum * 3,
    detected: true,
  });

  return {
    relicRewards: [
      reward("Forma Blueprint", 2, 0, { crafted: true, owned: 14, total: 1, components: [] }),
      reward("Akbronco Prime Link", 18, 45, { owned: 1, total: 2, setPlat: 72 }),
      reward("Nidus Prime Neuroptics Blueprint", 32, 100, {
        vaulted: true,
        favorite: true,
        setPlat: 95,
      }),
      reward("Paris Prime Upper Limb", 7, 15, { crafted: true, owned: 3, total: 2, setPlat: 24 }),
    ],
    globalData: { platinum: 59, ducats: 160 },
  };
}
