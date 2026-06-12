// Screenshot harness for UI work.
//
// Drives the frontend in headless Chromium against the Vite dev server with
// the browser IPC mock (src/mocks/tauri-mock.ts) supplying engine data, and
// captures every major UI state. Run the dev server first, then:
//
//   node scripts/ui-shots.mjs [output-dir]
//
// Defaults to /tmp/rbitt-ux/shots.

import { chromium } from "playwright";
import { mkdirSync } from "node:fs";

const BASE = process.env.SHOT_URL ?? "http://localhost:1420";
const OUT = process.argv[2] ?? "/tmp/rbitt-ux/shots";
mkdirSync(OUT, { recursive: true });

const browser = await chromium.launch();
const page = await browser.newPage({
  viewport: { width: 1440, height: 900 },
  deviceScaleFactor: 2,
  colorScheme: "dark",
});
page.on("pageerror", (e) => console.error("pageerror:", e.message));

async function shot(name) {
  await page.screenshot({ path: `${OUT}/${name}.png` });
  console.log("captured", name);
}

async function tryClick(selector) {
  try {
    await page.click(selector, { timeout: 2000 });
    return true;
  } catch {
    console.warn("skip (selector not found):", selector);
    return false;
  }
}

async function boot(url) {
  await page.goto(url);
  await page.waitForSelector(".toolbar", { timeout: 20000 });
  await page.waitForTimeout(1500); // let the 1s polling fill in data
}

await boot(BASE);
await shot("01-main-dark");

// Detail panel tabs (select the first torrent row first)
if (await tryClick(".torrent-row")) {
  await page.waitForTimeout(700);
  await shot("02-detail-general");
  for (const [tab, name] of [
    ["Trackers", "03-detail-trackers"],
    ["Peers", "04-detail-peers"],
    ["Files", "05-detail-files"],
  ]) {
    if (await tryClick(`.detail-tabs button:has-text("${tab}")`)) {
      await page.waitForTimeout(500);
      await shot(name);
    }
  }
}

// Add torrent modal with a parsed magnet preview
if (await tryClick('.toolbar-btn[title="Add Torrent"]')) {
  await page.waitForTimeout(300);
  try {
    await page.fill(
      ".magnet-input input",
      "magnet:?xt=urn:btih:a94a8fe5ccb19ba61c4c0873d391e987982fbbd3&dn=ubuntu-24.04.2-desktop-amd64.iso",
      { timeout: 2000 },
    );
    await tryClick('button:has-text("Parse")');
    await page.waitForTimeout(400);
  } catch {
    console.warn("magnet input not fillable");
  }
  await shot("06-add-modal");
  await tryClick(".modal-close");
}

// Settings modal, walking the tabs
if (await tryClick('.toolbar-btn[title="Settings"]')) {
  await page.waitForTimeout(500);
  await shot("07-settings-general");
  for (const [tab, name] of [
    ["Downloads", "08-settings-downloads"],
    ["Connection", "09-settings-connection"],
    ["Categories", "10-settings-categories"],
    ["Watch", "11-settings-watch"],
    ["Advanced", "12-settings-advanced"],
  ]) {
    if (await tryClick(`.settings-tabs button:has-text("${tab}")`)) {
      await page.waitForTimeout(300);
      await shot(name);
    }
  }
  await tryClick(".modal-close");
}

// RSS modal
if (await tryClick('.sidebar-item:has-text("RSS")')) {
  await page.waitForTimeout(600);
  await shot("13-rss-feeds");
  if (await tryClick('button:has-text("Rules")')) {
    await page.waitForTimeout(300);
    await shot("14-rss-rules");
  }
  await tryClick(".modal-close");
}

// Search modal with results
if (await tryClick('.sidebar-item:has-text("Search")')) {
  await page.waitForTimeout(400);
  try {
    await page.fill(".search-modal input, .modal input", "ubuntu", { timeout: 2000 });
    await tryClick('.search-input-row button:has-text("Search")');
    await page.waitForTimeout(1500);
  } catch {
    console.warn("search input not fillable");
  }
  await shot("15-search");
  await tryClick(".modal-close");
}

// Empty / first-run state
await boot(`${BASE}/?mock=empty`);
await shot("16-empty-state");

// Light theme pass
await page.evaluate(() => localStorage.setItem("theme", "light"));
await boot(BASE);
await shot("17-main-light");
if (await tryClick(".torrent-row")) {
  await page.waitForTimeout(700);
  await shot("18-detail-light");
}
await page.evaluate(() => localStorage.setItem("theme", "system"));

await browser.close();
console.log(`done → ${OUT}`);
