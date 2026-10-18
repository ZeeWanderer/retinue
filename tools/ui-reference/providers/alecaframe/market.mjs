import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { saveWindowHtml } from "../../shared/snapshot.mjs";
import { captureScreenshot } from "../../shared/screenshot.mjs";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { chromium } from "playwright-core";
import { resolveAlecaWebRoot } from "./source.mjs";
import { repositoryRoot } from "../../shared/paths.mjs";

const webRoot = await resolveAlecaWebRoot(repositoryRoot);
const outputDirectory =
  process.env.ALECA_LAYOUT_OUTPUT_DIR ?? path.join(repositoryRoot, "previews/warframe/reference");
const screenshotPath = path.join(outputDirectory, "alecaframe-market-orders.png");
const geometryPath = path.join(outputDirectory, "alecaframe-market-orders.json");
const documentPath = path.join(outputDirectory, ".alecaframe-market-orders.html");
const dimensions = /^(\d+)x(\d+)$/.exec(process.env.ALECA_LAYOUT_SIZE ?? "1920x1080");
if (!dimensions) throw new Error("Invalid ALECA_LAYOUT_SIZE");
const screen = {width: Number(dimensions[1]), height: Number(dimensions[2])};
if (screen.width < 640 || screen.height < 480) throw new Error("Preview screen is too small");
const viewport = { width: Math.min(1000, screen.width - 80), height: 320 };
const bounds = {left: Math.round((screen.width - viewport.width) / 2), top: Math.round((screen.height - viewport.height) / 2), ...viewport};

await mkdir(outputDirectory, { recursive: true });
const browser = await chromium.launch({
  headless: true,
  args: ["--allow-file-access-from-files"],
});

try {
  const page = await browser.newPage({ viewport, deviceScaleFactor: 1 });
  await writeFile(documentPath, await documentHtml(webRoot));
  await page.goto(pathToFileURL(documentPath).href, { waitUntil: "load" });
  await page.evaluate(() => document.fonts.ready);
  await page.waitForSelector(".wfmItem:nth-child(3)");

  const selectors = [
    ".foundryObjectContainer.wfmItems",
    ".wfmItem",
    ".wfmItemTop",
    ".wfmItemButton.visibility",
    ".wfmItemName",
    ".wfmItemOwned",
    ".wfmItemBottom",
    ".wfmItemLeft",
    ".wfmItemLeftText",
    ".wfmItemRight",
    ".wfmItemBottomAmount",
    ".wfmItemBottomPlat",
    ".wfmItemBottomExtra",
    ".wfmItemLowestPrice",
    ".wfmItemButtons",
    ".wfmItemButton.showMore",
    ".wfmItemButton.edit",
    ".wfmItemButton.addOne",
    ".wfmItemButton.markSold",
    ".wfmItemButton.remove",
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
              width: style.width,
              height: style.height,
              padding: style.padding,
              margin: style.margin,
              gap: style.gap,
              border: style.border,
              borderRadius: style.borderRadius,
              backgroundColor: style.backgroundColor,
              backgroundSize: style.backgroundSize,
              fontFamily: style.fontFamily,
              fontSize: style.fontSize,
              fontWeight: style.fontWeight,
              gridTemplateColumns: style.gridTemplateColumns,
              gridTemplateRows: style.gridTemplateRows,
              alignItems: style.alignItems,
              justifyContent: style.justifyContent,
            },
          };
        }),
      ]),
    );
  }, selectors);

  await saveWindowHtml(page, screenshotPath.replace(/\.png$/, ".html"));
  const cardImage = await captureScreenshot(page);
  await page.setViewportSize(screen);
  await page.setContent(`<html style="background:transparent"><body style="margin:0;overflow:hidden">
    <img style="position:absolute;left:${bounds.left}px;top:${bounds.top}px" src="data:image/png;base64,${cardImage.toString("base64")}">
    </body></html>`);
  await page.locator("img").evaluate(image => image.decode());
  await captureScreenshot(page, { path: screenshotPath, omitBackground: true });
  await writeFile(
    geometryPath,
    `${JSON.stringify(
      {
        source: "main_parts/warframeMarket.html",
        viewport,
        screen,
        bounds,
        elements,
      },
      null,
      2,
    )}\n`,
  );
  process.stdout.write(`${screenshotPath}\n${geometryPath}\n`);
} finally {
  await browser.close();
  await rm(documentPath, { force: true });
}

async function documentHtml(root) {
  const cssFiles = [
    "assets/css/base.css",
    "assets/css/color.css",
    "assets/css/main/foundry.css",
    "assets/css/main/inventory.css",
    "assets/css/main/rivenExplorer.css",
    "assets/css/main/wfmarket.css",
  ];
  const css = await Promise.all(
    cssFiles.map(async (file) => {
      const absolute = path.join(root, file);
      const source = (await readFile(absolute, "utf8")).replace(/^\uFEFF/, "");
      return rewriteCssUrls(source, path.dirname(absolute));
    }),
  );
  const itemImage = pathToFileURL(path.join(root, "assets/img/prime2.png")).href;

  return `<!doctype html>
    <html><head><style>${css.join("\n")}
      html,body{margin:0;width:100%;height:100%;overflow:hidden;background:var(--color-bg-1)}
      body{box-sizing:border-box;padding:18px;font-family:Roboto,sans-serif;color:white}
      *,*:before,*:after{box-sizing:border-box}
      .foundryObjectContainer.wfmItems{height:auto;overflow:hidden;align-content:start}
    </style></head><body>
      <div class="foundryObjectContainer wfmItems">
        ${orderCard({
          name: "Saryn Prime Chassis Blueprint",
          owned: 3,
          quantity: 5,
          platinum: 20,
          lowest: 20,
          image: itemImage,
          warning: true,
        })}
        ${orderCard({
          name: "Hush",
          owned: 8,
          quantity: 2,
          platinum: 4,
          lowest: 3,
          extra: "Rank 0",
          image: itemImage,
          visible: false,
        })}
        ${orderCard({
          name: "Axi A1 Relic",
          owned: 12,
          quantity: 1,
          platinum: 3,
          lowest: 1,
          image: itemImage,
        })}
      </div>
    </body></html>`;
}

function orderCard({
  name,
  owned,
  quantity,
  platinum,
  lowest,
  extra = "",
  image,
  visible = true,
  warning = false,
  side = "sell",
}) {
  return `<div class="wfmItem">
    <div class="wfmItemTop">
      <div class="wfmItemButton visibility ${visible ? "public" : ""}"></div>
      <div class="wfmItemName">${name}</div>
      <div class="wfmItemOwned">
        <div class="wfmItemOwnedText">${owned} owned</div>
        ${warning ? '<div class="wfmItemOwnedWarning">!</div>' : ""}
      </div>
    </div>
    <div class="wfmItemBottom ${visible ? "" : "private"}">
      <div class="wfmItemLeft" style="background-image:url('${image}')">
        <div class="wfmItemLeftText">${side === "sell" ? "WTS" : "WTB"}</div>
      </div>
      <div class="wfmItemRight">
        <div class="wfmItemBottomAmount"><span>${quantity}</span><div class="inventoryItemSellPlatIcon amount"></div></div>
        <div class="wfmItemBottomPlat"><span>${platinum}</span><div class="inventoryItemSellPlatIcon rivenExplorer"></div></div>
        <div class="wfmItemBottomExtra">${extra}</div>
        <div class="wfmItemLowestPrice">
          <div class="wfmItemButton showMore"></div><span>Lowest price:</span>${lowest}
          <div class="inventoryItemSellPlatIcon rivenExplorer"></div>
        </div>
        <div class="wfmItemButtons">
          <div class="wfmItemButton edit"></div><div class="wfmItemButton addOne"></div>
          <div class="wfmItemButton markSold"></div><div class="wfmItemButton remove"></div>
        </div>
      </div>
    </div>
  </div>`;
}

function rewriteCssUrls(css, directory) {
  return css.replace(/url\((['"]?)([^)'"\s]+)\1\)/g, (match, quote, value) => {
    if (/^(?:data:|https?:|file:|#|\/)/.test(value)) return match;
    return `url(${quote}${pathToFileURL(path.resolve(directory, value)).href}${quote})`;
  });
}
