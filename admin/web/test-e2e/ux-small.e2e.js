// ux-small (Kenny's decisions of 2026-11-01 on the UX review): the
// whole-screen cases for 44 px hit areas on a phone, the 44 px dialog close,
// the one stack filter on Backups and the System rail's words. The demo host
// answers (scripts/invariants-run.sh).
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

/** What a thumb presses, outside the kit-drawn grids that scroll inside their frame. */
const CONTROLS =
  "button.kp-button, a.kp-button, .kp-segmented__item, .kp-icon-button, .kp-nav__search-trigger, .kp-anchor, .kp-contents__link, .fw-stackbtn, .bk-group__name";

test("ux-small-1: on a phone every button, chip, tab and segmented item is a 44 px target", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 390, height: 900 },
    });
    await login(context);
    const small = [];
    // The seven pages are read side by side: one after the other their
    // 2.5 s settling waits alone took 17.5 s of the 20 s case deadline.
    const paths = [
      "/backups",
      "/settings",
      "/charts",
      "/host",
      "/firewall",
      "/stacks",
      "/needs-you",
    ];
    const found = await Promise.all(
      paths.map(async (path) => {
        const page = await context.newPage();
        await page.goto(`${BASE}${path}`);
        await page.waitForTimeout(2500);
        const hits = await page.$$eval(CONTROLS, (els) =>
          els
            .filter((e) => {
              const b = e.getBoundingClientRect();
              const s = getComputedStyle(e);
              return (
                b.width > 0 &&
                s.visibility !== "hidden" &&
                !e.closest("[hidden], .kp-sr-only") &&
                (b.height < 43.5 || b.width < 43.5)
              );
            })
            .map((e) => `${e.className} ${e.textContent?.trim().slice(0, 20)}`),
        );
        await page.close();
        return hits.map((f) => `${path}: ${f}`);
      }),
    );
    for (const hits of found) small.push(...hits);
    assert.deepEqual(small, [], "controls under 44 px on a phone");
    await context.close();
  } finally {
    await browser.close();
  }
});

test("ux-small-2/3/4: the dialog's close x is a 44 px target and Esc still closes it; Backups picks a stack in one place, the chips, and the table's own Filters panel does not offer it; the folded System rail names itself and its fold control in words", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1280, height: 1000 },
    });
    const page = await login(context);
    // ux-small-2: the dialog's close x.
    await page.goto(`${BASE}/backups`);
    await page.getByRole("button", { name: "Back up now" }).click();
    const close = page.locator("dialog[open] .kp-dialog__close");
    await close.waitFor({ timeout: 5000 });
    const b = await close.boundingBox();
    assert.ok(b && b.width >= 43.5 && b.height >= 43.5, JSON.stringify(b));
    await page.keyboard.press("Escape");
    await page.locator("dialog[open]").waitFor({ state: "detached" });
    // ux-small-3: the stack filter on Backups.
    await page.goto(`${BASE}/backups`);
    await page
      .locator(".bk-repos__facets .kp-facets__list li")
      .first()
      .waitFor({
        timeout: 15000,
      });
    const status = () =>
      page
        .locator("#bk-repo-table")
        .getByText(/Showing \d/)
        .first()
        .innerText();
    const all = await status();
    await page
      .locator("#bk-repo-table")
      .getByRole("button", { name: /Filters/ })
      .first()
      .click();
    const fields = await page
      .locator("#bk-repo-table fieldset legend:visible")
      .allTextContents();
    assert.deepEqual(fields, ["Restore drill"], "the Filters panel's fields");
    // Unticking a chip filters the table (the Filters panel still holds the
    // field, hidden, which is how the two stay in step).
    await page
      .locator(".bk-repos__facets .kp-facet__option", { hasText: "gateway" })
      .click();
    await page.waitForTimeout(500);
    const picked = await status();
    assert.match(picked, /of 3 \(filtered from 7\)/, picked);
    // The stack bar picks one stack: the chips follow and the table shows it.
    await page
      .locator('.kp-stackbar__item[data-kp-stackbar-key="admin"]')
      .click();
    await page.waitForTimeout(500);
    const ticked = await page
      .locator(".bk-repos__facets .kp-facet__check:checked")
      .evaluateAll((els) =>
        els.map((e) =>
          e
            .closest("li")
            ?.querySelector(".kp-facet__name")
            ?.textContent?.trim(),
        ),
      );
    assert.deepEqual(ticked, ["admin"]);
    assert.match(await status(), /of 1 \(filtered from 7\)/);
    // ux-small-4: the folded System rail.
    await page.goto(`${BASE}/host`);
    const rail = page.locator("#system-rail");
    await rail.waitFor({ timeout: 10000 });
    const title = rail.locator(".app-rail__short");
    assert.equal((await title.innerText()).trim().toLowerCase(), "system");
    assert.equal(await title.getAttribute("aria-label"), "System");
    const toggle = rail.locator("[data-kp-sidenav-slim-toggle]");
    assert.match(await toggle.innerText(), /Expand/);
    assert.doesNotMatch(await toggle.innerText(), /Collapse/);
    assert.ok(
      await toggle.getAttribute("aria-label"),
      "the fold control has no name",
    );
    await context.close();
  } finally {
    await browser.close();
  }
});
