// redesign-3.71 secrets (Kenny, 2026-10-03: the approved demo plus "Alle
// drie"): the Secrets page as a person sees it, on the demo host
// (scripts/invariants-run.sh starts it and runs every test-e2e/*.e2e.js).
// One whole-screen case for the layout at 1894 and 390 px, one for the
// reveal → 30 s → hidden cycle, the Copy, and the audit trail Activity
// reads ("Kenny revealed gateway/traefik/.env"), never the value.
import { test } from "node:test";
import assert from "node:assert/strict";
import { chromium } from "playwright";

const BASE = process.env.INVARIANTS_BASE_URL ?? "http://127.0.0.1:8099";
const TOKEN = process.env.INVARIANTS_TOKEN ?? "test-invariants-token-1234";

/** @param {import("playwright").BrowserContext} context */
async function login(context) {
  const page = await context.newPage();
  await page.goto(`${BASE}/login`);
  await page.fill("#token", TOKEN);
  await Promise.all([
    page.waitForURL("**/overview", { timeout: 5000 }).catch(() => {}),
    page.click("button[type=submit]"),
  ]);
  return page;
}

/** @param {import("playwright").Page} page */
const panes = (page) =>
  page.evaluate(() => {
    const box = (/** @type {string} */ sel) => {
      const e = document.querySelector(sel);
      if (!e) return null;
      const r = e.getBoundingClientRect();
      return { x: r.x, y: r.y + scrollY, w: r.width, h: r.height };
    };
    return {
      list: box(".sx-stacks"),
      detail: box(".sx-md > section"),
      drawer: box(".sx-drawer"),
      header: box(".sx .nx-header"),
      scrollW: document.documentElement.scrollWidth,
      viewW: innerWidth,
      chips: [...document.querySelectorAll(".sx-stacks button")].map((b) => [
        b.getAttribute("data-drive-row"),
        b.querySelector(".nx-chip")?.textContent ?? null,
      ]),
      skeletons: document.querySelectorAll(".sx .sk").length,
    };
  });

test("secrets: three panes side by side at 1894 px, stacked at 390 px, every count exact", async () => {
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await login(context);
    // The page's own read, to hold the chips to.
    const declared = await page.evaluate(async () => {
      const r = await fetch("/data/secrets");
      return (await r.json()).stacks;
    });
    await page.goto(`${BASE}/secrets`);
    await page.waitForSelector(".sx-stacks button");
    await page.waitForTimeout(600);
    const wide = await panes(page);
    assert.ok(wide.list && wide.detail && wide.drawer, JSON.stringify(wide));
    assert.equal(wide.skeletons, 0, "a skeleton left after the read");
    // One row: stacks | secrets | change drawer, tops aligned (rule 6).
    assert.ok(wide.list.x + wide.list.w <= wide.detail.x, "list beside detail");
    assert.ok(
      wide.detail.x + wide.detail.w <= wide.drawer.x,
      "detail beside drawer",
    );
    assert.equal(Math.round(wide.list.y), Math.round(wide.detail.y));
    assert.equal(Math.round(wide.detail.y), Math.round(wide.drawer.y));
    assert.ok(wide.scrollW <= wide.viewW, "no sideways scroll at 1894 px");
    // Exact counters: every chip is that stack's secrets + files.
    assert.ok(wide.chips.length >= 3, JSON.stringify(wide.chips));
    for (const [stack, chip] of wide.chips) {
      const d = declared[stack ?? ""];
      const want = d
        ? d.unreadable
          ? "!"
          : String(d.secrets.length + d.files.length)
        : "0";
      assert.equal(chip, want, `${stack}'s chip`);
    }
    // The detail pane lists exactly what the chosen stack declares.
    const shown = await page.$$eval("[data-drive=reveal-secret]", (b) =>
      b.map((x) => x.getAttribute("data-drive-row")),
    );
    const stack = new URL(page.url()).searchParams.get("stack") ?? "gateway";
    const d = declared[stack] ?? declared.gateway;
    assert.equal(shown.length, d.secrets.length + d.files.length);

    await page.setViewportSize({ width: 390, height: 844 });
    await page.waitForTimeout(300);
    const phone = await panes(page);
    assert.ok(phone.list && phone.detail && phone.drawer);
    assert.ok(phone.list.y + phone.list.h <= phone.detail.y, "list above");
    assert.ok(
      phone.detail.y + phone.detail.h <= phone.drawer.y,
      "detail above drawer",
    );
    assert.ok(phone.scrollW <= 390, `sideways scroll: ${phone.scrollW} px`);
    for (const p of [phone.list, phone.detail, phone.drawer])
      assert.ok(p.x >= 0 && p.x + p.w <= 390, "a pane runs off the phone");
  } finally {
    await browser.close();
  }
});

test("secrets: a reveal hides itself after 30 s, Copy copies, and Activity names who did both, never the value", async () => {
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    await context.grantPermissions(["clipboard-read", "clipboard-write"], {
      origin: BASE,
    });
    const page = await login(context);
    await page.clock.install();
    await page.goto(`${BASE}/secrets?stack=gateway`);
    const row = "gateway/traefik/.env";
    const reveal = page.locator(
      `[data-drive=reveal-secret][data-drive-row="${row}"]`,
    );
    await reveal.waitFor();
    const value = page.locator(`[data-secret-value="${row}"]`);
    await reveal.click();
    await page.waitForFunction(
      (r) =>
        document
          .querySelector(`[data-secret-value="${r}"]`)
          ?.textContent?.startsWith("DEMO_VALUE="),
      row,
    );
    const secret = (await value.textContent()) ?? "";
    assert.match(secret, /^DEMO_VALUE=/);
    assert.equal(await reveal.textContent(), "Hide");
    await page.clock.fastForward(29_000);
    assert.equal(await value.textContent(), secret, "still shown at 29 s");
    await page.clock.fastForward(1_500);
    await page.waitForFunction(
      (r) =>
        !document
          .querySelector(`[data-secret-value="${r}"]`)
          ?.textContent?.startsWith("DEMO_VALUE="),
      row,
    );
    assert.equal(await reveal.textContent(), "Reveal", "hidden after 30 s");

    // Copy: on the clipboard, never on screen.
    await page
      .locator(`[data-drive=copy-secret][data-drive-row="${row}"]`)
      .click();
    await page.waitForSelector(".sx-toast");
    const toast = (await page.textContent(".sx-toast")) ?? "";
    assert.match(toast, /Copied traefik\/\.env|Press Ctrl C/);
    if (toast.startsWith("Copied"))
      assert.equal(
        await page.evaluate(() => navigator.clipboard.readText()),
        secret,
      );

    // The audit trail: who and which, never the value.
    const history = await page.evaluate(async () => {
      const r = await fetch("/data/history?since=0");
      return r.text();
    });
    assert.ok(!history.includes("DEMO_VALUE"), "a value reached the history");
    assert.match(history, /revealed gateway\/traefik\/\.env/);
    assert.match(history, /copied gateway\/traefik\/\.env/);
    await page.goto(`${BASE}/activity`);
    await page.waitForFunction(() =>
      document.body.textContent?.includes(
        "Kenny revealed gateway/traefik/.env",
      ),
    );
    const activity = (await page.textContent("#page")) ?? "";
    assert.ok(activity.includes("Kenny copied gateway/traefik/.env"));
    assert.ok(!activity.includes("DEMO_VALUE"), "Activity shows a value");
  } finally {
    await browser.close();
  }
});
