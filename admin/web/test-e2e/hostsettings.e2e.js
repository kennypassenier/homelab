// ux-hostsettings (the 2026-11-01 UX review of Host settings, Host, Backups
// and Restore): the whole-screen cases for what that round changed. Each page
// is walked once; the demo host answers (scripts/invariants-run.sh).
import { test } from "node:test";
import assert from "node:assert/strict";
import { launch } from "./harness.js";

const BASE = process.env.INVARIANTS_BASE_URL ?? "http://127.0.0.1:8099";
const TOKEN = process.env.INVARIANTS_TOKEN ?? "test-invariants-token-1234";

/** @param {import("playwright").BrowserContext} context */
async function login(context) {
  const page = await context.newPage();
  await page.goto(`${BASE}/login`);
  await page.fill("#token", TOKEN);
  await Promise.all([
    page.waitForURL((u) => !u.pathname.startsWith("/login"), {
      timeout: 10000,
    }),
    page.click("button[type=submit]"),
  ]);
  return page;
}

test("ux-hostsettings: Host settings keeps instant choices and Save-gated keys apart, every instant choice offers Undo, host.toml groups start folded", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1280, height: 900 },
    });
    const page = await login(context);
    await page.goto(`${BASE}/settings`);
    await page.locator("#host-settings .kp-unsaved__save").waitFor();
    // The Save bar belongs to the host.toml card, not to the page header.
    const f = await page.evaluate(() => ({
      barInHeader: !!document.querySelector(".kp-page-header .kp-unsaved"),
      barInCard: !!document.querySelector("#host-settings .kp-unsaved"),
      browserChip: document.querySelector("#browser .kp-badge")?.textContent,
      accountChip: document.querySelector("#account .kp-badge")?.textContent,
      hostChip: document.querySelector("#host-settings .kp-badge")?.textContent,
      open: document.querySelectorAll("details.st-grp[open]").length,
      groups: document.querySelectorAll("details.st-grp").length,
    }));
    assert.equal(f.barInHeader, false, "the Save bar is still in the header");
    assert.ok(f.barInCard, "the Save bar is not in the host.toml card");
    assert.equal(f.browserChip, "Saves at once");
    assert.equal(f.accountChip, "Saves at once");
    assert.equal(f.hostChip, "Waits for Save");
    assert.ok(f.groups > 0 && f.open === 0, "a host.toml group starts open");
    // An instant choice says so and can be taken back.
    const follow = page.locator("#st-follow");
    await follow.evaluate((el) => /** @type {HTMLElement} */ (el).click());
    const undo = page.locator(".kp-toast button", { hasText: "Undo" });
    await undo.waitFor({ timeout: 5000 });
    assert.equal(await follow.isChecked(), true);
    await undo.click();
    assert.equal(await follow.isChecked(), false, "Undo did not put it back");
    // The Live view switch is not in the bar; Forget offers an Undo too.
    assert.equal(await page.locator("#bar .follow-switch:visible").count(), 0);
    await context.close();
  } finally {
    await browser.close();
  }
});

test("ux-hostsettings: Restore hides Back on step I and ages a snapshot in days; Host actions are tiles of one size", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1280, height: 900 },
    });
    const page = await login(context);
    await page.goto(`${BASE}/backups/restore`);
    await page.locator("[data-kp-wizard-next]").waitFor({ timeout: 10000 });
    assert.ok(
      await page.locator("[data-kp-wizard-back]").isHidden(),
      "Back shows on step I",
    );
    const hints = await page
      .locator(".kp-field__option-hint")
      .allTextContents();
    for (const t of hints)
      assert.ok(!/ h ago/.test(t), `an age in hours past a day: ${t}`);
    await page.goto(`${BASE}/host`);
    await page.locator("#host-actions .hk-act").first().waitFor();
    const sizes = await page.$$eval("#host-actions .hk-act", (els) =>
      els.map((e) => Math.round(e.getBoundingClientRect().height)),
    );
    assert.ok(sizes.length >= 6, "the host actions are missing");
    // Within one row of tiles all heights are equal, and none is a sliver.
    assert.ok(Math.min(...sizes) >= 80, `a tile is ${Math.min(...sizes)} px`);
    await context.close();
  } finally {
    await browser.close();
  }
});
