// redesign-3.71 secrets (Kenny, 2026-10-03: the approved demo plus "Alle
// drie"): the Secrets page as a person sees it, on the demo host
// (scripts/invariants-run.sh starts it and runs every test-e2e/*.e2e.js).
// One whole-screen case for a stack hub's Secrets at 1894 and 390 px
// (the fleet-wide page became each hub's since feat-shell-1), one for the
// reveal → 30 s → hidden cycle, the Copy, and the audit trail Activity
// reads ("Kenny revealed gateway/traefik/.env"), never the value.
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
    page.waitForURL("**/overview", { timeout: 5000 }).catch(() => {}),
    page.click("button[type=submit]"),
  ]);
  return page;
}

/** @param {import("playwright").Page} page */
const panes = (page) =>
  page.evaluate(() => {
    const box = (/** @type {Element | null} */ e) => {
      if (!e) return null;
      const r = e.getBoundingClientRect();
      return { x: r.x, y: r.y + scrollY, w: r.width, h: r.height };
    };
    const root = document.querySelector("#secrets .sx--one");
    const detail = root?.querySelector(".sx-one-detail") ?? null;
    return {
      list: box(document.querySelector(".sx-stacks")),
      detail: box(detail),
      drawer: box(root?.querySelector(".sx-drawer--one") ?? null),
      // review 5: inside the hub the pane is no second card: no heading of
      // its own, no Open stack, no other stack to move to.
      heads: detail ? detail.querySelectorAll("h2, h3").length : -1,
      cardInCard: detail?.classList.contains("kp-card") ?? null,
      openStack: [...(detail?.querySelectorAll("a") ?? [])].some(
        (a) => (a.textContent ?? "").trim() === "Open stack",
      ),
      keys: detail?.querySelector(".nx-card__foot")?.textContent ?? "",
      rows: [...document.querySelectorAll("[data-drive=reveal-secret]")].map(
        (b) => b.getAttribute("data-drive-row"),
      ),
      scrollW: document.documentElement.scrollWidth,
      viewW: innerWidth,
      skeletons: root ? root.querySelectorAll(".sk").length : -1,
    };
  });

// feat-shell-1 moved the Secrets page into each stack hub's Settings
// (`/secrets` redirects there), so the fleet-wide three panes are gone;
// this case holds the hub's one-stack Secrets instead (redesign-stackhub
// review 5, redesign-integrate-3).
test("secrets: a stack hub's Secrets lists exactly that stack's secrets beside its change drawer at 1894 px, stacked at 390 px, no card in a card", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await login(context);
    // The page's own read, to hold the rows to.
    const declared = await page.evaluate(async () => {
      const r = await fetch("/data/secrets");
      return (await r.json()).stacks;
    });
    const stack = Object.keys(declared).find(
      (s) =>
        !declared[s].unreadable &&
        declared[s].secrets.length + declared[s].files.length > 0,
    );
    assert.ok(
      stack,
      `no demo stack declares a secret: ${JSON.stringify(declared)}`,
    );
    await page.goto(`${BASE}/stacks/${stack}/settings?section=secrets`);
    await page.waitForSelector("[data-drive=reveal-secret]", {
      timeout: 10000,
    });
    await page.waitForTimeout(600);
    const wide = await panes(page);
    assert.ok(wide.detail && wide.drawer, JSON.stringify(wide));
    assert.equal(wide.list, null, "a fleet-wide stack list inside the hub");
    assert.equal(wide.skeletons, 0, "a skeleton left after the read");
    assert.equal(wide.heads, 0, "the pane carries a heading of its own");
    assert.equal(wide.cardInCard, false, "the pane is a card inside the card");
    assert.equal(wide.openStack, false, "the pane links to the stack it is on");
    assert.doesNotMatch(wide.keys, /stack ·/, "the keys offer another stack");
    // One row: the secrets and the change drawer, tops aligned (rule 6).
    assert.ok(
      wide.detail.x + wide.detail.w <= wide.drawer.x,
      "the secrets beside the drawer",
    );
    assert.equal(Math.round(wide.detail.y), Math.round(wide.drawer.y));
    assert.ok(wide.scrollW <= wide.viewW, "no sideways scroll at 1894 px");
    // Exactly what this stack declares, and only this stack.
    const d = declared[stack];
    assert.equal(wide.rows.length, d.secrets.length + d.files.length);
    assert.ok(
      wide.rows.every((r) => (r ?? "").startsWith(`${stack}/`)),
      JSON.stringify(wide.rows),
    );

    await page.setViewportSize({ width: 390, height: 844 });
    await page.waitForTimeout(300);
    const phone = await panes(page);
    assert.ok(phone.detail && phone.drawer);
    assert.ok(
      phone.detail.y + phone.detail.h <= phone.drawer.y,
      "the secrets above the drawer",
    );
    assert.ok(phone.scrollW <= 390, `sideways scroll: ${phone.scrollW} px`);
    for (const b of [phone.detail, phone.drawer])
      assert.ok(b.x >= 0 && b.x + b.w <= 390, "a pane runs off the phone");
  } finally {
    await browser.close();
  }
});

test("secrets: a reveal hides itself after 30 s, Copy copies, and Activity names who did both, never the value", async () => {
  const browser = await launch();
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
    await page.waitForSelector("#page .kp-toasts .kp-toast");
    const toast = (await page.textContent("#page .kp-toasts .kp-toast")) ?? "";
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
