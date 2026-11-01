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

test("ux-hostsettings: an instant choice in This browser offers Undo that puts it back", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1280, height: 900 },
    });
    const page = await login(context);
    await page.goto(`${BASE}/settings`);
    const follow = page.locator("#st-follow");
    await follow.waitFor({ timeout: 10000 });
    await follow.evaluate((el) => /** @type {HTMLElement} */ (el).click());
    const undo = page.locator(".kp-toast button", { hasText: "Undo" });
    await undo.waitFor({ timeout: 5000 });
    assert.equal(await follow.isChecked(), true);
    await undo.click();
    assert.equal(await follow.isChecked(), false, "Undo did not put it back");
    await context.close();
  } finally {
    await browser.close();
  }
});

test("ux-hostsettings: Restore hides Back on step I and ages a snapshot in days", async () => {
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
    await context.close();
  } finally {
    await browser.close();
  }
});
