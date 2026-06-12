// Captures the main view (with a selected torrent + detail panel) in every
// theme, plus the appearance settings pane. Requires the dev server and the
// browser IPC mock, same as ui-shots.mjs.
//
//   node scripts/theme-shots.mjs [output-dir]

import { chromium } from "playwright";
import { mkdirSync, readFileSync } from "node:fs";

const BASE = process.env.SHOT_URL ?? "http://localhost:1420";
const OUT = process.argv[2] ?? "/tmp/rbitt-ux/themes";
mkdirSync(OUT, { recursive: true });

// Pull theme ids from the registry so this never goes stale.
const registry = readFileSync(new URL("../src/themes.ts", import.meta.url), "utf8");
const ids = [...registry.matchAll(/^\s*id: "([a-z0-9-]+)",$/gm)].map((m) => m[1]);
console.log("themes:", ids.join(", "));

const browser = await chromium.launch();
const page = await browser.newPage({
  viewport: { width: 1440, height: 900 },
  deviceScaleFactor: 2,
  colorScheme: "dark",
});

for (const id of ids) {
  await page.goto(BASE);
  await page.evaluate((theme) => {
    localStorage.setItem("theme", theme);
    localStorage.setItem("accent", "auto");
  }, id);
  await page.goto(BASE);
  await page.waitForSelector(".torrent-row", { timeout: 15000 });
  await page.click(".torrent-row");
  await page.waitForTimeout(900);
  await page.screenshot({ path: `${OUT}/${id}.png` });
  console.log("captured", id);
}

// Appearance pane with the picker (in the default theme)
await page.evaluate(() => localStorage.setItem("theme", "dark"));
await page.goto(BASE);
await page.waitForSelector(".toolbar", { timeout: 15000 });
await page.click('.toolbar-btn[title="Settings"]');
await page.waitForTimeout(600);
await page.screenshot({ path: `${OUT}/appearance-pane.png` });
console.log("captured appearance-pane");

await browser.close();
console.log(`done → ${OUT}`);
