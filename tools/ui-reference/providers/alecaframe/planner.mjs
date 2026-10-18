import { mkdir, readFile, writeFile } from "node:fs/promises";
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
const screenshotPath = path.join(outputDirectory, "alecaframe-relic-planner.png");
const geometryPath = path.join(outputDirectory, "alecaframe-relic-planner.json");
const screen = displayDimensions();
const bounds = {
  width: Math.min(1280, screen.width - 80),
  height: Math.min(800, screen.height - 80),
};
bounds.left = Math.round((screen.width - bounds.width) / 2);
bounds.top = Math.round((screen.height - bounds.height) / 2);

await mkdir(outputDirectory, { recursive: true });
const browser = await chromium.launch({
  headless: true,
  args: ["--allow-file-access-from-files"],
});

try {
  const page = await browser.newPage({ viewport: bounds, deviceScaleFactor: 1 });
  await page.setContent(await documentHtml(webRoot), { waitUntil: "load" });
  await page.evaluate(() => document.fonts.ready);
  await page.waitForSelector(".inventoryObject.relicPlanner:nth-child(6)");

  const selectors = [
    "#tabRelicPlanner",
    ".foundryTopSettings",
    ".inventoryObjectContainer",
    ".inventoryObject.relicPlanner",
    ".inventoryItemImage",
    ".relicPlannerMiddle",
    ".relicDetailsBLTopExpected.relicPlanner",
    ".relicPlannerRewardContainer",
    ".foundryObjectComponentsComponent",
    ".inventorySummary",
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
              borderRadius: style.borderRadius,
              backgroundColor: style.backgroundColor,
              fontFamily: style.fontFamily,
              fontSize: style.fontSize,
            },
          };
        }),
      ]),
    );
  }, selectors);

  await saveWindowHtml(page, screenshotPath.replace(/\.png$/, ".html"));
  const planner = await captureScreenshot(page, { omitBackground: true });
  await page.setViewportSize(screen);
  await page.setContent(`
    <!doctype html>
    <html style="margin:0;background:transparent">
      <body style="margin:0;background:transparent;overflow:hidden">
        <img src="data:image/png;base64,${planner.toString("base64")}" width="${bounds.width}"
          height="${bounds.height}" style="position:absolute;left:${bounds.left}px;top:${bounds.top}px">
      </body>
    </html>
  `);
  await captureScreenshot(page, { path: screenshotPath, omitBackground: true });
  await writeFile(
    geometryPath,
    `${JSON.stringify({ source: "main_parts/relicPlanner.html", screen, bounds, elements }, null, 2)}\n`,
  );
  process.stdout.write(`${screenshotPath}\n${geometryPath}\n`);
} finally {
  await browser.close();
}

async function documentHtml(root) {
  const cssFiles = [
    "assets/css/base.css",
    "assets/css/color.css",
    "assets/css/main/foundry.css",
    "assets/css/main/inventory.css",
    "assets/css/main/modal.css",
    "assets/css/main/relicPlanner.css",
  ];
  const css = await Promise.all(cssFiles.map(async (file) => {
    const absolute = path.join(root, file);
    const source = (await readFile(absolute, "utf8")).replace(/^\uFEFF/, "");
    return rewriteCssUrls(source, path.dirname(absolute));
  }));
  const image = pathToFileURL(path.join(root, "assets/img/axi-intact.png")).href;
  const reward = pathToFileURL(path.join(root, "assets/img/prime2.png")).href;
  const platinum = pathToFileURL(path.join(root, "assets/img/platinum.png")).href;
  const ducats = pathToFileURL(path.join(root, "assets/img/ducats.png")).href;
  const vaulted = pathToFileURL(path.join(root, "assets/img/vaulted.png")).href;
  const cards = ["Axi A1", "Axi B3", "Axi G12", "Axi H7", "Axi L5", "Axi S17"]
    .map((name, index) => relicCard({ name, index, image, reward, vaulted }))
    .join("");

  return `<!doctype html>
    <html><head><style>${css.join("\n")}
        html,body{margin:0;width:100%;height:100%;overflow:hidden;background:var(--color-bg-1)}
        #tabRelicPlanner{display:flex;width:100%;height:100%;padding:14px;box-sizing:border-box}
        #relicPlannerMainBody{flex:1;min-height:0}
        #inventoryObjectContainer{height:100%;align-content:start;overflow:hidden}
        .reference-icon{width:16px;height:16px;background-size:contain;background-repeat:no-repeat;background-position:center}
        .inventoryRelicIcons{display:flex;gap:12px}.inventorySummary{position:absolute;left:14px;right:14px;bottom:0}
      </style>
    </head><body>
      <div id="tabRelicPlanner" class="tab relicPlanner">
        <div class="foundryTopSettings">
          <div class="foundryTopSettingsSide left">
            ${["All", "Lith", "Meso", "Neo", "Axi"].map((era) => `<div class="topSetting relicPlanner ${era === "Axi" ? "selected" : ""}">${era}</div>`).join("")}
          </div>
          <div class="foundryTopSettingsSide right">
            <div class="topSetting relicPlanner"><div class="topSettingIcon search"></div><input id="relicPlannerSearch" placeholder="Search"></div>
            <div class="topSetting2 relicPlanner"><select><option>Platinum profit</option></select></div>
            <div class="topSetting2 relicPlanner">Squad: <select><option>4</option></select></div>
          </div>
        </div>
        <div id="relicPlannerMainBody" class="foundryMainBody">
          <div id="inventoryObjectContainer" class="inventoryObjectContainer">${cards}</div>
        </div>
        <div class="inventorySummary"><div>1842</div><div class="reference-icon" style="background-image:url('${pathToFileURL(path.join(root, "assets/img/trace.webp")).href}')"></div></div>
      </div>
    </body></html>`;

  function relicCard({ name, index, image, reward, vaulted }) {
    const refinements = [
      [index + 1, "Intact", 12 + index, 65],
      [Math.max(0, index - 1), "Exceptional", 15 + index, 72],
      [index % 3, "Flawless", 18 + index, 81],
      [index % 2, "Radiant", 23 + index, 96],
    ];
    return `<div class="inventoryObject relicPlanner">
      <div class="inventoryItemImage" style="background-image:url('${image}')">
        <div class="inventoryItemQuantity absolute">x${index + 2}</div>
        ${index % 2 ? `<div class="foundryObjectTopIcon vaulted yes" style="background-image:url('${vaulted}')"></div>` : ""}
      </div>
      <div class="relicPlannerMiddle smallText">
        <div class="relicDetailsTop"><div class="inventoryItemName"><span>${name} Relic</span></div>
          <div class="inventoryRelicIcons"><div class="reference-icon" style="background-image:url('${platinum}')"></div><div class="reference-icon" style="background-image:url('${ducats}')"></div></div>
        </div>
        <div class="relicPlannerRarityHolder">
          ${refinements.map(([count, tier, plat, ducat]) => `<div class="relicDetailsBLTopExpected relicPlanner smallText"><div class="relicDetailsBLTopExpectedTitle">${count}x ${tier}:</div><div class="relicDetailsBLTopExpectedCoins"><div class="relicDetailsBLTopExpectedCoinsCoin relicPlanner">${plat}</div><div class="relicDetailsBLTopExpectedCoinsCoin relicPlanner">${ducat}</div></div></div>`).join("")}
        </div>
      </div>
      <div class="relicPlannerRewardContainer">
        ${Array.from({ length: 6 }, (_, rewardIndex) => `<div class="foundryObjectComponentsComponent ${rewardIndex === index % 6 ? "gotNeccessaryComponents" : ""}"><div class="relicPlannerRewardInnerImage" style="background-image:url('${reward}')"></div></div>`).join("")}
      </div>
    </div>`;
  }
}

function rewriteCssUrls(css, directory) {
  return css.replace(/url\((['"]?)([^)'"\s]+)\1\)/g, (match, quote, value) => {
    if (/^(?:data:|https?:|file:|#|\/)/.test(value)) return match;
    return `url(${quote}${pathToFileURL(path.resolve(directory, value)).href}${quote})`;
  });
}

function displayDimensions() {
  const value = process.env.ALECA_LAYOUT_SIZE ?? "2560x1440";
  const match = /^(\d+)x(\d+)$/i.exec(value);
  if (!match) throw new Error("ALECA_LAYOUT_SIZE must be WIDTHxHEIGHT");
  return { width: Number(match[1]), height: Number(match[2]) };
}
