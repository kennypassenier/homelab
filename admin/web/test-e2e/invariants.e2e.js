// docs/INVARIANTS.md — the Playwright smoke that pins the UI invariants
// Kenny has stated as "must always be so", driven against the real
// rendered page (the `demo-host` build; feat-platform-10, decided
// 2026-09-28: "Only in test builds"). Not run under `node --test test/`
// (tsc/prettier's normal admin/web suite): it needs a live server and
// browsers, so it is its own small suite, wired as `make invariants` and
// into `make gate`'s full run via `.githooks/gate-carry.sh invariants`
// (scripts/invariants-run.sh starts and stops the server around it).
//
// Kenny, 2026-09-28 (tech-js-checks): "one short Playwright smoke per
// milestone, not a huge suite" — this file stays short on purpose: one
// case per invariant, DOM assertions rather than screenshots, against the
// one demo server `scripts/invariants-run.sh` already has waiting at
// INVARIANTS_BASE_URL.
import { test } from "node:test";
import assert from "node:assert/strict";
import { launch } from "./harness.js";
// fix-210: this import was missing — the fix-206 spacing sweep below has
// referenced DRIVABLE_PATHS since it was written without ever importing
// it, a ReferenceError this suite's own runs apparently never surfaced
// loudly enough to get fixed; found while adding this file's own fix-210
// cases, which need the same list.
import { DRIVABLE_PATHS, PATH_TO_PAGE, route } from "../js/router.js";

const BASE = process.env.INVARIANTS_BASE_URL ?? "http://127.0.0.1:8099";
const TOKEN = process.env.INVARIANTS_TOKEN ?? "test-invariants-token-1234";

/** @type {any[] | null} the session's cookies, after the one login */
let session = null;

/**
 * Gives `context` a logged-in session: the suite logs in ONCE and every
 * later context reuses that session's cookies. Before 3.71.0 each case
 * logged in anew and then waited up to 5 s for an `/overview` the login
 * never landed on, which spaced the logins out by accident; since the
 * redesign (feat-shell-1) the wait ends at once and ~60 logins a run trip
 * the kit's own per-address `/login` rate limit (K10).
 * @param {import("playwright").BrowserContext} context
 */
async function logIn(context) {
  if (session) {
    await context.addCookies(session);
    return;
  }
  const page = await context.newPage();
  await page.goto(`${BASE}/login`);
  await page.fill("#token", TOKEN);
  await Promise.all([
    page.waitForURL((u) => !u.pathname.startsWith("/login"), {
      timeout: 10000,
    }),
    page.click("button[type=submit]"),
  ]);
  session = (await context.storageState()).cookies;
  await page.close();
}

/**
 * A logged-in page left on /stacks (feat-shell-1: the stack list's
 * address since 3.71.0; /overview redirects there).
 * @param {import("playwright").BrowserContext} context
 */
async function freshPage(context) {
  // A case that can hang for minutes is a defect: every Playwright wait in
  // a case's context ends after 20 s unless the case asks for longer.
  context.setDefaultTimeout(20000);
  context.setDefaultNavigationTimeout(20000);
  await logIn(context);
  const page = await context.newPage();
  await page.goto(`${BASE}/stacks`);
  return page;
}

/**
 * redesign-stackhub: open the stack hub header's More ▾ menu (where most
 * stack actions live since 3.71.0), when the page has one and it is shut.
 * @param {import("playwright").Page} page
 */
async function openStackMore(page) {
  const more = page.locator('[data-drive="stack-more"]');
  if ((await more.count()) === 0) return;
  if ((await more.getAttribute("aria-expanded")) !== "true")
    await more.click({ timeout: 2000 }).catch(() => {});
}

test("invariants: the nav bar stays inline, with brand Homelab and the version beside the search box, at 1600/1920/2560 CSS px and at 1894 in the widest themes", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    // Kenny, 2026-10-02: a 4K screen at 200% scaling is ~1894 CSS px, and
    // the widest themes (deco, cyberpunk: wide-spaced capitals) need it
    // all — measured: they fold at 1760 and fit at 1894. The default
    // theme folds at 1440 and fits from 1600 (1280 never fitted: the old
    // case passed only because it measured before the bar had filled). Each width is
    // checked in the default theme and in the widest one.
    const cases = [1600, 1920, 2560].map((w) => [w, "dark"]);
    cases.push([1894, "deco"], [1894, "cyberpunk"]);
    for (const [width, theme] of cases) {
      await page.evaluate((t) => localStorage.setItem("theme", t), theme);
      await page.reload();
      await page.setViewportSize({ width, height: 900 });
      // fix-176's own fitBar() re-measures on resize/content changes; give
      // it a turn of the event loop rather than assuming it already ran.
      await page.waitForTimeout(150);
      const folded = await page.locator("#bar").getAttribute("data-fold");
      assert.equal(
        folded,
        null,
        `the nav bar folded into the hamburger at ${width}px in ${theme} (fix-176)`,
      );
      const brand = await page.locator("#brand").textContent();
      assert.equal(brand, "Homelab", `brand text wrong at ${width}px`);
      // fix-176 (b): the version text and the "Go to…" search control sit
      // in the same right-hand cluster, not hundreds of pixels apart.
      const linkBox = await page.locator("#link").boundingBox();
      const searchBox = await page.locator(".kp-nav__search").boundingBox();
      assert.ok(linkBox && searchBox, `missing nav elements at ${width}px`);
      const gap = searchBox.x - (linkBox.x + linkBox.width);
      assert.ok(
        gap < 60,
        `version text sits ${gap}px from "Go to…" at ${width}px (fix-176 regression: it was hundreds of px)`,
      );
    }
  } finally {
    await browser.close();
  }
});

test("invariants: Pause and Stop are visible in every Live view drive state", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.check("#live-view");
    await page.waitForTimeout(150);
    // Drive one step through the demo-host-only test endpoint
    // (admin/src/shell/drive.rs::demo_step, "never mounted beside a real
    // host"); the dashboard announces it with its usual countdown
    // (HOMELAB_ADMIN_LIVE_ANNOUNCE_MS, 3 s default), during which Pause
    // and Stop must both be on screen (fix-172, fix-173).
    const fired = page
      .evaluate(() =>
        fetch("/data/drive/demo-step", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ do: "goto", path: "/host" }),
        }),
      )
      .catch(() => {});
    await page.waitForTimeout(800);
    assert.ok(
      await page.getByRole("button", { name: "Pause" }).isVisible(),
      "Pause is not visible while Live view is driving a step",
    );
    assert.ok(
      await page.getByRole("button", { name: "Stop" }).isVisible(),
      "Stop is not visible while Live view is driving a step",
    );
    await fired;
  } finally {
    await browser.close();
  }
});

test("invariants: the backup calendar shows its skeleton grid immediately, before any stack has answered", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    // fix-177: the real per-day grid is laid out at once (`gridShape`), each
    // cell pulsing, until every stack answers. Delay the read so the
    // skeleton's presence does not depend on how fast the demo host is.
    await page.route("**/data/backup-calendar*", async (route) => {
      await new Promise((r) => setTimeout(r, 1500));
      await route.continue();
    });
    await page.goto(`${BASE}/backupcalendar`);
    const skeletonCells = await page
      .locator(".backup-cal__cell--skeleton")
      .count();
    assert.ok(
      skeletonCells > 0,
      "no skeleton cells painted before the backup calendar's data arrived",
    );
  } finally {
    await browser.close();
  }
});

test("invariants: the job dialog's panel does not shift sideways between its running and done states", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.goto(`${BASE}/stacks/films`);
    await page.waitForTimeout(600);
    // H8 (per-stack enabled flag): "Park" ("Disable" before 3.71.0, the
    // approved rename) needs only the stack name, so it runs end to end
    // against the demo host without a working copy of the repository
    // (deploy/backup both refuse without one there).
    // redesign-stackhub: Park sits in the hub header's More ▾ (Pause).
    await page.locator('[data-drive="stack-more"]').click({ timeout: 5000 });
    await page
      .locator('[role=menuitem][data-action="disable"]')
      .click({ timeout: 5000 });
    await page.waitForTimeout(300);
    await page.locator("#action-dialog #act-run").click();
    await page.waitForTimeout(500); // the panel has mounted, job running
    const panel = page.locator("#action-dialog .job-panel").first();
    const running = await panel.boundingBox();
    assert.ok(running, "the job panel did not mount while the job ran");
    await page.waitForTimeout(4000); // the demo host finishes its op
    const done = await panel.boundingBox();
    assert.ok(done, "the job panel disappeared once the job finished");
    // New log lines and an outcome line are expected to make the panel
    // taller (rule 6 allows a log to grow); what it must never do is move
    // or resize sideways, the way an unstructured layout would when a
    // field of different width appeared above it.
    assert.equal(
      running.x,
      done.x,
      "the job panel shifted horizontally between running and done",
    );
    assert.equal(
      running.width,
      done.width,
      "the job panel changed width between running and done",
    );
  } finally {
    await browser.close();
  }
});

test("invariants: an expandable row opens and closes from a click anywhere in it, never from its own controls", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    // redesign-final-c3: the notices table left Notification rules for the
    // Inbox (3.71.0); the Firewall's rules are the dashboard's expandable
    // rows now: a click anywhere in a rule opens its details.
    await page.goto(`${BASE}/firewall`);
    const row = page.locator("tr.fw-rule").first();
    await row.waitFor({ timeout: 10000 });
    const key = await row.getAttribute("data-rule");
    const rule = page.locator(`tr.fw-rule[data-rule="${key}"]`);
    const before = await rule.getAttribute("aria-expanded");
    // A plain cell: Ports holds no control of its own.
    await rule.locator("td[data-label=Ports]").click();
    const after = await rule.getAttribute("aria-expanded");
    assert.notEqual(
      after,
      before,
      "a click in the row must toggle it (Kenny, 2026-10-02)",
    );
    await rule.locator("td[data-label=Ports]").click();
    assert.equal(
      await rule.getAttribute("aria-expanded"),
      before,
      "a second click must close it again",
    );
  } finally {
    await browser.close();
  }
});

test("invariants: the kit's Status and Clients pages are switched off, Passkeys stays", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.setViewportSize({ width: 1920, height: 900 });
    await page.waitForTimeout(500);
    const links = await page.locator("#bar a").allTextContents();
    const names = links.map((t) => t.trim().toLowerCase());
    assert.ok(!names.includes("status"), `Status still in the bar: ${names}`);
    assert.ok(!names.includes("clients"), `Clients still in the bar: ${names}`);
    for (const api of ["/api/kit/status", "/api/kit/clients"]) {
      const r = await page.request.get(`${BASE}${api}`);
      assert.equal(r.status(), 404, `${api} still answers ${r.status()}`);
    }
    const passkeys = await page.request.get(`${BASE}/api/kit/passkeys`);
    assert.notEqual(
      passkeys.status(),
      404,
      "Passkeys must stay (Kenny logs in with it)",
    );
    // An old link to /status: either the server no longer has it (404) or
    // the app sends it on to the Inbox (Health's home since 3.71.0, the
    // approved "Health → Inbox" merge) — never the old page.
    const old = await page.goto(`${BASE}/status`);
    if (old && old.status() !== 404) {
      await page.waitForURL("**/inbox", { timeout: 5000 });
    }
  } finally {
    await browser.close();
  }
});

test("invariants: a table's data loads once; live updates never duplicate its rows", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.goto(`${BASE}/backups`);
    await page.locator("table tbody tr").first().waitFor({ timeout: 15000 });
    // Several live updates arrive meanwhile (fix-204: each one used to
    // start another full read whose rows piled onto the table).
    await page.waitForTimeout(12000);
    const keys = await page.$$eval("table tbody tr", (trs) =>
      trs.map((tr) => tr.textContent?.trim() ?? ""),
    );
    const dupes = keys.filter((k, i) => k !== "" && keys.indexOf(k) !== i);
    assert.deepEqual(
      dupes,
      [],
      `rows shown more than once: ${dupes.join(" | ")}`,
    );
  } finally {
    await browser.close();
  }
});

test("invariants: a version difference never blocks — Live view still drives and updates a dashboard tab reporting an older version", async () => {
  // fix-199 (Kenny, 2026-10-02: "het enige wat een versie check moet doen
  // is om ons te laten weten welke pagina's of commandos we kunnen
  // gebruiken voor die versie, dat moet niks tegenhouden" — a version check
  // informs, it never blocks). fix-185's gate refused EVERY Live view step
  // once the driven tab's reported version differed from the client's own,
  // which also refused the one step meant to cure it — a 3.70.1 tab
  // refused being updated by a 3.70.3 client, because the drive could not
  // even open the update dialog on it. This case makes the demo host's own
  // tab report an OLDER version's capabilities (`POST /data/drive/attach`,
  // the same route the real browser bundle posts to) and proves Live view
  // still drives what that version knows, refuses only the one page/form
  // it does not, by name, and still opens the update path's own dialog —
  // never a blanket refusal naming two version numbers.
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.check("#live-view");
    await page.waitForTimeout(150);

    // The tab reports an older dashboard's capabilities: it knows "host"
    // and the install-native form, but not "jobs" — a page a later
    // release added. A real older tab would report this from its OWN
    // stale `formspec.json`; this is that same report, by hand.
    const attach = await page.evaluate(() =>
      fetch("/data/drive/attach", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          page_version: "3.69.0",
          known_pages: ["", "overview", "host"],
          known_forms: ["install-native"],
        }),
      }).then((r) => r.json()),
    );
    assert.deepEqual(attach.state.tab_caps.pages.sort(), [
      "",
      "host",
      "overview",
    ]);

    // A page this version does not know: refused, naming only "jobs" — no
    // version number, and the drive is still active afterwards (not a
    // blanket stop).
    const toJobs = await page.evaluate(() =>
      fetch("/data/drive/demo-step", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ do: "goto", path: "/jobs" }),
      }).then((r) => r.json()),
    );
    assert.equal(toJobs.ok, false);
    assert.match(toJobs.refusal.why, /page "jobs"/);
    assert.doesNotMatch(
      toJobs.refusal.why,
      /\d+\.\d+\.\d+/,
      "never a version number",
    );

    // A page this same older version DOES know: taken normally — a
    // capability gap on one step never blocks the next.
    const toHost = await page.evaluate(() =>
      fetch("/data/drive/demo-step", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ do: "goto", path: "/host" }),
      }).then((r) => r.json()),
    );
    assert.equal(toHost.ok, true, JSON.stringify(toHost));
    assert.equal(toHost.state.page, "/host");

    // The update path itself (install-native of a stack, the same family
    // as the dashboard's own "Update the host…"/install-native banner
    // action) opens against this older-reporting tab — fix-185 would have
    // refused this on the version mismatch alone, which is exactly the
    // bug: Live view could not drive the very step meant to cure a stale
    // tab. It is never refused here because the tab's own report already
    // lists "install-native" as known.
    const openUpdate = await page.evaluate(() =>
      fetch("/data/drive/demo-step", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          do: "open",
          form: "install-native",
          target: "films",
        }),
      }).then((r) => r.json()),
    );
    assert.equal(openUpdate.ok, true, JSON.stringify(openUpdate));
    assert.equal(openUpdate.state.form?.action, "install-native");
    // Let go of the drive: a form left open refused the next case's click.
    await page.evaluate(() =>
      fetch("/data/drive/demo-step", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ do: "done" }),
      }),
    );
  } finally {
    await browser.close();
  }
});

// ── fix-202: the backup calendar (Kenny, 2026-10-02 verbatim: "na een paar
// minuten zie ik nog altijd niks van data laden. nog altijd 11/13") ────────

test("invariants: the backup calendar paints a stack's own cells as soon as it answers, without waiting for the rest", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    // The demo host answers films/notes at once and oldstack (its own
    // no_backup stand-in, fix-202) after a delay — before this fix the
    // grid stayed a full skeleton until EVERY stack answered, which is
    // exactly what sat at "11 of 13" for minutes on the real fleet.
    await page.route(
      "**/data/backup-calendar?stack=oldstack*",
      async (route) => {
        await new Promise((r) => setTimeout(r, 2000));
        await route.continue();
      },
    );
    await page.goto(`${BASE}/backupcalendar`);
    await page.waitForTimeout(900);
    const finishedCells = await page
      .locator(
        ".backup-cal__cell:not(.backup-cal__cell--skeleton):not(.backup-cal__cell--pad)",
      )
      .count();
    assert.ok(
      finishedCells > 0,
      "the grid is still a full skeleton after films/notes answered, while oldstack is still pending",
    );
  } finally {
    await browser.close();
  }
});

test("invariants: a stack with nothing to back up is named at once, never left pending", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    // fix-202: the demo host's last demo stack (oldstack) answers
    // `no_backup: true` — before this fix an unrepresented name looked
    // identical to "cache still warming up" and was retried for minutes.
    // fix-224: named as its own chip now, not a separate list.
    await page.goto(`${BASE}/backupcalendar`);
    const noBackupChip = page.locator(".perstack-chip--no_backup");
    await noBackupChip.first().waitFor({ timeout: 5000 });
    const text = await noBackupChip.first().textContent();
    assert.ok(
      text && /oldstack/.test(text) && /no backups by design/.test(text),
      `oldstack was not named as excluded: ${text}`,
    );
    // The status line must reach "N of N" promptly — not stuck waiting on
    // oldstack the way a false "not read yet" would stall it. N is the demo
    // fleet's size, which other invariants' fixtures grow (fix-207).
    await page.waitForFunction(
      () => {
        const p = document.querySelector(
          "main#page.shell .measured[role=status]",
        );
        const m = /Stacks read: (\d+) of (\d+)/.exec(p?.textContent ?? "");
        return !!m && m[1] === m[2];
      },
      { timeout: 5000 },
    );
  } finally {
    await browser.close();
  }
});

test("invariants: the backup calendar's grid uses at least 75% of the content area at 1920px", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.setViewportSize({ width: 1920, height: 1000 });
    await page.goto(`${BASE}/backupcalendar`);
    await page.waitForTimeout(600);
    const gridBox = await page.locator(".backup-cal__grid").boundingBox();
    const shellBox = await page.locator("main#page.shell").boundingBox();
    assert.ok(gridBox && shellBox, "grid or shell did not render");
    const ratio = gridBox.width / shellBox.width;
    assert.ok(
      ratio >= 0.75,
      `the calendar grid is ${Math.round(ratio * 100)}% of the content area at 1920px, Kenny measured it at roughly a third`,
    );
  } finally {
    await browser.close();
  }
});

// ── fix-214: the backup calendar is a month view (a date picker) ─────────

test("invariants: the backup calendar shows a month heading, and prev/next change the month", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.goto(`${BASE}/backupcalendar`);
    const heading = page.locator("#backup-cal-heading");
    const before = await heading.textContent();
    assert.ok(
      /^[A-Z][a-z]+ \d{4}$/.test(before ?? ""),
      `the month heading does not read "Month Year": ${before}`,
    );
    await page.getByRole("button", { name: "Previous month" }).click();
    const after = await heading.textContent();
    assert.notEqual(after, before, "Prev did not change the month heading");
    await page.getByRole("button", { name: "Next month" }).click();
    const back = await heading.textContent();
    assert.equal(back, before, "Next did not return to the original month");
  } finally {
    await browser.close();
  }
});

test("invariants: the backup calendar grid is never more than 6 week rows, in every month", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.goto(`${BASE}/backupcalendar`);
    await page.waitForSelector(".backup-cal__grid");
    for (let i = 0; i < 12; i++) {
      const cells = await page.locator(".backup-cal__grid > *").count();
      // 7 weekday headers + 7 cells per week row.
      const weeks = (cells - 7) / 7;
      assert.ok(
        Number.isInteger(weeks) && weeks <= 6 && weeks >= 4,
        `month ${i}: ${weeks} week rows, expected 4-6`,
      );
      await page.getByRole("button", { name: "Next month" }).click();
      await page.waitForTimeout(50);
    }
  } finally {
    await browser.close();
  }
});

test("invariants: clicking a day in the backup calendar shows its detail with the stacks", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.goto(`${BASE}/backupcalendar`);
    await page.getByRole("button", { name: "Today" }).click();
    await page.waitForFunction(
      () =>
        document.querySelectorAll(
          ".backup-cal__cell:not(.backup-cal__cell--skeleton):not(.backup-cal__cell--pad)",
        ).length > 0,
      { timeout: 5000 },
    );
    const todayCell = page.locator(".backup-cal__cell--today");
    await todayCell.first().click();
    const detail = page.locator(".backup-cal__detail");
    await detail.locator(".backup-cal__detail-row").first().waitFor({
      timeout: 5000,
    });
    const rows = await detail.locator(".backup-cal__detail-row").count();
    assert.ok(rows > 0, "the day detail panel lists no stacks");
  } finally {
    await browser.close();
  }
});

// ── fix-224: a stuck per-stack read is named and offers a Retry ──────────

test("invariants: a stack that never answers is named in the progress and turns into a timed-out state with Retry", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await context.newPage();
    // fix-224: the production give-up window is 180 s — far too long for a
    // smoke test. `backupcalendar.js` reads this override only from
    // `globalThis`, never from anything a server or URL controls, so
    // production behaviour is unaffected by this hook existing at all.
    await page.addInitScript(() => {
      // @ts-ignore test-only override, see admin/web/js/pages/backupcalendar.js
      window.__HOMELAB_TEST_UNREAD_GIVE_UP_MS__ = 1200;
    });
    // "oldstack" is the demo fleet's own no_backup stand-in (fix-202);
    // "notes" is an ordinary stack here made to never settle, standing in
    // for Kenny's live "blijft op 12 steken" symptom.
    await page.route("**/data/backup-calendar?stack=notes*", (route) =>
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          stacks: {},
          measured_at: {},
          skipped: ["notes: not read yet"],
          no_backup: [],
          reasons: {},
        }),
      }),
    );
    await logIn(context);
    await page.goto(`${BASE}/backupcalendar`);
    const readingChip = page.locator(".perstack-chip--reading", {
      hasText: "notes",
    });
    await readingChip.first().waitFor({ timeout: 5000 });
    assert.ok(
      /reading \(\d+s\)/.test((await readingChip.first().textContent()) ?? ""),
      "the still-pending stack does not name how long it has waited",
    );
    const failedChip = page.locator(".perstack-chip--failed", {
      hasText: "notes",
    });
    await failedChip.first().waitFor({ timeout: 10000 });
    const text = await failedChip.first().textContent();
    assert.ok(
      text && /did not answer within/.test(text),
      `the timed-out chip does not name why: ${text}`,
    );
    assert.ok(
      await failedChip
        .first()
        .locator("button", { hasText: "Retry" })
        .isVisible(),
      "a failed/timed-out chip has no Retry button",
    );
  } finally {
    await browser.close();
  }
});

// ── fix-215: one topology, not a near-identical second one ───────────────

test("invariants: only one topology exists, on the Fleet view, and the traffic toggle works", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    // The invariants fixture configures no Prometheus, so `/data/fleet-
    // traffic` answers 503 for real — mocked here so the toggle has real
    // numbers to draw a ring from, the same way other invariants mock a
    // route rather than needing a live Prometheus.
    await page.route("**/data/fleet-traffic*", (route) =>
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          panels: [
            {
              panel: { title: "Network" },
              series: [{ label: "alpha-demo", points: [[0, 5000]] }],
            },
          ],
          measured_at: 0,
        }),
      }),
    );
    await page.goto(`${BASE}/firewall`);
    await page.waitForSelector("table", { timeout: 10000 });
    assert.equal(
      await page.locator(".topology__svg, .mp-svg").count(),
      0,
      "the firewall page must draw no topology of its own any more",
    );
    // "Map": the Fleet view's name since 3.71.0 (the approved rename);
    // redesign-firewall: the header's "Topology" action is that link.
    const link = page.locator('#page a.kp-button[href^="/map"]', {
      hasText: "Topology",
    });
    assert.equal(await link.count(), 1, "no link to the Map's topology");
    const href = await link.getAttribute("href");
    assert.ok(href?.includes("traffic=1"), `link missing traffic=1: ${href}`);

    await page.goto(`${BASE}/fleetview`);
    // redesign-371-map: the Map's own renderer (js/pages/mapgraph.js).
    await page.waitForSelector(".mp-svg", { timeout: 10000 });
    assert.equal(
      await page.locator(".mp-svg, .topology__svg").count(),
      1,
      "the Map must draw exactly one topology",
    );
    const toggle = page.locator("#fleetview-traffic");
    assert.equal(
      await page.locator(".mp-ring--traffic").count(),
      0,
      "a traffic ring is drawn before the toggle is on",
    );
    await toggle.check();
    await page.waitForFunction(
      () => document.querySelectorAll(".mp-ring--traffic").length > 0,
      { timeout: 5000 },
    );
  } finally {
    await browser.close();
  }
});

// ── fix-203: fleet view's topology (Kenny, verbatim: "waarom is bv almanac
// met niks gelinked?") ──────────────────────────────────────────────────

test("invariants: fleet view shows a legend and an edge for a stack whose own files name another stack's address", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.goto(`${BASE}/fleetview`);
    await page.waitForTimeout(600);
    const legendItems = await page.locator(".mp-kinds .mp-kind").count();
    assert.ok(legendItems > 0, "the topology has no legend");
    // scripts/invariants-run.sh's fixture working copy: alpha-demo names
    // beta-demo's address (10.10.10.91:8080) directly in its own manifest,
    // with no firewall rule involved at all (fix-203's `named` edge kind).
    // An SVG <path>'s own visibility heuristic can read "hidden" when its
    // bounding box has zero width (two nodes placed directly above each
    // other on the circle layout draw a dead-straight vertical curve) —
    // present in the DOM is what matters here, not CSS visibility.
    await page.waitForFunction(
      () => document.querySelectorAll(".mp-edge--named").length > 0,
      { timeout: 5000 },
    );
    const named = page.locator(".mp-edge--named");
    assert.ok(
      (await named.count()) > 0,
      "no named edge for alpha-demo -> beta-demo",
    );
  } finally {
    await browser.close();
  }
});

test("invariants: every page keeps clear vertical spacing between its top-level sections and is drawn in kp-themes styling", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.setViewportSize({ width: 1920, height: 1080 });

    // The token itself (admin/web/css/app.css: `--section-gap`, default
    // `--kp-space-xl`), measured through the cascade rather than hard-coded
    // here, so a themed override of the space scale still has to meet it.
    const tokenPx = await page.evaluate(() => {
      const probe = document.createElement("div");
      probe.style.marginTop = "var(--section-gap)";
      document.body.append(probe);
      const px = parseFloat(getComputedStyle(probe).marginTop);
      probe.remove();
      return px;
    });
    assert.ok(tokenPx > 0, "--section-gap did not resolve to a length");

    // `apply`'s plan can take a few seconds to read on a cold demo host and
    // `shell`/`log` open a live connection; `passkeys` is the kit's own
    // page (chassis-rs `kit_pages_in_webapp`) and, found while writing this
    // test, 404s as `route=unmatched` even against a demo host with
    // `HOMELAB_ADMIN_PUBLIC_URL` set — a separate, pre-existing gap outside
    // fix-206's UI-rules scope (noted in the fix-206 register row, not
    // fixed here). Every other drivable path (the stack pages have their
    // own fixture-heavy tests already) is swept.
    // `console` is `shell` at its 3.71.0 address (feat-shell-1).
    const skip = new Set(["apply", "shell", "console", "log", "passkeys"]);
    for (const p of DRIVABLE_PATHS.filter((p) => !skip.has(p))) {
      await page.goto(`${BASE}/${p}`, { waitUntil: "load" });
      await page.waitForTimeout(400);

      // fix-206: Metrics' page was registered at `/metrics`, a path
      // chassis always answers itself (Prometheus scrape text) — the SPA
      // never mounted there. Any drivable path must land on the actual
      // app shell, not a route the kit intercepted first.
      await page.locator("#bar").waitFor({ state: "visible", timeout: 5000 });
      const kpStyled = await page.locator('#page [class*="kp-"]').count();
      assert.ok(
        kpStyled > 0,
        `/${p}: no kp-themes-styled element in #page — an unstyled or misrouted page`,
      );

      const gaps = await page.evaluate(() => {
        // A zero-height placeholder (a slot filled in later, once its own
        // read answers) collapses its own margins through to its
        // neighbours — CSS's normal behaviour for an empty block, and not
        // a section a person can see, so it is not one of the "top-level
        // sections" the rule is about. Only sections with visible height
        // are compared, pairwise.
        const kids = [...document.getElementById("page").children].filter(
          (k) => k.getBoundingClientRect().height > 0,
        );
        const out = [];
        for (let i = 1; i < kids.length; i++) {
          // fix-244: a page's one-sentence description belongs to its
          // title (it sits 6 px under it, invariant 40) — title and
          // description are one header block, not two sections.
          const before = kids[i - 1];
          const isTitle =
            before.tagName === "H1" ||
            (before.classList.contains("title-row") &&
              !!before.querySelector(":scope > h1"));
          if (isTitle && kids[i].tagName === "P") continue;
          const prev = kids[i - 1].getBoundingClientRect();
          const cur = kids[i].getBoundingClientRect();
          out.push(cur.top - prev.bottom);
        }
        return out;
      });
      for (const [i, gap] of gaps.entries()) {
        assert.ok(
          gap >= tokenPx - 1,
          `/${p}: gap #${i + 1} between top-level sections is ${gap}px, under the ${tokenPx}px --section-gap token`,
        );
      }
    }
  } finally {
    await browser.close();
  }
});

// Review of redesign-371-map (2026-10-03): the demo has no per-stack chip
// row, so each stack's colour lives on its own node (and its mark in the
// side panel and the list), and a hover on the node isolates its edges.
test("invariants: the topology gives every stack its own colour", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.goto(`${BASE}/fleetview`);
    await page.waitForSelector(".mp-node:not(.mp-node--ext)", {
      timeout: 10000,
    });
    const nodeHues = await page
      .locator(".mp-node:not(.mp-node--ext)")
      .evaluateAll((els) =>
        els.map((el) => el.style.getPropertyValue("--stack-hue")),
      );
    assert.ok(
      nodeHues.length >= 2,
      `expected at least two stacks, got ${nodeHues.length}`,
    );
    assert.equal(
      new Set(nodeHues).size,
      nodeHues.length,
      `every stack's node must have its own hue, got ${nodeHues}`,
    );
    assert.equal(
      await page.locator(".mp-stackkeys, .mp-stackkey").count(),
      0,
      "the demo has no per-stack chip row",
    );
  } finally {
    await browser.close();
  }
});

test("invariants: hovering a stack in the topology isolates its own edges, leaving restores them", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.goto(`${BASE}/fleetview`);
    await page.waitForSelector(".mp-node:not(.mp-node--ext)", {
      timeout: 10000,
    });
    const stacks = await page
      .locator(".mp-node:not(.mp-node--ext)")
      .evaluateAll((els) => els.map((el) => el.getAttribute("data-stack")));
    assert.ok(
      stacks.length >= 2,
      "need at least two stacks to prove isolation",
    );
    const target = stacks[0];
    const before = await page.locator(".mp-edge.is-dim").count();
    assert.equal(before, 0, "nothing is dimmed before any hover");
    await page.locator(`.mp-node[data-stack="${target}"]`).first().hover();
    await page.waitForTimeout(50);
    const edgeStates = await page.locator(".mp-edge").evaluateAll((els) =>
      els.map((el) => ({
        from: el.getAttribute("data-from"),
        to: el.getAttribute("data-to"),
        dim: el.classList.contains("is-dim"),
      })),
    );
    for (const e of edgeStates) {
      const touches = e.from === target || e.to === target;
      assert.equal(
        e.dim,
        !touches,
        `edge ${e.from}->${e.to} dim=${e.dim} but touches ${target}=${touches}`,
      );
    }
    // Leaving restores every edge.
    await page.mouse.move(0, 0);
    await page.waitForTimeout(50);
    assert.equal(
      await page.locator(".mp-edge.is-dim").count(),
      0,
      "leaving the stack must restore every edge",
    );
  } finally {
    await browser.close();
  }
});

test("invariants: a stack whose firewall the host enforces is shown as enforced even when the repository disagrees", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    // scripts/invariants-run.sh's fixture declares "gateway" with its
    // firewall off; the demo host (admin/src/shell/demo.rs) always answers
    // that the first such stack is enforced on pve anyway, with the
    // mismatch flagged — Kenny's own "5 van de 11" case.
    await page.goto(`${BASE}/fleetview`);
    await page.waitForSelector(".mp-node", { timeout: 10000 });
    const mismatchRings = await page.locator(".mp-node .mp-fwring").count();
    assert.ok(
      mismatchRings > 0,
      "no node shows a host/repository firewall mismatch on the topology",
    );
    // Same source, the firewall page's table: the row reads "in force",
    // not "declared but off", for the stack the host enforces.
    // redesign-firewall: in the approved demo's words.
    await page.goto(`${BASE}/firewall`);
    await page.waitForSelector("tr.fw-row", { timeout: 10000 });
    const bodyText = await page.locator("#fw-stacks").innerText();
    assert.ok(
      bodyText.includes("live differs from files"),
      "the firewall table never shows a host-enforced-but-repo-disagrees row",
    );
  } finally {
    await browser.close();
  }
});

test("invariants: Deploy all changes reads its plan only once its panel is opened, and shows a loading state first", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    /** @type {string[]} */
    const planRequests = [];
    page.on("request", (r) => {
      if (r.url().includes("/data/apply/plan")) planRequests.push(r.url());
    });
    // Delay the plan read so the loading state's presence does not depend
    // on how fast the demo host answers (fix-177's own trick, reused).
    await page.route("**/data/apply/plan", async (route) => {
      await new Promise((r) => setTimeout(r, 1200));
      await route.continue();
    });
    // redesign-stacks (3.71.0): the old Apply section is the Stacks
    // header's "Deploy all changes" side panel (FLOWS.md task 13).
    await page.goto(`${BASE}/stacks`);
    await page.waitForSelector("#deploy-all", { timeout: 5000 });
    await page.waitForTimeout(500);
    assert.equal(
      planRequests.length,
      0,
      "the plan was read before Deploy all changes was ever opened",
    );
    await page.click("#deploy-all");
    // Right after opening, before the delayed plan has answered: a
    // skeleton row must already be on screen.
    await page
      .locator("dialog[open] .apply-row--skeleton")
      .first()
      .waitFor({ state: "attached", timeout: 1000 });
    assert.ok(
      planRequests.length > 0,
      "opening the panel never triggered the plan read",
    );
    // Once the delayed read lands, the skeleton is gone and the real plan
    // is drawn, with the one button that opens the apply dialog.
    await page.waitForSelector("dialog[open] .apply-row--skeleton", {
      state: "detached",
      timeout: 5000,
    });
    assert.ok(
      await page.locator("dialog[open] #apply-open").isVisible(),
      "the panel has no button that opens the deploy dialog",
    );
  } finally {
    await browser.close();
  }
});

test("invariants: /apply lands on Stacks with Deploy all changes open", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    // feat-shell-1: Overview is Stacks since 3.71.0 and its Apply section
    // is "Deploy all changes" (`?deploy-all=1`), the approved redirect;
    // redesign-stacks: it opens as the Stacks page's side panel.
    await page.goto(`${BASE}/apply`);
    await page.waitForURL("**/stacks?deploy-all=1", { timeout: 5000 });
    const title = page.locator("dialog[open] .kp-dialog__title");
    await title.waitFor({ timeout: 5000 });
    assert.equal(
      (await title.innerText()).trim(),
      "Deploy all changes",
      "/apply did not land with Deploy all changes open",
    );
    // redesign-flows-5 (review item 5): one heading, Deploy all changes,
    // with one sentence; the old sub-headings are not stacked under it.
    const subs = await page
      .locator("dialog[open] h3")
      .evaluateAll((l) => l.map((x) => x.textContent ?? ""));
    assert.ok(
      !subs.some((t) =>
        /Which stacks differ|What applying would change/.test(t),
      ),
      `the old sub-headings are still stacked under the heading: ${subs.join(" | ")}`,
    );
    // Closing the panel takes `deploy-all=1` out of the address again.
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => !location.search.includes("deploy-all"), {
      timeout: 3000,
    });
  } finally {
    await browser.close();
  }
});

test("invariants: every data-loading page shows a loading indicator before its data (or error) arrives", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    // One uniform delay on every data read, so a page's own loading state
    // (table skeleton, status text, chart placeholder) has to be on screen
    // from the very first frame, not merely arrive before a fast demo host
    // happens to answer.
    await page.route("**/data/**", async (route) => {
      await new Promise((r) => setTimeout(r, 900));
      await route.continue();
    });
    // apply/shell/log open their own slow or live connections and are
    // covered by their own dedicated cases above; passkeys is the kit's
    // own page (pre-existing gap, noted in the fix-206 test). `system` is
    // a landing page of links that reads no data at all (feat-shell-1),
    // and `console` is the shell module at its 3.71.0 address.
    const skip = new Set([
      "apply",
      "shell",
      "log",
      "passkeys",
      "system",
      "console",
    ]);
    for (const p of DRIVABLE_PATHS.filter((p) => !skip.has(p))) {
      await page.goto(`${BASE}/${p}`, { waitUntil: "domcontentloaded" });
      // Read the page's own loading affordance BEFORE the 900ms delay
      // elapses, i.e. before any `/data/...` response could have landed.
      const loading = await page.evaluate(() => {
        const root = document.getElementById("page");
        if (!root) return false;
        if (root.querySelector('[data-kp-state="loading"]')) return true;
        if (root.querySelector(".apply-row--skeleton, [class*='--skeleton']"))
          return true;
        return /reading|loading|waiting for/i.test(root.innerText);
      });
      assert.ok(
        loading,
        `/${p}: no loading indicator visible before its data arrived`,
      );
    }
  } finally {
    await browser.close();
  }
});

test("invariants: every top-level section on Overview and the stack page has a heading and a non-empty description", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    const check = async (/** @type {string} */ url) => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForTimeout(300);
      return page.evaluate(() => {
        /** @type {string[]} */
        const bad = [];
        const heads = document.querySelectorAll("#page h1, #page h2, #page h3");
        for (const head of heads) {
          // The description is the shared `.section-head__desc` sibling
          // when the heading uses `sectionHeader()`, or the next element
          // sibling of the heading's own row otherwise (a plain paragraph
          // right after it, or right after its enclosing `.title-row`).
          let desc =
            head.parentElement?.querySelector(".section-head__desc") ?? null;
          if (!desc) {
            const row = head.closest(".title-row") ?? head;
            let sib = row.nextElementSibling;
            while (sib && sib.tagName === "SCRIPT")
              sib = sib.nextElementSibling;
            if (
              sib &&
              (sib.tagName === "P" || sib.tagName === "SPAN") &&
              sib.textContent &&
              sib.textContent.trim().length > 10
            )
              desc = sib;
          }
          const text = desc?.textContent?.trim() ?? "";
          if (text.length < 10)
            bad.push(`"${head.textContent?.trim()}" has no description`);
        }
        return bad;
      });
    };
    const overviewBad = await check(`${BASE}/overview`);
    assert.deepEqual(
      overviewBad,
      [],
      `Overview section(s) missing a description: ${overviewBad.join("; ")}`,
    );
    const stackBad = await check(`${BASE}/stacks/films`);
    assert.deepEqual(
      stackBad,
      [],
      `Stack page section(s) missing a description: ${stackBad.join("; ")}`,
    );
  } finally {
    await browser.close();
  }
});

test("invariants: opening Restore from an app's row never asks for the app again and shows dated snapshots with the latest preselected (the Restore flow, redesign-final-h3)", async () => {
  // fix-216 (Kenny, 2026-10-02: "als ik daar bv op kyu restore pak, dan
  // vraagt die nog altijd welke app, terwijl ik restore al bij een app
  // selecteerde? ... ik heb toch geen idee wat die snapshot is?").
  // kp-soft's fixture stack declares two apps (kp-soft, jobtracker), so its
  // Backups rows are a real case of "the row already said which app" — the
  // demo host answers GetBackups with a few nights of made-up snapshots per
  // app (admin/src/shell/demo.rs).
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.goto(`${BASE}/backups`);
    const row = page.locator('tr[data-kp-row-key="kp-soft-jobtracker"]');
    await row.waitFor({ timeout: 15000 });
    await row.getByRole("button", { name: /Restore/ }).click();
    // redesign-final-h3: Restore… opens the Restore flow page (FLOWS.md
    // §5) with the row's app already chosen — never a select asking which
    // app all over again — and its nights dated, newest first, the newest
    // snapshot preselected.
    await page.waitForURL(/\/backups\/restore\?/, { timeout: 5000 });
    const app = page.locator('[data-app="jobtracker"]');
    await app.waitFor({ timeout: 8000 });
    assert.equal(await app.getAttribute("aria-pressed"), "true");
    assert.equal(
      await page.locator('#page select[name="app"]').count(),
      0,
      "the flow asks for the app again",
    );
    const nights = page.locator("[data-night]");
    await nights.first().waitFor({ timeout: 5000 });
    const count = await nights.count();
    assert.ok(count >= 2, `expected several snapshot nights, got ${count}`);
    const firstText = await nights.first().innerText();
    // redesign-final X4: the one date format (dd/mm/yyyy HH:MM, Kenny's
    // rule, fix-216); today's night says "Today".
    assert.match(
      firstText,
      /^(Today|\d{2}\/\d{2}\/\d{4}) \d{2}:\d{2}/,
      "a dated night",
    );
    assert.equal(
      await nights.first().getAttribute("aria-pressed"),
      "true",
      "the newest snapshot is preselected",
    );
    assert.equal(await nights.nth(1).getAttribute("aria-pressed"), "false");
    // Never a bare restic id alone: the short id sits beside the date.
    assert.match(firstText, /snapshot demo/);
  } finally {
    await browser.close();
  }
});

test("invariants: no /charts series label matches a raw-id pattern and every chart has a description", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.goto(`${BASE}/charts`, { waitUntil: "load" });
    // The host's own charts (no ?stack=) carry the temperature/SMART/
    // network/memory panels the demo metrics server answers with raw ids.
    // redesign-371-metrics: each chart is a card (`.mk-card`) drawing the
    // shared time chart; its sources are the legend's items, the drives
    // the Drives table's first column.
    await page.waitForSelector(".mk-card .tc-plot svg", { timeout: 8000 });
    await page.waitForTimeout(300);
    const found = await page.evaluate(() => {
      /** @type {string[]} */
      const labels = [];
      for (const li of document.querySelectorAll(
        ".tc-legend__item > span, .tc-tip__row > span:nth-child(2)",
      ))
        labels.push(li.textContent ?? "");
      for (const td of document.querySelectorAll(
        '[data-key="drives"] tbody td:first-child',
      ))
        labels.push(td.textContent ?? "");
      /** @type {string[]} */
      const noDesc = [];
      for (const card of document.querySelectorAll(".mk-card")) {
        const cap = card.querySelector(".nx-card__head > h3")?.textContent;
        const desc = card
          .querySelector(".nx-card__head > p")
          ?.textContent?.trim();
        if (!desc || desc.length < 10) noDesc.push(cap ?? "(no title)");
      }
      return { labels, noDesc };
    });
    assert.ok(found.labels.length >= 6, "no chart source labels found");
    const patterns = [
      /^(fwbr|fwln|fwpr|veth|tap)\d+[ip]?\d*$/,
      /^[0-9a-f]{4}:[0-9a-f]{2}:[0-9a-f]{2}[_.][0-9a-f]/i,
      /^lxc\/\d+$/,
    ];
    for (const label of found.labels) {
      // chart__key reads "<label>: <value>" (charts.js's panelEl) — the
      // value itself is never allowed to carry a colon (a plain number, a
      // unit, "ok"), so the LAST ": " is the split point; a PCI-style
      // label ("PCI device 01:00.0: 46 °C") has colons of its own earlier.
      const cut = label.lastIndexOf(": ");
      const bare = (cut >= 0 ? label.slice(0, cut) : label).trim();
      for (const re of patterns) {
        assert.ok(
          !re.test(bare),
          `chart series label "${label}" matches a raw-id pattern (${re})`,
        );
      }
    }
    assert.deepEqual(
      found.noDesc,
      [],
      `chart(s) with no description: ${found.noDesc.join("; ")}`,
    );
  } finally {
    await browser.close();
  }
});

test("invariants: the Drive health panel draws a status table naming each drive's own state, not an overlapping line", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.goto(`${BASE}/charts`, { waitUntil: "load" });
    // redesign-371-metrics: the Drives card's table, one row per drive;
    // a failing drive's state reads "not ok, since …".
    await page.waitForSelector('[data-key="drives"] tbody tr', {
      timeout: 8000,
    });
    const rows = await page.evaluate(() =>
      [...document.querySelectorAll('[data-key="drives"] tbody tr')].map(
        (tr) => ({
          device: tr.children[0]?.textContent?.trim() ?? "",
          state: tr.children[1]?.textContent?.trim() ?? "",
        }),
      ),
    );
    assert.ok(
      rows.length >= 2,
      `expected at least 2 drives, got: ${JSON.stringify(rows)}`,
    );
    assert.equal(
      await page.locator('[data-key="drives"] .tc-plot').count(),
      0,
      "drive health must never be drawn as lines",
    );
    assert.ok(
      rows.some((r) => r.state.startsWith("not ok")),
      `expected at least one "not ok" drive (the demo's sdb): ${JSON.stringify(rows)}`,
    );
    assert.ok(
      rows.some((r) => r.state === "ok"),
      `expected at least one healthy drive: ${JSON.stringify(rows)}`,
    );
  } finally {
    await browser.close();
  }
});

test("invariants: a refused /data/traffic read shows the standard error box, never raw page text", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    // Stands in for the Loki gateway's real 403 (fix-221): routes.rs's
    // `refused()` always answers a clean {what, why, fix} — `why` here is
    // exactly what `Loki::metric_now`'s `upstream_reason` produces from an
    // HTML error body (`admin/tests/loki_metric_now_tests.rs` pins that
    // conversion); this pins the OTHER half, that the browser renders such
    // a refusal through dom.js's errorBox, never as raw page text.
    await page.route("**/data/traffic*", (route) =>
      route.fulfill({
        status: 503,
        contentType: "application/json",
        body: JSON.stringify({
          what: "the traffic",
          why: "Loki refused the query (HTTP 403)",
          fix: "check the Loki gateway",
        }),
      }),
    );
    await page.goto(`${BASE}/charts?tab=traffic`, { waitUntil: "load" });
    await page.waitForTimeout(300);
    const text = await page.evaluate(
      () => document.querySelector("#page")?.textContent ?? "",
    );
    assert.ok(
      !text.toLowerCase().includes("<html"),
      `the page must never show a raw <html> error body as text, got: ${text}`,
    );
    const box = await page.locator(".kp-alert.error").count();
    assert.ok(
      box > 0,
      "expected the standard error box (dom.js errorBox) to be shown",
    );
  } finally {
    await browser.close();
  }
});

test("invariants: the Host page's Disk section names the root volume's device and size, and the biggest directories", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.goto(`${BASE}/host`, { waitUntil: "load" });
    // redesign-host: the Disk card (#host-disk) names the volume, its disk
    // and what fills it in one stacked bar with its legend.
    await page.waitForSelector("#host-disk .hk-stack-bar", { timeout: 5000 });
    await page.waitForTimeout(300);
    const diskFactsText = await page.evaluate(
      () => document.querySelector("#host-disk")?.textContent ?? "",
    );
    // fix-222 (Kenny, 2026-10-02: "is dat de 1TB SSD die erin zit?"): the
    // demo host's own made-up disk_detail (admin/src/shell/demo.rs) names
    // /dev/sda and its total size — this proves the Host page actually
    // shows them, not only that the pure view-model functions do.
    assert.ok(
      diskFactsText.includes("/dev/sda"),
      `expected the root volume's device, got: ${diskFactsText}`,
    );
    assert.ok(
      /\d+ GB/.test(diskFactsText),
      `expected a GB size on the Disk section, got: ${diskFactsText}`,
    );
    const topDirsText = await page.evaluate(
      () => document.querySelector("#host-disk .hk-dirs")?.textContent ?? "",
    );
    assert.ok(
      topDirsText.includes("/var"),
      `expected the biggest directories table to list /var, got: ${topDirsText}`,
    );
  } finally {
    await browser.close();
  }
});

// fix-225 (Kenny, 2026-10-02: "ik dacht dat de install a release pagina ook
// al met grid hermaakt was?"): the page sweeps (fix-206, fix-210) never
// opened a dialog, so an action dialog could still stack its fields loosely.
// This opens every action a stack page offers and measures each visible
// field: every label starts on one edge and every control on another, to
// the right of the labels.
test("invariants: every action dialog lays its fields on one grid — labels on one edge, controls on another", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/stacks/kp-soft`);
    const buttons = page.locator("button[data-action]");
    await buttons.first().waitFor({ timeout: 15000 });
    const actions = await buttons.evaluateAll((bs) =>
      bs.map((b) => b.getAttribute("data-action")).filter(Boolean),
    );
    assert.ok(actions.length >= 3, `expected several actions, got ${actions}`);
    let measured = 0;
    for (const action of actions) {
      const btn = page.locator(`button[data-action="${action}"]`).first();
      // redesign-stackhub: the hub keeps most actions in its More ▾.
      if (!(await btn.isVisible())) await openStackMore(page);
      if (!(await btn.isVisible()) || !(await btn.isEnabled())) continue;
      await btn.click({ timeout: 5000 });
      const dialog = page.locator("dialog#action-dialog[open]");
      if (!(await dialog.isVisible().catch(() => false))) {
        await page.waitForTimeout(300);
        if (!(await dialog.isVisible().catch(() => false))) {
          for (const close of await page
            .locator("dialog[open] .kp-dialog__close")
            .all())
            await close.click({ timeout: 2000 }).catch(() => {});
          continue;
        }
      }
      const edges = await dialog.evaluate((d) => {
        const step = [...d.querySelectorAll("[data-kp-step]")].find(
          (s) => !(/** @type {HTMLElement} */ (s).hidden),
        );
        if (!step) return [];
        return [...step.querySelectorAll(":scope > .kp-field")]
          .filter((f) => /** @type {HTMLElement} */ (f).offsetParent)
          .map((f) => {
            const label = f.querySelector(".kp-field__label");
            const ctl = f.querySelector(".kp-field__input, .kp-field__check");
            if (!label || !ctl) return null;
            const l = label.getBoundingClientRect();
            const c = ctl.getBoundingClientRect();
            // A control replaced on screen by a richer one (the snapshot
            // field behind its picker) has no box of its own to align.
            if (c.width === 0 || c.height === 0) return null;
            return {
              label: Math.round(l.left),
              ctl: Math.round(c.left),
              labelRight: Math.round(l.right),
            };
          })
          .filter(Boolean);
      });
      if (edges.length >= 1) {
        const labelEdges = new Set(edges.map((e) => e.label));
        const ctlEdges = new Set(edges.map((e) => e.ctl));
        assert.equal(
          labelEdges.size,
          1,
          `${action}: labels start at ${[...labelEdges]}`,
        );
        assert.equal(
          ctlEdges.size,
          1,
          `${action}: controls start at ${[...ctlEdges]}`,
        );
        for (const e of edges)
          assert.ok(
            e.ctl > e.label,
            `${action}: a control sits under its label instead of beside it`,
          );
        measured++;
      }
      // Close through the dialog's own close button: Escape lands on a
      // focused select first and leaves the dialog open over the page.
      // Some actions (Roll back…) open their own dialog, not the action
      // dialog: close whatever is open before the next button.
      for (const close of await page
        .locator("dialog[open] .kp-dialog__close")
        .all())
        await close.click({ timeout: 2000 }).catch(() => {});
      await page
        .locator("dialog[open]")
        .first()
        .waitFor({ state: "detached", timeout: 5000 })
        .catch(() => {});
    }
    assert.ok(measured >= 2, `only ${measured} dialogs had fields to measure`);
  } finally {
    await browser.close();
  }
});

// fix-234 (Kenny, 2026-10-02: "nu staan deze per twee zo half links
// aligned, terwijl ze nog altijd per twee kunnen staan, maar dan wel
// gecentreerd in het beeld"): a grid of content blocks fills its own width —
// its last block ends where the grid ends, never a dead empty track on the
// right — on the System and the Traffic tab alike, at Kenny's own width.
test("invariants: chart blocks fill their grid's width, with no empty track left over", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    for (const tab of ["system", "traffic"]) {
      await page.goto(`${BASE}/charts?tab=${tab}`);
      // redesign-371-metrics: the cards' 3-column grid.
      const grid = page.locator(".mk-panels").first();
      await grid.waitFor({ timeout: 15000 });
      await page.waitForTimeout(1500);
      const gap = await grid.evaluate((g) => {
        const gr = g.getBoundingClientRect();
        const right = Math.max(
          ...[...g.children].map((c) => c.getBoundingClientRect().right),
        );
        return Math.round(gr.right - right);
      });
      assert.ok(gap <= 2, `${tab}: ${gap}px of the grid's width left empty`);
    }
  } finally {
    await browser.close();
  }
});

// fix-235 (Kenny, 2026-10-02: "consistentie is ook belangrijk. En niet
// gewoon alles links aligned naast elkaar proppen"): every page's title row
// has the same shape — the title on the left edge, whatever acts on the
// page grouped against the right edge.
test("invariants: every page's title row puts the title left and its controls against the right edge", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    let checked = 0;
    for (const path of DRIVABLE_PATHS) {
      await page.goto(`${BASE}/${path}`);
      await page.waitForTimeout(800);
      const rows = await page.evaluate(() =>
        [...document.querySelectorAll("main .title-row")]
          .filter((r) => /** @type {HTMLElement} */ (r).offsetParent)
          .filter(
            (r) => r.querySelector(":scope > h1") && r.children.length > 1,
          )
          .map((r) => {
            const box = r.getBoundingClientRect();
            const kids = [...r.children].filter(
              (c) => /** @type {HTMLElement} */ (c).offsetParent,
            );
            const last = kids[kids.length - 1].getBoundingClientRect();
            return {
              rowRight: Math.round(box.right),
              lastRight: Math.round(last.right),
            };
          }),
      );
      for (const r of rows) {
        assert.ok(
          r.rowRight - r.lastRight <= 2,
          `/${path}: the title row's controls stop ${r.rowRight - r.lastRight}px short of its right edge`,
        );
        checked++;
      }
    }
    assert.ok(checked >= 2, `only ${checked} title rows with controls found`);
  } finally {
    await browser.close();
  }
});

// ── fix-230/231/232: the Fleet view's blocks, its stale images, and every
// link (Kenny, 2026-10-02, Dutch: "zet eens deftige titels per chunk zodat
// ik direct weet welke tabel hoort te tonen"; "ik zie die tabel wel, maar er
// zijn geen actions aan verbonden?"; "en de links werken niet") ──────────

test("invariants: every Fleet view block has a heading and a one-sentence description, and no table repeats its heading as a caption", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1920, height: 1080 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/fleetview`);
    await page.waitForTimeout(600);
    // redesign-371-map: each block is a card (ui.js `section`); Capacity
    // and Disk growth sit side by side in one grid row, and Dependencies
    // is the topology's List view since 3.71.0.
    await page.waitForSelector("#page section.nx-card", { timeout: 10000 });
    const blocks = await page.evaluate(() =>
      [...document.querySelectorAll("#page section.nx-card")].map((s) => {
        const head = s.querySelector(":scope > .nx-card__head h2");
        const desc = s.querySelector(
          ":scope > .nx-card__head .section-head__desc",
        );
        const captions = [...s.querySelectorAll("caption")].filter(
          (c) => c.getBoundingClientRect().height > 1,
        );
        return {
          label: s.getAttribute("aria-label") ?? "",
          heading: head?.textContent?.trim() ?? "",
          desc: desc?.textContent?.trim() ?? "",
          visibleCaptions: captions.map((c) => c.textContent?.trim() ?? ""),
        };
      }),
    );
    assert.ok(blocks.length >= 4, `expected 4 blocks, got ${blocks.length}`);
    for (const b of blocks) {
      assert.ok(b.heading.length > 0, `block "${b.label}" has no h2 heading`);
      assert.ok(
        b.desc.length >= 20 && /[a-z]/.test(b.desc),
        `block "${b.heading || b.label}" has no one-sentence description`,
      );
      assert.deepEqual(
        b.visibleCaptions,
        [],
        `block "${b.heading}" repeats a visible table caption under its heading`,
      );
    }
  } finally {
    await browser.close();
  }
});

test("invariants: a stale-image row offers Update with the from/to versions, and a major jump requires the release-notes tick", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/fleetview`);
    const updates = page.locator(
      'section[aria-label="Stale images"] button[data-pin-update]',
    );
    await updates.first().waitFor({ timeout: 15000 });
    // A pin that lives in code (the demo's made-up agent on several stacks)
    // says where it is updated instead of offering a button.
    const elsewhere = page.locator('section[aria-label="Stale images"] td', {
      hasText: "with a homelab release",
    });
    assert.ok(
      (await elsewhere.count()) >= 1,
      "a pin that lives outside the stack files must say it is updated with a homelab release",
    );

    // Each source link is an absolute https link to the newer release's
    // page, opening in a new tab.
    const links = await page
      .locator('section[aria-label="Stale images"] tbody a')
      .evaluateAll((as) =>
        as.map((a) => ({
          href: a.getAttribute("href") ?? "",
          target: a.getAttribute("target") ?? "",
          rel: a.getAttribute("rel") ?? "",
        })),
      );
    assert.ok(links.length >= 1, "the stale-image rows have no source link");
    for (const l of links) {
      assert.match(
        l.href,
        /^https:\/\/github\.com\/[^/]+\/[^/]+\/releases\/tag\/[^/]+$/,
        `not a release-page link: ${l.href}`,
      );
      assert.equal(l.target, "_blank");
      assert.match(l.rel, /noopener/);
    }

    // redesign-flows-1: the row marked as a major jump opens the one Update
    // flow with that app picked; its impact step names from and to, warns,
    // and keeps "Back up and update" off until the release notes are
    // ticked as read.
    const major = page.locator(
      'section[aria-label="Stale images"] button[data-pin-update][data-major]',
    );
    assert.ok((await major.count()) >= 1, "no major-jump row in the demo");
    const from = (await major.first().getAttribute("data-from")) ?? "";
    const to = (await major.first().getAttribute("data-to")) ?? "";
    assert.match(await major.first().innerText(), /Update to/);
    await major.first().click();
    // redesign-flows-11: the flow's own address.
    await page.waitForURL(/\/update\?stack=[^&]+&app=/, { timeout: 5000 });
    const items = page.locator("#page .uf-item");
    await items.first().waitFor({ timeout: 15000 });
    assert.equal(
      await page.locator("#page .uf-item input:checked").count(),
      1,
      "only the row's own app starts ticked",
    );
    const see = page.locator("[data-drive=update-see-impact]");
    await see.click();
    const go = page.locator("[data-drive=update-go]");
    const tick = page.locator("[data-drive=update-major-read]");
    await tick.waitFor({ timeout: 5000 });
    const text = await page.locator("#page").innerText();
    assert.ok(text.includes(from), `the flow does not name the pinned ${from}`);
    assert.ok(text.includes(to), `the flow does not name the target ${to}`);
    assert.match(text, /is a major version/);
    assert.equal(await tick.isChecked(), false);
    // The file change is read first; the tick is still missing.
    await page.waitForTimeout(1500);
    assert.equal(
      await go.isEnabled(),
      false,
      "Back up and update must stay off until the release notes are ticked",
    );
    await tick.check();
    await page.waitForFunction(
      () =>
        !(
          /** @type {HTMLButtonElement | null} */ (
            document.querySelector("[data-drive=update-go]")
          )?.disabled ?? true
        ),
      null,
      { timeout: 10000 },
    );

    // A minor move asks no tick: the button is on once the change is read.
    await page.goto(`${BASE}/map`);
    const minor = page.locator(
      'section[aria-label="Stale images"] button[data-pin-update]:not([data-major])',
    );
    await minor.first().waitFor({ timeout: 15000 });
    await minor.first().click();
    await page.locator("#page .uf-item").first().waitFor({ timeout: 15000 });
    await page.locator("[data-drive=update-see-impact]").click();
    await page.waitForFunction(
      () =>
        !(
          /** @type {HTMLButtonElement | null} */ (
            document.querySelector("[data-drive=update-go]")
          )?.disabled ?? true
        ),
      null,
      { timeout: 10000 },
    );
    assert.equal(
      await page.locator("[data-drive=update-major-read]").count(),
      0,
      "a minor move asks no release-notes tick",
    );
  } finally {
    await browser.close();
  }
});

test("invariants: every link on every drivable page has an absolute http(s) href or a same-site path that resolves", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    // `console` is `shell` at its 3.71.0 address (feat-shell-1).
    const skip = new Set(["apply", "shell", "console", "log", "passkeys"]);
    const paths = [
      ...DRIVABLE_PATHS.filter((p) => !skip.has(p)).map((p) => `/${p}`),
      "/stacks/kp-soft",
    ];
    /** @type {string[]} */
    const bad = [];
    /** @type {Map<string, string>} where each same-site path was first seen */
    const sameSite = new Map();
    for (const p of paths) {
      await page.goto(`${BASE}${p}`, { waitUntil: "load" });
      await page.waitForTimeout(1200);
      // Expandable rows hold their own links: open every one first.
      await page.evaluate(() => {
        for (const b of document.querySelectorAll(
          "#page [data-kp-row-toggle]:not([aria-expanded='true'])",
        ))
          /** @type {HTMLElement} */ (b).click();
      });
      await page.waitForTimeout(200);
      const hrefs = await page.evaluate(() =>
        [...document.querySelectorAll("a[href]")].map(
          (a) => a.getAttribute("href") ?? "",
        ),
      );
      // A repository address shown as text reads as a link and is not one
      // ("en de links werken niet"): it must be a real, absolute link.
      const deadText = await page.evaluate(() => {
        const out = [];
        const walk = document.createTreeWalker(
          document.getElementById("page") ?? document.body,
          NodeFilter.SHOW_TEXT,
        );
        for (let n = walk.nextNode(); n; n = walk.nextNode()) {
          const m =
            /\b(?:github\.com|gitlab\.com|codeberg\.org)\/[\w.-]+\/[\w.-]+/.exec(
              n.textContent ?? "",
            );
          if (!m) continue;
          const a = n.parentElement?.closest("a[href]");
          if (!a || !/^https?:\/\//.test(a.getAttribute("href") ?? ""))
            out.push(m[0]);
        }
        return out;
      });
      for (const t of deadText)
        bad.push(`${p}: "${t}" is shown as text, not as a working link`);
      for (const href of hrefs) {
        if (/^https?:\/\/[^/\s]+/.test(href)) continue;
        if (href.startsWith("/") && !href.startsWith("//")) {
          if (!sameSite.has(href)) sameSite.set(href, p);
          continue;
        }
        bad.push(
          `${p}: ${JSON.stringify(href)} has no scheme and no leading /`,
        );
      }
    }
    for (const [href, where] of sameSite) {
      const path = href.split(/[?#]/)[0];
      if (route(path).page !== "notfound") continue;
      const r = await page.request.get(`${BASE}${path}`, { maxRedirects: 0 });
      if (r.status() >= 400)
        bad.push(`${where}: ${href} does not resolve (${r.status()})`);
      else if (!/^\/(data|static|login|logout|hooks|healthz)\b/.test(path))
        bad.push(`${where}: ${href} is no page of this dashboard`);
    }
    assert.deepEqual(bad, [], `broken links:\n${bad.join("\n")}`);
  } finally {
    await browser.close();
  }
});

// fix-236 (Kenny, 2026-10-02: "ik moet niet raden naar wat een functie
// doet"): every page says under its title what it is for, not only
// Overview and the stack page.
test("invariants: every page has a one-sentence description right under its title", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    const missing = [];
    for (const path of DRIVABLE_PATHS) {
      await page.goto(`${BASE}/${path}`);
      await page.waitForTimeout(700);
      const ok = await page.evaluate(() => {
        const h1 = document.querySelector("main h1");
        if (!h1) return true;
        const top = h1.getBoundingClientRect().bottom;
        // A description is a short muted paragraph within 120 px under the
        // title, wherever the page's header puts it.
        return [...document.querySelectorAll("main p")].some((p) => {
          const r = p.getBoundingClientRect();
          return (
            r.height > 0 &&
            r.top >= top - 4 &&
            r.top - top < 120 &&
            (p.textContent ?? "").trim().length > 20
          );
        });
      });
      if (!ok) missing.push(`/${path}`);
    }
    assert.deepEqual(missing, [], `pages without a description: ${missing}`);
  } finally {
    await browser.close();
  }
});

// fix-236: an action button's label never wraps onto a second line and is
// never cut off, at desktop and at phone width.
test("invariants: every action button's label fits on one line, uncut, on desktop and phone", async () => {
  const browser = await launch();
  try {
    const bad = [];
    for (const width of [1894, 390]) {
      const context = await browser.newContext({
        viewport: { width, height: 1000 },
      });
      const page = await freshPage(context);
      for (const path of ["stacks/kp-soft", "host"]) {
        await page.goto(`${BASE}/${path}`);
        await page
          .locator(".actions-row .kp-button")
          .first()
          .waitFor({ timeout: 15000 });
        const found = await page.evaluate(() =>
          [...document.querySelectorAll(".actions-row .kp-button")]
            .filter((b) => /** @type {HTMLElement} */ (b).offsetParent)
            .filter((b) => {
              const el = /** @type {HTMLElement} */ (b);
              const lh = parseFloat(getComputedStyle(el).lineHeight) || 20;
              const pad =
                parseFloat(getComputedStyle(el).paddingTop) +
                parseFloat(getComputedStyle(el).paddingBottom);
              return (
                el.scrollWidth > el.clientWidth + 1 ||
                el.clientHeight - pad > lh * 1.5
              );
            })
            .map((b) => (b.textContent ?? "").trim()),
        );
        for (const f of found) bad.push(`${width}px /${path}: "${f}"`);
      }
      await context.close();
    }
    assert.deepEqual(bad, [], `labels that wrap or are cut: ${bad.join("; ")}`);
  } finally {
    await browser.close();
  }
});

// fix-239 (Kenny, 2026-10-02: "waarom gebruik je live view niet?"; standing
// rule: Claude can always drive every dashboard command through Live view,
// even when pages are renamed or moved). Every drivable page is walked as it
// draws itself in the demo host; every visible button (or switch) whose
// click opens a dialog or sends a change — found by doing it, with every
// change request held back, never from a list — must be reachable through
// the Live view driver: a server-modelled form (`data-action` /
// `data-drive-form` naming one of formspec.json's forms, `homelab ui open`)
// or a page control its page declared (drivable.js, `homelab ui click`).
// Each page control found is then driven for real: `ui click` through the
// demo host's driver, with Live view on, must take it and have the same
// effect a click has.
// fix-239's button walk (every button on every page, 1.5 s reloads) is folded
// into the drive-reach sweep below, which clicks only the unmarked buttons and
// drives every declared control from the served catalog (coordinator, 2026-10-03).

// ui-pass-3 (Kenny, 2026-10-02: "vraag jezelf af of het next-level
// frontend werk is of het nog beter kan, en verbeter het dan"): the shared
// walk the cases below use — every drivable page plus a stack page.
const UI_PASS_3_PATHS = [...DRIVABLE_PATHS, "stacks/kp-soft"];

/**
 * Opens every action dialog on a stack page and on the host page, calls
 * `measure` with each open dialog's locator, and closes it again.
 * @param {import("playwright").Page} page
 * @param {(action: string, dialog: import("playwright").Locator) => Promise<void>} measure
 */
async function eachActionDialog(page, measure) {
  for (const path of ["stacks/kp-soft", "host"]) {
    await page.goto(`${BASE}/${path}`);
    const buttons = page.locator("button[data-action]");
    await buttons.first().waitFor({ timeout: 15000 });
    const actions = await buttons.evaluateAll((bs) => [
      ...new Set(bs.map((b) => b.getAttribute("data-action")).filter(Boolean)),
    ]);
    for (const action of actions) {
      const btn = page.locator(`button[data-action="${action}"]`).first();
      // redesign-stackhub: the hub keeps most actions in its More ▾.
      if (!(await btn.isVisible())) await openStackMore(page);
      if (!(await btn.isVisible()) || !(await btn.isEnabled())) continue;
      await btn.click({ timeout: 5000 });
      const dialog = page.locator("dialog#action-dialog[open]");
      await dialog.waitFor({ timeout: 1500 }).catch(() => {});
      if (await dialog.isVisible().catch(() => false))
        await measure(`${path} ${action}`, dialog);
      for (const close of await page
        .locator("dialog[open] .kp-dialog__close")
        .all())
        await close.click({ timeout: 2000 }).catch(() => {});
      await page
        .locator("dialog[open]")
        .first()
        .waitFor({ state: "detached", timeout: 5000 })
        .catch(() => {});
    }
  }
}

// fix-244: a page's one-sentence description is its title's subtitle — it
// sits right under the title (and under the title row's own controls when
// a phone folds them below it), never a full section gap away where it
// reads as a section of its own.
test("invariants: every page's description sits directly under its title, never a section gap away", async () => {
  const browser = await launch();
  try {
    const bad = [];
    for (const width of [1894, 390]) {
      const context = await browser.newContext({
        viewport: { width, height: 1000 },
      });
      const page = await freshPage(context);
      for (const path of UI_PASS_3_PATHS) {
        await page.goto(`${BASE}/${path}`);
        await page.waitForTimeout(600);
        const gap = await page.evaluate(() => {
          const h1 = document.querySelector("main h1");
          if (!h1) return null;
          const head = h1.closest(".title-row") ?? h1;
          const desc = head.nextElementSibling;
          if (!desc || desc.tagName !== "P") return null;
          // The title block's own visible content, without its margins.
          const parts =
            head === h1
              ? [h1]
              : [...head.children].filter(
                  (c) => /** @type {HTMLElement} */ (c).offsetParent,
                );
          const bottom = Math.max(
            ...parts.map((p) => p.getBoundingClientRect().bottom),
          );
          return Math.round(desc.getBoundingClientRect().top - bottom);
        });
        if (gap !== null && (gap > 12 || gap < 0))
          bad.push(`${width}px /${path}: ${gap}px`);
      }
      await context.close();
    }
    assert.deepEqual(
      bad,
      [],
      `descriptions detached from their title: ${bad.join("; ")}`,
    );
  } finally {
    await browser.close();
  }
});

// fix-245: a page title row's controls are one row beside the title on a
// desktop — never a column stacked under each other — and on a phone, once
// they fold under the title, they start on the title's own left edge
// instead of hanging off the right.
test("invariants: a title row's controls sit in one row beside the title, and fold under it left-aligned on a phone", async () => {
  const browser = await launch();
  try {
    const bad = [];
    for (const width of [1894, 390]) {
      const context = await browser.newContext({
        viewport: { width, height: 1000 },
      });
      const page = await freshPage(context);
      for (const path of UI_PASS_3_PATHS) {
        await page.goto(`${BASE}/${path}`);
        await page.waitForTimeout(600);
        const r = await page.evaluate(() => {
          const h1 = document.querySelector("main h1");
          const row = h1?.closest(".title-row");
          if (!h1 || !row) return null;
          const t = h1.getBoundingClientRect();
          const ctls = [
            ...row.querySelectorAll("button, a.kp-button, .state, .badge"),
          ]
            .filter((e) => /** @type {HTMLElement} */ (e).offsetParent)
            .map((e) => e.getBoundingClientRect())
            .filter((b) => b.width > 0);
          if (!ctls.length) return null;
          // Stacked: one control sits wholly under another (badges of
          // different heights centred on one line are still one row).
          const stacked = ctls.some((a) =>
            ctls.some((b) => b.top >= a.bottom - 1),
          );
          const below = ctls.filter((b) => b.top >= t.bottom - 2);
          return {
            stacked,
            belowLeft: below.length
              ? Math.round(Math.min(...below.map((b) => b.left)) - t.left)
              : null,
          };
        });
        if (!r) continue;
        if (width > 800 && r.stacked)
          bad.push(`${width}px /${path}: controls stacked in a column`);
        if (r.belowLeft !== null && Math.abs(r.belowLeft) > 2)
          bad.push(
            `${width}px /${path}: folded controls start ${r.belowLeft}px from the title's edge`,
          );
      }
      await context.close();
    }
    assert.deepEqual(bad, [], bad.join("; "));
  } finally {
    await browser.close();
  }
});

// fix-246: code-like text (an address, a key, a port list) is set in the
// theme's monospace at a readable size — never the browser's bare
// `monospace` fallback, whose 13 px base shrank it to 11 px beside 16 px
// text.
test("invariants: monospace text reads at nearly its row's own size, in the theme's mono font", async () => {
  const browser = await launch();
  try {
    const bad = [];
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    for (const path of UI_PASS_3_PATHS) {
      await page.goto(`${BASE}/${path}`);
      await page.waitForTimeout(900);
      const found = await page.evaluate(() =>
        [...document.querySelectorAll("main .mono")]
          .filter((e) => /** @type {HTMLElement} */ (e).offsetParent)
          .map((e) => {
            const own = parseFloat(getComputedStyle(e).fontSize);
            const parent = parseFloat(
              getComputedStyle(/** @type {Element} */ (e.parentElement))
                .fontSize,
            );
            return { own, ratio: own / parent, text: e.textContent ?? "" };
          })
          .filter((m) => m.ratio < 0.8 || m.own < 12)
          .slice(0, 2)
          .map((m) => `"${m.text.trim().slice(0, 30)}" ${m.own}px`),
      );
      for (const f of found) bad.push(`/${path}: ${f}`);
    }
    assert.deepEqual(bad, [], `undersized monospace: ${bad.join("; ")}`);
  } finally {
    await browser.close();
  }
});

// fix-247: a link drawn as a button looks like every other button — no
// underline on one ("Export bundle") beside a plain one ("Compare with the
// files") in the same row.
test("invariants: a link drawn as a button is never underlined", async () => {
  const browser = await launch();
  try {
    const bad = [];
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    for (const path of UI_PASS_3_PATHS) {
      await page.goto(`${BASE}/${path}`);
      await page.waitForTimeout(900);
      const found = await page.evaluate(() =>
        [...document.querySelectorAll("a.kp-button")]
          .filter((a) => /** @type {HTMLElement} */ (a).offsetParent)
          .filter((a) =>
            getComputedStyle(a).textDecorationLine.includes("underline"),
          )
          .map((a) => (a.textContent ?? "").trim() || a.outerHTML.slice(0, 60)),
      );
      for (const f of found) bad.push(`/${path}: "${f}"`);
    }
    assert.deepEqual(bad, [], `underlined button links: ${bad.join("; ")}`);
  } finally {
    await browser.close();
  }
});

// fix-248: on a phone, every action dialog field reads label first, then
// its control — never the control above the label it belongs to.
test("invariants: on a phone every action dialog field shows its label above its control", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 390, height: 844 },
    });
    const page = await freshPage(context);
    const bad = [];
    let measured = 0;
    await eachActionDialog(page, async (action, dialog) => {
      const r = await dialog.evaluate((d) => {
        const step = [...d.querySelectorAll("[data-kp-step]")].find(
          (s) => !(/** @type {HTMLElement} */ (s).hidden),
        );
        if (!step) return { n: 0, inverted: [] };
        const pairs = [...step.querySelectorAll(":scope > .kp-field")]
          .filter((f) => /** @type {HTMLElement} */ (f).offsetParent)
          .map((f) => {
            const l = f.querySelector(".kp-field__label");
            const c = f.querySelector(".kp-field__input, .kp-field__check");
            if (!l || !c) return null;
            const cb = c.getBoundingClientRect();
            if (!cb.height) return null;
            return {
              label: (l.textContent ?? "").trim(),
              inverted: cb.top < l.getBoundingClientRect().top - 2,
            };
          })
          .filter((p) => p !== null);
        return {
          n: pairs.length,
          inverted: pairs.filter((p) => p.inverted).map((p) => p.label),
        };
      });
      measured += r.n;
      for (const l of r.inverted) bad.push(`${action}: "${l}"`);
    });
    assert.ok(measured >= 5, `only ${measured} fields measured`);
    assert.deepEqual(bad, [], `controls above their label: ${bad.join("; ")}`);
  } finally {
    await browser.close();
  }
});

// fix-249: an action dialog says what it does once — its own description
// at the top — and the review step does not repeat it in other words.
test("invariants: an action dialog describes what it does once, never twice", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    const bad = [];
    let seen = 0;
    await eachActionDialog(page, async (action, dialog) => {
      const r = await dialog.evaluate((d) => ({
        intro: !!d.querySelector(".act-intro"),
        what: d.querySelectorAll(".act-what").length,
      }));
      if (r.intro) seen++;
      if (r.intro && r.what > 0)
        bad.push(`${action}: its description and a second "what" line`);
    });
    assert.ok(seen >= 5, `only ${seen} dialogs with a description`);
    assert.deepEqual(bad, [], bad.join("; "));
  } finally {
    await browser.close();
  }
});

// fix-250: a collapsible block's chevron sits beside its heading at every
// width — at phone width it wrapped onto a line of its own above the
// heading and description (Overview's "Apply the whole fleet").
// fix-250, carried to the 3.71.0 kit (redesign-integrate-6): Overview and
// Health, where the old `details.health-block` lived, are Stacks and the
// Inbox now; a folded block is ui.js's `section({collapsible})`
// (`details.nx-card--fold`), its chevron at the head's right edge. Its
// heading stays on the chevron's line at both widths.
test("invariants: a collapsible block's heading stays beside its chevron, on desktop and phone", async () => {
  const browser = await launch();
  try {
    const bad = [];
    let seen = 0;
    for (const width of [1894, 390]) {
      const context = await browser.newContext({
        viewport: { width, height: 1000 },
      });
      const page = await freshPage(context);
      for (const path of ["host", "backups", "stacks/films"]) {
        await page.goto(`${BASE}/${path}`);
        await page.locator("details.nx-card--fold > summary").first().waitFor();
        const found = await page.evaluate(() =>
          [...document.querySelectorAll("details.nx-card--fold > summary")]
            .filter((s) => /** @type {HTMLElement} */ (s).offsetParent)
            .map((s) => {
              const head = s.querySelector("h2, h3");
              if (!head) return null;
              const a = head.getBoundingClientRect();
              const b = s.querySelector(".nx-chev")?.getBoundingClientRect();
              if (b && b.width > 0) {
                // The ops look: the chevron at the head's right edge, beside
                // the heading and its sentence (centred on them), never
                // alone on a line above or below them.
                const desc = s.querySelector(".section-head__desc");
                const end = desc
                  ? desc.getBoundingClientRect().bottom
                  : a.bottom;
                return {
                  name: (head.textContent ?? "").trim(),
                  beside:
                    b.left > a.left + 20 && b.top < end && b.bottom > a.top,
                };
              }
              // The next.css look: the chevron is the summary's ::before at
              // its left; a heading beside it starts ~12 px in, one wrapped
              // under it at 0 (fix-250's rule).
              const st = getComputedStyle(s);
              const left =
                s.getBoundingClientRect().left +
                parseFloat(st.paddingInlineStart) +
                parseFloat(st.borderInlineStartWidth);
              return {
                name: (head.textContent ?? "").trim(),
                beside: a.left - left >= 10,
              };
            })
            .filter((x) => x !== null),
        );
        for (const f of found) {
          seen++;
          if (!f.beside)
            bad.push(
              `${width}px /${path}: "${f.name}" is not on its chevron's line`,
            );
        }
      }
      await context.close();
    }
    assert.ok(seen >= 4, `only ${seen} collapsible blocks found`);
    assert.deepEqual(bad, [], bad.join("; "));
  } finally {
    await browser.close();
  }
});

test("invariants: a running job's log scrolls inside its dialog and never makes the dialog taller", async () => {
  // fix-261 (Kenny, 2026-10-03: "tijdens de deploy in de dialog, de logs
  // daarin maakten de dialog langer verticaal, dat moet scrollbaar zijn").
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/stacks/films`);
    await page.waitForTimeout(600);
    // "Park": "Disable" before 3.71.0 (the approved rename).
    // redesign-stackhub: Park sits in the hub header's More ▾ (Pause).
    await page.locator('[data-drive="stack-more"]').click({ timeout: 5000 });
    await page
      .locator('[role=menuitem][data-action="disable"]')
      .click({ timeout: 5000 });
    await page.waitForTimeout(300);
    await page.locator("#action-dialog #act-run").click();
    const log = page.locator("#action-dialog .job-log").first();
    await log.waitFor({ timeout: 5000 });
    const dialog = page.locator("#action-dialog");
    const before = await dialog.boundingBox();
    assert.ok(before, "the dialog is not on screen");
    // A long deploy prints hundreds of lines; append them the way the
    // panel does (one element per line) and measure again.
    await log.evaluate((el) => {
      for (let i = 0; i < 400; i += 1) {
        const line = document.createElement("div");
        line.textContent = `HOST  [run ] step ${i} of a long deploy`;
        el.append(line);
      }
    });
    await page.waitForTimeout(200);
    const after = await dialog.boundingBox();
    assert.ok(after, "the dialog left the screen");
    assert.equal(
      Math.round(after.height),
      Math.round(before.height),
      `the dialog grew from ${before.height} to ${after.height} px as log lines arrived`,
    );
    const scrolls = await log.evaluate(
      (el) => el.scrollHeight > el.clientHeight + 1,
    );
    assert.ok(scrolls, "the log does not scroll inside its own region");
  } finally {
    await browser.close();
  }
});

// fix-251 (design review, 2026-10-03): a mark drawn on a chart or the
// timeline is coloured with a foreground-strength token, never with a
// kp-themes background token (`--success`, `--warning`, `--info` are the
// pale/dark plates text sits on): in dark, "succeeded" was hsl(155 40% 16%)
// on a near-black page, all but invisible. Every timeline mark and legend
// swatch must stand out from the page by WCAG's 3:1 for graphics.
test("invariants: every timeline mark and legend swatch contrasts at least 3:1 with the page, in light and dark", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    const bad = [];
    for (const theme of ["dark", "light"]) {
      await page.evaluate((t) => localStorage.setItem("theme", t), theme);
      await page.goto(`${BASE}/timeline`);
      await page.waitForSelector(".timeline-legend .swatch", {
        timeout: 8000,
      });
      await page
        .waitForSelector(".timeline svg", { timeout: 8000 })
        .catch(() => {});
      await page.waitForTimeout(300);
      const found = await page.evaluate(() => {
        /** @param {string} c */
        const rgb = (c) => {
          const probe = document.createElement("canvas").getContext("2d");
          if (!probe) return [0, 0, 0];
          probe.fillStyle = "#000";
          probe.fillStyle = c;
          probe.fillRect(0, 0, 1, 1);
          const d = probe.getImageData(0, 0, 1, 1).data;
          return [d[0], d[1], d[2]];
        };
        /** @param {number[]} c */
        const lum = (c) => {
          const [r, g, b] = c.map((v) => {
            const s = v / 255;
            return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
          });
          return 0.2126 * r + 0.7152 * g + 0.0722 * b;
        };
        let bgEl = /** @type {Element | null} */ (
          document.querySelector(".timeline")
        );
        let bg = "rgb(255, 255, 255)";
        while (bgEl) {
          const c = getComputedStyle(bgEl).backgroundColor;
          if (c && c !== "transparent" && !/^rgba\(.*,\s*0\)$/.test(c)) {
            bg = c;
            break;
          }
          bgEl = bgEl.parentElement;
        }
        const lb = lum(rgb(bg));
        const ratio = (/** @type {string} */ c) => {
          const l = lum(rgb(c));
          return (Math.max(l, lb) + 0.05) / (Math.min(l, lb) + 0.05);
        };
        const out = [];
        for (const sw of document.querySelectorAll(
          ".timeline-legend .swatch",
        )) {
          const c = getComputedStyle(sw).backgroundColor;
          const r = ratio(c);
          if (r < 3)
            out.push(
              `swatch ${sw.className} ${c} is ${r.toFixed(2)}:1 on ${bg}`,
            );
        }
        for (const m of document.querySelectorAll(
          ".timeline .mark rect, .timeline .mark path",
        )) {
          const c = getComputedStyle(m).fill;
          const r = ratio(c);
          if (r < 3)
            out.push(
              `mark ${m.parentElement?.getAttribute("data-kind")} ${c} is ${r.toFixed(2)}:1 on ${bg}`,
            );
        }
        return out;
      });
      for (const f of found) bad.push(`${theme}: ${f}`);
    }
    assert.deepEqual(bad, [], `marks too faint:\n${bad.join("\n")}`);
  } finally {
    await browser.close();
  }
});

// fix-252 (design review, 2026-10-03): the charts' axis labels were drawn
// in a fixed 560-wide viewBox that a phone scales down to ~5 px text; every
// chart and timeline label must render at 11 px or more at 390 px.
test("invariants: every chart and timeline axis label renders at 11 px or more on a 390 px phone", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 390, height: 900 },
    });
    const page = await freshPage(context);
    const bad = [];
    let seen = 0;
    for (const path of ["charts", "charts?tab=traffic", "timeline"]) {
      await page.goto(`${BASE}/${path}`);
      await page
        .waitForSelector(".tc-plot svg text, .timeline svg text", {
          timeout: 8000,
        })
        .catch(() => {});
      await page.waitForTimeout(400);
      const found = await page.evaluate(() => {
        const out = [];
        let n = 0;
        for (const t of document.querySelectorAll(
          ".tc-plot svg text, .timeline svg text",
        )) {
          const svg = /** @type {SVGSVGElement | null} */ (t.closest("svg"));
          if (!svg) continue;
          const vb = svg.viewBox.baseVal;
          const shown = svg.getBoundingClientRect().width;
          if (!vb || !vb.width || !shown) continue;
          n++;
          const px =
            parseFloat(getComputedStyle(t).fontSize) * (shown / vb.width);
          if (px < 10.95) out.push(`"${t.textContent}" ${px.toFixed(1)} px`);
        }
        return { out, n };
      });
      seen += found.n;
      if (found.out.length)
        bad.push(`/${path}: ${found.out.slice(0, 4).join(", ")}`);
    }
    assert.ok(seen > 0, "no chart or timeline label found to measure");
    assert.deepEqual(bad, [], `labels too small at 390 px:\n${bad.join("\n")}`);
  } finally {
    await browser.close();
  }
});

// fix-253 (design review, 2026-10-03): with one reading per series a line
// path is a lone "M" and draws nothing, so every chart looked empty; and a
// small count range labelled its y axis "1, 1, 0".
test("invariants: a chart with one reading per series shows a visible point and says so, and its y ticks are distinct", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    await page.route("**/data/charts*", async (route) => {
      const r = await route.fetch();
      const body = await r.json();
      for (const p of body.panels ?? [])
        for (const s of p.series ?? []) s.points = s.points.slice(-1);
      await route.fulfill({ response: r, json: body });
    });
    // redesign-371-metrics: each card draws the shared time chart; its y
    // ticks are the end-anchored `.tc-tick`s, a lone reading a `.tc-point`.
    await page.goto(`${BASE}/charts`);
    await page.waitForSelector(".mk-card .tc-plot svg", { timeout: 8000 });
    await page.waitForTimeout(300);
    const charts = await page.evaluate(() =>
      [...document.querySelectorAll(".mk-card")]
        .filter((f) => f.querySelector(".tc-plot svg"))
        .map((f) => ({
          title: f.querySelector(".nx-card__head > h3")?.textContent ?? "",
          series: Math.max(1, f.querySelectorAll(".tc-legend__item").length),
          points: [...f.querySelectorAll(".tc-point")].filter((c) => {
            const r = c.getBoundingClientRect();
            return r.width >= 4 && r.height >= 4;
          }).length,
          note: f.textContent?.includes("one reading so far") ?? false,
        })),
    );
    assert.ok(charts.length > 0, "no line chart drawn");
    const bad = charts.filter((c) => c.points < c.series || !c.note);
    assert.deepEqual(bad, [], "charts that look empty with one reading");
    // Distinct y ticks, over the demo's own (multi-point) answer and the
    // one-reading one above.
    const dupAt = async () =>
      page.evaluate(() =>
        [...document.querySelectorAll(".mk-card")]
          .map((f) => ({
            title: f.querySelector(".nx-card__head > h3")?.textContent ?? "",
            ticks: [...f.querySelectorAll('.tc-tick[text-anchor="end"]')].map(
              (t) => t.textContent ?? "",
            ),
          }))
          .filter((c) => new Set(c.ticks).size !== c.ticks.length),
      );
    const dupOne = await dupAt();
    await page.unroute("**/data/charts*");
    await page.goto(`${BASE}/charts`);
    await page.waitForSelector(".mk-card .tc-plot svg", { timeout: 8000 });
    await page.waitForTimeout(300);
    const yTicks = await page.locator('.tc-tick[text-anchor="end"]').count();
    assert.ok(yTicks > 0, "no y tick labels found");
    const dupMany = await dupAt();
    assert.deepEqual(
      [...dupOne, ...dupMany],
      [],
      "charts with repeated y tick labels",
    );
  } finally {
    await browser.close();
  }
});

// fix-254 (design review, 2026-10-03): the gateway's access log sometimes
// carries no client address; the busiest-clients table showed a bare "—".
test("invariants: the busiest-clients table names an empty client address instead of a bare dash", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    await page.route("**/data/traffic*", async (route) => {
      const r = await route.fetch();
      const body = await r.json();
      body.clients = {
        ...(body.clients ?? {}),
        rows: [["", 42], ...(body.clients?.rows ?? [])],
      };
      delete body.clients.error;
      await route.fulfill({ response: r, json: body });
    });
    await page.goto(`${BASE}/charts?tab=traffic`);
    // redesign-371-metrics: the Busiest clients card's table.
    await page.waitForSelector('[data-key="clients"] tbody tr', {
      timeout: 8000,
    });
    await page.waitForTimeout(300);
    const cells = await page.evaluate(() => {
      const t = document.querySelector('[data-key="clients"] table');
      return [...(t?.querySelectorAll("tbody tr td:first-child") ?? [])].map(
        (td) => ({
          text: (td.textContent ?? "").trim(),
          hint:
            td.querySelector("[title]")?.getAttribute("title") ??
            td.getAttribute("title") ??
            "",
        }),
      );
    });
    assert.ok(cells.length > 0, "no busiest-clients rows");
    assert.ok(
      !cells.some((c) => c.text === "—" || c.text === ""),
      `a bare dash or empty client cell: ${JSON.stringify(cells)}`,
    );
    const named = cells.find((c) => c.text.includes("not logged"));
    assert.ok(named, `no "unknown (not logged)" row: ${JSON.stringify(cells)}`);
    assert.ok(named.hint.length > 20, "the empty-address label has no hint");
  } finally {
    await browser.close();
  }
});

// fix-255 (design review, 2026-10-03): red means destructive, nothing
// else — the Host page showed Update and Apply red because their token
// scope is full access. Whole-screen: every red action button on the host
// and a stack page is an action the catalog marks destructive.
test("invariants: every red action button belongs to an action the catalog marks destructive", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    const catalog = await page.evaluate(async () =>
      (await fetch("/data/actions/catalog")).json(),
    );
    const destructive = new Set(
      catalog.actions
        .filter((/** @type {any} */ a) => a.destructive === true)
        .map((/** @type {any} */ a) => a.action),
    );
    const bad = [];
    for (const path of ["host", "stacks/kp-soft"]) {
      await page.goto(`${BASE}/${path}`);
      await page
        .locator(".actions-row .kp-button")
        .first()
        .waitFor({ timeout: 15000 });
      const red = await page.evaluate(() =>
        [
          ...document.querySelectorAll(
            // redesign-host: the Host page's action tiles mark red the same way.
            ".actions-area .kp-button--destructive, .actions-area .hk-act--destructive",
          ),
        ]
          .filter((b) => /** @type {HTMLElement} */ (b).offsetParent)
          .map((b) => ({
            action: b.getAttribute("data-action"),
            label: (b.querySelector("b") ?? b).textContent?.trim() ?? "",
          })),
      );
      for (const r of red)
        if (!r.action || !destructive.has(r.action))
          bad.push(`/${path}: "${r.label}" (${r.action}) is red`);
    }
    assert.ok(destructive.size > 0, "the catalog marks nothing destructive");
    assert.deepEqual(bad, [], `red but not destructive:\n${bad.join("\n")}`);
  } finally {
    await browser.close();
  }
});

// fix-256 (design review, 2026-10-03): /status and /passkeys answered the
// kit's bare "no such route" — chassis 3.4.0's root fallback refuses every
// path the kit reserves, even the pages `kit_pages_in_webapp` hands to the
// web app. Every address the router knows, retired ones included, renders
// a page or redirects to one.
test("invariants: every address the router knows renders a page or redirects to one, never no such route", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    const bad = [];
    for (const key of Object.keys(PATH_TO_PAGE)) {
      const path = `/${key}`;
      const res = await page.goto(`${BASE}${path}`);
      await page.waitForTimeout(500);
      const status = res?.status() ?? 0;
      const text = await page.evaluate(() => document.body?.innerText ?? "");
      const landed = new URL(page.url()).pathname;
      const h1 = await page.locator("main h1").count();
      if (status >= 400 || /no such route/i.test(text) || h1 === 0)
        bad.push(`${path} → ${landed} (HTTP ${status}, h1 ${h1})`);
      else if (route(landed).page === "notfound")
        bad.push(`${path} → ${landed}, which is no page`);
    }
    assert.deepEqual(
      bad,
      [],
      `addresses that render no page:\n${bad.join("\n")}`,
    );
  } finally {
    await browser.close();
  }
});

// fix-257 (design review, 2026-10-03): Settings' token read failed with
// "invalid type: map, expected a sequence" — the demo host answered
// TokenList with `{}` where the real host answers a list.
test("invariants: Settings lists the per-machine tokens without a read error", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/settings`);
    await page
      .locator("[data-token]")
      .first()
      .waitFor({ timeout: 8000 })
      .catch(() => {});
    const text = await page.evaluate(() => document.body?.innerText ?? "");
    assert.ok(
      !/invalid type|did not read/i.test(text),
      `the tokens read failed: ${(text.match(/.*(invalid type|did not read).*/i) ?? [""])[0]}`,
    );
    assert.ok(
      (await page.locator("[data-token]").count()) > 0,
      "no token row on Settings",
    );
  } finally {
    await browser.close();
  }
});

// fix-258 (design review, 2026-10-03): Backups dropped every stack the host
// holds no repository for without a word. Every stack of the fleet has a
// row, and one without a repository says so and why.
test("invariants: Backups has a row for every stack, naming why one has no repository", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    const fleet = await page.evaluate(async () =>
      (await fetch("/data/fleet")).json(),
    );
    const names = (fleet.stacks ?? fleet.fleet?.stacks ?? []).map(
      (/** @type {any} */ s) => s.name,
    );
    assert.ok(
      names.length > 1,
      `no fleet read: ${JSON.stringify(fleet).slice(0, 200)}`,
    );
    await page.goto(`${BASE}/backups`);
    // Every stack's own read settled (redesign-backups: the repositories card's foot
    // says "N of N stacks answered" and marks the moment it is so).
    await page.waitForFunction(
      () => document.querySelector("[data-bk-read='all']") != null,
      null,
      { timeout: 15000 },
    );
    await page.waitForTimeout(500);
    const rows = await page.evaluate(() =>
      [...document.querySelectorAll("#page table tbody tr")].map((tr) => ({
        stack: tr.children[0]?.textContent?.trim() ?? "",
        repo: tr.children[1]?.textContent?.trim() ?? "",
      })),
    );
    const missing = names.filter(
      (/** @type {string} */ n) => !rows.some((r) => r.stack === n),
    );
    assert.deepEqual(missing, [], `stacks with no row on Backups`);
    const none = rows.filter((r) => /no repository/i.test(r.repo));
    assert.ok(
      none.length > 0 &&
        none.every((r) => r.repo.length > "no repository".length + 10),
      `a stack without a repository is not named with a reason: ${JSON.stringify(rows)}`,
    );
  } finally {
    await browser.close();
  }
});

// fix-259 (design review, 2026-10-03): the backup calendar painted "every
// stack backed up" in the decorative chart palette (pink in dark). Whole
// screen, both themes: every status cell, legend swatch and read chip uses
// the status token its state means.
test("invariants: the backup calendar's status colours are the status tokens, in light and dark", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    const bad = [];
    let seen = 0;
    for (const theme of ["dark", "light"]) {
      await page.evaluate((t) => localStorage.setItem("theme", t), theme);
      await page.goto(`${BASE}/backupcalendar`);
      await page
        .locator(".backup-cal__cell--ok")
        .first()
        .waitFor({ timeout: 15000 });
      await page.waitForTimeout(300);
      const found = await page.evaluate(() => {
        const probe = document.createElement("div");
        document.body.append(probe);
        const tokenColour = (/** @type {string} */ t) => {
          probe.style.background = `var(${t})`;
          return getComputedStyle(probe).backgroundColor;
        };
        const want = {
          "backup-cal__cell--ok": tokenColour("--success-foreground"),
          "backup-cal__cell--warn": tokenColour("--warning-foreground"),
          "backup-cal__cell--bad": tokenColour("--destructive"),
        };
        const out = [];
        let n = 0;
        for (const [cls, colour] of Object.entries(want))
          for (const el of document.querySelectorAll(`.${cls}`)) {
            n++;
            const got = getComputedStyle(el).backgroundColor;
            if (got !== colour) out.push(`.${cls} is ${got}, not ${colour}`);
          }
        const okBorder = tokenColour("--success-foreground");
        for (const el of document.querySelectorAll(".perstack-chip--read")) {
          n++;
          const got = getComputedStyle(el).borderTopColor;
          if (got !== okBorder)
            out.push(`.perstack-chip--read border is ${got}, not ${okBorder}`);
        }
        probe.remove();
        return { out: [...new Set(out)], n };
      });
      seen += found.n;
      for (const f of found.out) bad.push(`${theme}: ${f}`);
    }
    assert.ok(seen > 0, "no status cell found on the backup calendar");
    assert.deepEqual(bad, [], `status painted off-token:\n${bad.join("\n")}`);
  } finally {
    await browser.close();
  }
});

// fix-260 (design review, 2026-10-03): Health › Doctor showed "overall
// fail" while the doctor was still running (an answer with no verdict read
// as fail), and two loading indicators at once (the busy card over the
// table and the status line's own spinner under it) beside "not read yet".
// While a run reads: no verdict, one visible spinner, no "not read yet";
// the verdict appears once every check has answered.
test("invariants: the Doctor shows no verdict and one loading indicator until every check has answered", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    // Both ways a run starts: over the dashboard's last answer (one that
    // carries no verdict, the shape the demo host gives), and from nothing.
    for (const first of [
      { report: {}, refreshing: { run: 7 } },
      { running: true, run: 7 },
    ]) {
      const page = await freshPage(context);
      /** @type {() => void} */
      let release = () => {};
      const held = new Promise((r) => (release = () => r(undefined)));
      await page.route("**/data/doctor*", async (route) => {
        const url = new URL(route.request().url());
        if (!url.searchParams.has("run")) {
          await route.fulfill({ json: first });
          return;
        }
        await held;
        await route.fulfill({
          json: {
            report: {
              overall: "ok",
              checks: [{ name: "disk", health: "ok", detail: "fine" }],
            },
            read_run: 7,
            read_at: Math.floor(Date.now() / 1000),
          },
        });
      });
      // redesign-final-h5: the Doctor's report lives in Host's checks card
      // (`/host?section=doctor`); `/doctor` itself goes to the Inbox.
      await page.goto(`${BASE}/host?section=doctor`);
      await page.waitForTimeout(1500);
      const during = await page.evaluate(() => {
        const shown = (/** @type {Element} */ el) => {
          for (
            let e = /** @type {Element | null} */ (el);
            e;
            e = e.parentElement
          ) {
            const cs = getComputedStyle(e);
            if (
              cs.display === "none" ||
              cs.visibility === "hidden" ||
              Number(cs.opacity) === 0
            )
              return false;
          }
          const r = el.getBoundingClientRect();
          return r.width > 0 && r.height > 0;
        };
        const main =
          document.querySelector("#host-checks-card") ?? document.body;
        return {
          verdict: [...main.querySelectorAll(".state")]
            .map((s) => s.textContent ?? "")
            .join(" "),
          spinners: [...main.querySelectorAll(".kp-spinner")].filter(shown)
            .length,
          notRead: [...main.querySelectorAll("p, span")].some(
            (e) =>
              shown(e) && /^not read yet/i.test((e.textContent ?? "").trim()),
          ),
        };
      });
      release();
      assert.ok(
        !/overall/.test(during.verdict),
        `a verdict while the doctor runs (${JSON.stringify(first)}): "${during.verdict}"`,
      );
      assert.equal(
        during.spinners,
        1,
        `loading indicators shown at once (${JSON.stringify(first)})`,
      );
      assert.equal(during.notRead, false, `"not read yet" beside the loading`);
      await page.waitForFunction(
        () => !!document.querySelector("#host-checks li[data-health]"),
        null,
        { timeout: 10000 },
      );
      const after = await page.evaluate(() => ({
        rows: document.querySelector("#host-checks")?.textContent ?? "",
        spinners: document.querySelectorAll("#host-checks-card .kp-spinner")
          .length,
      }));
      assert.match(after.rows, /disk · ok/);
      assert.equal(after.spinners, 0, "a spinner after the read ended");
      await page.close();
    }
  } finally {
    await browser.close();
  }
});

// Design review, 2026-10-03 (checked, not a defect here): a hover must never redraw
// a topology node, or a click right after the pointer moved lands on a
// node that is no longer in the page. Whole-screen: hover one node, move
// to another and click it; the node clicked is the one hovered, the same
// element as before any hover.
test("invariants: hovering the Fleet view topology never redraws a node, so a click right after a move lands", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/fleetview`);
    await page.locator(".mp-node").nth(1).waitFor({ timeout: 10000 });
    await page.waitForTimeout(500);
    const stacks = await page.evaluate(() => {
      /** @type {any} */ (window).__clicks = [];
      const nodes = [...document.querySelectorAll(".mp-node")];
      /** @type {any} */ (window).__nodes = nodes;
      document.addEventListener(
        "click",
        (e) => {
          const g = /** @type {Element} */ (e.target).closest?.(".mp-node");
          /** @type {any} */ (window).__clicks.push(
            g ? g.getAttribute("data-stack") : null,
          );
        },
        true,
      );
      return nodes.map((n) => n.getAttribute("data-stack"));
    });
    const dots = page.locator(".mp-node .mp-ring");
    const a = await dots.nth(0).boundingBox();
    const b = await dots.nth(1).boundingBox();
    assert.ok(a && b, "no node to point at");
    await page.mouse.move(a.x + a.width / 2, a.y + a.height / 2);
    await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2, {
      steps: 2,
    });
    await page.mouse.click(b.x + b.width / 2, b.y + b.height / 2);
    const out = await page.evaluate(() => ({
      clicks: /** @type {any} */ (window).__clicks,
      same: [...document.querySelectorAll(".mp-node")].every(
        (n, i) => n === /** @type {any} */ (window).__nodes[i],
      ),
    }));
    assert.equal(out.same, true, "a hover redrew the topology's nodes");
    assert.deepEqual(out.clicks, [stacks[1]], "the click did not land");
  } finally {
    await browser.close();
  }
});

// ── feat-shell-1..4 (redesign 3.71.0, Kenny approved 2026-10-03): every old
// address still lands, Ctrl K finds an action by intent, the theme picker
// is on every page, and counters are exact ─────────────────────────────

test("invariants: every pre-3.71.0 address lands on its new home", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    const { redirectFor: target } = await import("../js/router.js");
    const stacks = await page.evaluate(async () => {
      const r = await fetch("/data/fleet", {
        headers: { accept: "application/json" },
      });
      const b = await r.json();
      return (b.fleet?.stacks ?? []).map((/** @type {any} */ s) => s.name);
    });
    const old = [
      ...Object.entries(PATH_TO_PAGE)
        .filter(([, p]) => p === "retired")
        .map(([k]) => `/${k}`),
      "/stacks/kp-soft/checks",
      "/stacks/kp-soft/firewall",
      "/overview?section=apply",
      "/health?block=doctor",
      "/secrets?stack=gateway",
    ];
    const bad = [];
    for (const from of old) {
      const [path, q = ""] = from.split("?");
      const want = target(route(path), q ? `?${q}` : "", { stacks });
      await page.goto(`${BASE}${from}`);
      // `/` (from /start) is the landing, which goes on to the Inbox or
      // Apps by itself (Kenny, 2026-10-03).
      const landed = (/** @type {URL} */ u) =>
        want === "/"
          ? ["/inbox", "/apps"].includes(u.pathname)
          : u.pathname + u.search === want;
      await page.waitForURL(landed, { timeout: 5000 }).catch(() => {});
      await page.waitForTimeout(200);
      const u = new URL(page.url());
      const h1 = await page.locator("main h1").count();
      if (!landed(u) || h1 === 0)
        bad.push(
          `${from} → ${u.pathname}${u.search} (wanted ${want}, h1 ${h1})`,
        );
    }
    assert.deepEqual(
      bad,
      [],
      `old addresses that did not land:\n${bad.join("\n")}`,
    );
  } finally {
    await browser.close();
  }
});

test("invariants: Ctrl K finds an action by intent, in any word order, and Enter opens its dialog", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    // No Inbox rows (they come first in the palette), so the first row is
    // the best action.
    await page.route("**/data/notifications", (r) =>
      r.request().method() === "GET"
        ? r.fulfill({
            json: {
              notices: [],
              unread: 0,
              unread_by_stack: {},
              settings: { push: true, muted_stacks: [], digest_at: null },
              snoozed: false,
            },
          })
        : r.continue(),
    );
    await page.goto(`${BASE}/stacks`);
    await page.waitForTimeout(1200);
    /** @param {string} q */
    const first = async (q) => {
      await page.keyboard.press("Escape");
      await page.keyboard.press("Control+k");
      const input = page.locator("#commands input");
      await input.waitFor({ timeout: 3000 });
      await input.fill(q);
      await page.waitForTimeout(150);
      return page.evaluate(() => {
        const opt = [
          ...document.querySelectorAll("#commands [data-kp-option]"),
        ].find((o) => !(/** @type {HTMLElement} */ (o).hidden));
        const group = opt
          ?.closest("[data-kp-group]")
          ?.querySelector(".kp-palette__group-label")?.textContent;
        const label = opt?.firstChild?.textContent ?? "";
        return { group: group ?? "", label: label.trim() };
      });
    };
    for (const q of ["update kp-soft", "kp-soft update", "upd kp"]) {
      const r = await first(q);
      assert.deepEqual(r, { group: "Do", label: "Update · kp-soft" }, q);
    }
    for (const q of ["gateway logs", "logs gateway"]) {
      const r = await first(q);
      assert.ok(
        /gateway/.test(r.label) && /Logs/.test(r.label),
        `${q} → ${r.group}: ${r.label}`,
      );
    }
    // Enter starts it: redesign-flows-1, a person's Update is the one
    // Update flow (nothing runs before its step 2's button).
    await first("update kp-soft");
    await page.keyboard.press("Enter");
    // redesign-flows-11: the flow's own address.
    await page.waitForURL("**/update?stack=kp-soft", { timeout: 5000 });
    const title = await page.locator("#page h1").innerText();
    assert.match(title, /^Update kp-soft/, `the flow that opened: ${title}`);
    assert.equal(
      await page.locator("#page .nx-steps li").count(),
      6,
      "the Update flow shows its six steps",
    );
  } finally {
    await browser.close();
  }
});

test("invariants: the kp-themes theme picker is in the bar on every page, desktop and phone", async () => {
  const browser = await launch();
  try {
    const bad = [];
    for (const width of [1894, 390]) {
      const context = await browser.newContext({
        viewport: { width, height: 900 },
      });
      const page = await freshPage(context);
      for (const p of [
        ...DRIVABLE_PATHS,
        "stacks/kp-soft",
        "stacks/kp-soft/logs",
      ]) {
        await page.goto(`${BASE}/${p}`);
        await page.waitForTimeout(300);
        const ok = await page.evaluate(() => {
          const b = document.querySelector(
            "#bar .kp-theme-menu > button[popovertarget]",
          );
          if (!b) return false;
          const r = b.getBoundingClientRect();
          return r.width > 0 && r.height > 0 && r.top < 120;
        });
        if (!ok) bad.push(`${width}px /${p}`);
      }
      await context.close();
    }
    assert.deepEqual(bad, [], `no theme picker in the bar: ${bad.join(", ")}`);
  } finally {
    await browser.close();
  }
});

test("invariants: the Inbox counter shows the exact number, never 9+, and equals the Inbox's rows", async () => {
  const browser = await launch();
  try {
    for (const width of [1600, 390]) {
      const context = await browser.newContext({
        viewport: { width, height: 900 },
      });
      const page = await context.newPage();
      // Twelve unread notices that need a person: more than the old bell's
      // "9+" cap ever showed.
      const now = Math.floor(Date.now() / 1000);
      await page.route("**/data/notifications", (r) =>
        r.request().method() === "GET"
          ? r.fulfill({
              json: {
                notices: Array.from({ length: 12 }, (_, i) => ({
                  id: i + 1,
                  at: now - i * 60,
                  kind: "action_failed",
                  level: "warning",
                  stack: "kp-soft",
                  title: `Made-up failure ${i + 1}`,
                  body: "a test notice",
                  read: false,
                  push: { state: "sent" },
                })),
                unread: 12,
                unread_by_stack: { "kp-soft": 12 },
                settings: { push: true, muted_stacks: [], digest_at: null },
                snoozed: false,
              },
            })
          : r.continue(),
      );
      await page.route("**/data/asks", (r) =>
        r.fulfill({ json: { asks: [] } }),
      );
      await logIn(context);
      // `/` opens the Inbox when it holds something (Kenny, 2026-10-03).
      await page.goto(`${BASE}/`);
      await page.waitForURL("**/inbox", { timeout: 5000 });
      await page.waitForTimeout(500);
      const badge =
        width > 960
          ? page.locator("#nav a[data-area='inbox'] .nx-count")
          : page.locator(".nx-tabbar [data-area='inbox'] .nx-count");
      // redesign-flows-2: the updates, setup, checks and Today sources
      // add their rows to the twelve notices; the counter is the rows.
      const rows = await page
        .locator("#page .inbox-card .nx-inbox__row")
        .count();
      assert.ok(rows >= 12, `${width}px: the Inbox shows ${rows} rows`);
      assert.equal(
        (await badge.innerText()).trim(),
        String(rows),
        `${width}px: the counter is not the number of rows`,
      );
      assert.ok(
        !/\+/.test(await badge.innerText()),
        "the counter is never capped",
      );
      assert.match(await page.title(), new RegExp(`^● ${rows} in the Inbox`));
      await context.close();
    }
  } finally {
    await browser.close();
  }
});

// ── redesign-backups: the Backups page's nightly coverage heatmap (redesign 3.71,
// approved by Kenny 2026-10-03: stacks × 30 nights replaces the month
// calendar, with a whole-fleet row, a hover card per night, a click that
// pins the night into the address, and Esc / Unpin / Today to go back) ──

/** Fleet stack names, from the dashboard's own fleet read. */
async function fleetNames(page) {
  const fleet = await page.evaluate(async () =>
    (await fetch("/data/fleet")).json(),
  );
  return (fleet.stacks ?? fleet.fleet?.stacks ?? []).map(
    (/** @type {any} */ s) => s.name,
  );
}

test("invariants: the Backups heatmap has a row for every stack and a whole-fleet row, one cell per night", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    const names = await fleetNames(page);
    assert.ok(names.length > 1, "no fleet read");
    await page.goto(`${BASE}/backups`);
    await page
      .locator(".bk-heat__cell--ok")
      .first()
      .waitFor({ timeout: 15000 });
    await page.waitForFunction(
      () => !document.querySelector(".bk-heat__cell--load"),
      null,
      { timeout: 15000 },
    );
    const got = await page.evaluate((stacks) => {
      const cells = (/** @type {string} */ s) =>
        document.querySelectorAll(
          `.bk-heat [data-drive="backup-night"][data-drive-row^="${s}/"]`,
        ).length;
      return {
        labels: stacks.filter(
          (/** @type {string} */ s) =>
            !document.querySelector(
              `.bk-heat__label[data-stack="${CSS.escape(s)}"]`,
            ),
        ),
        perStack: Object.fromEntries(
          stacks.map((/** @type {string} */ s) => [s, cells(s)]),
        ),
        fleet: document.querySelectorAll(
          '.bk-heat [data-drive="backup-fleet-night"]',
        ).length,
      };
    }, names);
    assert.deepEqual(got.labels, [], "stacks without a heatmap row label");
    for (const n of names)
      assert.equal(
        got.perStack[n],
        30,
        `${n} has ${got.perStack[n]} night cells, not 30`,
      );
    assert.equal(got.fleet, 30, "the whole-fleet row is not 30 nights");
  } finally {
    await browser.close();
  }
});

test("invariants: the Backups heatmap paints status in the status tokens, in light and dark", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    const bad = [];
    const seen = { ok: 0, miss: 0, fleet: 0 };
    for (const theme of ["dark", "light"]) {
      await page.evaluate((t) => localStorage.setItem("theme", t), theme);
      await page.goto(`${BASE}/backups`);
      await page
        .locator(".bk-heat__cell--ok")
        .first()
        .waitFor({ timeout: 15000 });
      await page.waitForTimeout(300);
      const found = await page.evaluate(() => {
        const probe = document.createElement("div");
        document.body.append(probe);
        const paint = (/** @type {string} */ v) => {
          probe.style.background = v;
          return getComputedStyle(probe).backgroundColor;
        };
        const ok = paint(
          "color-mix(in oklab, var(--success-foreground) 70%, var(--card))",
        );
        const miss = paint("var(--destructive)");
        const warn = paint("var(--warning-foreground)");
        const charts = [1, 2, 3, 4, 5].map((i) => paint(`var(--chart-${i})`));
        /** @type {string[]} */
        const out = [];
        const n = { ok: 0, miss: 0, fleet: 0 };
        const check = (
          /** @type {string} */ sel,
          /** @type {string} */ want,
          /** @type {"ok" | "miss" | "fleet"} */ key,
        ) => {
          for (const el of document.querySelectorAll(sel)) {
            n[key]++;
            const got = getComputedStyle(el).backgroundColor;
            if (got !== want) out.push(`${sel} is ${got}, not ${want}`);
            if (charts.includes(got))
              out.push(`${sel} is a decorative chart colour (${got})`);
          }
        };
        check(
          ".bk-heat__row:not(.bk-heat__fleet) .bk-heat__cell--ok",
          ok,
          "ok",
        );
        check(".bk-heat__cell--miss", miss, "miss");
        check(
          ".bk-heat__fleet .bk-heat__cell:not(.bk-heat__cell--warn) i",
          ok,
          "fleet",
        );
        check(".bk-heat__fleet .bk-heat__cell--warn i", warn, "fleet");
        check(".bk-legend__ok", ok, "ok");
        check(".bk-legend__miss", miss, "miss");
        probe.remove();
        return { out: [...new Set(out)], n };
      });
      seen.ok += found.n.ok;
      seen.miss += found.n.miss;
      seen.fleet += found.n.fleet;
      for (const f of found.out) bad.push(`${theme}: ${f}`);
    }
    assert.ok(seen.ok > 0, "no backed-up cell on the heatmap");
    assert.ok(
      seen.miss > 0,
      "no missed cell on the heatmap (the demo host misses one night)",
    );
    assert.ok(seen.fleet > 0, "no whole-fleet bar on the heatmap");
    assert.deepEqual(bad, [], `status painted off-token:\n${bad.join("\n")}`);
  } finally {
    await browser.close();
  }
});

test("invariants: hovering a Backups heatmap night shows its snapshot time and every app's snapshot id", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/backups`);
    const cell = page
      .locator(
        '.bk-heat [data-drive="backup-night"][data-drive-row^="gateway/"].bk-heat__cell--ok',
      )
      .last();
    await cell.waitFor({ timeout: 15000 });
    await page.waitForTimeout(500);
    const night = (await cell.getAttribute("data-drive-row"))?.split("/")[1];
    await cell.hover();
    const tip = page.locator(".nx-tip[role=tooltip]");
    await tip.waitFor({ state: "visible", timeout: 3000 });
    const text = await tip.innerText();
    assert.match(text, /gateway/);
    assert.match(text, /snapshot at \d{2}:\d{2}/, `no snapshot time: ${text}`);
    // Every app of the stack that wrote a snapshot that night names its id:
    // a night runs from noon to noon and carries its evening's date.
    const want = await page.evaluate(async (n) => {
      const r = await (await fetch("/data/backups/gateway")).json();
      const key = (/** @type {number} */ t) => {
        const d = new Date((t - 12 * 3600) * 1000);
        const p = (/** @type {number} */ x) => String(x).padStart(2, "0");
        return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
      };
      return r.repos.flatMap((/** @type {any} */ x) =>
        x.snapshots
          .filter((/** @type {any} */ s) => key(s.time) === n)
          .map((/** @type {any} */ s) => `${x.owner} ${s.short_id}`),
      );
    }, night);
    assert.ok(want.length > 1, `gateway wrote too few snapshots on ${night}`);
    for (const w of want)
      assert.ok(text.includes(w), `the hover card misses "${w}": ${text}`);
    await page.mouse.move(2, 2);
    await tip.waitFor({ state: "hidden", timeout: 3000 });
  } finally {
    await browser.close();
  }
});

test("invariants: clicking a Backups heatmap night pins it into the address and the side panel; Esc, Unpin and Today go back to last night", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/backups`);
    const cells = page.locator(
      '.bk-heat [data-drive="backup-night"][data-drive-row^="gateway/"].bk-heat__cell--ok',
    );
    await cells.first().waitFor({ timeout: 15000 });
    await page.waitForTimeout(500);
    const label = page.locator("#bk-night .bk-night__label");
    assert.match(await label.innerText(), /Last night/);
    const pinOne = async () => {
      const cell = cells.nth((await cells.count()) - 2);
      const night = (await cell.getAttribute("data-drive-row"))?.split("/")[1];
      await cell.click();
      await page.waitForTimeout(150);
      return night;
    };
    const night = await pinOne();
    const param = () => new URL(page.url()).searchParams.get("night");
    assert.equal(param(), night, "the pinned night is not in the address");
    assert.match(await label.innerText(), /Pinned night/);
    assert.ok(
      (await page.locator(".bk-heat__cell.is-col").count()) > 1,
      "the pinned night's column is not marked",
    );
    // The address carries it: a reload (or a shared link) shows it pinned.
    await page.reload();
    await cells.first().waitFor({ timeout: 15000 });
    await page.waitForTimeout(500);
    assert.match(await label.innerText(), /Pinned night/);
    assert.equal(param(), night);
    // Esc on the grid unpins.
    await page.locator(`.bk-heat [data-drive-row="gateway/${night}"]`).focus();
    await page.keyboard.press("Escape");
    await page.waitForTimeout(150);
    assert.equal(param(), null, "Esc left the night in the address");
    assert.match(await label.innerText(), /Last night/);
    // Unpin does the same.
    await pinOne();
    await page.locator('[data-drive="backup-night-unpin"]').click();
    await page.waitForTimeout(150);
    assert.equal(param(), null, "Unpin left the night in the address");
    assert.match(await label.innerText(), /Last night/);
    // Today as well.
    await pinOne();
    await page.locator('[data-drive="backup-nights-today"]').click();
    await page.waitForTimeout(150);
    assert.equal(param(), null, "Today left the night in the address");
    assert.match(await label.innerText(), /Last night/);
  } finally {
    await browser.close();
  }
});

// ── redesign-host (3.71.0, Kenny approved the Host demo 2026-10-03:
// "approved demos are implemented exactly") — the whole-screen shape of
// the redesigned Host page, in the demo host's own data. ─────────────────

test("invariants: the Host page lays out two columns, Containers first, the line and the facts on the right", async () => {
  const browser = await launch();
  try {
    const bad = [];
    for (const width of [1894, 390]) {
      const context = await browser.newContext({
        viewport: { width, height: 1000 },
      });
      const page = await freshPage(context);
      await page.goto(`${BASE}/host`);
      await page
        .locator("#host-guests tr[data-vmid]")
        .first()
        .waitFor({ timeout: 10000 });
      const r = await page.evaluate(() => {
        const box = (/** @type {string} */ sel) =>
          document.querySelector(sel)?.getBoundingClientRect() ?? null;
        const main = [
          ...document.querySelectorAll(".hk-col-main > section"),
        ].map((s) => s.id);
        const side = [
          ...document.querySelectorAll(".hk-col-side > section"),
        ].map((s) => s.id);
        return {
          main,
          side,
          containers: box("#host-containers"),
          connection: box("#host-connection"),
          kpis: [...document.querySelectorAll(".nx-kpi")].map(
            (k) => /** @type {HTMLElement} */ (k).dataset.key,
          ),
          kpiText: [...document.querySelectorAll(".nx-kpi")].map(
            (k) => k.textContent ?? "",
          ),
          primary: [
            ...document.querySelectorAll(".nx-head-actions .kp-button"),
          ].map((b) => ({
            label: (b.textContent ?? "").trim(),
            primary: b.classList.contains("kp-button--primary"),
          })),
          reach: document.querySelector("#host-reach")?.textContent ?? "",
          // No sideways page scroll, at any width.
          overflow: document.documentElement.scrollWidth - window.innerWidth,
        };
      });
      if (r.main[0] !== "host-containers")
        bad.push(`${width}px: the left column opens with ${r.main[0]}`);
      if (r.main.join() !== "host-containers,host-actions-card,host-disk-card")
        bad.push(`${width}px: left column ${r.main}`);
      if (
        r.side.join() !==
        "host-connection,host-about-card,host-templates-card,host-checks-card"
      )
        bad.push(`${width}px: right column ${r.side}`);
      if (
        r.kpis.join() !== "cpu,memory,root,pool,containers" ||
        !/7\s*of 8 running/.test(r.kpiText[4])
      )
        bad.push(`${width}px: KPI strip ${r.kpis} / ${r.kpiText[4]}`);
      const labels = r.primary.map((p) => p.label);
      if (
        labels.join("|") !== "Open the console|Host log|Run host checks" ||
        !r.primary[2].primary ||
        r.primary.filter((p) => p.primary).length !== 1
      )
        bad.push(`${width}px: header actions ${JSON.stringify(r.primary)}`);
      if (r.overflow > 0)
        bad.push(`${width}px: the page scrolls ${r.overflow}px sideways`);
      if (!/^Reachable · \d+ ms$/.test(r.reach.trim()))
        bad.push(`${width}px: the live chip reads "${r.reach}"`);
      const c = r.containers;
      const l = r.connection;
      if (!c || !l) bad.push(`${width}px: a card is missing`);
      else if (width > 1200) {
        // Two columns: the line card sits right of Containers, level
        // with it, and Containers takes about two thirds.
        if (!(l.left > c.right) || Math.abs(l.top - c.top) > 2)
          bad.push(`${width}px: Connection is not beside Containers`);
        const share = c.width / (l.right - c.left);
        if (share < 0.6 || share > 0.72)
          bad.push(
            `${width}px: Containers takes ${share.toFixed(2)} of the row`,
          );
      } else if (!(l.top > c.bottom))
        bad.push(`${width}px: on a phone Connection is not under Containers`);
      await context.close();
    }
    assert.deepEqual(bad, [], bad.join("; "));
  } finally {
    await browser.close();
  }
});

// redesign-host-4: the seven facts the approved demo shows that the host
// did not send before 3.71.0. The demo host sends every one, so nothing on
// the page may read "not reported", and each shows its real value.
test("invariants: the Host page shows every host fact the demo host sends, none reported missing", async () => {
  const browser = await launch();
  try {
    const bad = [];
    for (const width of [1894, 390]) {
      const context = await browser.newContext({
        viewport: { width, height: 1000 },
      });
      const page = await freshPage(context);
      await page.goto(`${BASE}/host`);
      await page
        .locator("#host-guests tr[data-vmid='113']")
        .waitFor({ timeout: 10000 });
      await page
        .locator("#host-growth", { hasText: /Root/ })
        .waitFor({ timeout: 10000 });
      const r = await page.evaluate(() => {
        const text = (/** @type {string} */ sel) =>
          (document.querySelector(sel)?.textContent ?? "")
            .replace(/\s+/g, " ")
            .trim();
        return {
          all: text("main") || document.body.textContent || "",
          pool: text(".nx-kpi[data-key='pool']"),
          promised: text("#host-pool-promised"),
          written: text("#host-pool-written"),
          disk: text("#host-disk .hk-disk-line"),
          about: text("#host-about"),
          signature: text("#host-daemon-signature"),
          metrics: text("#host-guests tr[data-vmid='113']"),
          growth: text("#host-growth"),
        };
      });
      if (/not reported/i.test(r.all))
        bad.push(
          `${width}px: the page still says "not reported": ${r.all.match(/.{0,60}not reported.{0,40}/i)?.[0]}`,
        );
      if (!/41\s*%/.test(r.pool) || !/460 GB free of 780 GB/.test(r.pool))
        bad.push(`${width}px: pool KPI "${r.pool}"`);
      if (!/412 GB/.test(r.promised))
        bad.push(`${width}px: promised "${r.promised}"`);
      if (!/320 GB · 41%/.test(r.written))
        bad.push(`${width}px: written "${r.written}"`);
      if (!/\(932 GB SSD\)/.test(r.disk))
        bad.push(`${width}px: root disk line "${r.disk}"`);
      if (!/Up for ?12 days 3 h/.test(r.about))
        bad.push(`${width}px: about "${r.about}"`);
      if (!/\(signed release\)/.test(r.signature))
        bad.push(`${width}px: daemon "${r.signature}"`);
      // The unmanaged container's memory and CPU (hidden columns on a
      // phone still carry the text).
      if (!/3\.0 \/ 8\.0 GiB/.test(r.metrics) || !/13%/.test(r.metrics))
        bad.push(`${width}px: CT 113 row "${r.metrics}"`);
      if (
        !/^Root grows 0\.2 GB a week — full in about \d+ years/.test(r.growth)
      )
        bad.push(`${width}px: growth "${r.growth}"`);
      await context.close();
    }
    assert.deepEqual(bad, [], bad.join("; "));
  } finally {
    await browser.close();
  }
});

test("invariants: the Host page's Containers filter toggles with a plain click and / focuses its search", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/host`);
    await page
      .locator("#host-guests tr[data-vmid]")
      .first()
      .waitFor({ timeout: 10000 });
    const shown = () =>
      page.$$eval("#host-guests tr[data-vmid]", (rows) =>
        rows
          .filter((r) => !(/** @type {HTMLElement} */ (r).hidden))
          .map((r) => /** @type {HTMLElement} */ (r).dataset.status),
      );
    const all = await shown();
    assert.ok(all.length >= 3, `only ${all.length} containers listed`);
    assert.ok(all.includes("stopped") && all.includes("running"));
    const chip = (/** @type {string} */ v) =>
      page.locator(`[data-drive="container-filter"][data-drive-row="${v}"]`);
    await chip("stopped").click();
    assert.deepEqual(
      [...new Set(await shown())],
      ["stopped"],
      "Stopped did not narrow to stopped",
    );
    // A second plain click turns it off again: back to every row.
    await chip("stopped").click();
    assert.equal((await shown()).length, all.length);
    assert.equal(await chip("all").getAttribute("aria-pressed"), "true");
    await chip("running").click();
    assert.deepEqual([...new Set(await shown())], ["running"]);
    await chip("all").click();
    assert.equal((await shown()).length, all.length);
    // Sorting by a header keeps every row and reorders them.
    await page.locator("#host-containers th", { hasText: "ID" }).click();
    await page.locator("#host-containers th", { hasText: "ID" }).click();
    const ids = await page.$$eval("#host-guests tr[data-vmid]", (rows) =>
      rows.map((r) => Number(/** @type {HTMLElement} */ (r).dataset.vmid)),
    );
    assert.deepEqual(
      ids,
      [...ids].sort((a, b) => b - a),
      "ID did not sort descending on the second click",
    );
    await page.locator("body").click({ position: { x: 5, y: 5 } });
    await page.keyboard.press("/");
    assert.equal(
      await page.evaluate(() => document.activeElement?.id),
      "host-guest-search",
      "/ did not focus the containers search",
    );
  } finally {
    await browser.close();
  }
});

test("invariants: Host settings show only the values host.toml changes until All is picked", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/host`);
    await page
      .locator("#host-settings tr[data-key]")
      .first()
      .waitFor({ state: "attached", timeout: 10000 });
    const read = () =>
      page.evaluate(() => {
        const rows = [
          ...document.querySelectorAll("#host-settings tr[data-key]"),
        ];
        const vis = rows.filter(
          (r) => /** @type {HTMLElement} */ (r).offsetParent,
        );
        return {
          total: rows.length,
          changed: rows.filter(
            (r) => /** @type {HTMLElement} */ (r).dataset.set === "1",
          ).length,
          shown: vis.length,
          shownAllChanged: vis.every(
            (r) => /** @type {HTMLElement} */ (r).dataset.set === "1",
          ),
          open: /** @type {HTMLDetailsElement | null} */ (
            document.querySelector("#host-settings-card")
          )?.open,
          desc:
            document.querySelector("#host-settings-card .section-head__desc")
              ?.textContent ?? "",
        };
      });
    const before = await read();
    assert.equal(before.open, true, "the settings card starts folded");
    assert.ok(before.changed > 0, "the demo host.toml changes nothing");
    assert.ok(
      before.total > before.changed + 10,
      `only ${before.total} settings`,
    );
    assert.equal(
      before.shown,
      before.changed,
      "rows at their default are shown by default",
    );
    assert.ok(before.shownAllChanged);
    assert.ok(
      before.desc.includes(
        `${before.changed} of ${before.total} settings changed`,
      ),
      `the description does not count them: ${before.desc}`,
    );
    assert.equal(
      await page
        .locator("#host-settings-card a.kp-button", {
          hasText: "Edit in Settings",
        })
        .getAttribute("href"),
      "/settings",
    );
    await page
      .locator('[data-drive="host-settings-view"][data-drive-row="all"]')
      .click();
    assert.equal(
      (await read()).shown,
      before.total,
      "All does not show every setting",
    );
    await page
      .locator('[data-drive="host-settings-view"][data-drive-row="changed"]')
      .click();
    assert.equal((await read()).shown, before.changed);
  } finally {
    await browser.close();
  }
});

test("invariants: the Host page's actions are grouped by intent as described tiles, red only when destructive", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    const catalog = await page.evaluate(async () =>
      (await fetch("/data/actions/catalog")).json(),
    );
    const host = catalog.actions.filter(
      (/** @type {any} */ a) => a.target === "host",
    );
    await page.goto(`${BASE}/host`);
    await page
      .locator("#host-actions .hk-act")
      .first()
      .waitFor({ timeout: 10000 });
    const groups = await page.evaluate(() =>
      [...document.querySelectorAll("#host-actions .hk-act-group")].map(
        (g) => ({
          title: /** @type {HTMLElement} */ (g).dataset.group,
          full: !!g.querySelector(".hk-lock"),
          tiles: [...g.querySelectorAll(".hk-act")].map((t) => ({
            action: t.getAttribute("data-action"),
            label: (t.querySelector("b")?.textContent ?? "").trim(),
            desc: (t.querySelector("span")?.textContent ?? "").trim(),
            red: t.classList.contains("hk-act--destructive"),
          })),
        }),
      ),
    );
    assert.deepEqual(
      groups.map((g) => g.title),
      ["Keep it healthy", "Build and guard", "Change the host"],
    );
    assert.deepEqual(
      groups.map((g) => g.full),
      [false, false, true],
    );
    const tiles = groups.flatMap((g) => g.tiles);
    assert.deepEqual(
      tiles.map((t) => t.action).sort(),
      host.map((/** @type {any} */ a) => a.action).sort(),
      "a host action is missing from the tiles",
    );
    for (const t of tiles) {
      assert.ok(
        t.desc.length > 10 && t.desc.endsWith("."),
        `${t.label} has no one-line description`,
      );
      const entry = host.find((/** @type {any} */ a) => a.action === t.action);
      assert.equal(
        t.red,
        entry.destructive === true,
        `${t.label} red=${t.red}`,
      );
    }
    // The left column reads Containers, then the actions, then Disk.
    const order = await page.$$eval(".hk-col-main > section", (s) =>
      s.map((x) => x.id),
    );
    assert.deepEqual(order.slice(0, 2), [
      "host-containers",
      "host-actions-card",
    ]);
    // A tile opens its existing action dialog.
    await page.locator('#host-actions .hk-act[data-action="patch"]').click();
    const dialog = page.locator("dialog#action-dialog[open]");
    await dialog.waitFor({ timeout: 3000 });
    assert.ok(
      await dialog.isVisible(),
      "the tile did not open the action dialog",
    );
  } finally {
    await browser.close();
  }
});

// ── redesign-flows: the Inbox with all its sources, the one Update flow,
// Help and the first-use tour (redesign 3.71, approved by Kenny
// 2026-10-03: demos flows/inbox.html, flows/update.html, flows/shell.js) ──

test("invariants: every Inbox row says what and why and carries its fix, and a plain click on a kind shows only those", async () => {
  const browser = await launch();
  try {
    for (const width of [1894, 390]) {
      const context = await browser.newContext({
        viewport: { width, height: 1000 },
      });
      const page = await freshPage(context);
      await page.goto(`${BASE}/inbox`);
      // The demo host: an Updates row (its stale images), a Setup row
      // (stacks without a sealed env) and Today's made-up item.
      const update = page.locator(
        "#page .inbox-card .nx-inbox__row[data-kind=update]",
      );
      await update.waitFor({ timeout: 20000 });
      const setup = page.locator(
        "#page .inbox-card .nx-inbox__row[data-kind=setup]",
      );
      await setup.waitFor({ timeout: 5000 });
      assert.match(
        await update.locator("strong").first().innerText(),
        /^\d+ apps? (has|have) a newer version$/,
      );
      assert.ok(
        (await update.locator(".nx-inbox__what > span").first().innerText())
          .length > 10,
        "the Updates row says why",
      );
      assert.match(
        await update.locator(".nx-inbox__acts a").last().innerText(),
        /^Review and update/,
      );
      assert.equal(
        await setup.locator(".nx-inbox__acts button").last().innerText(),
        "Push the envs…",
      );
      // Counter = rows, here as on every page.
      const rows = await page
        .locator("#page .inbox-card .nx-inbox__row")
        .count();
      const badge =
        width > 960
          ? page.locator("#nav a[data-area='inbox'] .nx-count")
          : page.locator(".nx-tabbar [data-area='inbox'] .nx-count");
      assert.equal((await badge.innerText()).trim(), String(rows));
      // A plain click on Setup shows only the Setup row; again shows all.
      // The kit (redesign-kit-1) marks a chip with data-v.
      const chip = page.locator("#page .nx-tb button[data-v=setup]");
      await chip.click();
      assert.equal(
        await page.locator("#page .inbox-card .nx-inbox__row:visible").count(),
        1,
      );
      assert.match(
        await page.locator("#page .inbox-shown").innerText(),
        new RegExp(`^1 of ${rows} shown$`),
      );
      await chip.click();
      assert.equal(
        await page.locator("#page .inbox-card .nx-inbox__row").count(),
        rows,
      );
      // The counter on another page is the same number.
      await page.goto(`${BASE}/stacks`);
      await page.waitForTimeout(2500);
      assert.equal(
        (await badge.innerText()).trim(),
        String(rows),
        `${width}px: the counter on Stacks is not the Inbox's row count`,
      );
      const sideways = await page.evaluate(
        () =>
          document.documentElement.scrollWidth -
          document.documentElement.clientWidth,
      );
      assert.ok(sideways <= 0, `${width}px scrolls sideways by ${sideways}`);
      await context.close();
    }
  } finally {
    await browser.close();
  }
});

test("invariants: the Update flow runs See, Impact, Back up, Update, Verify and Done from the Inbox, and nothing runs before step 2's button", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/inbox`);
    const update = page.locator(
      "#page .inbox-card .nx-inbox__row[data-kind=update]",
    );
    await update.waitFor({ timeout: 20000 });
    /** @type {string[]} */
    const posts = [];
    page.on("request", (r) => {
      if (r.method() === "POST") posts.push(new URL(r.url()).pathname);
    });
    await update.locator(".nx-inbox__acts a").last().click();
    // redesign-flows-11: its own address, `/update?all=1`, or the one stack
    // when only one has newer apps.
    await page.waitForURL(/\/update\?/, { timeout: 5000 });
    const now = () =>
      page.locator("#page .nx-steps li[data-s=now] span").innerText();
    assert.equal(await now(), "See");
    await page.locator("#page .uf-item").first().waitFor({ timeout: 15000 });
    // A plain click on a row leaves it out, again includes it.
    const first = page.locator("#page .uf-item input").first();
    await page
      .locator("#page .uf-item")
      .first()
      .click({ position: { x: 200, y: 12 } });
    assert.equal(await first.isChecked(), false);
    await page
      .locator("#page .uf-item")
      .first()
      .click({ position: { x: 200, y: 12 } });
    assert.equal(await first.isChecked(), true);
    await page.locator("[data-drive=update-see-impact]").click();
    assert.equal(await now(), "Impact");
    const tiles = await page.locator("#page .uf-tile b").allInnerTexts();
    assert.deepEqual(
      tiles.map((t) => t.toLowerCase()),
      ["downtime", "restarts", "safety net", "undo later"],
    );
    await page
      .locator("#page .uf-deps .uf-dep, #page .uf-deps .uf-hint")
      .first()
      .waitFor({ timeout: 5000 });
    // Only plans were asked so far: nothing ran.
    assert.deepEqual(
      posts.filter((p) => !p.endsWith("/plan")),
      [],
      `something ran before the button: ${posts.join(", ")}`,
    );
    for (const t of await page.locator("[data-drive=update-major-read]").all())
      await t.check();
    const go = page.locator("[data-drive=update-go]");
    await page.waitForFunction(
      () =>
        !(
          /** @type {HTMLButtonElement | null} */ (
            document.querySelector("[data-drive=update-go]")
          )?.disabled ?? true
        ),
      null,
      { timeout: 10000 },
    );
    assert.match(await go.innerText(), /^Back up and update/);
    // redesign-flows-6 (review items 3 and 16): the button starts ONE job
    // on the dashboard's server — backup, commit, deploy, verify — waited
    // for as a request, never a fixed sleep.
    const started = page.waitForRequest(
      (r) =>
        r.method() === "POST" &&
        new URL(r.url()).pathname === "/data/actions/_host/update-apps",
      { timeout: 5000 },
    );
    await go.click();
    await started;
    await page.locator("#page .nx-check").waitFor({ timeout: 5000 });
    assert.equal(
      posts.find((p) => !p.endsWith("/plan")),
      "/data/actions/_host/update-apps",
      "the first thing sent is not the one update job",
    );
    await page
      .locator("#page .uf-done h2")
      .waitFor({ state: "visible", timeout: 120000 });
    assert.equal(await now(), "Done");
    assert.match(
      await page.locator("#page .uf-done h2").innerText(),
      /^Updated/,
    );
    const states = await page
      .locator("#page .nx-check li")
      .evaluateAll((l) => l.map((x) => x.getAttribute("data-s")));
    assert.ok(
      states.every((s) => s === "ok" || s === "skip"),
      `a step did not pass: ${states.join(", ")}`,
    );
    assert.ok(
      (await page.locator("[data-drive=update-roll-back]").count()) >= 1,
      "the result offers Roll back…",
    );
  } finally {
    await browser.close();
  }
});

test("invariants: ? opens Help with the six areas, the words and the keys, and its tour walks six steps on desktop and phone", async () => {
  const browser = await launch();
  try {
    for (const width of [1894, 390]) {
      const context = await browser.newContext({
        viewport: { width, height: 900 },
      });
      const page = await freshPage(context);
      await page.goto(`${BASE}/apps`);
      await page.waitForTimeout(800);
      // Apps focuses its launcher search on a desktop (redesign-stacks),
      // where "?" is a character typed; Help's key works outside a field.
      await page.evaluate(() =>
        /** @type {HTMLElement | null} */ (document.activeElement)?.blur(),
      );
      await page.keyboard.press("?");
      const help = page.locator("dialog#shortcuts[open]");
      await help.waitFor({ timeout: 3000 });
      const text = await help.innerText();
      for (const w of [
        "Where things live",
        "Apps",
        "Inbox",
        "Stacks",
        "Activity",
        "Backups",
        "System",
        "Words",
        "Stack",
        "App",
        "Deploy",
        "Update",
        "Back up / snapshot",
        "Restore",
        "Secret",
        "Job",
        "Ctrl K",
      ])
        assert.ok(text.includes(w), `${width}px: Help does not say ${w}`);
      await help.getByText("Take the 1-minute tour").click();
      const tour = page.locator(".tour");
      /** @type {string[]} */
      const titles = [];
      for (let i = 0; i < 6; i++) {
        await tour.waitFor({ timeout: 3000 });
        titles.push(await tour.locator("h3").innerText());
        assert.equal(
          await tour.locator("footer span").first().innerText(),
          `${i + 1} of 6`,
        );
        assert.equal(
          await page.locator(".tour-target").count(),
          1,
          `${width}px: step ${i + 1} points at nothing`,
        );
        const box = await tour.boundingBox();
        assert.ok(
          box && box.x >= 0 && box.x + box.width <= width + 1,
          `${width}px: the tour card leaves the screen`,
        );
        await tour
          .getByRole("button", { name: i === 5 ? "Done" : "Next" })
          .click();
      }
      assert.deepEqual(titles, [
        "One box for everything",
        "Inbox",
        "Stacks",
        "Activity",
        "Backups",
        "System",
      ]);
      assert.equal(await page.locator(".tour").count(), 0);
      assert.equal(
        await page.evaluate(() => localStorage.getItem("homelab-toured")),
        "1",
        "the tour remembers it was seen",
      );
      await context.close();
    }
  } finally {
    await browser.close();
  }
});

test("invariants: Deploy all changes shows four count tiles that each show only their column, and its bar says what Apply will do", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    // redesign-flows-4: a fixed plan, so every column has a stack.
    await page.route("**/data/apply/plan", (route) =>
      route.fulfill({
        contentType: "application/json",
        body: JSON.stringify({
          plan: {
            deploy: ["alpha", "beta"],
            new: ["beta"],
            destroy: ["gone"],
            broken: [["gamma", "compose.yaml did not parse :: fix line 3"]],
            unchanged: ["delta"],
            ephemeral: [],
          },
          pending: 3,
          measured_at: Math.floor(Date.now() / 1000),
        }),
      }),
    );
    await page.goto(`${BASE}/stacks?deploy-all=1`);
    const tiles = page.locator("#apply-section .ap-tile");
    await tiles.first().waitFor({ timeout: 10000 });
    assert.equal(await tiles.count(), 4, "not four count tiles");
    const shown = () =>
      page.$$eval("#apply-section .ap-col", (cs) =>
        cs
          .filter((c) => !(/** @type {HTMLElement} */ (c).hidden))
          .map((c) => c.getAttribute("data-col")),
      );
    assert.deepEqual(await shown(), ["deploy", "destroy", "broken"]);
    // A plain click shows only that column; again shows all.
    await tiles.nth(1).click();
    assert.deepEqual(await shown(), ["destroy"]);
    assert.equal(await tiles.nth(1).getAttribute("aria-pressed"), "true");
    await tiles.nth(1).click();
    assert.deepEqual(await shown(), ["deploy", "destroy", "broken"]);
    // redesign-flows-5: Apply deploys the ticked stacks; the stack that
    // cannot be planned and an unticked one are left alone, by name.
    const go = page.locator("#apply-open");
    assert.equal((await go.innerText()).trim(), "Apply 2 deploys…");
    await page.getByLabel("Include alpha").uncheck();
    assert.equal((await go.innerText()).trim(), "Apply 1 deploy…");
    const bar = await page.locator("#apply-section .ap-go").innerText();
    assert.ok(
      bar.includes("alpha") && bar.includes("gamma"),
      `the bar does not name what is left alone: ${bar}`,
    );
  } finally {
    await browser.close();
  }
});

// ── redesign-stacks (3.71.0, the approved Stacks and Apps demos:
// redesign-3.71/overview.html under FLOWS.md §1 row 3, and apps.html) ──

test("invariants: Stacks lists every stack once as a card and as a table row, with its identity mark and its hub link; a card's click opens the hub, its tick only ticks (redesign-stacks)", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    const names = await fleetNames(page);
    assert.ok(names.length > 1, "the demo host has no stacks");
    await page.goto(`${BASE}/stacks?view=cards`);
    await page
      .locator(".sk-card[data-stack]")
      .first()
      .waitFor({ timeout: 8000 });
    /** @param {string} sel */
    const read = (sel) =>
      page.$$eval(sel, (els) =>
        els.map((e) => ({
          stack: /** @type {HTMLElement} */ (e).dataset.stack,
          mark: e.querySelector("svg.nx-mark")?.getAttribute("data-stack"),
          hub: e.querySelector("a[href^='/stacks/']")?.getAttribute("href"),
        })),
      );
    for (const [view, sel] of [
      ["cards", ".sk-cards .sk-card[data-stack]"],
      ["table", ".sk-table tbody tr[data-stack]"],
    ]) {
      if (view === "table") await page.click(".nx-seg button[data-v=table]");
      const got = await read(sel);
      // Only the view that is on is drawn, so a Live view lookup by id and
      // row never lands on the hidden twin (redesign-stacks review 6).
      const hidden = view === "table" ? ".sk-cards" : ".sk-table tbody";
      assert.equal(
        await page.locator(`${hidden} [data-drive]`).count(),
        0,
        `${view}: the hidden view still holds controls`,
      );
      assert.deepEqual(
        got.map((g) => g.stack).sort(),
        [...names].sort(),
        `${view}: not every stack exactly once`,
      );
      for (const g of got) {
        assert.equal(
          g.mark,
          g.stack,
          `${view}: ${g.stack} has no identity mark`,
        );
        assert.equal(
          g.hub,
          `/stacks/${encodeURIComponent(String(g.stack))}`,
          `${view}: ${g.stack} does not link to its hub`,
        );
      }
    }
    await page.click(".nx-seg button[data-v=cards]");
    // The view is in the address (Table, the default since redesign-final
    // M1, is not), so a reload keeps it.
    assert.ok(page.url().includes("view=cards"), "the view is not in the URL");
    // A tick only ticks (invariant 12): the page stays, the card is marked.
    const first = names[0];
    await page.locator(`.sk-cards [data-tick="${first}"]`).check();
    assert.equal(new URL(page.url()).pathname, "/stacks");
    assert.ok(
      await page
        .locator(`.sk-cards .sk-card[data-stack="${first}"].is-ticked`)
        .isVisible(),
      "the ticked card is not marked",
    );
    // A click on the card's body opens that stack's hub.
    await page
      .locator(`.sk-cards .sk-card[data-stack="${first}"] .sk-card__bar`)
      .click();
    await page.waitForURL(`**/stacks/${encodeURIComponent(first)}`, {
      timeout: 5000,
    });
  } finally {
    await browser.close();
  }
});

test("invariants: Stacks keeps its ticks across Table/Cards and live updates, the batch bar names the exact count, and Esc unticks (redesign-stacks)", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    const names = await fleetNames(page);
    await page.goto(`${BASE}/stacks?view=cards`);
    await page
      .locator(".sk-card[data-stack]")
      .first()
      .waitFor({ timeout: 8000 });
    assert.equal(
      await page.locator(".sk-batch").isVisible(),
      false,
      "the batch bar shows with nothing ticked",
    );
    const two = names.slice(0, 2);
    for (const n of two)
      await page.locator(`.sk-cards [data-tick="${n}"]`).check();
    const bar = async () =>
      (await page.locator(".sk-batch > strong").innerText()).trim();
    assert.equal(await bar(), "2 ticked");
    assert.equal(
      (await page.locator(".sk-batch [data-batch=deploy]").innerText()).trim(),
      "Deploy 2 stacks",
    );
    await page.click(".nx-seg button[data-v=table]");
    const tickedRows = await page.$$eval(
      ".sk-table tbody input[data-tick]:checked",
      (els) => els.map((e) => /** @type {HTMLElement} */ (e).dataset.tick),
    );
    assert.deepEqual(
      tickedRows.sort(),
      [...two].sort(),
      "Table lost the ticks",
    );
    // The demo host pushes the fleet every 5 s: the ticks, the focused
    // control and the rows themselves (so their hover) survive it.
    const link = page.locator(
      `.sk-table [data-drive="stacks-open"][data-drive-row="${two[0]}"]`,
    );
    await link.focus();
    await page.evaluate((n) => {
      const tr = document.querySelector(`.sk-table tr[data-stack="${n}"]`);
      if (tr) /** @type {any} */ (tr).__kept = true;
    }, two[0]);
    const at0 = await page.getAttribute(
      ".sk-head [data-ago-at]",
      "data-ago-at",
    );
    await page.waitForFunction(
      (a) =>
        document
          .querySelector(".sk-head [data-ago-at]")
          ?.getAttribute("data-ago-at") !== a,
      at0,
      { timeout: 15000 },
    );
    assert.equal(await bar(), "2 ticked", "a live update dropped the ticks");
    const kept = await page.evaluate((n) => {
      const a = document.activeElement;
      const tr = document.querySelector(`.sk-table tr[data-stack="${n}"]`);
      return {
        focus:
          a instanceof HTMLElement &&
          a.dataset.drive === "stacks-open" &&
          a.dataset.driveRow === n,
        node: !!(/** @type {any} */ (tr)?.__kept),
      };
    }, two[0]);
    assert.ok(kept.focus, "a live update took the keyboard focus away");
    assert.ok(kept.node, "a live update rebuilt an unchanged row (hover lost)");
    // The header's tick-all counts what is shown, never more.
    await page.click(".sk-table thead input[type=checkbox]");
    assert.equal(await bar(), `${names.length} ticked`);
    await page.keyboard.press("Escape");
    assert.equal(
      await page.locator(".sk-batch").isVisible(),
      false,
      "Esc did not untick",
    );
    // An Only chip narrows the list and says its exact count.
    const chip = page.locator(".nx-chip-toggle[data-v=newer]");
    const want = Number(
      (await chip.locator(".nx-chip-toggle__count").innerText()).trim(),
    );
    await chip.click();
    const shown = await page.locator(".sk-table tbody tr[data-stack]").count();
    assert.equal(shown, want, "the chip's count is not the rows it shows");
    assert.ok(
      page.url().includes("only=newer"),
      "the filter is not in the URL",
    );
  } finally {
    await browser.close();
  }
});

test("invariants: Stacks has the host strip, Deploy all changes and New stack; New stack's panel reaches the preset and bundle forms (redesign-stacks)", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/stacks`);
    await page.waitForSelector(".sk-kpis .nx-kpi:not([data-loading])", {
      timeout: 8000,
    });
    const tiles = await page.$$eval(".sk-kpis .nx-kpi", (els) =>
      els.map((e) => ({
        label: e.querySelector(".nx-kpi__label")?.textContent?.trim(),
        href: e.getAttribute("href"),
        h: Math.round(e.getBoundingClientRect().height),
      })),
    );
    // redesign-final M1: the approved flows/stacks.html's five tiles.
    assert.deepEqual(
      tiles.map((t) => t.label),
      ["Stacks running", "Need you", "Newer versions", "Host CPU", "Root disk"],
    );
    assert.ok(
      tiles.every((t) => t.href?.startsWith("/")),
      "a host tile is not a link to its detail",
    );
    assert.notEqual(
      tiles[0].href,
      "/stacks",
      "Stacks running links to the page it is on",
    );
    // The count on Deploy all changes is there before any click (FLOWS.md
    // §3 #13), and it is the plan's own: deploys, new ones and removed
    // ones together.
    const plan = await page.evaluate(async () => {
      const r = await (await fetch("/data/apply/plan")).json();
      return r.plan.deploy.length + r.plan.destroy.length;
    });
    await page.waitForFunction(
      () =>
        !document
          .querySelector("#deploy-all .sk-badge")
          ?.hasAttribute("hidden"),
      null,
      { timeout: 10000 },
    );
    assert.equal(
      (await page.locator("#deploy-all .sk-badge").innerText()).trim(),
      String(plan),
      "the header's count is not what Deploy all changes would do",
    );
    assert.equal(
      new Set(tiles.map((t) => t.h)).size,
      1,
      `the host tiles differ in height: ${tiles.map((t) => t.h).join(", ")}`,
    );
    // The sparklines come from the last day's trend, not one point.
    // redesign-final M1: the Host CPU tile's line (Load left the strip).
    assert.ok(
      (await page.locator(".sk-kpis .nx-spark").count()) >= 1,
      "Host CPU draws no line",
    );
    const heads = await page.$$eval(".nx-head .actions-row > button", (bs) =>
      bs.map((b) => b.textContent?.trim()),
    );
    assert.ok(
      heads.some((t) => t?.startsWith("Deploy all changes")),
      "no Deploy all changes in the header",
    );
    assert.equal(
      heads[heads.length - 1],
      "New stack…",
      "New stack is not last",
    );
    await page.click("#new-stack");
    const forms = await page.$$eval("dialog[open] [data-drive-form]", (els) =>
      els.map((e) => /** @type {HTMLElement} */ (e).dataset.driveForm),
    );
    // Three routes, as FLOWS.md §1.5 draws them: preset, bundle, empty.
    assert.deepEqual(forms, ["new-stack", "import", "new-stack"]);
    assert.ok(
      await page.locator('dialog[open] [data-drive="new-empty"]').isVisible(),
      "New stack has no Empty route",
    );
    assert.equal(
      await page.locator("dialog[open] .kp-dialog__close").count(),
      1,
      "the panel's ✕ is not the dialogs' close button",
    );
    await page.click('dialog[open] [data-drive="new-empty"]');
    const wiz = page.locator("dialog#new-stack-dialog[open]");
    await wiz.waitFor({ timeout: 5000 });
    assert.equal(
      await wiz.locator('[data-step="preset"]').count(),
      0,
      "the Empty route still asks for a preset",
    );
  } finally {
    await browser.close();
  }
});

test("invariants: Stacks and Apps fit a 390 px phone with no sideways scroll, in light and dark (redesign-stacks)", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 390, height: 844 },
    });
    const page = await freshPage(context);
    /** @type {string[]} */
    const bad = [];
    for (const theme of ["formal", "dark"])
      for (const path of ["/stacks", "/stacks?view=table", "/apps"]) {
        await page.evaluate((t) => localStorage.setItem("theme", t), theme);
        await page.goto(`${BASE}${path}`);
        await page.waitForSelector(
          path === "/apps"
            ? ".ap-board .ap-grp:not(.ap-grp--skeleton), .ap-board .sk-empty-slot, .ap-board .ap-empty"
            : ".sk-list [data-stack]",
          { timeout: 10000 },
        );
        const over = await page.evaluate(
          () => document.documentElement.scrollWidth - window.innerWidth,
        );
        if (over > 1) bad.push(`${path} in ${theme}: ${over}px`);
        // No box scrolls sideways inside the page either, and every row's
        // action is on screen (redesign-stacks review 5).
        const inner = await page.$$eval(".kp-table-wrap", (els) =>
          els
            .filter((e) => e.getClientRects().length > 0)
            .map((e) => e.scrollWidth - e.clientWidth),
        );
        for (const d of inner)
          if (d > 1) bad.push(`${path} in ${theme}: a table scrolls ${d}px`);
        // The toolbar's switches stay on screen (none cut off by the card).
        const cut = await page.$$eval(".sk-page .nx-tb__state button", (els) =>
          els
            .filter((e) => e.getClientRects().length > 0)
            .filter((e) => e.getBoundingClientRect().right > window.innerWidth)
            .map((e) => e.textContent?.trim()),
        );
        if (cut.length)
          bad.push(`${path} in ${theme}: toolbar cut off: ${cut.join(", ")}`);
        if (path.includes("table")) {
          const off = await page.$$eval(
            ".sk-table tbody tr[data-stack] .sk-acts > *:first-child",
            (els) =>
              els
                .filter((e) => {
                  const r = e.getBoundingClientRect();
                  return r.right > window.innerWidth || r.left < 0;
                })
                .map((e) => e.closest("tr")?.getAttribute("data-stack")),
          );
          if (off.length)
            bad.push(
              `${path} in ${theme}: actions off screen: ${off.join(", ")}`,
            );
        }
      }
    assert.deepEqual(bad, [], `sideways scroll: ${bad.join("; ")}`);
  } finally {
    await browser.close();
  }
});

test("invariants: Deploy all changes marks its steps, says what each column holds and shows a stack that cannot be planned with its fix (redesign-stacks, redesign-flows-4)", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/stacks?deploy-all=1`);
    // Both demos put the live status beside the title.
    assert.equal(
      await page
        .locator(".sk-page .nx-head .title-row > h1 + .nx-live")
        .count(),
      1,
      "the header's 'updated' is not beside the title",
    );
    const panel = page.locator("dialog[open]");
    // The 3.71.0 merge: the panel's body is the approved apply demo
    // (apply.js), its Compare step the panel's Compare again.
    await panel.locator(".ap-col").first().waitFor({ timeout: 10000 });
    const steps = await panel.locator(".ap-steps li").evaluateAll((els) =>
      els.map((e) => ({
        step: /** @type {HTMLElement} */ (e).dataset.step,
        done: e.classList.contains("done"),
        on: e.classList.contains("on"),
      })),
    );
    assert.deepEqual(
      steps.slice(0, 3).map((x) => x.step),
      ["compare", "review", "apply"],
    );
    assert.ok(steps[0].done, "a filled plan says it never compared");
    assert.ok(steps[1].on, "the steps do not say where it stands");
    assert.equal(
      await panel.locator(".ap-steps #drift-compare").count(),
      1,
      "Compare again is not the plan's Compare step",
    );
    const text = await panel.innerText();
    assert.doesNotMatch(text, /\(s\)/, "bracketed plurals");
    assert.doesNotMatch(text, / :: |\/tmp\//, "raw error text in the panel");
    const cols = await panel.locator(".ap-col").evaluateAll((els) =>
      els.map((g) => ({
        title: g.querySelector("h3")?.textContent?.trim(),
        desc: g.querySelector(".ap-hint")?.textContent?.trim() ?? "",
      })),
    );
    assert.ok(cols.length > 0);
    for (const g of cols)
      assert.ok(g.desc.length > 10, `${g.title} has no description`);
    // The demo's alpha-demo declares no apps, so it cannot be planned:
    // shown with what is wrong and its fix.
    const broken = panel.locator(".ap-item--broken");
    assert.ok(
      (await broken.count()) > 0,
      "the stack that cannot be planned is not shown",
    );
    assert.match(await broken.first().innerText(), /Fix:/);
  } finally {
    await browser.close();
  }
});

test("invariants: a row's Update and the batch Update… open the one Update flow for the ticked stacks, never a link-free blind batch (redesign-stacks, redesign-integrate-5)", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    const names = await fleetNames(page);
    await page.goto(`${BASE}/stacks?view=table`);
    await page
      .locator(".sk-table tbody tr[data-stack]")
      .first()
      .waitFor({ timeout: 8000 });
    assert.equal(
      await page.locator('.sk-table a[data-action="update"]').count(),
      0,
      "a row's Update is still a link",
    );
    // invariant 149: whichever Update opened it, it is the one flow.
    await page.locator(`.sk-table [data-tick="${names[0]}"]`).check();
    await page.locator(`.sk-table [data-tick="${names[1]}"]`).check();
    const upd = page.locator(".sk-batch [data-batch=update]");
    assert.equal((await upd.innerText()).trim(), "Update…");
    await upd.click();
    await page.waitForURL("**/update?**", { timeout: 5000 });
    const q = new URL(page.url()).searchParams;
    assert.equal(q.get("all"), "1");
    assert.deepEqual(
      (q.get("only") ?? "").split(",").sort(),
      [names[0], names[1]].sort(),
    );
    await page.locator("#page .nx-steps").waitFor({ timeout: 10000 });
    assert.match(
      await page.locator("#page .nx-crumbs").innerText(),
      /Stacks/,
      "the batch's flow does not lead back to Stacks",
    );
  } finally {
    await browser.close();
  }
});

test("invariants: Apps groups the stacks' tiles, each opening in a new tab; the search narrows them and a star survives a reload (redesign-stacks)", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    const tiles = await page.evaluate(async () => {
      const r = await (await fetch("/data/tiles")).json();
      return r.report?.tiles ?? [];
    });
    await page.goto(`${BASE}/apps`);
    if (!tiles.length) {
      await page.waitForSelector(".ap-empty", { timeout: 8000 });
      return;
    }
    await page
      .locator(".ap-tile[data-host]")
      .first()
      .waitFor({ timeout: 8000 });
    const shown = await page.$$eval(".ap-tile[data-host]", (els) =>
      els.map((e) => ({
        host: /** @type {HTMLElement} */ (e).dataset.host,
        target: e.getAttribute("target"),
      })),
    );
    assert.deepEqual(
      shown.map((s) => s.host).sort(),
      tiles.map((/** @type {any} */ t) => t.host).sort(),
      "not every declared tile is on the page exactly once",
    );
    assert.ok(
      shown.every((s) => s.target === "_blank"),
      "a tile does not open in a new tab",
    );
    const groups = await page.$$eval(".ap-grp > h2", (hs) =>
      hs.map((x) => x.firstChild?.textContent?.trim()),
    );
    assert.deepEqual(
      groups,
      [...new Set(tiles.map((/** @type {any} */ t) => t.group))],
      "the groups are not the stacks' own, in their order",
    );
    // Search narrows, and the first match is the one Enter opens.
    const name = String(tiles[tiles.length - 1].name);
    await page.fill(".ap-search", name.slice(0, 4));
    const first = await page
      .locator(".ap-tile.is-first")
      .getAttribute("data-host");
    assert.equal(
      first,
      tiles.find((/** @type {any} */ t) =>
        [t.name, t.host, t.description ?? "", t.stack]
          .join(" ")
          .toLowerCase()
          .includes(name.slice(0, 4).toLowerCase()),
      )?.host,
    );
    await page.fill(".ap-search", "zzzz-nothing");
    assert.ok(
      await page.getByRole("button", { name: "Show every app" }).isVisible(),
      "no way back from an empty search",
    );
    await page.getByRole("button", { name: "Show every app" }).click();
    // A star moves the tile to Starred, and it is still there after a reload.
    const host = String(tiles[0].host);
    await page.locator(`.ap-tile[data-host="${host}"] .ap-tile__pin`).click();
    await page.reload();
    await page
      .locator(".ap-tile[data-host]")
      .first()
      .waitFor({ timeout: 8000 });
    const starred = await page.$$eval(
      '.ap-grp[aria-label="Starred"] .ap-tile',
      (els) => els.map((e) => /** @type {HTMLElement} */ (e).dataset.host),
    );
    assert.deepEqual(starred, [host], "the star did not survive a reload");
  } finally {
    await browser.close();
  }
});

// ── redesign-schedules (release 3.71.0, Kenny approved the demo 2026-10-03):
// the Schedules page as the approved demo draws it — a week calendar with
// the host's nightly round for reference, a plain-click switch with Undo, a
// sentence-builder drawer that previews its next three runs, and an empty
// state with templates. Each case clears the demo host's schedules first
// and again at its end, so the rest of this suite sees what it always saw.

/**
 * One JSON call from inside the page (its session cookie included).
 * @param {import("playwright").Page} page
 * @param {string} method
 * @param {string} url
 * @param {unknown} [body]
 */
async function schedApi(page, method, url, body) {
  return page.evaluate(
    async ([m, u, b]) => {
      const r = await fetch(/** @type {string} */ (u), {
        method: /** @type {string} */ (m),
        headers: b == null ? {} : { "content-type": "application/json" },
        body: b == null ? undefined : JSON.stringify(b),
      });
      const t = await r.text();
      return { status: r.status, body: t ? JSON.parse(t) : null };
    },
    [method, url, body ?? null],
  );
}

/** @param {import("playwright").Page} page */
async function clearSchedules(page) {
  const l = await schedApi(page, "GET", "/data/schedules");
  for (const v of l.body?.schedules ?? [])
    await schedApi(page, "DELETE", `/data/schedules/${v.schedule.id}`);
}

/**
 * @param {import("playwright").Page} page
 * @param {string} stack
 * @param {string} action
 * @param {unknown} when
 */
async function addSchedule(page, stack, action, when) {
  const r = await schedApi(page, "POST", "/data/schedules", {
    stack,
    action,
    args: {},
    when,
    enabled: true,
    note: "",
  });
  assert.equal(r.status, 201, `creating a schedule: ${JSON.stringify(r.body)}`);
  return /** @type {string} */ (r.body.schedule.id);
}

/** The Europe/Brussels wall clock of a unix second, as named parts. */
function brussels(/** @type {number} */ unix) {
  return Object.fromEntries(
    new Intl.DateTimeFormat("en-GB", {
      timeZone: "Europe/Brussels",
      weekday: "short",
      day: "numeric",
      month: "short",
      hour: "2-digit",
      minute: "2-digit",
      hourCycle: "h23",
    })
      .formatToParts(new Date(unix * 1000))
      .map((x) => [x.type, x.value]),
  );
}

test("invariants: Schedules page: the next 7 days show every run by time, the host's nightly round dashed, a now-line, and a pill finds its row", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1100 },
      timezoneId: "Europe/Brussels",
    });
    const page = await freshPage(context);
    await clearSchedules(page);
    const daily = await addSchedule(page, "films", "backup", {
      every: "day",
      at: "10:00",
    });
    await addSchedule(page, "_host", "patch", {
      every: "week",
      days: [0, 3],
      at: "22:00",
    });
    await page.goto(`${BASE}/schedules`);
    const week = page.locator(".sch-week");
    await week.waitFor({ timeout: 10000 });
    const heads = await page.locator(".sch-week__day").allTextContents();
    assert.equal(heads.length, 7, `the calendar shows ${heads.length} days`);
    assert.equal(heads[0].trim(), "Today");
    // The nightly round: one dashed block a day, at the host's hour.
    await page.locator(".sch-pill--host").first().waitFor({ timeout: 10000 });
    const host = await page.locator(".sch-pill--host").allTextContents();
    assert.equal(host.length, 7, `nightly round blocks: ${host.length}`);
    assert.ok(
      host.every((t) => /03:00\s*nightly round/.test(t)),
      `nightly round blocks read ${host[0]}`,
    );
    const style = await page
      .locator(".sch-pill--host")
      .first()
      .evaluate((e) => getComputedStyle(e).borderTopStyle);
    assert.equal(style, "dashed");
    // The daily backup is on all seven days; the patch on its own two.
    assert.equal(
      await page.locator(`.sch-pill[data-id="${daily}"]`).count(),
      7,
    );
    assert.equal(
      await page
        .locator(".sch-week__col")
        .first()
        .locator(".sch-nowline")
        .count(),
      1,
      "today's column has no now-line",
    );
    assert.equal(await page.locator(".sch-nowline").count(), 1);
    const patches = await page
      .locator(".sch-pill:not(.sch-pill--host)")
      .filter({ hasText: "Patch the fleet" })
      .count();
    assert.equal(patches, 2, `patch pills: ${patches}`);
    // A pill finds its row.
    await page.locator(`.sch-pill[data-id="${daily}"]`).nth(3).click();
    const row = page.locator(`tr[data-id="${daily}"]`);
    await page.waitForTimeout(200);
    assert.equal(
      await row.evaluate((r) => r.classList.contains("sch-flash")),
      true,
      "the clicked pill's row is not marked",
    );
    assert.equal(
      await row.evaluate((r) => document.activeElement === r),
      true,
      "the clicked pill's row has no focus",
    );
    // On a phone the week becomes an agenda list.
    await page.setViewportSize({ width: 390, height: 900 });
    await page.waitForTimeout(200);
    assert.equal(
      await week.isVisible(),
      false,
      "the week grid shows at 390 px",
    );
    const agenda = page.locator(".sch-agenda > div");
    assert.ok((await agenda.count()) >= 3, "the agenda lists the coming runs");
    assert.match(
      (await agenda.first().textContent()) ?? "",
      /\d\d:\d\d.*(Back up films|Patch the fleet)/,
    );
    await clearSchedules(page);
  } finally {
    await browser.close();
  }
});

test("invariants: Schedules page: a plain click turns a schedule off, greys its row and pills, and Undo turns it back on", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1100 },
      timezoneId: "Europe/Brussels",
    });
    const page = await freshPage(context);
    await clearSchedules(page);
    const id = await addSchedule(page, "films", "backup", {
      every: "day",
      at: "10:00",
    });
    await addSchedule(page, "notes", "backup", { every: "day", at: "11:00" });
    await page.goto(`${BASE}/schedules`);
    const row = page.locator(`tr[data-id="${id}"]`);
    await row.waitFor({ timeout: 10000 });
    assert.equal(
      ((await page.locator("#sched-count").textContent()) ?? "").trim(),
      "2 schedules · 2 on",
    );
    // The switch is the row's first cell.
    assert.equal(
      await row.locator("td").first().locator('input[role="switch"]').count(),
      1,
      "the on/off switch is not the first cell",
    );
    await row.locator('input[role="switch"]').click();
    await page.waitForTimeout(400);
    assert.equal(await row.evaluate((r) => r.classList.contains("off")), true);
    const pills = page.locator(`.sch-pill[data-id="${id}"]`);
    assert.ok((await pills.count()) > 0);
    assert.equal(
      await pills.evaluateAll((ps) =>
        ps.every((p) => p.classList.contains("off")),
      ),
      true,
      "its pills are not greyed",
    );
    assert.equal(
      ((await page.locator("#sched-count").textContent()) ?? "").trim(),
      "2 schedules · 1 on",
    );
    let list = await schedApi(page, "GET", "/data/schedules");
    assert.equal(
      list.body.schedules.find((/** @type {any} */ v) => v.schedule.id === id)
        .schedule.enabled,
      false,
      "the host still has it on",
    );
    const toast = page.locator(".nx-toast");
    assert.match((await toast.textContent()) ?? "", /is off/);
    await toast.getByRole("button", { name: "Undo" }).click();
    await page.waitForTimeout(400);
    assert.equal(await row.evaluate((r) => r.classList.contains("off")), false);
    list = await schedApi(page, "GET", "/data/schedules");
    assert.equal(
      list.body.schedules.find((/** @type {any} */ v) => v.schedule.id === id)
        .schedule.enabled,
      true,
      "Undo did not turn it back on",
    );
    // Space on a focused row toggles too.
    await row.focus();
    await page.keyboard.press(" ");
    await page.waitForTimeout(400);
    assert.equal(await row.evaluate((r) => r.classList.contains("off")), true);
    await page
      .locator(".nx-toast")
      .getByRole("button", { name: "Undo" })
      .click();
    await page.waitForTimeout(400);
    // Delete from the row menu is undoable: nothing is sent while Undo shows.
    /** @type {string[]} */
    const deletes = [];
    page.on("request", (r) => {
      if (r.method() === "DELETE") deletes.push(r.url());
    });
    await row.locator('[data-drive="schedule-menu"]').click();
    await page.locator('dialog.nx-rowmenu[open] [data-drive="delete"]').click();
    assert.equal(await row.isVisible(), false, "a deleted row still shows");
    await page
      .locator(".nx-toast")
      .getByRole("button", { name: "Undo" })
      .click();
    await page.waitForTimeout(300);
    assert.equal(
      await row.isVisible(),
      true,
      "Undo did not bring the row back",
    );
    assert.deepEqual(deletes, [], "Undo still deleted it");
    await clearSchedules(page);
  } finally {
    await browser.close();
  }
});

test("invariants: Schedules page: the drawer reads as a sentence, warns near the nightly round and previews the next 3 runs", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1100 },
      timezoneId: "Europe/Brussels",
    });
    const page = await freshPage(context);
    await clearSchedules(page);
    await addSchedule(page, "films", "backup", { every: "day", at: "10:00" });
    await page.goto(`${BASE}/schedules`);
    await page.locator("tr[data-id]").first().waitFor({ timeout: 10000 });
    await page.keyboard.press("n");
    const drawer = page.locator("dialog.sch-drawer[open]");
    await drawer.waitFor({ timeout: 3000 });
    assert.match(
      ((await drawer.locator(".sch-sentence").textContent()) ?? "").replace(
        /\s+/g,
        " ",
      ),
      /^Run .* on .* at/,
    );
    // The demo's defaults: chosen weekdays at 02:30, inside an hour of 03:00.
    await drawer.locator("#sched-every").selectOption("week");
    await drawer.locator("#sched-at").fill("02:30");
    await page.waitForTimeout(150);
    assert.equal(await drawer.locator("#sched-near").isVisible(), true);
    const shown = (
      await drawer.locator(".sch-preview [data-slot]").allTextContents()
    ).map((t) => t.replace(/\s+/g, " ").trim());
    assert.equal(shown.length, 3, `the preview lists ${shown.length} runs`);
    // The same three, worked out here from the host's wall clock.
    const days = await drawer
      .locator('[data-drive^="day-"][aria-pressed="true"]')
      .evaluateAll((bs) => bs.map((b) => b.getAttribute("data-drive") ?? ""));
    const want = new Set(days.map((d) => d.slice(4)));
    const now = Math.floor(Date.now() / 1000);
    /** @type {string[]} */
    const expected = [];
    for (let t = now - (now % 60) + 60; expected.length < 3; t += 60) {
      const p = brussels(t);
      if (
        p.hour === "02" &&
        p.minute === "30" &&
        want.has(p.weekday.toLowerCase())
      )
        expected.push(`${p.weekday} ${p.day} ${p.month}, 02:30`);
    }
    for (const [i, e] of expected.entries())
      assert.ok(
        shown[i].startsWith(e),
        `run ${i + 1}: "${shown[i]}", expected ${e}`,
      );
    // Away from the round, the warning goes.
    await drawer.locator("#sched-at").fill("05:00");
    await page.waitForTimeout(150);
    assert.equal(await drawer.locator("#sched-near").isVisible(), false);
    assert.match(
      (await drawer
        .locator(".sch-preview [data-slot]")
        .first()
        .textContent()) ?? "",
      /05:00/,
    );
    // Adding it lists it.
    await drawer.locator('[data-drive="save"]').click();
    await page.waitForTimeout(800);
    assert.equal(await page.locator("dialog.sch-drawer[open]").count(), 0);
    assert.equal(await page.locator("tr[data-id]").count(), 2);
    await clearSchedules(page);
  } finally {
    await browser.close();
  }
});

test("invariants: Schedules page: with no schedules it offers three templates, each opening the drawer filled in", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1100 },
      timezoneId: "Europe/Brussels",
    });
    const page = await freshPage(context);
    await clearSchedules(page);
    await page.goto(`${BASE}/schedules`);
    const cards = page.locator(".sch-tpl");
    await cards.first().waitFor({ timeout: 10000 });
    assert.equal(await cards.count(), 3);
    assert.equal(await page.locator(".sch-week").count(), 0);
    await cards.nth(1).click();
    const drawer = page.locator("dialog.sch-drawer[open]");
    await drawer.waitFor({ timeout: 3000 });
    assert.equal(await drawer.locator("#sched-action").inputValue(), "patch");
    assert.equal(await drawer.locator("#sched-every").inputValue(), "week");
    assert.equal(await drawer.locator("#sched-at").inputValue(), "22:00");
    assert.equal(await drawer.locator(".sch-preview [data-slot]").count(), 3);
  } finally {
    await browser.close();
  }
});

test("invariants: Schedules page: Live view reaches the drawer and its fields, the row menu, the switch and Undo", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
      timezoneId: "Europe/Brussels",
    });
    const page = await freshPage(context);
    await clearSchedules(page);
    const id = await addSchedule(page, "films", "backup", {
      every: "day",
      at: "10:00",
    });
    await page.goto(`${BASE}/schedules`);
    const row = page.locator(`tr[data-id="${id}"]`);
    await row.waitFor({ timeout: 10000 });
    await page.check("#live-view");
    const step = (/** @type {any} */ s) =>
      page.evaluate(
        (body) =>
          fetch("/data/drive/demo-step", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(body),
          }).then((r) => r.json()),
        s,
      );
    try {
      let r = await step({ do: "click", control: "new-schedule" });
      assert.ok(r.ok, `new-schedule: ${JSON.stringify(r.refusal)}`);
      await page.locator("dialog.sch-drawer[open]").waitFor({ timeout: 3000 });
      r = await step({ do: "pick", field: "sched-every", value: "day" });
      assert.ok(r.ok, `pick sched-every: ${JSON.stringify(r.refusal)}`);
      assert.equal(await page.locator("#sched-every").inputValue(), "day");
      r = await step({ do: "press", button: "cancel" });
      assert.ok(r.ok, `press cancel: ${JSON.stringify(r.refusal)}`);
      assert.equal(await page.locator("dialog.sch-drawer[open]").count(), 0);
      r = await step({ do: "click", control: "schedule-menu", row: id });
      assert.ok(r.ok, `schedule-menu: ${JSON.stringify(r.refusal)}`);
      await page.locator("dialog.nx-rowmenu[open]").waitFor({ timeout: 3000 });
      r = await step({ do: "press", button: "delete" });
      assert.ok(r.ok, `press delete: ${JSON.stringify(r.refusal)}`);
      assert.equal(await row.isVisible(), false, "the deleted row still shows");
      r = await step({ do: "click", control: "undo-schedule-change" });
      assert.ok(r.ok, `undo: ${JSON.stringify(r.refusal)}`);
      await row.waitFor({ timeout: 3000 });
      r = await step({ do: "click", control: "toggle-schedule", row: id });
      assert.ok(r.ok, `toggle: ${JSON.stringify(r.refusal)}`);
      await page.waitForTimeout(400);
      assert.equal(
        await row.evaluate((x) => x.classList.contains("off")),
        true,
      );
      r = await step({ do: "click", control: "undo-schedule-change" });
      assert.ok(r.ok, `undo the switch: ${JSON.stringify(r.refusal)}`);
      await page.waitForTimeout(400);
      assert.equal(
        await row.evaluate((x) => x.classList.contains("off")),
        false,
      );
    } finally {
      await step({ do: "done" });
    }
    await clearSchedules(page);
  } finally {
    await browser.close();
  }
});

// fix-233 (the 3.70.6 visual pass, proven here for the 3.71.0 Go): at phone
// width a stack's tabs ran off the edge ("Ch…" cut, Settings out of reach).
// The tab row scrolls sideways inside itself, every tab stays whole, and
// the page itself never scrolls sideways.
test("invariants: on a 390 px phone a stack's tab row scrolls inside itself with every tab whole, never the page", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 390, height: 900 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/stacks/kp-soft`);
    await page.locator(".kp-tabs__list .kp-tab").first().waitFor();
    const r = await page.evaluate(() => {
      const list = /** @type {HTMLElement} */ (
        document.querySelector(".kp-tabs__list")
      );
      const tabs = [...list.querySelectorAll(".kp-tab")].map((t) => ({
        name: (t.textContent ?? "").trim(),
        cut: t.scrollWidth > t.clientWidth + 1,
      }));
      return {
        overflowX: getComputedStyle(list).overflowX,
        wider: list.scrollWidth > list.clientWidth + 1,
        tabs,
        pageOverflow: document.documentElement.scrollWidth - window.innerWidth,
      };
    });
    assert.ok(r.tabs.length >= 4, `only ${r.tabs.length} tabs found`);
    assert.deepEqual(
      r.tabs.filter((t) => t.cut).map((t) => t.name),
      [],
      "tabs cut short",
    );
    if (r.wider)
      assert.ok(
        ["auto", "scroll"].includes(r.overflowX),
        `the tab row is wider than the phone but its overflow-x is ${r.overflowX}`,
      );
    assert.ok(
      r.pageOverflow <= 1,
      `the page scrolls sideways by ${r.pageOverflow} px`,
    );
    await context.close();
  } finally {
    await browser.close();
  }
});

/**
 * A page at phone width in one theme.
 * @param {import("playwright").Browser} browser @param {string} theme
 */
async function phonePage(browser, theme) {
  const context = await browser.newContext({
    viewport: { width: 390, height: 844 },
  });
  await context.addInitScript((t) => {
    try {
      localStorage.setItem("theme", t);
    } catch {}
  }, theme);
  return { context, page: await freshPage(context) };
}

test("invariants: redesign-kit: a narrow page header reads the title with its live status, the description, then the actions", async () => {
  // redesign-kit-16: both header shapes a page shows at 390 px — the
  // plain one (Backups) and the meta one (Host; Schedules, drawn as
  // Activity's Planned view: the live status in the meta row under the
  // description) — read title, description, actions.
  const browser = await launch();
  try {
    for (const theme of ["light", "dark"])
      for (const path of ["/backups", "/host", "/schedules"]) {
        const { context, page } = await phonePage(browser, theme);
        await page.goto(`${BASE}${path}`);
        // Schedules sits under Activity's own title since the 3.71.0 merge:
        // its header is the one inside `.sch-root`, its title an h2.
        const sel =
          path === "/schedules" ? "main .sch-root .nx-head" : "main .nx-head";
        await page
          .locator(`${sel} :is(h1, h2)`)
          .first()
          .waitFor({ timeout: 10000 });
        const order = await page.evaluate((sel) => {
          const head = /** @type {HTMLElement} */ (document.querySelector(sel));
          const top = (/** @type {string} */ q) =>
            head.querySelector(q)?.getBoundingClientRect().top ?? -1;
          return {
            meta: head.classList.contains("nx-head--meta"),
            title: top(":is(h1, h2)"),
            titleBottom:
              head.querySelector(":is(h1, h2)")?.getBoundingClientRect()
                .bottom ?? -1,
            live: top(".nx-live"),
            desc: top(".nx-head-desc"),
            metaRow: top(".nx-head-meta"),
            actions: top(".nx-head-actions"),
          };
        }, sel);
        const where = `${theme} ${path}`;
        // A meta header may carry its freshness as a chip of its own
        // (Schedules' "read 4 s ago") instead of a live status.
        for (const k of [
          "title",
          "titleBottom",
          "desc",
          "actions",
          ...(order.meta ? [] : ["live"]),
        ])
          assert.ok(
            /** @type {Record<string, number>} */ (order)[k] >= 0,
            `${where}: the header has no ${k}`,
          );
        assert.ok(
          order.titleBottom <= order.desc && order.desc < order.actions,
          `${where}: the narrow header is not title, description, actions: ${JSON.stringify(order)}`,
        );
        if (order.meta)
          assert.ok(
            order.desc < order.metaRow &&
              order.metaRow < order.actions &&
              (order.live < 0 ||
                (order.metaRow <= order.live && order.live < order.actions)),
            `${where}: the meta row with the live status is not between the description and the actions: ${JSON.stringify(order)}`,
          );
        else
          assert.ok(
            order.title < order.live && order.live < order.titleBottom,
            `${where}: the live status is not beside the title: ${JSON.stringify(order)}`,
          );
        await context.close();
      }
  } finally {
    await browser.close();
  }
});

test("invariants: redesign-kit: a bad attention item is a soft red tint with a red border and the page's text colour", async () => {
  const browser = await launch();
  try {
    for (const theme of ["light", "dark"]) {
      const { context, page } = await phonePage(browser, theme);
      await page.goto(`${BASE}/backups`);
      await page.locator("main .nx-head h1").waitFor({ timeout: 10000 });
      const att = await page.evaluate(async () => {
        const ui = await import("/js/ui.js");
        const band = ui.attentionBand([
          { tone: "bad", title: "Two stacks missed", text: "a test item" },
        ]);
        document.querySelector("main")?.prepend(band.el);
        const item = /** @type {HTMLElement} */ (
          band.el.querySelector(".nx-attention__item")
        );
        const probe = document.createElement("span");
        probe.style.color = "var(--foreground)";
        probe.style.backgroundColor = "var(--destructive)";
        document.body.append(probe);
        const p = getComputedStyle(probe);
        const s = getComputedStyle(item);
        const out = {
          bg: s.backgroundColor,
          fg: s.color,
          border: s.borderTopColor,
          solid: p.backgroundColor,
          text: p.color,
        };
        probe.remove();
        band.el.remove();
        return out;
      });
      assert.notEqual(att.bg, att.solid, `${theme}: a bad item is solid red`);
      assert.equal(
        att.fg,
        att.text,
        `${theme}: a bad item's text is not the page's text colour`,
      );
      assert.notEqual(
        att.border,
        att.bg,
        `${theme}: a bad item has no border of its own`,
      );
      await context.close();
    }
  } finally {
    await browser.close();
  }
});

test("invariants: redesign-kit: a row menu opened near the foot of the screen sits above its button and leaves the page where it was", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
      timezoneId: "Europe/Brussels",
    });
    const page = await freshPage(context);
    await clearSchedules(page);
    await addSchedule(page, "films", "backup", { every: "day", at: "10:00" });
    const id = await addSchedule(page, "notes", "backup", {
      every: "day",
      at: "11:00",
    });
    await page.goto(`${BASE}/schedules`);
    const sel = `tr[data-id="${id}"] [data-drive="schedule-menu"]`;
    await page.locator(sel).waitFor({ timeout: 10000 });
    // Put the button near the foot, where its menu cannot fit under it.
    await page.evaluate((q) => {
      const r = /** @type {HTMLElement} */ (
        document.querySelector(q)
      ).getBoundingClientRect();
      scrollBy(0, r.bottom - innerHeight + 40);
    }, sel);
    const before = await page.evaluate(() => scrollY);
    await page.locator(sel).click();
    await page.locator("dialog.nx-rowmenu[open]").waitFor({ timeout: 3000 });
    const placed = await page.evaluate((q) => {
      const b = /** @type {HTMLElement} */ (document.querySelector(q));
      const m = /** @type {HTMLElement} */ (
        document.querySelector("dialog.nx-rowmenu[open]")
      );
      return {
        scrollY,
        button: b.getBoundingClientRect().top,
        menuTop: m.getBoundingClientRect().top,
        menuBottom: m.getBoundingClientRect().bottom,
      };
    }, sel);
    assert.equal(
      placed.scrollY,
      before,
      "opening the row menu scrolled the page",
    );
    assert.ok(
      placed.menuBottom <= placed.button && placed.menuTop >= 0,
      `the row menu is not above its button on the screen: ${JSON.stringify(placed)}`,
    );
    await page.keyboard.press("Escape");
    await clearSchedules(page);
  } finally {
    await browser.close();
  }
});

test("invariants: redesign-kit-17: no page scrolls sideways at 390 px, and no table's box overflows its card", async () => {
  const browser = await launch();
  try {
    const { context, page } = await phonePage(browser, "light");
    /** @type {string[]} */
    const bad = [];
    for (const path of [
      "/",
      "/inbox",
      "/stacks",
      "/stacks/films",
      "/stacks/films/settings",
      "/activity",
      "/backups",
      "/host",
      "/schedules",
      "/secrets",
      "/charts",
      "/fleetview",
      "/firewall",
      "/settings",
      "/presets",
      "/system/notifications",
      "/jobs",
      "/apply",
      "/doctor",
    ]) {
      await page.goto(`${BASE}${path}`);
      const shown = await page
        .locator("#page")
        .waitFor({ state: "attached", timeout: 10000 })
        .then(() => true)
        .catch(() => false);
      if (!shown) {
        bad.push(`${path}: no page drawn (${page.url()})`);
        continue;
      }
      // Wait for the page's reads to land (each has its own deadline).
      await page
        .waitForFunction(
          () => !document.querySelector('#page [data-kp-state="loading"]'),
          null,
          { timeout: 8000 },
        )
        .catch(() => {});
      const r = await page.evaluate(() => ({
        over:
          document.documentElement.scrollWidth -
          document.documentElement.clientWidth,
        wraps: [
          ...document.querySelectorAll(
            "main .kp-table-wrap, main .bk-table-wrap",
          ),
        ]
          .map((w) => w.scrollWidth - w.clientWidth)
          .filter((x) => x > 1),
      }));
      if (r.over > 0)
        bad.push(`${path}: the page scrolls sideways by ${r.over} px`);
      if (r.wraps.length)
        bad.push(
          `${path}: a table overflows its box by ${r.wraps.join(", ")} px`,
        );
    }
    assert.deepEqual(bad, [], "at 390 px");
    await context.close();
  } finally {
    await browser.close();
  }
});

test("invariants: redesign-kit-13: a row menu's right edge lines up with its button's", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await clearSchedules(page);
    const id = await addSchedule(page, "films", "backup", {
      every: "day",
      at: "10:00",
    });
    await page.goto(`${BASE}/schedules`);
    const sel = `tr[data-id="${id}"] [data-drive="schedule-menu"]`;
    await page.locator(sel).waitFor({ timeout: 10000 });
    await page.locator(sel).click();
    await page.locator("dialog.nx-rowmenu[open]").waitFor({ timeout: 3000 });
    const r = await page.evaluate((q) => {
      const b = /** @type {HTMLElement} */ (document.querySelector(q));
      const m = /** @type {HTMLElement} */ (
        document.querySelector("dialog.nx-rowmenu[open]")
      );
      return {
        button: b.getBoundingClientRect().right,
        menu: m.getBoundingClientRect().right,
        width: m.getBoundingClientRect().width,
        expanded: b.getAttribute("aria-expanded"),
      };
    }, sel);
    assert.ok(
      Math.abs(r.button - r.menu) <= 1,
      `the menu ends ${r.menu - r.button} px past its button: ${JSON.stringify(r)}`,
    );
    assert.equal(r.width, 260, "the menu is the width it plans for");
    assert.equal(r.expanded, "true");
    await page.locator(sel).click();
    assert.equal(
      await page.locator("dialog.nx-rowmenu[open]").count(),
      0,
      "a second click on the button closes the menu",
    );
    await clearSchedules(page);
  } finally {
    await browser.close();
  }
});

test("invariants: redesign-kit-10: Esc on Host shows every container again, and its sorted headers are Live view controls that keep their sort", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/host`);
    const running = page.locator(
      '[data-drive="container-filter"][data-drive-row="running"]',
    );
    await running.waitFor({ timeout: 10000 });
    await running.click();
    assert.equal(await running.getAttribute("aria-pressed"), "true");
    await page.locator("main h1").click();
    await page.keyboard.press("Escape");
    assert.equal(
      await page
        .locator('[data-drive="container-filter"][data-drive-row="all"]')
        .getAttribute("aria-pressed"),
      "true",
      "Esc did not reset the status filter",
    );
    const th = (/** @type {string} */ row) =>
      page.locator(
        `th[data-drive="sort-host-containers"][data-drive-row="${row}"]`,
      );
    await th("memory").click();
    await th("id").click({ modifiers: ["Shift"] });
    assert.equal(await th("memory").getAttribute("aria-sort"), "ascending");
    assert.equal(await th("id").getAttribute("aria-sort"), "ascending");
    await page.reload();
    await th("memory").waitFor({ timeout: 10000 });
    assert.equal(
      await th("memory").getAttribute("aria-sort"),
      "ascending",
      "the Containers table forgot its sort",
    );
    assert.equal(await th("id").getAttribute("data-mark"), "↑2");
    await page.evaluate(() =>
      localStorage.removeItem("nx-sort:host-containers"),
    );
  } finally {
    await browser.close();
  }
});

test("invariants: redesign-kit-18: Backups keeps its KPI context whole at 390 px, its search hint inside the box at 1894 px, and the newest snapshot's age in one phrase", async () => {
  const browser = await launch();
  try {
    {
      const { context, page } = await phonePage(browser, "light");
      await page.goto(`${BASE}/backups`);
      await page
        .locator('.nx-kpi[data-key="newest"]:not([data-loading])')
        .waitFor({ timeout: 10000 });
      const cut = await page.evaluate(() =>
        [...document.querySelectorAll("main .nx-kpi__ctx")]
          .filter((c) => {
            const box = c.getBoundingClientRect();
            return [...c.querySelectorAll("*")].some(
              (k) => k.getBoundingClientRect().right > box.right + 1,
            );
          })
          .map((c) => c.textContent),
      );
      assert.deepEqual(cut, [], "a KPI context line is cut off at 390 px");
      await context.close();
    }
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/backups`);
    await page.locator("#bk-filter").waitFor({ timeout: 10000 });
    const r = await page.evaluate(() => {
      const box = /** @type {HTMLElement} */ (
        document.querySelector("#bk-filter")
      ).getBoundingClientRect();
      const kbd = /** @type {HTMLElement} */ (
        document.querySelector("#bk-repos .nx-card__tools .nx-kbd")
      ).getBoundingClientRect();
      return {
        box: [box.top, box.bottom, box.right],
        kbd: [kbd.top, kbd.bottom, kbd.right],
      };
    });
    assert.ok(
      r.kbd[0] >= r.box[0] && r.kbd[1] <= r.box[1] && r.kbd[2] <= r.box[2],
      `the / hint is not inside the search box: ${JSON.stringify(r)}`,
    );
    await page
      .locator('.nx-kpi[data-key="newest"]:not([data-loading])')
      .waitFor({ timeout: 10000 });
    const tile = await page.evaluate(() => ({
      value:
        document.querySelector('.nx-kpi[data-key="newest"] .nx-kpi__value')
          ?.textContent ?? "",
      ctx:
        document.querySelector('.nx-kpi[data-key="newest"] .nx-kpi__ctx')
          ?.textContent ?? "",
    }));
    assert.match(
      tile.value,
      /\bago$/,
      `the age is cut in two: ${JSON.stringify(tile)}`,
    );
    assert.doesNotMatch(tile.ctx, /^ago\b/);
  } finally {
    await browser.close();
  }
});

// drive-reach (Kenny, 2026-10-03: "Claude must always be able to reach
// every control, also after a control is renamed or moved"; he kept seeing
// "there is no control X on screen" in Live view). The three cases below
// iterate the catalog the running dashboard serves (`/data/drive/controls`,
// generated from drivable.js declarations and the router), so a page that
// merges later is covered without touching this file.

/**
 * One Live view step through the demo host's driver: the same
 * `Driver::step` a `homelab ui` line reaches through the host relay.
 * @param {import("playwright").Page} page
 * @param {any} s
 */
const reachStep = (page, s) =>
  page.evaluate(
    (body) =>
      fetch("/data/drive/demo-step", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      }).then((r) => r.json()),
    s,
  );

/**
 * The first row key of control `id` on screen that can be pressed, waiting
 * (2.5 s at most) for the page to draw one; null when none comes.
 * @param {import("playwright").Page} page
 * @param {string} id
 */
async function firstRow(page, id) {
  const sel = `[data-drive="${id}"][data-drive-row]`;
  const h = await page
    .waitForFunction(
      (s) => {
        // redesign-integrate-8: a row ready to be pressed (on screen,
        // enabled, not busy), and one whose press changes something first:
        // the level already chosen, a tab already open, does nothing.
        const ready = [...document.querySelectorAll(s)].filter(
          (e) =>
            e.getClientRects().length > 0 &&
            !(/** @type {HTMLButtonElement} */ (e).disabled) &&
            e.getAttribute("aria-busy") !== "true",
        );
        const on = (/** @type {Element} */ e) =>
          /** @type {HTMLInputElement} */ (e).checked === true ||
          e.getAttribute("aria-pressed") === "true" ||
          e.getAttribute("aria-current") != null ||
          e.getAttribute("aria-selected") === "true";
        const pick = ready.find((e) => !on(e)) ?? ready[0];
        return pick
          ? /** @type {HTMLElement} */ (pick.dataset.driveRow ?? null)
          : null;
      },
      sel,
      { timeout: 2500 },
    )
    .catch(() => null);
  return h ? /** @type {string | null} */ (await h.jsonValue()) : null;
}

/**
 * The control catalog the RUNNING dashboard serves (`/data/drive/controls`,
 * generated from the declarations and compiled into the release), each
 * control with the address a driver sends it to: its own, or for one that
 * lives per stack its page's old address, which the router sends on.
 * @param {import("playwright").Page} page
 */
async function catalogOf(page) {
  const cat = await page.evaluate(() =>
    fetch("/data/drive/controls", {
      headers: { accept: "application/json" },
    }).then((r) => r.json()),
  );
  return {
    cat,
    /** @type {any[]} */
    all: cat.controls.map((/** @type {any} */ c) => ({
      ...c,
      // redesign-integrate-8: a control of the stack hub (no old address
      // of its own) lives at its hub tab, on the demo's kp-soft.
      home: c.href?.startsWith("/stacks/<stack>")
        ? c.href.replace("<stack>", "kp-soft").replace(/\/<[^>]+>.*$/, "")
        : !c.href || /[<{]/.test(c.href)
          ? `/${c.page}`
          : c.href,
    })),
  };
}

/**
 * Live view without its 3 s countdown per step (the demo host's
 * `/data/drive/demo-timing`); answers the function that puts it back.
 * @param {import("playwright").Page} page
 */
async function noCountdown(page) {
  const set = (/** @type {object} */ body) =>
    page.evaluate(
      (b) =>
        fetch("/data/drive/demo-timing", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(b),
        }).then((r) => r.json()),
      body,
    );
  // redesign-integrate-8: and no 420 ms press flash (a person's pace).
  const before = await set({ announce_ms: 0, press_ms: 0 });
  return () =>
    set({
      announce_ms: before.announce_ms ?? 3000,
      press_ms: before.press_ms ?? null,
    });
}

/**
 * Change requests a sweep lets through: Live view's own, staging a secret
 * (nothing is written) and the snooze (its End is pressed right after).
 */
const PASSES = [
  "/data/drive/",
  "/data/secrets/stage",
  "/data/notifications/snooze",
  // redesign-integrate-8: a stack's plan is a read (what an edit would
  // change), sent as a POST; the Update flow's step 2 waits for it.
  /^\/data\/stacks\/[^/]+\/plan$/,
];

/**
 * Hold back every other change request, so nothing a sweep presses changes
 * the demo for the cases after it (the press itself still happened); `seen`
 * hears each one held back.
 * @param {import("playwright").Page} page
 * @param {(what: string) => void} [seen]
 */
const holdChanges = (page, seen) =>
  page.route("**/data/**", (r) => {
    const req = r.request();
    const url = new URL(req.url());
    if (
      req.method() !== "GET" &&
      !PASSES.some((x) =>
        typeof x === "string"
          ? url.pathname.startsWith(x)
          : x.test(url.pathname),
      )
    ) {
      seen?.(`${req.method()} ${url.pathname}`);
      return r.abort();
    }
    return r.continue();
  });

/**
 * Wait (8 s at most) until the page is newly drawn: the content `markOld`
 * marked is gone, a title is there, and no skeleton is left.
 * @param {import("playwright").Page} page
 */
const drawn = (page) =>
  page
    .waitForFunction(
      () => {
        const p = document.getElementById("page");
        if (!p || p.querySelector("[data-sweep-old]")) return false;
        const busy = [...p.querySelectorAll(".kp-skeleton")].some(
          (e) => e.getClientRects().length > 0,
        );
        return !!p.querySelector("h1") && !busy;
      },
      null,
      { timeout: 8000 },
    )
    .catch(() => {});

/**
 * Mark what the page shows now, so `drawn` sees it replaced.
 * @param {import("playwright").Page} page
 */
const markOld = (page) =>
  page.evaluate(() =>
    [...(document.getElementById("page")?.children ?? [])].forEach((e) =>
      e.setAttribute("data-sweep-old", ""),
    ),
  );

/**
 * `homelab ui goto <path>` through the demo host's driver, then wait until
 * the tab has drawn it, freshly even when it is there now (it goes to
 * /apps first), so no state an earlier press left carries over.
 * @param {import("playwright").Page} page
 * @param {string} path
 * @param {Map<string, string>} landed where each path ended up last time
 * @returns {Promise<any>} the step's answer
 */
async function liveGo(page, path, landed) {
  const u = new URL(page.url());
  if (
    path !== "/apps" &&
    (landed.get(path) === page.url() || u.pathname + u.search === path)
  )
    await liveGo(page, "/apps", landed);
  await markOld(page);
  const r = await reachStep(page, { do: "goto", path });
  if (!r.ok) return r;
  await drawn(page);
  landed.set(path, page.url());
  return r;
}

/** Close every dialog a press left open. @param {import("playwright").Page} page */
/**
 * redesign-integrate-8: what is still open on the screen: a dialog, a
 * menu (its button says it is expanded), a popover.
 * @param {import("playwright").Page} page
 * @returns {Promise<string[]>}
 */
const stillOpen = (page) =>
  page.evaluate(() =>
    [
      ...document.querySelectorAll(
        'dialog[open], [aria-haspopup][aria-expanded="true"], :popover-open',
      ),
    ]
      // Live view's own chrome (its cursor, banner, plan) is not the page's.
      .filter(
        (e) =>
          !e.closest(
            ".drive-banner, .drive-announce, .drive-plan, .drive-cursor, #follow",
          ),
      )
      .map(
        (e) =>
          `${e.tagName.toLowerCase()}${e.id ? `#${e.id}` : ""}${/** @type {HTMLElement} */ (e).dataset.drive ? `[${/** @type {HTMLElement} */ (e).dataset.drive}]` : ""}.${String(e.className).split(" ")[0]}`,
      ),
  );

/**
 * A seeded shuffle (mulberry32): the same seed, the same order.
 * @param {number} seed
 */
const shuffled = (seed) => {
  let a = seed >>> 0;
  const next = () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  /** @template T @param {T[]} xs @returns {T[]} */
  return (xs) => {
    const out = [...xs];
    for (let i = out.length - 1; i > 0; i--) {
      const j = Math.floor(next() * (i + 1));
      [out[i], out[j]] = [out[j], out[i]];
    }
    return out;
  };
};

const closeAll = (page) =>
  page.evaluate(() =>
    document
      .querySelectorAll("dialog[open]")
      .forEach((d) => /** @type {HTMLDialogElement} */ (d).close()),
  );

test("invariants: drive-reach: every button that opens a dialog or runs an action is reachable through Live view (the walk of every route)", async (t) => {
  const started = Date.now();
  const { default: SPEC } = await import("../js/formspec.json", {
    with: { type: "json" },
  });
  const forms = new Set(SPEC.forms);
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
      timezoneId: "Europe/Brussels",
    });
    // One page for the whole sweep: every route and every control reuse it.
    const page = await freshPage(context);
    page.setDefaultTimeout(10000);
    /**
     * A route on page `p`, opened directly (Live view off), once drawn.
     * @param {import("playwright").Page} p
     * @param {string} href
     */
    const show = async (p, href) => {
      await markOld(p);
      await p.goto(`${BASE}${href}`, { waitUntil: "domcontentloaded" });
      await drawn(p);
    };
    const dialogs = () => page.$$eval("dialog[open]", (d) => d.length);

    // 1. Every route: a button that opens a dialog or sends a change must
    // carry a Live view mark: a declared control or a server form. Only
    // the unmarked buttons are clicked; the declared ones are driven in 2.
    // Three pages walk the routes side by side, each reused route after
    // route (Live view is off here, so they do not meet).
    const routes = [
      ...Object.entries(PATH_TO_PAGE)
        .filter(([, p]) => p !== "retired")
        .map(([k]) => `/${k}`),
      ...["", "/apps", "/logs", "/backups", "/history", "/settings"].map(
        (x) => `/stacks/beta-demo${x}`,
      ),
    ];
    const SEL =
      "#page button:not([disabled]), #page [role=button], #page [role=switch]:not([disabled])";
    /** @type {string[]} */
    const unreachable = [];
    let looked = 0;
    const queue = [...routes];
    /** @param {import("playwright").Page} p */
    const walk = async (p) => {
      /** @type {string[]} change requests held back since the last look */
      const changes = [];
      /** @type {(() => void) | null} */
      let heard = null;
      await holdChanges(p, (what) => {
        changes.push(what);
        heard?.();
      });
      /** What a click did: a dialog or a change request, 400 ms at most. */
      const effect = async (/** @type {number} */ open0) => {
        const sent = new Promise((r) => (heard = () => r(null)));
        await Promise.race([
          sent,
          p
            .waitForFunction(
              (n) => document.querySelectorAll("dialog[open]").length > n,
              open0,
              { timeout: 400 },
            )
            .catch(() => {}),
        ]);
        heard = null;
        const titles = await p.$$eval("dialog[open]", (d) =>
          d.map((x) =>
            (x.querySelector(".kp-dialog__title")?.textContent ?? "").trim(),
          ),
        );
        return [
          ...titles.slice(open0).map((x) => `opens "${x}"`),
          ...changes.map((c) => `sends ${c}`),
        ].join(", ");
      };
      const unmarked = () =>
        p.$$eval(
          SEL,
          (els, known) =>
            els
              .map((e, i) => ({ e: /** @type {HTMLElement} */ (e), i }))
              .filter(
                ({ e }) =>
                  e.getClientRects().length > 0 &&
                  !e.closest("dialog") &&
                  !e.dataset.drive &&
                  !(
                    e.dataset.action ??
                    /** @type {HTMLElement | null} */ (
                      e.closest("[data-drive-form]")
                    )?.dataset.driveForm
                  )
                    ?.split(" ")
                    .some((f) => known.includes(f)),
              )
              .map(({ e, i }) => ({
                i,
                label: (e.getAttribute("aria-label") || e.textContent || "")
                  .trim()
                  .replace(/\s+/g, " ")
                  .slice(0, 60),
              })),
          [...forms],
        );
      for (let route = queue.shift(); route; route = queue.shift()) {
        await show(p, route);
        const here = p.url();
        const seen = new Set();
        for (;;) {
          const next = (await unmarked()).find((x) => !seen.has(x.label));
          if (!next) break;
          seen.add(next.label);
          looked += 1;
          changes.length = 0;
          const open0 = await p.$$eval("dialog[open]", (d) => d.length);
          await p
            .locator(SEL)
            .nth(next.i)
            .click({ timeout: 1000 })
            .catch(() => {});
          const did = await effect(open0);
          await closeAll(p);
          if (did) unreachable.push(`${route}: "${next.label}" ${did}`);
          if (p.url() !== here || changes.length) await show(p, route);
        }
      }
    };
    const helpers = [await context.newPage(), await context.newPage()];
    await Promise.all([page, ...helpers].map(walk));
    await Promise.all(helpers.map((p) => p.close()));
    const step1 = Math.round((Date.now() - started) / 1000);
    assert.deepEqual(
      unreachable,
      [],
      `buttons Live view cannot reach (declare them in drivable.js, or mark the form that reaches them): ${unreachable.join("; ")}`,
    );
    t.diagnostic(
      `looked at ${looked} unmarked buttons on ${routes.length} routes in ${step1} s`,
    );
  } finally {
    await browser.close();
  }
});

/**
 * redesign-integrate-8: keep the page's live channel (store.js's one
 * EventSource) at `window.__sweepLive`, so a sweep state can deliver an
 * event the way the dashboard would. An init script: it runs before the
 * page's own modules.
 */
function catchLive() {
  const ES = window.EventSource;
  /** @type {any} */ (window).EventSource = class extends ES {
    /** @param {any[]} a */
    constructor(...a) {
      // @ts-ignore spread of the constructor's own arguments
      super(...a);
      /** @type {any} */ (window).__sweepLive = this;
    }
  };
}

/**
 * Deliver one live event to the page as the dashboard's channel would.
 * @param {import("playwright").Page} p
 * @param {string} name
 * @param {any} data
 */
const liveEvent = (p, name, data) =>
  p.evaluate(
    ([n, d]) =>
      /** @type {any} */ (window).__sweepLive?.dispatchEvent(
        new MessageEvent(n, { data: JSON.stringify(d) }),
      ),
    /** @type {[string, any]} */ ([name, data]),
  );

/** The made-up job a sweep state shows, and the request it ran as. */
const SWEEP_JOB = 990001;
const SWEEP_REQ = 990001;

/**
 * A job this tab is told about (an `action` event), running or ended:
 * Running now, Every job and the lines it ran as show it; nothing is sent.
 * @param {import("playwright").Page} p
 * @param {string} state
 * @param {number[]} reqs
 */
const sweepJob = (p, state, reqs) => {
  const now = Math.floor(Date.now() / 1000);
  return liveEvent(p, "action", {
    job: SWEEP_JOB,
    origin: { from: "manual" },
    stack: "films",
    action: "backup",
    args: {},
    state,
    queued_at: now - 5,
    started_at: now - 4,
    finished_at: state === "running" ? null : now,
    reqs,
    finished: state !== "running",
    changed: false,
    expected_step_s: null,
    progress: null,
  });
};

/** A read answered with 502 for this one press. @param {string} url */
const failRead =
  (url) =>
  /** @param {import("playwright").Page} p */
  async (p) => {
    await p.route(url, (r) =>
      r.request().method() === "GET"
        ? r.fulfill({
            status: 502,
            json: {
              what: "the read",
              why: "the demo read fails on purpose for the sweep",
              fix: "Try again",
            },
          })
        : r.fallback(),
    );
    return () => p.unroute(url);
  };

/** Activity's controls that show only in some state. */
/** The answer of a GET `url` changed by `edit` for this one press. */
const editRead =
  (/** @type {string} */ url, /** @type {(b: any) => void} */ edit) =>
  /** @param {import("playwright").Page} p */
  async (p) => {
    await p.route(url, async (r) => {
      if (r.request().method() !== "GET") return r.fallback();
      const res = await r.fetch();
      const body = await res.json();
      edit(body);
      await r.fulfill({ response: res, json: body });
    });
    return () => p.unroute(url);
  };

const NOTHING = "sweep-matches-nothing";
const nowS = () => Math.floor(Date.now() / 1000);

/** One open manual check on kp-soft (`/data/manual-checks`). */
const openCheck = editRead("**/data/manual-checks", (b) => {
  b.report = {
    ...(b.report ?? {}),
    now: nowS(),
    checks: [
      {
        id: "sweep-check",
        record: {
          stack: "kp-soft",
          app: "kp-soft",
          text: "the sweep's manual check: does the app answer?",
          registered_at: nowS() - 3600,
        },
      },
    ],
  };
});

/** A dashboard reached over HTTPS, with one passkey kept. */
const passkeys = editRead("**/api/kit/passkeys", (b) => {
  b.https = true;
  b.passkeys = [
    {
      id: "sweep",
      label: "sweep key",
      created_at: "2026-10-01 10:00",
      last_used_at: null,
    },
  ];
});

/**
 * Deploy all changes: type the first gone stack's name (Live view's own
 * type step), and tick the destroy's confirmation when `tick`.
 * @param {boolean} tick
 */
const armDestroy =
  (tick) => async (/** @type {import("playwright").Page} */ p) => {
    // The panel draws its plan after its own read.
    await p
      .waitForSelector('[id^="deploy-all-destroy-name-"]', { timeout: 8000 })
      .catch(() => {});
    // The destroy runs after the deploys: leave every deploy out first.
    if (tick) {
      const picks = await p.$$eval(
        '[data-drive="deploy-all-pick"][data-drive-row]',
        (els) =>
          els
            .filter((e) => /** @type {HTMLInputElement} */ (e).checked)
            .map((e) => /** @type {HTMLElement} */ (e).dataset.driveRow ?? ""),
      );
      for (const row of picks)
        await reachStep(p, { do: "click", control: "deploy-all-pick", row });
    }
    const field = await p.evaluate(
      () =>
        document.querySelector('[id^="deploy-all-destroy-name-"]')?.id ?? null,
    );
    if (!field) return;
    await reachStep(p, {
      do: "type",
      field,
      text: field.slice("deploy-all-destroy-name-".length),
    });
    if (tick)
      await reachStep(p, {
        do: "click",
        control: "deploy-all-destroy-confirm",
      });
  };

/**
 * The Update flow's update-apps job, as the live channel tells it: one
 * major move of beta-demo's api, committed.
 * @param {import("playwright").Page} p
 * @param {string} state
 */
const updateJob = async (p, state, ended = nowS()) => {
  const now = nowS();
  await liveEvent(p, "action", {
    job: SWEEP_JOB + 1,
    origin: { from: "manual" },
    stack: "_host",
    action: "update-apps",
    args: {},
    state,
    queued_at: now - 30,
    started_at: now - 29,
    finished_at: state === "running" ? null : ended,
    reqs: [],
    finished: state !== "running",
    changed: state === "done",
    expected_step_s: null,
    progress: null,
    flow: {
      step: state === "running" ? 4 : 6,
      items: [
        {
          kind: "pin",
          stack: "beta-demo",
          key: "api/api",
          from: "ghcr.io/example/api:v2.3.0",
          to: "ghcr.io/example/api:v3.0.0",
        },
      ],
      rows: [],
      commits: state === "done" ? ["0123456789ab"] : [],
    },
  });
  await p.waitForTimeout(300);
};
/** An undo that ends the made-up update a day ago. @param {import("playwright").Page} p */
const updateGone = async (p) => async () =>
  void (await updateJob(p, "done", nowS() - 86400));
/** The flow adopts the running update, which then ends. @param {import("playwright").Page} p */
const updateRan = async (p) => {
  await updateJob(p, "running");
  await updateJob(p, "done");
};

/**
 * A person stages one host.toml value on Settings, with Live view off for
 * those clicks (under Live view the page stages on the dashboard's
 * server); Live view goes on again after.
 * @param {import("playwright").Page} p
 */
const personStages = async (p) => {
  await p.uncheck("#live-view");
  try {
    if (!(await p.$("#key-backup-hour")))
      await p.click(
        '[data-drive="settings-edit"][data-drive-row="backup_hour"]',
      );
    await p.fill("#key-backup-hour", "23", { timeout: 3000 });
    await p.click("#key-stage");
  } finally {
    await p.check("#live-view");
  }
};

/**
 * redesign-integrate-8: the sweep's named states, one fixture each, shared
 * by every control that shows in it (SHOWN_IN): an address that draws it,
 * a read answered differently for the press (`set`, undone after it), or
 * what the live channel or a person brings after the reach steps
 * (`ready`). Each was checked against the product by hand first.
 * @type {Record<string, {home?: string, set?: (p: import("playwright").Page) => Promise<() => Promise<void>>, ready?: (p: import("playwright").Page) => Promise<void>}>}
 */
const FIXTURES = {
  // Reads that fail: the page's own Try again.
  "history-unread": { set: failRead("**/data/history?*") },
  "host-log-unread": { set: failRead("**/data/host-log") },
  "firewall-unread": { set: failRead("**/data/firewall") },
  "presets-unread": { set: failRead("**/data/presets") },
  "settings-unread": { set: failRead("**/data/host-settings") },
  // Its own address, so the page is drawn anew under the failed read.
  "apps-unread": { home: "/apps?sweep=retry", set: failRead("**/data/tiles") },
  "loki-unread": {
    home: "/stacks/kp-soft/logs",
    set: failRead("**/data/logs?*"),
  },
  // A search or filter that matches nothing, from the page's address.
  "activity-nothing": { home: `/activity?q=${NOTHING}` },
  "stacks-nothing": { home: `/stacks?q=${NOTHING}` },
  "firewall-nothing": { home: `/firewall?q=${NOTHING}` },
  "presets-nothing": { home: `/presets?q=${NOTHING}` },
  "settings-nothing": { home: `/settings?q=${NOTHING}` },
  "apps-nothing": { home: `/apps?q=${NOTHING}` },
  "activity-days-picked": { home: "/activity?from=1&to=4102444800" },
  // A job of this dashboard, as the live channel tells it.
  "job-running": {
    set: async (p) => {
      await sweepJob(p, "running", []);
      return async () => void (await sweepJob(p, "done", []));
    },
  },
  "job-ran": {
    set: async (p) => {
      await sweepJob(p, "done", []);
      return async () => {};
    },
  },
  // A person's failed update (Failed's first row) ran as the job's request.
  "history-op-of-a-job": {
    home: "/activity?show=failed",
    set: async (p) => {
      const reqs = await p.evaluate(() =>
        fetch("/data/history?since=0&limit=5000")
          .then((r) => r.json())
          .then((b) =>
            (b.report?.entries ?? [])
              .map((/** @type {any} */ e) => e.req)
              .filter((/** @type {any} */ r) => typeof r === "number"),
          ),
      );
      await sweepJob(p, "done", reqs);
      return async () => {};
    },
  },
  // A host line that arrives while the tail is paused.
  "host-line-arrives": {
    ready: async (p) => {
      const last = await p.evaluate(() =>
        fetch("/data/host-log")
          .then((r) => r.json())
          .then((b) => (b.lines ?? []).at(-1) ?? null),
      );
      if (last)
        await liveEvent(p, "host_log", {
          ...last,
          seq: last.seq + 1000,
          msg: "a line that came while the tail was paused",
        });
    },
  },
  // Every host line ran as the job's request.
  "host-lines-of-a-job": {
    set: async (p) => {
      const url = "**/data/host-log";
      await p.route(url, async (r) => {
        const res = await r.fetch();
        const body = await res.json();
        for (const l of body.lines ?? []) l.req = SWEEP_REQ;
        await r.fulfill({ response: res, json: body });
      });
      await sweepJob(p, "done", [SWEEP_REQ]);
      return () => p.unroute(url);
    },
  },
  "no-presets": { set: editRead("**/data/presets", (b) => (b.presets = [])) },
  // The fleet without a stack, as the live channel brings it; the demo's
  // own fleet comes back at its next tick (5 s).
  "no-stacks": {
    ready: async (p) => {
      const f = await p.evaluate(() =>
        import("/js/store.js").then((m) => m.current().fleet),
      );
      await liveEvent(p, "fleet", { fleet: { ...f, stacks: [] } });
    },
  },
  "charts-zoomed": {
    ready: (p) =>
      p.evaluate(() =>
        import("/js/timechart.js").then((m) => {
          const t = Date.now() / 1000;
          m.pageCharts.setZoom({ from: t - 1800, to: t - 600 });
        }),
      ),
  },
  "https-with-a-passkey": { set: passkeys },
  "manual-check-open": { home: "/stacks/kp-soft", set: openCheck },
  // The Inbox's sources are read once for the whole app: read the checks
  // again under the answer, and again after it.
  "inbox-check-open": {
    set: async (p) => {
      const undo = await openCheck(p);
      return async () => {
        await undo();
        await p.evaluate(() =>
          import("/js/inboxsources.js").then((m) => m.readChecks()),
        );
      };
    },
    ready: (p) =>
      p.evaluate(() =>
        import("/js/inboxsources.js").then((m) => m.readChecks()),
      ),
  },
  // A stack that differs from its files: its row suggests Deploy.
  "stack-drifted": {
    set: editRead("**/data/drift*", (b) => {
      b.stacks = { ...(b.stacks ?? {}) };
      for (const k of ["gateway", "notes", "films", "oldstack", "admin"])
        b.stacks[k] = { ...(b.stacks[k] ?? {}), state: "changed" };
    }),
  },
  // kp-soft's own app pinned to an older release.
  "pin-stale": {
    home: "/stacks/kp-soft/apps",
    set: editRead("**/data/stale-images*", (b) => {
      b.images = [
        ...(b.images ?? []),
        {
          where_: "kp-soft/kp-soft",
          key: "kp-soft/kp-soft",
          pinned: "1.0.0",
          latest: "1.1.0",
          upstream: "",
        },
      ];
    }),
  },
  "update-moved-a-pin": {
    set: editRead("**/data/update-flows?*", (b) => {
      b.updates = [
        {
          items: [
            {
              kind: "pin",
              stack: "kp-soft",
              key: "kp-soft/kp-soft",
              from: "ghcr.io/example/kp-soft:1.0.0",
              to: "ghcr.io/example/kp-soft:1.1.0",
            },
          ],
        },
      ];
    }),
  },
  // The host asks whether an operation may go on.
  "host-asks": {
    set: async (p) => {
      await liveEvent(p, "asks", {
        asks: [
          {
            id: 1,
            boot: "sweep",
            op: "deploy-demo",
            step: "apply",
            what: "the sweep's question: may the deploy go on?",
            if_allowed: "goes on",
            if_stopped: "stops here",
            asked_at: nowS(),
            deadline: nowS() + 600,
          },
        ],
      });
      return async () => void (await liveEvent(p, "asks", { asks: [] }));
    },
  },
  // An unread notice that suggests no fix.
  "notice-without-fix": {
    set: async (p) => {
      await liveEvent(p, "notification", {
        notice: {
          id: 990001,
          at: nowS(),
          kind: "host_event",
          stack: "films",
          title: "A notice from the sweep",
          body: "Nothing to do: mark it as seen.",
          read: false,
          push: { state: "sent" },
          level: "info",
        },
      });
      return async () => {};
    },
  },
  // The Update flow's own update running, then ended. Each undo ends it a
  // day ago, so no later visit adopts it as this tab's (a tab's own update
  // shows for 30 min).
  "update-running": {
    set: updateGone,
    ready: (p) => updateJob(p, "running"),
  },
  "update-done": { set: updateGone, ready: updateRan },
  // Step 2 of the Update flow with every major version's notes ticked
  // (its own address: a fresh flow).
  "update-notes-read": {
    home: "/update?sweep=go",
    ready: async (p) => {
      const rows = await p.$$eval(
        '[data-drive="update-major-read"][data-drive-row]',
        (els) =>
          els
            .filter((e) => !(/** @type {HTMLInputElement} */ (e).checked))
            .map((e) => /** @type {HTMLElement} */ (e).dataset.driveRow ?? ""),
      );
      for (const row of rows)
        await reachStep(p, { do: "click", control: "update-major-read", row });
    },
  },
  // A change a person staged on Settings (Live view stages on the
  // dashboard's server, so the page's own staged list shows a person's).
  "settings-staged": { home: "/settings?sweep=stage", ready: personStages },
  "destroy-name-typed": { ready: armDestroy(false) },
  "destroy-armed": { ready: armDestroy(true) },
};

/**
 * redesign-integrate-8: the state each control shows in (FIXTURES).
 * @type {Record<string, string>}
 */
const SHOWN_IN = {
  "activity-retry": "history-unread",
  "console-retry": "host-log-unread",
  "host-log-retry": "host-log-unread",
  "firewall-try-again": "firewall-unread",
  "presets-try-again": "presets-unread",
  "settings-try-again": "settings-unread",
  "apps-retry": "apps-unread",
  "stack-log-retry": "loki-unread",
  "activity-clear-filters": "activity-nothing",
  "stacks-show-all": "stacks-nothing",
  "firewall-clear-rule-filter": "firewall-nothing",
  "presets-clear-search": "presets-nothing",
  "settings-clear-search": "settings-nothing",
  "apps-show-all": "apps-nothing",
  "activity-show-every-day": "activity-days-picked",
  "activity-open-job": "job-running",
  "activity-job-row": "job-ran",
  "activity-show-log": "history-op-of-a-job",
  "console-back-to-tail": "host-line-arrives",
  "host-log-back-to-tail": "host-line-arrives",
  "console-open-job": "host-lines-of-a-job",
  "host-log-open-job": "host-lines-of-a-job",
  "presets-import-first": "no-presets",
  "presets-new-preset-first": "no-presets",
  "presets-new-stack-first": "no-presets",
  "stacks-empty-new": "no-stacks",
  "chart-zoom-reset": "charts-zoomed",
  "register-passkey": "https-with-a-passkey",
  "register-passkey-go": "https-with-a-passkey",
  "register-passkey-cancel": "https-with-a-passkey",
  "delete-passkey": "https-with-a-passkey",
  "stack-check-answer": "manual-check-open",
  "inbox-answer-check": "inbox-check-open",
  "stacks-row-action": "stack-drifted",
  "stack-app-update": "pin-stale",
  "stack-undo-update": "update-moved-a-pin",
  "inbox-answer": "host-asks",
  "inbox-mark-seen": "notice-without-fix",
  "update-leave": "update-running",
  "update-roll-back": "update-done",
  "update-after": "update-done",
  "update-go": "update-notes-read",
  "settings-undo": "settings-staged",
  "settings-discard": "settings-staged",
  "settings-check-and-write": "settings-staged",
  "deploy-all-destroy-confirm": "destroy-name-typed",
  "deploy-all-destroy": "destroy-armed",
};

/**
 * drive-reach review H1: the state each control that shows only in some
 * state needs, drawn for the sweep: a read answered differently for this
 * one press (Playwright routes, undone after it) and, where the state lives
 * on one stack, the address that shows it. The demo host itself draws the
 * notification centre's unread notice with a fix (shell/demo.rs
 * `seed_unread_notice`); `holdChanges` keeps it unread.
 * redesign-integrate-8: `ready` brings what must come after the reach
 * steps (a line that arrives while the tail is paused).
 * @type {Record<string, {home?: string, set?: (p: import("playwright").Page) => Promise<() => Promise<void>>, ready?: (p: import("playwright").Page) => Promise<void>}>}
 */
const STATES = {
  ...Object.fromEntries(
    Object.entries(SHOWN_IN).map(([id, f]) => [id, FIXTURES[f]]),
  ),
  // A working copy holding a commit its upstream lacks.
  "repo-choice": {
    set: async (p) => {
      const url = "**/data/repo";
      await p.route(url, async (r) => {
        const res = await r.fetch();
        const body = await res.json();
        body.repo.unpushed = [
          {
            commit: "0123456789ab",
            subject: "demo: an unpushed commit",
            at: 1790000000,
          },
        ];
        await r.fulfill({ response: res, json: body });
      });
      return () => p.unroute(url);
    },
  },
  // An empty Schedules page (the sweep's own schedule hidden).
  "schedule-template": {
    set: async (p) => {
      const url = "**/data/schedules";
      await p.route(url, (r) =>
        r.request().method() === "GET" ? r.fulfill({ json: [] }) : r.fallback(),
      );
      return () => p.unroute(url);
    },
  },
  // One stack whose repositories could not be read.
  "backup-stack-retry": {
    set: async (p) => {
      const url = "**/data/backups/films*";
      await p.route(url, (r) =>
        r.fulfill({
          status: 502,
          json: {
            what: "films's backups",
            why: "the demo read fails on purpose for the sweep",
            fix: "Retry",
          },
        }),
      );
      return () => p.unroute(url);
    },
  },
  // One stack whose secrets file could not be read.
  "secrets-try-again": {
    home: "/stacks/films/settings?section=secrets",
    set: async (p) => {
      const why = "the demo read fails on purpose for the sweep";
      // The list of every stack's secrets names films unreadable, and the
      // read Try again sends fails the same way.
      await p.route("**/data/secrets", async (r) => {
        const res = await r.fetch();
        const body = await res.json();
        body.stacks = {
          ...(body.stacks ?? {}),
          films: { secrets: [], files: [], unreadable: `${why} :: read again` },
        };
        await r.fulfill({ response: res, json: body });
      });
      await p.route("**/data/secrets/films", (r) =>
        r.fulfill({
          status: 502,
          json: { what: "films's secrets", why, fix: "read again" },
        }),
      );
      return async () => {
        await p.unroute("**/data/secrets");
        await p.unroute("**/data/secrets/films");
      };
    },
  },
};

/**
 * Count what changes on screen from now on, Live view's own marks left
 * out (its bar, cursor, banner, the press flash's class and style).
 * @param {import("playwright").Page} page
 */
const watchScreen = (page) =>
  page.evaluate(() => {
    const w = /** @type {any} */ (window);
    if (w.__sweepObs) return;
    w.__sweepMut = 0;
    // Where the focus is, and what every radio and box on the page holds.
    w.__sweepMark = () => {
      const a = document.activeElement;
      const at =
        a && a !== document.body
          ? `${a.tagName}#${a.id}[${/** @type {HTMLElement} */ (a).dataset?.drive ?? ""}]`
          : "";
      const held = [
        ...document.querySelectorAll(
          "#page input[type=radio], #page input[type=checkbox]",
        ),
      ]
        .map((e) => (/** @type {HTMLInputElement} */ (e).checked ? "1" : "0"))
        .join("");
      return `${at}|${held}`;
    };
    const ours =
      ".drive-banner, .drive-announce, .drive-plan, .drive-cursor, #follow";
    w.__sweepObs = new MutationObserver((list) => {
      for (const m of list) {
        const el =
          m.target instanceof Element ? m.target : m.target.parentElement;
        if (el?.closest(ours)) continue;
        if (
          m.type === "attributes" &&
          ["class", "style"].includes(m.attributeName ?? "")
        )
          continue;
        w.__sweepMut += 1;
      }
    });
    w.__sweepObs.observe(document.body, {
      subtree: true,
      childList: true,
      characterData: true,
      attributes: true,
    });
  });

/**
 * review M6: declared controls drawn more than once without rows, which
 * are not declared twins; and every control's copies on screen now.
 * @param {import("playwright").Page} page
 * @param {Map<string, any>} byId the catalog's controls
 * @returns {Promise<string[]>}
 */
async function untwinned(page, byId) {
  const counts = await page.$$eval(
    "[data-drive]:not([data-drive-row])",
    (els) => {
      /** @type {Record<string, number>} */
      const n = {};
      for (const e of els) {
        if (e.getClientRects().length === 0) continue;
        const id = /** @type {HTMLElement} */ (e).dataset.drive ?? "";
        n[id] = (n[id] ?? 0) + 1;
      }
      return n;
    },
  );
  return Object.entries(counts)
    .filter(([id, k]) => k > 1 && byId.has(id) && !byId.get(id).twins)
    .map(([id, k]) => `${id} ×${k}`);
}

// redesign-integrate-8: 276 controls, each pressed with its effect, run
// past the 300 s every other case gets (the run timed out at control 100);
// each try keeps its own 45 s deadline and the watchdog its 90 s.
test(
  "invariants: drive-reach: Live view finds and presses every declared control, and each press has its effect",
  { timeout: 600_000 },
  async (t) => {
    const started = Date.now();
    const browser = await launch();
    try {
      const context = await browser.newContext({
        viewport: { width: 1600, height: 1000 },
        timezoneId: "Europe/Brussels",
      });
      const page = await freshPage(context);
      page.setDefaultTimeout(10000);
      // redesign-integrate-8: a press that opens a new tab (release notes)
      // has its effect there; count it, and close the tab.
      let tabs = 0;
      context.on("page", (pg) => {
        tabs += 1;
        void pg.close().catch(() => {});
      });
      // A row for the controls that repeat per schedule.
      const sched = await addSchedule(page, "films", "backup", {
        every: "day",
        at: "10:00",
      });
      const restore = await noCountdown(page);
      /** Change requests held back since the last look. @type {string[]} */
      const held = [];
      await holdChanges(page, (what) => held.push(what));
      const dialogs = () => page.$$eval("dialog[open]", (d) => d.length);
      // redesign-integrate-8: the tab's live channel, so a state can bring
      // what only the channel brings (a new host line, a running job).
      await page.addInitScript(catchLive);
      await markOld(page);
      await page.goto(`${BASE}/apps`, { waitUntil: "domcontentloaded" });
      await drawn(page);
      await page.check("#live-view");
      const { all } = await catalogOf(page);
      assert.ok(all.length > 40, `only ${all.length} controls are declared`);
      const byId = new Map(all.map((c) => [c.id, c]));
      // INVARIANTS_SWEEP_ONLY (a regular expression over ids): press only
      // those, while fixing a few; such a run never writes the stamp.
      const only = process.env.INVARIANTS_SWEEP_ONLY
        ? new RegExp(process.env.INVARIANTS_SWEEP_ONLY)
        : null;
      /** @type {Map<string, any[]>} */
      const byHome = new Map();
      for (const c of all.filter((x) => !only || only.test(x.id))) {
        const home = STATES[c.id]?.home ?? c.home;
        byHome.set(home, [...(byHome.get(home) ?? []), c]);
      }
      // redesign-integrate-8: INVARIANTS_SWEEP_SEED presses the pages and
      // their controls in a shuffled order (a gated check passes in any
      // order); the seed is printed.
      const seed = Number(process.env.INVARIANTS_SWEEP_SEED ?? "") || 0;
      if (seed) {
        const rnd = shuffled(seed);
        const homes = rnd([...byHome]);
        byHome.clear();
        for (const [h, list] of homes) byHome.set(h, rnd(list));
        t.diagnostic(`sweep order: shuffled, seed ${seed}`);
      }
      /** What a press left open, named (set by the reset after it). */
      let leftOpen = "";
      /** The control before was drawn in a state (STATES). */
      let afterState = false;
      // Where a press's time goes (INVARIANTS_PROGRESS prints it).
      /** @type {Record<string, number>} */
      let laps = {};
      let lapAt = Date.now();
      const lap = (/** @type {string} */ k) => {
        const n = Date.now();
        if (k !== "start") laps[k] = (laps[k] ?? 0) + n - lapAt;
        lapAt = n;
      };
      /** @type {Map<string, string>} */
      const landed = new Map();
      /** @type {string[]} why each failed or conditional control failed */
      const failed = [];
      // redesign-integrate-8: every catalog control's outcome, accounted
      // for at the end (sweepAccount): a run never drops one silently.
      /** @type {Map<string, import("./sweepkey.js").Outcome>} */
      const outcome = new Map(
        all
          .filter((x) => only && !only.test(x.id))
          .map((x) => [x.id, /** @type {const} */ ("skipped")]),
      );
      /** @type {Set<string>} */
      const twins = new Set();
      try {
        for (const [home, list] of byHome) {
          let fromElsewhere = true;
          for (const c of list) {
            const state = STATES[c.id];
            const first = fromElsewhere && !c.row && !c.reach.length && !state;
            if (first) fromElsewhere = false;
            /**
             * One try: from its home as the tab has it now (or freshly drawn),
             * the reach steps, the click, and its effect. Answers what went
             * wrong, or null.
             * @param {boolean} fresh
             * @returns {Promise<string | null>}
             */
            const attempt = async (fresh) => {
              lap("start");
              const undo = state?.set ? await state.set(page) : null;
              try {
                if (first) {
                  if (new URL(page.url()).pathname !== "/apps")
                    await liveGo(page, "/apps", landed);
                } else if (fresh || state || landed.get(home) !== page.url()) {
                  const g = await liveGo(page, home, landed);
                  if (!g.ok)
                    return `ui goto ${home} refused: ${g.refusal?.why}`;
                }
                lap("goto");
                for (const s of c.reach) {
                  const body = { ...s };
                  if (body.row === "*")
                    body.row = await firstRow(page, body.control ?? "");
                  const r = await reachStep(page, body);
                  if (!r.ok)
                    return `its reach step ${JSON.stringify(s)} was refused: ${r.refusal?.why}`;
                }
                if (state?.ready) await state.ready(page);
                lap("reach");
                for (const x of await untwinned(page, byId))
                  twins.add(`${home}: ${x}`);
                const row = c.row ? await firstRow(page, c.id) : null;
                const open0 = await dialogs();
                await watchScreen(page);
                const before = {
                  url: page.url(),
                  held: held.length,
                  tabs,
                  mut: await page.evaluate(
                    () => /** @type {any} */ (window).__sweepMut,
                  ),
                  // redesign-integrate-8: a press whose effect is where
                  // the focus goes (Run a command) or what a radio or a
                  // box holds (a level) changes no element: from no focus,
                  // both count.
                  mark: await page.evaluate(() => {
                    /** @type {HTMLElement | null} */ (
                      document.activeElement
                    )?.blur?.();
                    return /** @type {any} */ (window).__sweepMark();
                  }),
                };
                lap("look");
                const r = await reachStep(page, {
                  do: "click",
                  control: c.id,
                  ...(row == null ? {} : { row }),
                });
                try {
                  if (!r.ok) {
                    if (c.shows && String(r.refusal?.why).includes(c.shows)) {
                      return `shows only ${c.shows}, and the sweep did not draw that state: ${r.refusal?.why}`;
                    }
                    return `${row ? `${row}: ` : ""}refused, ${r.refusal?.why}; ${r.refusal?.fix}`;
                  }
                  // review H1: "no error" is not enough; the press did what
                  // it does: a dialog opened, a change was sent (and held
                  // back), the address or the screen changed.
                  const opened = await page
                    .waitForFunction(
                      (n) =>
                        document.querySelectorAll("dialog[open]").length > n,
                      open0,
                      { timeout: c.opens === "dialog" ? 3000 : 1 },
                    )
                    .then(() => true)
                    .catch(() => false);
                  // redesign-openpoints-1: a dialog control whose row goes
                  // to another address instead (the hub header's Update is
                  // the Update flow; since redesign-final-h3 a Restore… is
                  // the Restore flow page) opens no dialog; its effect is
                  // the new address, as pagedrive.js answers it.
                  if (c.opens === "dialog" && !opened) {
                    // Another page, not the same one with another query
                    // (a sort, a filter): that is no dialog's effect.
                    const went = await page
                      .waitForFunction(
                        (u) => location.pathname !== new URL(u).pathname,
                        before.url,
                        { timeout: 1500 },
                      )
                      .then(() => true)
                      .catch(() => false);
                    return went ? null : "pressed, but no dialog opened";
                  }
                  if (c.opens === "dialog") return null;
                  const moved = await page
                    .waitForFunction(
                      (b) =>
                        location.href !== b.url ||
                        /** @type {any} */ (window).__sweepMut > b.mut ||
                        /** @type {any} */ (window).__sweepMark() !== b.mark,
                      before,
                      { timeout: 2000 },
                    )
                    .then(() => true)
                    .catch(() => false);
                  if (
                    opened ||
                    moved ||
                    held.length > before.held ||
                    tabs > before.tabs
                  )
                    return null;
                  return `pressed, but nothing happened: no dialog, no change sent, no new tab, the address and the screen as they were`;
                } finally {
                  lap("effect");
                  // A refused press inside a page dialog (Deploy all
                  // changes) leaves it open too: close it, or every later
                  // goto is refused.
                  // redesign-integrate-8: every press ends on a known
                  // screen: Live view's dialog or form left, every dialog
                  // and menu closed; one the reset cannot close fails
                  // this press, by name.
                  if (
                    !r.ok ||
                    r.state?.page_dialog ||
                    r.state?.form ||
                    (await dialogs())
                  )
                    await reachStep(page, { do: "close" });
                  await closeAll(page);
                  // A toast (an Undo for 5 s) goes too: the next press
                  // starts without one (two Undo toasts are one id twice).
                  await page.evaluate(() =>
                    document
                      .querySelectorAll("#page .kp-toasts > *")
                      .forEach((t) =>
                        /** @type {any} */ (t).dismiss
                          ? /** @type {any} */ (t).dismiss()
                          : t.remove(),
                      ),
                  );
                  // A dialog's close event comes a task later; a row
                  // menu's button says closed only then. Clicking it before
                  // that opened the menu again: wait until every menu
                  // button says closed (at most 500 ms).
                  await page
                    .waitForFunction(
                      () =>
                        !document.querySelector(
                          '[aria-haspopup][aria-expanded="true"]',
                        ),
                      null,
                      { timeout: 500 },
                    )
                    .catch(() => {});
                  // A menu still open closes on its own button (Escape is
                  // Live view's Stop while it plays).
                  await page.$$eval(
                    '[aria-haspopup][aria-expanded="true"]',
                    (els) =>
                      els.forEach((e) =>
                        /** @type {HTMLElement} */ (e).click(),
                      ),
                  );
                  const left = await stillOpen(page);
                  if (left.length) {
                    leftOpen = left.join(", ");
                    await closeAll(page);
                  }
                  lap("reset");
                }
              } finally {
                await undo?.();
              }
            };
            // The page as the control before left it first (no navigation
            // when it is already there); once more freshly drawn when that
            // state was in the way.
            // Every try has a deadline: a press that waits on a selector or a
            // read that never comes is named, never a run that hangs.
            /** @param {boolean} fresh */
            const timed = (fresh) =>
              Promise.race([
                // A state or a step that throws is this control's failure,
                // never the end of the run (every control is accounted for).
                attempt(fresh).catch(
                  (e) => `threw: ${String(e?.message ?? e).split("\n")[0]}`,
                ),
                new Promise((res) =>
                  setTimeout(
                    () => res("hung: the try took more than 45 s"),
                    45000,
                  ),
                ),
              ]);
            const t0 = Date.now();
            leftOpen = "";
            laps = {};
            // A control drawn in a state leaves its page in that state (a failed
            // read, a filtered list): the next one starts on a freshly
            // drawn page instead of failing its first try there.
            const why1 = await timed(afterState);
            if (why1 && process.env.INVARIANTS_PROGRESS)
              console.error(`sweep ${c.id} first try: ${why1}`);
            const why =
              (why1 && (await timed(true))) ||
              (leftOpen ? `its press left open: ${leftOpen}` : null);
            afterState = !!state;
            if (process.env.INVARIANTS_PROGRESS)
              console.error(
                `sweep ${c.id}${why ? ` ✖ ${why}` : ""} (${Date.now() - t0} ms; ${Object.entries(
                  laps,
                )
                  .map(([k, v]) => `${k} ${v}`)
                  .join(", ")})`,
              );
            if (why) failed.push(`${c.id}: ${why}`);
            outcome.set(
              c.id,
              !why
                ? "passed"
                : why.startsWith("shows only ")
                  ? "conditional"
                  : "failed",
            );
            // The tab went to the first control's home itself.
            if (first && new URL(page.url()).pathname === home.split("?")[0])
              landed.set(home, page.url());
          }
        }
      } finally {
        await reachStep(page, { do: "done" });
        await restore();
        await page.unroute("**/data/**");
        await schedApi(page, "DELETE", `/data/schedules/${sched}`);
      }
      // redesign-integrate-8: one count per run, every control in exactly
      // one category, each named; a run whose counts do not add up fails.
      const { sweepKey, sweepAccount, catalogDiff, fullStamp, partialStamp } =
        await import("./sweepkey.js");
      const acct = sweepAccount(
        all.map((c) => c.id),
        outcome,
      );
      const secs = Math.round((Date.now() - started) / 1000);
      t.diagnostic(`sweep: ${acct.line}, in ${secs} s`);
      for (const k of /** @type {const} */ ([
        "failed",
        "conditional",
        "skipped",
      ]))
        t.diagnostic(`sweep ${k}: ${acct[k].join(", ") || "none"}`);
      // The catalog's size against the last passing sweep's stamp: every
      // change named by the controls added or removed.
      const { readFileSync, writeFileSync, existsSync } =
        await import("node:fs");
      const stampFile = new URL("./sweep-stamp.json", import.meta.url);
      if (existsSync(stampFile)) {
        const d = catalogDiff(
          JSON.parse(readFileSync(stampFile, "utf8")).controls ?? [],
          all.map((c) => c.id),
        );
        t.diagnostic(
          `sweep catalog: ${d.now} controls, the stamp had ${d.before}; added: ${d.added.join(", ") || "none"}; removed: ${d.removed.join(", ") || "none"}`,
        );
      }
      assert.deepEqual(
        acct.failed,
        [],
        `controls Live view could not reach:\n${failed.join("\n")}`,
      );
      // review H1: no control may be excused as "shows only in a state": the
      // sweep draws every state (STATES, and the demo host's notice).
      assert.deepEqual(
        acct.conditional,
        [],
        `controls the sweep did not draw:\n${failed.join("\n")}`,
      );
      // review M6: copies of one control on screen only from declared twins.
      assert.deepEqual(
        [...twins],
        [],
        "controls drawn twice that are not declared twins",
      );
      // redesign-integrate-8: a passing sweep stamps every control it pressed;
      // the commit-time catalog check refuses a control no sweep has pressed
      // since its entry changed.
      // The stamp names the catalog it was produced for (`catalog`, the hash
      // of every pressed key) and the run that produced it; the commit check
      // refuses a stamp whose keys do not hash to its `catalog`.
      // redesign-final (coordinator, 2026-10-04): a sweep of every control
      // writes a full stamp (the release gate's demand); a sweep of some
      // (INVARIANTS_SWEEP_ONLY, scripts/sweep-changed.sh: the controls a
      // commit changed) writes a partial one of those it pressed, which the
      // commit-time guard accepts for exactly those.
      const at = new Date(started).toISOString();
      const st = only
        ? partialStamp(
            all
              .filter((c) => acct.passed.includes(c.id))
              .map((c) => sweepKey(c)),
            at,
          )
        : acct.skipped.length
          ? null
          : fullStamp(all.map(sweepKey), at);
      if (!st) return;
      writeFileSync(
        new URL("./sweep-stamp.json", import.meta.url),
        `${JSON.stringify(
          { ...st, run: { seconds: secs, pressed: acct.passed.length } },
          null,
          2,
        )}\n`,
      );
    } finally {
      await browser.close();
    }
  },
);

/**
 * redesign-integrate-8: one Live view press of `id` on `path` as a driver
 * sends it (its first row on screen), with what it did: Live view's
 * answer, the address after it, the dialogs open, and the control's
 * catalog entry.
 * @param {string} path
 * @param {string} id
 * @param {string | null} [want] the row to press (redesign-openpoints-1);
 *   default: its first row on screen
 */
async function liveClick(path, id, want = null) {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    const restore = await noCountdown(page);
    await page.goto(`${BASE}/apps`, { waitUntil: "domcontentloaded" });
    await drawn(page);
    await page.check("#live-view");
    const { all } = await catalogOf(page);
    const entry = all.find((c) => c.id === id) ?? null;
    /** @type {Map<string, string>} */
    const landed = new Map();
    const g = await liveGo(page, path, landed);
    assert.ok(g.ok, `ui goto ${path}: ${g.refusal?.why}`);
    const row = want ?? (entry?.row ? await firstRow(page, id) : null);
    const t0 = Date.now();
    const r = await reachStep(page, {
      do: "click",
      control: id,
      ...(row == null ? {} : { row }),
    });
    const ms = Date.now() - t0;
    await page.waitForTimeout(800);
    const out = {
      r,
      ms,
      entry,
      row,
      url: new URL(page.url()),
      dialogs: await page.$$eval("dialog[open]", (d) => d.length),
      focused: await page.evaluate(() => document.activeElement?.id ?? ""),
      pressed: await page.$$eval(
        `[data-drive="${id}"][aria-pressed="true"]`,
        (e) => e.length,
      ),
    };
    await reachStep(page, { do: "done" });
    await restore();
    return out;
  } finally {
    await browser.close();
  }
}

// redesign-integrate-8: the three Update presses the flows merge turned
// into the one Update flow (an address) were still declared as opening a
// dialog, so Live view waited 3 s for a dialog that never came and the
// sweep failed them; each now is a control that opens a view.
test("invariants: redesign-integrate-8: Live view's press of a failed update's Run it again opens the Update flow for its stack, no dialog", async () => {
  const x = await liveClick("/activity", "activity-update-again");
  assert.equal(x.entry?.opens, "view", "declared as opening a view");
  assert.ok(x.r.ok, `refused: ${x.r.refusal?.why}`);
  assert.equal(x.url.pathname, "/update");
  assert.ok(x.url.searchParams.get("stack"), `${x.url}`);
  assert.equal(x.dialogs, 0);
});

test("invariants: redesign-integrate-8: Live view's press of a Stacks row's Update opens the Update flow for that stack, no dialog", async () => {
  const x = await liveClick("/stacks?view=table", "stacks-row-update");
  assert.equal(x.entry?.opens, "view", "declared as opening a view");
  assert.ok(x.r.ok, `refused: ${x.r.refusal?.why}`);
  assert.equal(x.url.pathname, "/update");
  assert.equal(x.url.searchParams.get("stack"), x.row);
  assert.equal(x.dialogs, 0);
});

test("invariants: redesign-integrate-8: Live view's press of the Map's stale-image Update opens the Update flow with that app picked, no dialog", async () => {
  const x = await liveClick("/map", "pin-update");
  assert.equal(x.entry?.opens, "view", "declared as opening a view");
  assert.ok(x.r.ok, `refused: ${x.r.refusal?.why}`);
  assert.equal(x.url.pathname, "/update");
  const [stack, ...app] = String(x.row).split("/");
  assert.equal(x.url.searchParams.get("stack"), stack);
  assert.equal(x.url.searchParams.get("app"), app.join("/"));
  assert.equal(x.dialogs, 0);
});

// redesign-openpoints-1: the hub header's Update is the one Update flow (an
// address) like every other Update press, but stack-head (whose Back up and
// Deploy rows do open dialogs) is declared as opening a dialog, so Live
// view waited 3 s for one before it answered.
test("invariants: redesign-openpoints-1: Live view's press of the stack hub header's Update opens the stack's Update flow and answers at once, no 3 s wait for a dialog", async () => {
  const x = await liveClick("/stacks/gateway", "stack-head", "gateway/update");
  assert.ok(x.r.ok, `refused: ${x.r.refusal?.why}`);
  assert.equal(x.url.pathname, "/update");
  assert.equal(x.url.searchParams.get("stack"), "gateway");
  assert.equal(x.dialogs, 0);
  assert.ok(x.ms < 2000, `Live view answered after ${x.ms} ms`);
});

// redesign-integrate-8: a Map node is an SVG <g>, which has no click();
// Live view took the step and never answered.
test("invariants: redesign-integrate-8: Live view presses a Map node (an SVG element) and answers: the node is selected", async () => {
  const x = await liveClick("/map", "map-node");
  assert.ok(x.r.ok, `refused: ${x.r.refusal?.why}`);
  assert.equal(x.url.searchParams.get("select"), x.row);
  assert.equal(x.pressed, 1, "the pressed node shows pressed");
});

// redesign-integrate-8: ui.js's KPI tile draws no fourth row without a
// spark or a meter; the hub put Compare now into that row, so the
// "Matches its files" tile never showed it.
test("invariants: redesign-integrate-8: the stack hub's Matches its files tile holds Compare now, and Live view presses it", async () => {
  const x = await liveClick("/stacks/kp-soft", "stack-compare");
  assert.ok(x.r.ok, `refused: ${x.r.refusal?.why}`);
  assert.equal(x.row, "kp-soft");
});

// redesign-integrate-8: the merge declared the Console's command text box
// as a control too; a press on a text box does nothing. Its old id now
// presses "Run a command", which focuses the command line.
test("invariants: redesign-integrate-8: Live view's console-command focuses the Console's command line", async () => {
  const x = await liveClick("/console", "console-command");
  assert.ok(x.r.ok, `refused: ${x.r.refusal?.why}`);
  assert.equal(x.focused, "shell-line");
});

test("invariants: drive-reach: every old control name a declaration keeps still clicks", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
      timezoneId: "Europe/Brussels",
    });
    const page = await freshPage(context);
    const sched = await addSchedule(page, "films", "backup", {
      every: "day",
      at: "10:00",
    });
    await holdChanges(page);
    await page.goto(`${BASE}/apps`);
    await page.check("#live-view");
    const restore = await noCountdown(page);
    const { all } = await catalogOf(page);
    const old = all.flatMap((c) =>
      c.was.map((/** @type {any} */ w) => ({ c, w })),
    );
    // 3.71.0 replaced Edit and Delete on a schedule's row by its menu.
    for (const id of ["edit-schedule", "delete-schedule"])
      assert.ok(
        old.some((x) => x.w.id === id),
        `no declaration keeps the old name ${id}`,
      );
    /** @type {Map<string, string>} */
    const landed = new Map();
    /** @type {string[]} */
    const failed = [];
    try {
      for (const { c, w } of old) {
        // The row as its home shows it, then the old name sent from
        // another page: the tab goes back there itself.
        await liveGo(page, c.home, landed);
        const row = c.row ? await firstRow(page, c.id) : null;
        await liveGo(page, "/apps", landed);
        const r = await reachStep(page, {
          do: "click",
          control: w.id,
          ...(row == null ? {} : { row }),
        });
        if (!r.ok)
          failed.push(`${w.id} (now ${c.id}): refused, ${r.refusal?.why}`);
        if (r.state?.page_dialog) await reachStep(page, { do: "close" });
        await closeAll(page);
      }
    } finally {
      await reachStep(page, { do: "done" });
      await restore();
      await page.unroute("**/data/**");
      await schedApi(page, "DELETE", `/data/schedules/${sched}`);
    }
    assert.deepEqual(failed, [], failed.join("\n"));
  } finally {
    await browser.close();
  }
});

test("invariants: drive-reach: ui goto accepts every address the router knows and lands where the browser's own redirect does", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    const { redirectFor: target } = await import("../js/router.js");
    await page.goto(`${BASE}/apps`);
    await page.check("#live-view");
    const restore = await noCountdown(page);
    const stacks = await page.evaluate(async () => {
      const r = await fetch("/data/fleet", {
        headers: { accept: "application/json" },
      });
      const b = await r.json();
      return (b.fleet?.stacks ?? []).map((/** @type {any} */ s) => s.name);
    });
    const addresses = [
      ...Object.keys(PATH_TO_PAGE).map((k) => `/${k}`),
      "/stacks/kp-soft/checks",
      "/stacks/kp-soft/firewall",
    ];
    /** @type {string[]} */
    const bad = [];
    try {
      for (const from of addresses) {
        const r = await reachStep(page, { do: "goto", path: from });
        if (!r.ok) {
          bad.push(`${from}: refused, ${r.refusal?.why}`);
          continue;
        }
        const want = target(route(from), "", { stacks }) ?? from;
        const landed = (/** @type {URL} */ u) =>
          want === "/"
            ? ["/", "/inbox", "/apps"].includes(u.pathname)
            : u.pathname === new URL(want, BASE).pathname;
        await page.waitForURL(landed, { timeout: 5000 }).catch(() => {});
        const u = new URL(page.url());
        if (!landed(u)) bad.push(`${from} → ${u.pathname} (wanted ${want})`);
      }
    } finally {
      await reachStep(page, { do: "done" });
      await restore();
    }
    assert.deepEqual(bad, [], `addresses ui goto missed:\n${bad.join("\n")}`);
  } finally {
    await browser.close();
  }
});

// redesign-activity (3.71.0, the approved Activity demo): Jobs, the
// timeline, History, Schedules and the host log are one page with three
// views, each its own address, and every old address opens its view.
test("invariants: Activity is one page with Now and history, Planned and Host log, and the old addresses open their view", async () => {
  const browser = await launch();
  try {
    const bad = [];
    for (const width of [1894, 390]) {
      const context = await browser.newContext({
        viewport: { width, height: 1000 },
      });
      const page = await freshPage(context);
      for (const [from, view, mark] of [
        ["/activity", "Now and history", "#history .ac-row"],
        ["/jobs", "Now and history", "#running"],
        ["/timeline", "Now and history", "#timeline .timeline svg"],
        ["/log", "Host log", ".hl-side .hl-opt"],
        // The kit (redesign-kit-1) drew the header with ui.js pageHeader;
        // .sch-ph is gone. Hosted, it is a section (redesign-final-h5).
        ["/schedules", "Planned", ".sch-head--hosted h2"],
      ]) {
        await page.goto(`${BASE}${from}`);
        const found = await page
          .locator(mark)
          .first()
          .waitFor({ timeout: 10000 })
          .then(() => true)
          .catch(() => false);
        const r = await page.evaluate(() => ({
          h1: [...document.querySelectorAll("#page h1")].map((e) =>
            (e.textContent ?? "").trim(),
          ),
          tab: (
            document.querySelector('.ac-tabs [aria-selected="true"]')
              ?.textContent ?? ""
          ).trim(),
          overflow: document.documentElement.scrollWidth - window.innerWidth,
        }));
        if (!found) bad.push(`${width}px ${from}: ${mark} never drew`);
        if (r.h1.join("|") !== "Activity")
          bad.push(`${width}px ${from}: page titles ${JSON.stringify(r.h1)}`);
        if (r.tab !== view)
          bad.push(`${width}px ${from}: the current tab is "${r.tab}"`);
        if (r.overflow > 0)
          bad.push(`${width}px ${from}: scrolls ${r.overflow}px sideways`);
      }
      // The KPI strip in exact numbers, and who started every row.
      await page.goto(`${BASE}/activity`);
      await page
        .locator("#history .ac-row")
        .first()
        .waitFor({ timeout: 10000 });
      const k = await page.evaluate(() => ({
        tiles: [...document.querySelectorAll(".nx-kpi .nx-kpi__label")].map(
          (e) => (e.textContent ?? "").trim(),
        ),
        values: [...document.querySelectorAll(".nx-kpi .nx-kpi__value")].map(
          (e) => (e.textContent ?? "").trim(),
        ),
        unnamed: [...document.querySelectorAll("#history .ac-row")].filter(
          (r) => !(r.querySelector(".ac-by-chip")?.textContent ?? "").trim(),
        ).length,
      }));
      if (
        k.tiles.join("|") !==
        "Operations|Succeeded|Open incidents|Nightly round|Running now"
      )
        bad.push(`${width}px: KPI tiles ${k.tiles}`);
      if (k.values.some((v) => /\+|k$/i.test(v)))
        bad.push(`${width}px: a KPI value is not exact: ${k.values}`);
      if (k.unnamed) bad.push(`${width}px: ${k.unnamed} rows name no actor`);
      await context.close();
    }
    assert.deepEqual(bad, [], bad.join("; "));
  } finally {
    await browser.close();
  }
});

// redesign-activity: History's Show filter is the demo's one-of switch
// (All, Failed, Nightly, By Claude) with a plain click, kept in the
// address (senior review, finding 7); a failed row
// opens in place with its error and its fixes.
test("invariants: Activity's History filters with a plain click, keeps it in the address, and a failed row opens with its fixes", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/activity`);
    await page.locator("#history .ac-row").first().waitFor({ timeout: 10000 });
    const count = () =>
      page.locator("#history .ac-count").textContent({ timeout: 2000 });
    const all = await count();
    const pick = (/** @type {string} */ v) =>
      page
        .locator(`#history [data-drive="activity-show"][data-drive-row="${v}"]`)
        .click();
    await pick("claude");
    const claude = await page.evaluate(() =>
      [...document.querySelectorAll("#history .ac-row .ac-by-chip")].map((c) =>
        (c.textContent ?? "").trim(),
      ),
    );
    assert.ok(claude.length > 0, "By Claude shows no row");
    assert.ok(
      claude.every((c) => /Claude/.test(c)),
      `By Claude shows ${[...new Set(claude)]}`,
    );
    assert.match(page.url(), /show=claude/);
    await pick("failed");
    assert.match(page.url(), /show=failed(&|$)/);
    const onlyFailed = await page.evaluate(() =>
      [...document.querySelectorAll("#history .ac-row")].every(
        (r) => /** @type {HTMLElement} */ (r).dataset.tone === "bad",
      ),
    );
    assert.ok(onlyFailed, "Failed shows a row that did not fail");
    // All shows everything again.
    await pick("all");
    assert.equal(await count(), all);
    const failed = page.locator('#history .ac-row[data-tone="bad"]').first();
    await failed.click();
    const detail = page.locator("#history .ac-detail").first();
    await detail.waitFor({ timeout: 3000 });
    assert.equal(await failed.getAttribute("aria-expanded"), "true");
    const d = await detail.evaluate((el) => ({
      err: (el.querySelector(".ac-err")?.textContent ?? "").trim(),
      steps: el.querySelectorAll(".ac-gantt > div").length,
      buttons: [...el.querySelectorAll("button, a.kp-button")].map((b) =>
        (b.textContent ?? "").trim(),
      ),
    }));
    assert.ok(d.err.length > 0, "the failed row shows no error");
    assert.ok(d.steps > 0, "the failed row shows no steps");
    assert.ok(
      d.buttons.includes("Open the incident") &&
        d.buttons.includes("Charts at this time"),
      `the failed row offers ${d.buttons}`,
    );
  } finally {
    await browser.close();
  }
});

// redesign-activity, invariant 47's rule on the page: a running job's log
// in Running now scrolls inside its own box; the card never grows as lines
// arrive.
test("invariants: a running job's log in Activity scrolls inside its box and never makes Running now taller", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/stacks/films`);
    await page.waitForTimeout(600);
    await page
      .getByRole("button", { name: "Park", exact: true })
      .click({ timeout: 5000 });
    await page.waitForTimeout(300);
    await page.locator("#action-dialog #act-run").click();
    await page.waitForTimeout(300);
    await page.evaluate(() =>
      document
        .querySelectorAll("dialog[open]")
        .forEach((d) => /** @type {HTMLDialogElement} */ (d).close()),
    );
    await page.goto(`${BASE}/activity?view=running`);
    const log = page.locator("#running .ac-job__log").first();
    await log.waitFor({ state: "attached", timeout: 5000 });
    const card = page.locator("#running");
    const before = await card.boundingBox();
    assert.ok(before, "Running now is not on screen");
    await log.evaluate((el) => {
      el.hidden = false;
      el.textContent += Array.from(
        { length: 400 },
        (_, i) =>
          `12:00:${String(i % 60).padStart(2, "0")}  step ${i} of a long deploy`,
      ).join("\n");
    });
    await page.waitForTimeout(200);
    const after = await card.boundingBox();
    assert.ok(after, "Running now left the screen");
    assert.equal(
      Math.round(after.height),
      Math.round(before.height),
      `Running now grew from ${before.height} to ${after.height} px as log lines arrived`,
    );
    assert.ok(
      await log.evaluate((el) => el.scrollHeight > el.clientHeight + 1),
      "the job's log does not scroll inside its own box",
    );
    // Open the log: the dialog's log scrolls too (invariant 47).
    await page
      .locator("#running .ac-job button", { hasText: "Open the log" })
      .first()
      .click();
    await page.locator("dialog#job-dialog[open] .job-log").waitFor({
      state: "attached",
      timeout: 3000,
    });
  } finally {
    await browser.close();
  }
});

// redesign-console (Kenny, 2026-10-03, on the Live log: "alle items lijken
// gewoon links aligned naast elkaar gezet, ipv mooi gestructureerd in hun
// eigen sectie van het beeld"): the host log is a log explorer — sources
// and levels in a side column with exact counts, the toolbar's search on
// the left at its own width and the count and follow against the right
// edge; one column on a phone, never a sideways scroll.
test("invariants: the host log puts its filters in a side column and its toolbar in three zones, on Console and Activity", async () => {
  const browser = await launch();
  try {
    const bad = [];
    for (const width of [1894, 390]) {
      const context = await browser.newContext({
        viewport: { width, height: 1000 },
      });
      const page = await freshPage(context);
      for (const path of ["/console", "/activity?view=host-log"]) {
        await page.goto(`${BASE}${path}`);
        await page.locator(".hl-out .hl-ln[data-seq]").first().waitFor({
          timeout: 10000,
        });
        const r = await page.evaluate(() => {
          const box = (/** @type {Element | null} */ e) =>
            e?.getBoundingClientRect() ?? null;
          const bar = document.querySelector(".hl-bar");
          return {
            bar: box(bar),
            search: box(bar?.querySelector(".nx-tb__search") ?? null),
            state: box(bar?.querySelector(".nx-tb__state") ?? null),
            count: (bar?.querySelector(".hl-count")?.textContent ?? "").trim(),
            side: box(document.querySelector(".hl-side")),
            out: box(document.querySelector(".hl-out")),
            groups: [...document.querySelectorAll(".hl-side h3")].map((h) =>
              (h.textContent ?? "").trim(),
            ),
            counts: [
              ...document.querySelectorAll(".hl-side .hl-opt small"),
            ].map((s) => (s.textContent ?? "").trim()),
            overflow: document.documentElement.scrollWidth - window.innerWidth,
          };
        });
        const at = `${width}px ${path}`;
        if (!r.bar || !r.search || !r.state || !r.side || !r.out) {
          bad.push(`${at}: a zone is missing`);
          continue;
        }
        if (r.groups.join("|") !== "Source|Level")
          bad.push(`${at}: side column groups ${r.groups}`);
        if (r.counts.some((c) => !/^\d+$/.test(c)))
          bad.push(`${at}: counts ${r.counts}`);
        if (!/^\d+ of \d+ lines?$/.test(r.count))
          bad.push(`${at}: the count reads "${r.count}"`);
        if (r.overflow > 0) bad.push(`${at}: scrolls ${r.overflow}px sideways`);
        if (width > 1200) {
          if (!(r.side.right <= r.out.left))
            bad.push(`${at}: the side column is not beside the lines`);
          if (r.search.left - r.bar.left > 40)
            bad.push(`${at}: the search is not at the left`);
          if (r.search.width > 22 * 16 + 1)
            bad.push(`${at}: the search stretches to ${r.search.width}px`);
          if (r.bar.right - r.state.right > 40)
            bad.push(`${at}: the count and follow are not at the right edge`);
          if (Math.abs(r.search.top - r.state.top) > 12)
            bad.push(`${at}: search and state are not on one row`);
        } else if (!(r.side.bottom <= r.out.top))
          bad.push(`${at}: on a phone the filters are not above the lines`);
      }
      // A source turns off with a plain click, and the lines follow.
      await page.goto(`${BASE}/console`);
      await page.locator(".hl-out .hl-ln[data-seq]").first().waitFor({
        timeout: 10000,
      });
      const lines = () => page.locator(".hl-out .hl-ln[data-seq]").count();
      const n0 = await lines();
      await page
        .locator(
          '.hl-side input[data-drive="console-source"][data-drive-row="HOST"]',
        )
        .click();
      const n1 = await lines();
      if (!(n1 < n0))
        bad.push(`${width}px: turning the host off left ${n1} of ${n0}`);
      if (!/hide=HOST/.test(page.url()))
        bad.push(`${width}px: the filter is not in the address`);
      await context.close();
    }
    assert.deepEqual(bad, [], bad.join("; "));
  } finally {
    await browser.close();
  }
});

// redesign-console (3.71.0): Console is the host's lines and the shell in
// one place — the shell bar under the lines, a line opens its details.
test("invariants: Console shows the host's lines with the shell bar under them, and a line opens its details", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/shell?vmid=104`);
    assert.equal(new URL(page.url()).pathname, "/console");
    const first = page.locator(".hl-out .hl-ln[data-seq]").first();
    await first.waitFor({ timeout: 10000 });
    const r = await page.evaluate(() => ({
      h1: (document.querySelector("#page h1")?.textContent ?? "").trim(),
      out:
        document.querySelector(".hl-out")?.getBoundingClientRect().bottom ?? 0,
      shell:
        document.querySelector(".con-shell")?.getBoundingClientRect().top ?? 0,
      inTerm: !!document.querySelector(".hl-term .con-shell #shell-line"),
      actions: [
        ...document.querySelectorAll(".nx-head-actions .kp-button"),
      ].map((b) => (b.textContent ?? "").trim()),
    }));
    assert.equal(r.h1, "Console");
    assert.ok(
      r.inTerm && r.shell >= r.out - 1,
      "the shell bar is not under the lines",
    );
    assert.deepEqual(r.actions, ["Pause", "Download", "Run a command"]);
    await first.click();
    await page.locator(".hl-more").waitFor({ timeout: 2000 });
    assert.equal(await first.getAttribute("aria-expanded"), "true");
    await page
      .locator(".nx-head-actions .kp-button", { hasText: "Pause" })
      .click();
    assert.match(
      (await page.locator(".con-live").textContent()) ?? "",
      /paused/,
    );
  } finally {
    await browser.close();
  }
});

// Senior review of redesign-371-activity: a fake live channel, so a test
// can hand the page a host line at the moment it chooses.
const FAKE_EVENTS = () => {
  const Real = window.EventSource;
  /** @type {EventSource[]} */
  const all = [];
  // @ts-ignore
  window.__es = all;
  // @ts-ignore
  window.EventSource = class extends Real {
    /** @param {string | URL} u @param {EventSourceInit} [o] */
    constructor(u, o) {
      super(u, o);
      all.push(this);
    }
  };
};
/**
 * @param {import("playwright").Page} page
 * @param {object[]} lines
 */
const pushLines = (page, lines) =>
  page.evaluate((ls) => {
    // @ts-ignore
    for (const es of window.__es)
      for (const l of ls)
        es.dispatchEvent(
          new MessageEvent("host_log", { data: JSON.stringify(l) }),
        );
  }, lines);
/** @param {number} seq @param {string} source @param {string} msg */
const hostLine = (seq, source, msg) => ({
  seq,
  ts: Math.floor(Date.now() / 1000),
  level: "info",
  source,
  msg,
  req: null,
  by: null,
});

// Finding 1: a live line that arrives before the snapshot no longer hides
// every snapshot line. Finding 3: the side column is built once, so a
// focused source keeps focus while lines (and a new source) arrive.
test("invariants: review-activity: the host log merges early live lines with its snapshot and keeps side-column focus", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    const snap = await page.evaluate(async () => {
      const r = await fetch("/data/host-log");
      const b = await r.json();
      return (b.lines ?? []).filter(
        (/** @type {{level: string}} */ l) => l.level !== "debug",
      ).length;
    });
    await page.addInitScript(FAKE_EVENTS);
    let release = () => {};
    const held = new Promise((r) => (release = () => r(undefined)));
    await page.route("**/data/host-log*", async (route) => {
      await Promise.race([held, new Promise((r) => setTimeout(r, 8000))]);
      await route.continue();
    });
    await page.goto(`${BASE}/console`);
    await page.waitForFunction(
      // @ts-ignore
      () => (window.__es ?? []).length > 0,
      null,
      { timeout: 5000 },
    );
    assert.ok(snap > 1, `the demo snapshot holds ${snap} lines`);
    await pushLines(page, [hostLine(9_000_000, "HOST", "early live line")]);
    release();
    await page.locator(".hl-out .hl-ln[data-seq]").first().waitFor({
      timeout: 10000,
    });
    await page.waitForTimeout(300);
    const r = await page.evaluate(() => ({
      count: (document.querySelector(".hl-count")?.textContent ?? "").trim(),
      early: !!document.querySelector('.hl-ln[data-seq="9000000"]'),
    }));
    const total = Number(/of (\d+)/.exec(r.count)?.[1] ?? 0);
    assert.ok(r.early, "the early live line is missing");
    assert.ok(
      total >= snap,
      `"${r.count}": the snapshot's ${snap} lines were dropped`,
    );
    // Focus a source; lines and a new source arrive; focus stays put.
    const host = page.locator(
      '.hl-side input[data-drive="console-source"][data-drive-row="HOST"]',
    );
    await host.focus();
    await host.evaluate((el) => (el.dataset.mark = "kept"));
    await pushLines(page, [
      hostLine(9_000_001, "HOST", "one"),
      hostLine(9_000_002, "zz-new-source", "two"),
      hostLine(9_000_003, "HOST", "three"),
    ]);
    await page.waitForTimeout(400);
    const after = await page.evaluate(() => ({
      mark: /** @type {HTMLElement} */ (document.activeElement)?.dataset?.mark,
      newRow: !!document.querySelector(
        '.hl-side input[data-drive-row="zz-new-source"]',
      ),
    }));
    assert.equal(after.mark, "kept", "the focused source lost focus");
    assert.ok(after.newRow, "the new source got no row");
  } finally {
    await browser.close();
  }
});

// Findings 12, 14, 16: Space opens a focused line like Enter, "Copied"
// turns back into "Copy line", and the side column's hint and reset use
// the demo's words.
test("invariants: review-activity: a host line opens with Space, Copy line comes back, and the hint reads as the demo", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/console`);
    const first = page.locator(".hl-out .hl-ln[data-seq]").first();
    await first.waitFor({ timeout: 10000 });
    const hint = async () =>
      page.evaluate(() => {
        const vis = (/** @type {Element | null} */ e) =>
          !!e && /** @type {HTMLElement} */ (e).offsetParent !== null;
        const reset = [...document.querySelectorAll(".hl-side button")].find(
          (b) => (b.textContent ?? "").trim() === "Reset the filters",
        );
        return {
          hint: vis(document.querySelector(".hl-side .hl-hint"))
            ? (
                document.querySelector(".hl-side .hl-hint")?.textContent ?? ""
              ).trim()
            : null,
          reset: vis(reset ?? null),
        };
      });
    assert.deepEqual(await hint(), {
      hint: "Hover a source for “only”.",
      reset: false,
    });
    await page
      .locator(
        '.hl-side input[data-drive="console-source"][data-drive-row="HOST"]',
      )
      .click();
    assert.deepEqual(await hint(), { hint: null, reset: true });
    await page
      .locator(
        '.hl-side input[data-drive="console-source"][data-drive-row="HOST"]',
      )
      .click();
    const row = page.locator(".hl-out .hl-ln[data-seq]").first();
    await row.focus();
    await page.keyboard.press(" ");
    assert.equal(await row.getAttribute("aria-expanded"), "true");
    assert.equal(
      await page.evaluate(
        () =>
          /** @type {HTMLElement} */ (document.querySelector(".con-body"))
            ?.dataset.following,
      ),
      "true",
      "Space on a line paused the log instead of opening it",
    );
    const copy = page.locator(".hl-more button", { hasText: /Copy line/ });
    await copy.click();
    await page.waitForFunction(
      () =>
        [...document.querySelectorAll(".hl-more button")].some((b) =>
          /^(Copied|Press Ctrl C)$/.test((b.textContent ?? "").trim()),
        ),
      null,
      { timeout: 2000 },
    );
    await page.waitForFunction(
      () =>
        [...document.querySelectorAll(".hl-more button")].some(
          (b) => (b.textContent ?? "").trim() === "Copy line",
        ),
      null,
      { timeout: 4000 },
    );
  } finally {
    await browser.close();
  }
});

// Findings 5 and 11: the live chip beside the title, "2 000", the demo's
// phone header order, the first container chosen, and the exec hint there
// from the first frame even when the host's settings cannot be read.
test("invariants: review-activity: Console's header and shell bar are the demo's", async () => {
  const browser = await launch();
  try {
    const bad = [];
    for (const width of [1894, 390]) {
      const context = await browser.newContext({
        viewport: { width, height: 1000 },
      });
      const page = await freshPage(context);
      await page.route("**/data/host-settings*", (r) => r.abort());
      await page.goto(`${BASE}/console`);
      await page.locator(".hl-out .hl-ln[data-seq]").first().waitFor({
        timeout: 10000,
      });
      const r = await page.evaluate(() => {
        const box = (/** @type {string} */ q) =>
          document.querySelector(q)?.getBoundingClientRect() ?? null;
        const btn = (/** @type {string} */ t) =>
          [...document.querySelectorAll(".nx-head .kp-button")]
            .find((b) => (b.textContent ?? "").trim() === t)
            ?.getBoundingClientRect() ?? null;
        const sel = /** @type {HTMLSelectElement | null} */ (
          document.querySelector("#shell-target")
        );
        const off = /** @type {HTMLElement | null} */ (
          document.querySelector(".con-off")
        );
        return {
          beside: !!document
            .querySelector(".nx-head h1")
            ?.nextElementSibling?.classList.contains("con-live"),
          chip: (document.querySelector(".con-live")?.textContent ?? "").trim(),
          h1: box(".nx-head h1"),
          desc: box(".nx-head .nx-head-desc"),
          head: box(".nx-head"),
          run: btn("Run a command"),
          pause: btn("Pause"),
          download: btn("Download"),
          select: sel?.value ?? "",
          first: sel?.options[0]?.value ?? "",
          pick: [...(sel?.options ?? [])].some((o) =>
            /Pick a container/.test(o.textContent ?? ""),
          ),
          hint: off && !off.hidden ? (off.textContent ?? "").trim() : "",
          overflow: document.documentElement.scrollWidth - window.innerWidth,
        };
      });
      const at = `${width}px`;
      if (!r.beside) bad.push(`${at}: the live chip is not beside the title`);
      if (!/2\s000-line ring/.test(r.chip))
        bad.push(`${at}: the chip reads "${r.chip}"`);
      if (!r.select || r.select !== r.first || r.pick)
        bad.push(`${at}: the container select starts on "${r.select}"`);
      if (!r.hint) bad.push(`${at}: no exec hint when the settings fail`);
      if (r.overflow > 0) bad.push(`${at}: scrolls ${r.overflow}px sideways`);
      if (
        width === 390 &&
        r.h1 &&
        r.desc &&
        r.run &&
        r.pause &&
        r.download &&
        r.head
      ) {
        if (!(r.desc.top > r.h1.top))
          bad.push(`${at}: the description is above the title`);
        if (!(r.run.top > r.desc.top))
          bad.push(`${at}: Run a command is not under the description`);
        if (r.run.width < r.head.width - 4)
          bad.push(
            `${at}: Run a command is ${r.run.width}px of ${r.head.width}`,
          );
        if (!(r.pause.top > r.run.top))
          bad.push(`${at}: Pause is not under Run a command`);
        if (Math.abs(r.pause.top - r.download.top) > 2)
          bad.push(`${at}: Pause and Download are not side by side`);
      }
      await context.close();
    }
    assert.deepEqual(bad, [], bad.join("; "));
  } finally {
    await browser.close();
  }
});

// Findings 7, 8, 9, 10, 13: History's Show switch (All and Failed with
// exact counts), owners instead of tokens, the Host log's Pause and
// Download together and its key hints once, its description true on a
// phone, and a cancelled touch leaves no brush on the timeline.
test("invariants: review-activity: History's Show switch counts, By names owners, the Host log groups its buttons, the brush never sticks", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/activity`);
    await page.locator("#history .ac-row").first().waitFor({ timeout: 10000 });
    const sw = await page.evaluate(() =>
      [
        ...document.querySelectorAll('#history [data-drive="activity-show"]'),
      ].map((b) => (b.textContent ?? "").trim()),
    );
    const total = Number(
      /of (\d+)/.exec(
        (await page.locator("#history .ac-count").textContent()) ?? "",
      )?.[1] ?? -1,
    );
    assert.equal(sw.length, 4, `the Show switch has ${sw}`);
    assert.match(sw[0], /^All\s*(\d+)$/);
    assert.equal(Number(/(\d+)$/.exec(sw[0])?.[1]), total, "All's count");
    assert.match(sw[1], /^Failed\s*\d+$/);
    assert.deepEqual(sw.slice(2), ["Nightly", "By Claude"]);
    await page
      .locator('#history [data-drive="activity-show"][data-drive-row="failed"]')
      .click();
    assert.match(page.url(), /show=failed/);
    const tones = await page.evaluate(() =>
      [...document.querySelectorAll("#history .ac-row")].map(
        (r) => /** @type {HTMLElement} */ (r).dataset.tone,
      ),
    );
    assert.ok(
      tones.every((t) => t === "bad"),
      "Failed shows a row that did not fail",
    );
    await page.locator("body").press("f");
    assert.doesNotMatch(page.url(), /show=/);
    const by = await page.evaluate(() =>
      [...document.querySelectorAll("#history .ac-by-chip")].map((c) =>
        (c.textContent ?? "").trim(),
      ),
    );
    const raw = by.filter((b) => /^[a-z0-9][a-z0-9._-]*$/.test(b));
    assert.deepEqual([...new Set(raw)], [], "By shows a raw token");
    // The timeline: a touch scroll ends in pointercancel; no brush stays.
    const tl = page.locator("#timeline .timeline");
    const b = await tl.boundingBox();
    assert.ok(b, "no timeline");
    await page.mouse.move(b.x + 100, b.y + 40);
    await page.mouse.down();
    await page.mouse.move(b.x + 260, b.y + 40, { steps: 4 });
    await tl.evaluate((el) =>
      el.dispatchEvent(
        new PointerEvent("pointercancel", { pointerId: 1, bubbles: true }),
      ),
    );
    await page.mouse.move(b.x + 300, b.y + 40);
    await page.waitForTimeout(100);
    const brushes = await page.locator("#timeline .ac-tl__brush").count();
    await page.mouse.up();
    assert.equal(brushes, 0, "the brush stayed after pointercancel");
    // The Host log: Pause and Download in the card's head, hints once.
    await page.goto(`${BASE}/activity?view=host-log`);
    await page
      .locator(".hl-out .hl-ln[data-seq]")
      .first()
      .waitFor({ timeout: 10000 });
    const hl = await page.evaluate(() => {
      const card = document.querySelector("#host-log-card");
      const outside = [...(card?.querySelectorAll(".kp-button") ?? [])]
        .filter((b) => !b.closest(".hl"))
        .map((b) => (b.textContent ?? "").trim());
      return {
        outside,
        inBar: !!document.querySelector(".hl-bar .hl-follow"),
        keys: document.querySelector(".nx-keys, .ac-keys")?.textContent ?? "",
        desc: card?.textContent ?? "",
      };
    });
    assert.deepEqual(hl.outside, ["Pause", "Download"]);
    assert.equal(hl.inBar, false, "Pause is in the toolbar as well");
    assert.doesNotMatch(hl.keys, /Space|End/, "the key hints show twice");
    assert.doesNotMatch(hl.desc, /on the left/);
  } finally {
    await browser.close();
  }
});

// Finding 4: a minute's reread that fails keeps the rows, with a note,
// and the focused row keeps focus across the repaint.
test("invariants: review-activity: a failed History reread keeps the rows and the focused row", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.clock.install();
    let reads = 0;
    await page.route("**/data/history*", (route) =>
      ++reads > 1 ? route.abort() : route.continue(),
    );
    await page.goto(`${BASE}/activity`);
    const row = page.locator("#history .ac-row").nth(2);
    await row.waitFor({ timeout: 10000 });
    const key = await row.getAttribute("data-key");
    await row.focus();
    const before = await page.locator("#history .ac-row").count();
    await page.clock.fastForward(61_000);
    await page
      .waitForFunction(() => !!document.querySelector(".ac-stale"), null, {
        timeout: 5000,
      })
      .catch(() => {});
    const r = await page.evaluate(() => ({
      rows: document.querySelectorAll("#history .ac-row").length,
      alert: !!document.querySelector("#history .kp-alert--destructive"),
      stale: (
        document.querySelector("#history .ac-stale")?.textContent ?? ""
      ).trim(),
      focus:
        /** @type {HTMLElement} */ (document.activeElement)?.dataset?.key ??
        null,
    }));
    assert.ok(reads > 1, "the minute's reread never ran");
    assert.equal(r.alert, false, "the rows were replaced by the error");
    assert.equal(r.rows, before);
    assert.match(r.stale, /Not refreshed/);
    assert.equal(r.focus, key, "the focused row lost focus");
  } finally {
    await browser.close();
  }
});

// ── redesign-371-metrics / redesign-371-map (Kenny approved the demos
// 2026-10-03: metrics.html, fleetview.html): the Metrics page and the Map
// as the approved demos, every chart the shared time chart ──────────────

test("invariants: the Metrics page is laid out as the approved demo, every chart a shared time chart with the host's events marked", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/charts`);
    // Loading shows in the final geometry from the first frame.
    await page.waitForSelector(".nx-kpis .nx-kpi", { timeout: 8000 });
    await page.waitForSelector(".mk-card .tc-plot svg", { timeout: 10000 });
    await page.waitForTimeout(400);
    const got = await page.evaluate(() => ({
      chip: document.querySelector(".nx-head-meta .nx-chip")?.textContent,
      views: [
        ...document.querySelectorAll(
          '.nx-seg[aria-label="Metrics view"] button',
        ),
      ].map((b) => b.textContent),
      windows: [
        ...document.querySelectorAll(
          '.mk-toolbar .nx-seg[aria-label="Window"] button',
        ),
      ].map((b) => b.textContent),
      pressed: document.querySelector(
        '.nx-seg[aria-label="Window"] [aria-pressed="true"]',
      )?.textContent,
      tiles: [...document.querySelectorAll(".nx-kpis .nx-kpi__label")].map(
        (l) => l.textContent,
      ),
      attention: document.querySelector(".nx-attention")?.textContent ?? "",
      sections: [...document.querySelectorAll(".mk-section__head h2")].map(
        (x) => x.textContent,
      ),
      charts: document.querySelectorAll(".mk-card .tc-plot svg").length,
      marks: document.querySelectorAll(".mk-card .tc-mark").length,
      window: document.querySelector(".mk-window")?.textContent ?? "",
    }));
    assert.match(got.chip ?? "", /Live · reads every 30 s/);
    assert.deepEqual(got.views, ["System", "Traffic"]);
    assert.deepEqual(got.windows, ["1h", "6h", "24h", "7d", "30d"]);
    assert.equal(got.pressed, "24h");
    assert.deepEqual(got.tiles, [
      "CPU",
      "Memory",
      "Root disk",
      "Load (1 min)",
      "Hottest chip",
      "Drives",
    ]);
    assert.match(got.attention, /Drive sdb reports SMART not ok/);
    assert.deepEqual(got.sections, ["Compute", "Storage and temperature"]);
    assert.ok(got.charts >= 7, `only ${got.charts} time charts`);
    assert.ok(got.marks > 0, "no event marked on the charts");
    assert.match(got.window, /events? marked · drag across a chart to zoom/);
  } finally {
    await browser.close();
  }
});

test("invariants: a plain click on a Metrics chart source turns it on or off, several stay on, Show all resets", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/charts`);
    const card = page.locator('[data-key="net"]');
    await card.locator(".tc-legend__item").nth(2).waitFor({ timeout: 10000 });
    const items = card.locator(".tc-legend__item");
    const state = () =>
      items.evaluateAll((els) =>
        els.map((e) =>
          e.getAttribute("aria-pressed") === "true"
            ? "on"
            : e.hasAttribute("data-off")
              ? "off"
              : "all",
        ),
      );
    assert.deepEqual(await state(), ["all", "all", "all"]);
    await items.nth(0).click();
    assert.deepEqual(await state(), ["on", "off", "off"]);
    // A second source, plain click: both stay on (no Shift).
    await items.nth(1).click();
    assert.deepEqual(await state(), ["on", "on", "off"]);
    await items.nth(0).click();
    assert.deepEqual(await state(), ["off", "on", "off"]);
    await card.locator(".tc-legend__reset").click();
    assert.deepEqual(await state(), ["all", "all", "all"]);
    // Esc on the chart resets it too.
    await items.nth(2).click();
    await card.locator(".tc-plot").focus();
    await page.keyboard.press("Escape");
    assert.deepEqual(await state(), ["all", "all", "all"]);
  } finally {
    await browser.close();
  }
});

test("invariants: hovering a Metrics chart shows the reading and the change over the hour before on every chart at once; a drag zooms them all", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/charts`);
    const plot = page.locator('[data-key="cpu"] .tc-plot');
    await plot.locator("svg").waitFor({ timeout: 10000 });
    await plot.scrollIntoViewIfNeeded();
    await page.waitForTimeout(300);
    const box = await plot.boundingBox();
    assert.ok(box, "no CPU chart");
    await page.mouse.move(box.x + box.width * 0.5, box.y + box.height * 0.5);
    await page.waitForTimeout(100);
    const tip = await page
      .locator('[data-key="cpu"] .tc-tip')
      .evaluate((t) => ({
        hidden: /** @type {HTMLElement} */ (t).hidden,
        text: t.textContent ?? "",
      }));
    assert.equal(tip.hidden, false, "no reading on hover");
    assert.match(tip.text, /change over the hour before/);
    assert.match(tip.text, /[▲▼]|±0/);
    const synced = await page.evaluate(() => ({
      charts: document.querySelectorAll(".mk-card .tc-plot svg").length,
      lines: document.querySelectorAll(".mk-card .tc-xhair").length,
    }));
    assert.equal(
      synced.lines,
      synced.charts,
      "the crosshair is not on every chart",
    );
    const ticks = () =>
      page
        .locator('[data-key="mem"] .tc-tick[text-anchor="middle"]')
        .allTextContents();
    const before = await ticks();
    await page.mouse.move(box.x + box.width * 0.3, box.y + box.height * 0.5);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width * 0.6, box.y + box.height * 0.5, {
      steps: 6,
    });
    await page.mouse.up();
    const chip = page.locator(".mk-toolbar .tc-zoom");
    await chip.waitFor({ state: "visible", timeout: 3000 });
    assert.match((await chip.textContent()) ?? "", /Zoomed/);
    assert.notDeepEqual(await ticks(), before, "the other charts did not zoom");
    await chip.locator("button").click();
    await chip.waitFor({ state: "hidden", timeout: 3000 });
    assert.deepEqual(await ticks(), before);
  } finally {
    await browser.close();
  }
});

test("invariants: the Traffic tab stacks requests by status class, and a plain click on a hostname row switches it on or off in its chart", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/charts?tab=traffic`);
    await page.waitForSelector('[data-key="hostnames"] tbody tr', {
      timeout: 10000,
    });
    const tiles = await page
      .locator(".nx-kpis .nx-kpi__label")
      .allTextContents();
    assert.deepEqual(tiles, [
      "Requests",
      "Server errors (5xx)",
      "Client errors (4xx)",
      "Hostnames",
      "Client addresses",
    ]);
    // Exact counters, never "38.1k".
    const req =
      (await page
        .locator('.nx-kpi[data-key="requests"] .nx-kpi__value')
        .textContent()) ?? "";
    assert.match(req, /^\d{1,3}(,\d{3})*$/);
    const classes = await page
      .locator('[data-key="requests"] .tc-legend__item > span')
      .allTextContents();
    assert.deepEqual(classes, ["2xx", "3xx", "4xx", "5xx"]);
    const legend = page.locator('[data-key="perhost"] .tc-legend__item');
    const row = page.locator('[data-key="hostnames"] tbody tr').first();
    const name = ((await row.locator("td").first().textContent()) ?? "").trim();
    const labels = await legend.locator("span").allTextContents();
    const idx = labels.indexOf(name);
    assert.ok(idx >= 0, `${name} is not in the chart's legend`);
    await row.click();
    assert.equal(await legend.nth(idx).getAttribute("aria-pressed"), "true");
    assert.match((await row.getAttribute("class")) ?? "", /is-on/);
    await row.click();
    assert.equal(await legend.nth(idx).getAttribute("aria-pressed"), "false");
  } finally {
    await browser.close();
  }
});

test("invariants: the Map is laid out as the approved demo, and a plain click on a node selects it into the address, a second releases it, Esc clears", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/map`);
    await page.waitForSelector(".mp-node", { timeout: 10000 });
    const layout = await page.evaluate(() => ({
      title: document.querySelector("#page h1")?.textContent,
      tiles: [...document.querySelectorAll(".nx-kpis .nx-kpi__label")].map(
        (l) => l.textContent,
      ),
      blocks: [...document.querySelectorAll("#page section.nx-card h2")].map(
        (x) => x.textContent,
      ),
      side: document.querySelector(".mp-side h3")?.textContent,
      half: [...document.querySelectorAll(".mp-grid > section")].map((s) =>
        Math.round(s.getBoundingClientRect().top),
      ),
    }));
    assert.equal(layout.title, "Map");
    assert.deepEqual(layout.tiles, [
      "Stacks in the graph",
      "Connections",
      "Firewall",
      "Stale images",
    ]);
    assert.deepEqual(layout.blocks, [
      "Topology",
      "Capacity",
      "Disk growth",
      "Stale images",
    ]);
    assert.equal(layout.side, "Nothing selected");
    assert.equal(
      new Set(layout.half).size,
      1,
      "Capacity and Disk growth are not side by side",
    );

    const node = page.locator('.mp-node[data-stack="gateway"]');
    await node.locator(".mp-ring").click();
    assert.equal(await node.getAttribute("aria-pressed"), "true");
    assert.match(page.url(), /select=gateway/);
    assert.match(
      (await page.locator(".mp-side").textContent()) ?? "",
      /1 selected/,
    );
    // A second node joins the selection with a plain click.
    const other = page.locator('.mp-node[data-stack="admin"]');
    await other.locator(".mp-ring").click();
    assert.equal(await other.getAttribute("aria-pressed"), "true");
    assert.equal(await node.getAttribute("aria-pressed"), "true");
    await other.locator(".mp-ring").click();
    assert.equal(await other.getAttribute("aria-pressed"), "false");
    await page.mouse.move(5, 5);
    await page.keyboard.press("Escape");
    assert.equal(await node.getAttribute("aria-pressed"), "false");
    assert.doesNotMatch(page.url(), /select=/);
    // List view: one row per stack, the graph hidden; back again.
    const seg = page.locator('.nx-seg[aria-label="View"] button');
    await seg.filter({ hasText: "List" }).click();
    await page.locator(".mp-list tbody tr").first().waitFor({ timeout: 3000 });
    assert.equal(await page.locator(".mp-topo").isVisible(), false);
    await seg.filter({ hasText: "Graph" }).click();
    assert.equal(await page.locator(".mp-topo").isVisible(), true);
  } finally {
    await browser.close();
  }
});

// ── review of redesign-371-metrics / redesign-371-map (2026-10-03): the
// findings a whole screen proves. Every case's name starts with "review
// metrics" so a fix's own run is INVARIANTS_ONLY="review metrics".

test("invariants: review metrics — Metrics and the Map fit a phone: no table scrolls sideways, Stale images are cards, Disk growth rows are two lines, KPI context lines are whole", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    /** No KPI context line cut off with an ellipsis. */
    const cutCtx = () =>
      page.evaluate(() =>
        [...document.querySelectorAll(".nx-kpi__ctx")]
          .filter((e) => e.scrollWidth > e.clientWidth + 1)
          .map((e) => e.textContent),
      );
    for (const [path, ready] of [
      ["/charts", "#chart-drives tbody tr"],
      ["/charts?tab=traffic", "#chart-hostnames tbody tr"],
      ["/map", "#stale tbody tr"],
    ]) {
      await page.setViewportSize({ width: 1894, height: 1000 });
      await page.goto(`${BASE}${path}`);
      await page.waitForSelector(ready);
      await page.waitForSelector(".nx-kpi:not([data-loading])");
      assert.deepEqual(await cutCtx(), [], `${path} at 1894: cut KPI lines`);
      await page.setViewportSize({ width: 390, height: 900 });
      await page.waitForTimeout(300);
      const r = await page.evaluate(() => ({
        wraps: [...document.querySelectorAll(".kp-table-wrap")]
          .filter((e) => e.scrollWidth > e.clientWidth + 1)
          .map(
            (e) =>
              `${e.closest("[id]")?.id}: ${e.scrollWidth - e.clientWidth}px`,
          ),
        page:
          document.documentElement.scrollWidth -
          document.documentElement.clientWidth,
        cols: getComputedStyle(
          /** @type {Element} */ (document.querySelector(".nx-kpis")),
        ).gridTemplateColumns.split(" ").length,
      }));
      assert.deepEqual(r.wraps, [], `${path} at 390: a table scrolls sideways`);
      assert.equal(r.page, 0, `${path} at 390: the page scrolls sideways`);
      assert.equal(r.cols, 2, `${path} at 390: two KPI tiles a row`);
      assert.deepEqual(await cutCtx(), [], `${path} at 390: cut KPI lines`);
    }
    // The Map at 390: Stale images as cards, every Update in reach.
    assert.equal(
      await page
        .locator("#stale thead")
        .evaluate((e) => getComputedStyle(e).display),
      "none",
      "Stale images keep their head row on a phone",
    );
    const reach = await page
      .locator("#stale [data-pin-update]")
      .evaluateAll((els) =>
        els.map((e) => {
          const b = e.getBoundingClientRect();
          return b.width > 0 && b.left >= 0 && b.right <= 390;
        }),
      );
    assert.ok(reach.length > 0, "no Update button on the phone");
    assert.ok(reach.every(Boolean), "an Update button is out of reach");
    // Disk growth at 390: no head row, two lines a row.
    await page.waitForSelector(".mp-growth[role=row] .mp-g-full");
    const g = await page.evaluate(() => {
      const head = document.querySelector(
        '[aria-label="Disk growth per filesystem"] .mp-cap--head',
      );
      const rows = [...document.querySelectorAll(".mp-growth[role=row]")]
        .filter((r) => !r.classList.contains("mp-cap--head"))
        .map((r) => {
          const top = (/** @type {string} */ s) =>
            Math.round(
              /** @type {Element} */ (
                r.querySelector(s)
              ).getBoundingClientRect().top,
            );
          return {
            line1: top(".mp-g-name") === top(".mp-g-full"),
            line2: top(".mp-g-bar") === top(".mp-g-trend"),
            below: top(".mp-g-bar") > top(".mp-g-name"),
            label: r.querySelector(".mp-g-full .mp-inl")?.checkVisibility(),
          };
        });
      return { head: head ? getComputedStyle(head).display : "none", rows };
    });
    assert.equal(g.head, "none", "Disk growth keeps its head row on a phone");
    assert.ok(g.rows.length > 0);
    for (const row of g.rows)
      assert.deepEqual(row, {
        line1: true,
        line2: true,
        below: true,
        label: true,
      });
  } finally {
    await browser.close();
  }
});

test("invariants: review metrics — the tiles are skeletons until the read lands, and a failed read says not read instead of pulsing", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    /** @type {(v?: unknown) => void} */
    let release = () => {};
    const held = new Promise((r) => (release = r));
    await page.route("**/data/charts**", async (route) => {
      await held;
      await route.fulfill({
        status: 502,
        contentType: "application/json",
        body: JSON.stringify({
          error: "Prometheus did not answer",
          fix: "check Prometheus",
        }),
      });
    });
    await page.goto(`${BASE}/charts`);
    await page.waitForSelector(".nx-kpi[data-loading]");
    const loading = await page.evaluate(() => ({
      tiles: document.querySelectorAll(".nx-kpi[data-loading]").length,
      skeletons: document.querySelectorAll(
        ".mk-card .kp-skeleton[data-kp-state=loading]",
      ).length,
    }));
    assert.equal(loading.tiles, 6, "six tile skeletons before the read");
    assert.ok(loading.skeletons >= 6, "every chart card a skeleton");
    release();
    await page.waitForSelector(".mk-card-error");
    await page.waitForTimeout(300);
    const after = await page.evaluate(() => ({
      loading: document.querySelectorAll(".nx-kpi[data-loading]").length,
      values: [...document.querySelectorAll(".nx-kpi__value")].map((e) =>
        e.textContent?.trim(),
      ),
      ctx: [...document.querySelectorAll(".nx-kpi__ctx")].map((e) =>
        e.textContent?.trim(),
      ),
      band: document.querySelectorAll(".hk-attention > *, .nx-attention > *")
        .length,
    }));
    assert.equal(after.loading, 0, "a tile still pulses after the failure");
    assert.deepEqual(after.values, Array(6).fill("—"));
    assert.deepEqual(after.ctx, Array(6).fill("not read"));
    assert.equal(after.band, 0, "the attention band kept an old alert");
  } finally {
    await browser.close();
  }
});

test("invariants: review metrics — hostname rows follow the chart (Esc and Show all clear them), the legend's totals are the table's, chart controls are Live view controls, the sixth line is dashed", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.setViewportSize({ width: 1894, height: 1000 });
    await page.goto(`${BASE}/charts?tab=traffic`);
    await page.waitForSelector("#chart-hostnames tbody tr");
    await page.waitForSelector("#chart-perhost .tc-legend__item");
    await page.mouse.move(0, 0);
    // The legend's total and the table's Requests are one number.
    const pairs = await page.evaluate(() => {
      const table = new Map(
        [...document.querySelectorAll("#chart-hostnames tbody tr")].map(
          (tr) => [
            /** @type {HTMLElement} */ (tr).dataset.kpRowKey || "no hostname",
            tr.querySelector("td.mk-n")?.textContent?.trim(),
          ],
        ),
      );
      return [
        ...document.querySelectorAll("#chart-perhost .tc-legend__item"),
      ].map((b) => ({
        host: b.querySelector("span")?.textContent,
        legend: b.querySelector(".tc-num")?.textContent?.trim(),
        table: table.get(b.querySelector("span")?.textContent ?? ""),
        drive: /** @type {HTMLElement} */ (b).dataset.drive,
        row: /** @type {HTMLElement} */ (b).dataset.driveRow,
      }));
    });
    assert.ok(
      pairs.length >= 6,
      `six hostnames in the demo, got ${pairs.length}`,
    );
    for (const p of pairs) {
      assert.equal(p.legend, p.table, `${p.host}: legend and table differ`);
      assert.equal(p.drive, "chart-source");
      assert.equal(p.row, `perhost/${p.host}`);
    }
    assert.equal(
      await page
        .locator("#chart-perhost .tc-legend__reset")
        .getAttribute("data-drive"),
      "chart-show-all",
    );
    assert.equal(
      await page.locator(".tc-zoom__reset").first().getAttribute("data-drive"),
      "chart-zoom-reset",
    );
    // The sixth source's line is dashed, the first five are not.
    const dashes = await page
      .locator("#chart-perhost .tc-line")
      .evaluateAll((els) => els.map((e) => e.getAttribute("stroke-dasharray")));
    assert.deepEqual(dashes.slice(0, 5), Array(5).fill(null));
    assert.ok(dashes[5], "the sixth line repeats colour 1 undashed");
    // A plain click on a row turns it on; Esc on the chart clears it.
    const row = page.locator("#chart-hostnames tbody tr").first();
    const state = async () =>
      row.evaluate((r) => [
        r.classList.contains("is-on"),
        r.getAttribute("aria-pressed"),
      ]);
    await row.click();
    assert.deepEqual(await state(), [true, "true"], "the click did not take");
    await page.locator("#chart-perhost .tc-plot").focus();
    await page.keyboard.press("Escape");
    assert.deepEqual(await state(), [false, "false"], "Esc left the row on");
    await row.click();
    assert.deepEqual(await state(), [true, "true"]);
    await page.locator("#chart-perhost .tc-legend__reset").click();
    assert.deepEqual(await state(), [false, "false"], "Show all left it on");
  } finally {
    await browser.close();
  }
});

test("invariants: review metrics — a pointer resting on a chart keeps that chart through the 30 s refresh", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.clock.install();
    await page.setViewportSize({ width: 1894, height: 1000 });
    await page.goto(`${BASE}/charts`);
    await page.waitForSelector("#chart-cpu .tc-plot");
    await page.locator("#chart-cpu .tc-plot").hover();
    await page.evaluate(() => {
      /** @type {any} */ (window).__plot = document.querySelector(
        "#chart-cpu .tc-plot",
      );
    });
    await page.clock.runFor(31_000);
    await page.waitForTimeout(1500);
    assert.ok(
      await page.evaluate(
        () =>
          /** @type {any} */ (window).__plot ===
          document.querySelector("#chart-cpu .tc-plot"),
      ),
      "the refresh replaced the chart under the pointer",
    );
  } finally {
    await browser.close();
  }
});

test("invariants: review metrics — the wording is the demo's, Connections is green and amber, no chip row, and a touch screen shows no pointer hints", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.setViewportSize({ width: 1894, height: 1000 });
    await page.goto(`${BASE}/charts`);
    await page.waitForSelector("#chart-drives tbody tr");
    const sys = await page.evaluate(() => ({
      back: [
        ...document.querySelectorAll("[data-drive=metrics-drive-backups]"),
      ].map((e) => e.textContent?.trim()),
      foot: document.querySelector("#chart-drives .nx-card__foot")?.textContent,
      ct: [
        ...document.querySelectorAll("#chart-ctmem .tc-legend__item > span"),
      ].map((e) => e.textContent),
    }));
    assert.deepEqual(sys.back, ["Back up devices now"]);
    assert.match(sys.foot ?? "", /smartctl via node-exporter/);
    assert.ok(
      sys.ct.includes("CT 903 · films"),
      `the container legend names its stack: ${sys.ct}`,
    );
    await page.goto(`${BASE}/map`);
    await page.waitForSelector(
      '.nx-kpi[data-key="connections"]:not([data-loading])',
    );
    const conn = await page.evaluate(() => {
      const c = document.querySelector(
        '.nx-kpi[data-key="connections"] .nx-kpi__ctx',
      );
      return {
        ok: c?.querySelector(".nx-dot--ok")?.textContent,
        warn: c?.querySelector(".nx-dot--warn")?.textContent,
      };
    });
    assert.match(conn.ok ?? "", /enforced$/);
    assert.match(conn.warn ?? "", /open$/);
    assert.equal(await page.locator(".mp-stackkey").count(), 0);
    // A touch screen: no hover, so no "hover"/"point at" hints.
    const touch = await browser.newContext({
      viewport: { width: 390, height: 900 },
      isMobile: true,
      hasTouch: true,
    });
    const t = await freshPage(touch);
    await t.goto(`${BASE}/charts`);
    await t.waitForSelector(".tc-legend__hint");
    const hints = await t.evaluate(() => ({
      hover: matchMedia("(hover: none)").matches,
      pointer: [
        ...document.querySelectorAll(".tc-hint--pointer, .mk-pointer-only"),
      ].some((e) => /** @type {HTMLElement} */ (e).checkVisibility()),
      touch: [...document.querySelectorAll(".tc-hint--touch")].some((e) =>
        /** @type {HTMLElement} */ (e).checkVisibility(),
      ),
      keys: [...document.querySelectorAll(".mk-page .nx-keys")].some((e) =>
        /** @type {HTMLElement} */ (e).checkVisibility(),
      ),
    }));
    assert.deepEqual(hints, {
      hover: true,
      pointer: false,
      touch: true,
      keys: false,
    });
  } finally {
    await browser.close();
  }
});

// ── redesign-config (3.71.0): Firewall, Settings (with Sign-in), Presets ──
// Kenny's approved demos (~/.local/share/homelab/redesign-3.71/
// firewall.html, settings.html, presets.html), implemented exactly; these
// pin what each page must always do.

test("invariants: redesign-config Firewall shows its totals, one attention row per unprotected stack, and lights stacks up with plain clicks", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1100 },
    });
    const page = await freshPage(context);
    const fw = await page.evaluate(async () =>
      (await fetch("/data/firewall")).json(),
    );
    await page.goto(`${BASE}/firewall`);
    await page.locator("tr.fw-row").first().waitFor({ timeout: 10000 });
    const labels = await page
      .locator(".nx-kpis .nx-kpi__label")
      .allTextContents();
    assert.deepEqual(
      labels.map((l) => l.trim().toLowerCase()),
      ["protected", "unprotected", "open paths", "rules"],
    );
    const rulesValue = await page
      .locator('.nx-kpi:has(.nx-kpi__label:text-is("Rules")) .nx-kpi__value')
      .innerText();
    assert.equal(rulesValue.trim(), String(fw.matrix.rules.length));
    const undeclared = fw.stacks.filter(
      (/** @type {any} */ s) => !s.declared && !s.live_enforced,
    ).length;
    const bad = await page
      .locator(".nx-attention .kp-alert--destructive")
      .count();
    assert.equal(bad, undeclared, "one red row per stack with no firewall");
    // A plain click lights a stack up; the squares not in its row or
    // column fade; a second click turns it off again; no modifier keys.
    const name = fw.matrix.stacks[0];
    const row = page.locator(`tr.fw-row[data-stack="${name}"]`);
    await row.click({ position: { x: 300, y: 10 } });
    const lit = () =>
      page.evaluate(() => ({
        pressed: [
          ...document.querySelectorAll('.fw-mx__row[aria-pressed="true"]'),
        ].map((b) => b.textContent?.trim()),
        dim: document.querySelector(".fw-mx")?.classList.contains("has-focus"),
      }));
    assert.deepEqual(await lit(), { pressed: [name], dim: true });
    assert.ok(new URL(page.url()).searchParams.get("lit")?.includes(name));
    await row.click({ position: { x: 300, y: 10 } });
    assert.deepEqual(await lit(), { pressed: [], dim: false });
    // Two at once, then Esc resets.
    await page.locator(".fw-mx__col").nth(0).click();
    await page.locator(".fw-mx__col").nth(1).click();
    assert.equal((await lit()).pressed.length, 2);
    await page.keyboard.press("Escape");
    assert.deepEqual(await lit(), { pressed: [], dim: false });
  } finally {
    await browser.close();
  }
});

test("invariants: redesign-config Firewall pins a square with its story and outlines the squares a hovered rule decides", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1100 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/firewall`);
    await page.locator(".fw-mx__cell--some").first().waitFor({
      timeout: 10000,
    });
    const cell = page.locator(".fw-mx__cell--open").first();
    const label = await cell.getAttribute("aria-label");
    await cell.click();
    assert.equal(await cell.getAttribute("aria-pressed"), "true");
    const [from, to] = String(label).split(":")[0].split(" to ");
    const story = await page.locator(".fw-path").innerText();
    assert.ok(story.includes(`${from} → ${to}`), story);
    assert.ok(/every port/.test(story), story);
    assert.equal(new URL(page.url()).searchParams.get("cell"), `${from}>${to}`);
    // A rule naming a managed stack outlines its squares while hovered.
    const named = page.locator("tr.fw-rule:has(.nx-chip)").first();
    await named.hover();
    assert.ok(
      (await page.locator(".fw-mx__cell.is-rule").count()) > 0,
      "hovering a rule outlined no square",
    );
    // Inbound beside outbound, in Proxmox's order.
    const sides = await page.$$eval(".fw-side h3", (hs) =>
      hs.map((x) => x.firstChild?.textContent),
    );
    assert.deepEqual(sides, ["Inbound", "Outbound"]);
  } finally {
    await browser.close();
  }
});

test("invariants: redesign-config Settings stages an edit inline, counts it in the header, reviews it, and Discard can be undone", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1100 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/settings`);
    const edit = page.locator('button[data-key="backup_concurrency"]');
    await edit.waitFor({ timeout: 10000 });
    assert.equal(
      (await page.locator("#settings-save").innerText()).trim(),
      "Check and write",
    );
    assert.ok(await page.locator("#settings-save").isDisabled());
    await edit.click();
    const input = page.locator("#key-backup-concurrency");
    await input.fill("99");
    await page.locator("#key-stage").click();
    assert.ok(
      await page.locator(".st-err").isVisible(),
      "an out-of-range value was staged without a word",
    );
    await input.fill("2");
    await input.press("Enter");
    assert.equal(
      (await page.locator("#settings-save").innerText()).trim(),
      "Check and write 1…",
    );
    assert.equal(
      (await page.locator(".nx-head-actions .cf-chip").innerText()).trim(),
      "1 change staged",
    );
    assert.ok(
      await page
        .locator('[data-key="backup_concurrency"].st-set--staged')
        .count(),
    );
    await page.locator("#settings-save").click();
    const dialog = page.locator("dialog#settings-review[open]");
    await dialog.waitFor({ timeout: 3000 });
    assert.match(await dialog.innerText(), /backup_concurrency/);
    await dialog.locator("button", { hasText: "Back" }).click();
    // redesign-config-5: leaving the page and coming back keeps what is
    // staged (for this tab), and the page says so.
    // An in-app link, as the nav's: the page unmounts, the tab stays.
    await page.evaluate(() => {
      const a = document.createElement("a");
      a.href = "/firewall";
      a.id = "e2e-away";
      a.textContent = "away";
      document.body.append(a);
    });
    await page.locator("#e2e-away").click();
    await page.waitForURL("**/firewall", { timeout: 5000 });
    await page.locator("tr.fw-row").first().waitFor({ timeout: 10000 });
    await page.goBack();
    await page.locator('button[data-key="backup_concurrency"]').waitFor({
      timeout: 10000,
    });
    await page
      .locator("#settings-save", { hasText: "Check and write 1…" })
      .waitFor({ timeout: 5000 });
    await page
      .locator(".nx-head-actions button", { hasText: "Discard" })
      .click();
    assert.ok(await page.locator("#settings-save").isDisabled());
    await page.locator(".kp-toast button", { hasText: "Undo" }).click();
    assert.equal(
      (await page.locator("#settings-save").innerText()).trim(),
      "Check and write 1…",
      "Undo did not bring the staged change back",
    );
    // Keys the dashboard must never change offer the ssh way instead.
    const how = page.locator(
      '[data-drive="settings-how-to"][data-drive-row="listen"]',
    );
    await how.click();
    assert.match(
      await page.locator(".st-howto").innerText(),
      /ssh pve, then set listen/,
    );
  } finally {
    await browser.close();
  }
});

test("invariants: redesign-config Sign-in holds the passkeys beside the machine tokens, and /passkeys opens it", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/passkeys`);
    await page.waitForURL("**/settings?section=sign-in", { timeout: 5000 });
    await page.locator("#signin [data-token]").first().waitFor({
      timeout: 10000,
    });
    const heads = await page.$$eval("#signin .si-col h3", (hs) =>
      hs.map((x) => x.textContent?.trim()),
    );
    assert.deepEqual(heads, ["Passkeys", "Machine tokens"]);
    assert.equal(
      await page
        .locator("#signin button", { hasText: "Register a passkey" })
        .count(),
      1,
    );
    const top = await page
      .locator("#signin")
      .evaluate((e) => Math.round(e.getBoundingClientRect().top));
    assert.ok(top < 400, `Sign-in is not scrolled into view (top ${top})`);
    // The side list names both dashboard sections and every host.toml group.
    const side = await page.locator(".st-side a").allTextContents();
    assert.ok(
      side[0].startsWith("Working copy") && side[1].startsWith("Sign-in"),
    );
    assert.ok(side.length > 5, `${side}`);
  } finally {
    await browser.close();
  }
});

test("invariants: redesign-config Presets is a gallery: one card per preset, the empty one last, a search, and Use this preset opens the wizard on it", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    const read = await page.evaluate(async () =>
      (await fetch("/data/presets")).json(),
    );
    const withApps = read.presets.filter(
      (/** @type {any} */ p) => p.apps.length > 0,
    );
    assert.ok(withApps.length > 2, "the fixture has no presets");
    await page.goto(`${BASE}/presets`);
    await page.locator("article.ps-card[data-preset]").first().waitFor({
      timeout: 10000,
    });
    assert.equal(
      await page.locator("article.ps-card[data-preset]").count(),
      withApps.length,
    );
    assert.equal(
      await page.locator(".ps-gallery > article:last-child h3").innerText(),
      "Empty stack",
    );
    const count = await page.locator(".ps-tb .cf-count").innerText();
    assert.ok(
      count.startsWith(`${withApps.length} presets · next free container`),
      count,
    );
    await page.locator(".ps-tb .nx-search").fill("vaapi");
    assert.equal(await page.locator("article.ps-card[data-preset]").count(), 1);
    assert.equal(
      (await page.locator(".ps-tb .cf-count").innerText()).trim(),
      `1 of ${withApps.length} presets`,
    );
    const name = await page
      .locator("article.ps-card[data-preset]")
      .getAttribute("data-preset");
    await page.locator(`button[data-preset-use="${name}"]`).click();
    const wizard = page.locator("dialog#new-stack-dialog[open]");
    await wizard.waitFor({ timeout: 5000 });
    assert.equal(
      await wizard.locator("select").first().inputValue(),
      name,
      "the wizard did not open on the chosen preset",
    );
  } finally {
    await browser.close();
  }
});

test("invariants: redesign-config pages show their skeleton from the first frame and never scroll sideways on a phone", async () => {
  const browser = await launch();
  try {
    const bad = [];
    for (const width of [1894, 390]) {
      const context = await browser.newContext({
        viewport: { width, height: 900 },
      });
      const page = await freshPage(context);
      // Each page's skeleton is its final geometry: the matrix's 5×5
      // squares and the four KPI tiles, the host.toml group cards, eight
      // preset cards.
      /** @type {Record<string, [string, number][]>} */
      const shapes = {
        firewall: [
          [".fw-mx__cell--sk", 25],
          [".nx-kpi[data-loading]", 4],
        ],
        settings: [[".st-grp--sk", 4]],
        presets: [[".ps-card--sk", 8]],
      };
      /** The content each page draws once its data is in. */
      const filled = {
        firewall: "tr.fw-row",
        settings: "button[data-key]",
        presets: "article.ps-card[data-preset]",
      };
      for (const path of /** @type {const} */ ([
        "firewall",
        "settings",
        "presets",
      ])) {
        // Hold every read until the first frame is checked: what the
        // page shows then is what it shows before any answer arrives.
        /** @type {() => void} */
        let release = () => {};
        const held = new Promise((r) => (release = () => r(undefined)));
        await page.route("**/data/**", async (r) => {
          await held;
          await r.continue().catch(() => {});
        });
        await page.goto(`${BASE}/${path}`);
        for (const [sel, n] of shapes[path]) {
          const got = await page.locator(`#page ${sel}`).count();
          if (got < n)
            bad.push(
              `${width}px /${path}: ${got} of ${n} ${sel} in the first frame`,
            );
        }
        release();
        await page.unroute("**/data/**");
        await page.locator(`#page ${filled[path]}`).first().waitFor({
          timeout: 10000,
        });
        const over = await page.evaluate(
          () => document.documentElement.scrollWidth - innerWidth,
        );
        if (over > 1) bad.push(`${width}px /${path}: ${over}px sideways`);
      }
      await context.close();
    }
    assert.deepEqual(bad, [], bad.join("; "));
  } finally {
    await browser.close();
  }
});

test("invariants: redesign-config every control Firewall, Settings and Presets draw carries a declared Live view id", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1100 },
    });
    const page = await freshPage(context);
    const declared = new Set(
      await page.evaluate(async () => {
        const m = await import("/js/drivable.js");
        return m.controls().map((/** @type {any} */ c) => c.id);
      }),
    );
    const bad = [];
    /** @type {Record<string, string>} what each page draws once its data is in */
    const filled = {
      firewall: "tr.fw-rule",
      settings: "button[data-key]",
      presets: "article.ps-card[data-preset]",
    };
    for (const path of ["firewall", "settings", "presets"]) {
      await page.goto(`${BASE}/${path}`);
      await page.locator(`#page ${filled[path]}`).first().waitFor({
        timeout: 10000,
      });
      // Open what folds, so the controls inside are on screen too.
      if (path === "firewall") await page.locator("tr.fw-rule").first().click();
      if (path === "settings")
        await page.locator('[data-drive="settings-fold-all"]').click();
      const found = await page.$$eval(
        "#page a[href], #page button, #page summary, #page input, #page select, #page [tabindex]:not([tabindex='-1']), #page [role=button]",
        (els) =>
          els
            .filter(
              (e) => e.getClientRects().length > 0 && !e.closest("dialog"),
            )
            .filter(
              (e) => !e.closest("[data-kp-datatable], .nx-crumbs, .nx-tip"),
            )
            .map((e) => ({
              drive: /** @type {HTMLElement} */ (e).dataset.drive ?? null,
              form: /** @type {HTMLElement} */ (e).dataset.driveForm ?? null,
              label: (
                e.getAttribute("aria-label") ||
                e.textContent ||
                e.tagName
              )
                .trim()
                .slice(0, 40),
            })),
      );
      for (const f of found)
        if (!f.drive || !declared.has(f.drive))
          bad.push(`/${path}: "${f.label}" (${f.drive ?? "no id"})`);
    }
    assert.deepEqual(
      bad,
      [],
      `controls without a declared Live view id:\n${bad.join("\n")}`,
    );
  } finally {
    await browser.close();
  }
});

// ── redesign-config review (3.71.0 senior review of this branch) ────────

test("invariants: redesign-config review Firewall on a phone: no Stacks cell draws over its neighbour, no KPI context is cut, and the rules toolbar reads toggles, search, count", async () => {
  const browser = await launch();
  try {
    const phone = await browser.newContext({
      viewport: { width: 390, height: 900 },
    });
    let page = await freshPage(phone);
    await page.goto(`${BASE}/firewall`);
    await page.locator("tr.fw-row").first().waitFor({ timeout: 10000 });
    const spill = await page.$$eval(".fw-stacks tbody td", (tds) =>
      tds.flatMap((td) => {
        if (!td.getClientRects().length) return [];
        const b = td.getBoundingClientRect();
        return [...td.querySelectorAll("*")]
          .filter((x) => x.getClientRects().length)
          .map((x) => x.getBoundingClientRect())
          .filter((r) => r.right > b.right + 1 || r.left < b.left - 1)
          .map(
            () =>
              `${td.closest("tr")?.getAttribute("data-stack")} cell ${/** @type {HTMLTableCellElement} */ (td).cellIndex}`,
          );
      }),
    );
    assert.deepEqual([...new Set(spill)], [], "content drawn over a neighbour");
    const clipped = await page.$eval(
      "#fw-stacks .kp-table-wrap",
      (w) => w.scrollWidth - w.clientWidth,
    );
    assert.ok(clipped <= 1, `the Stacks table is cut by ${clipped}px`);
    const cut = await page.$$eval(".nx-kpi__ctx", (cs) =>
      cs
        .filter(
          (c) =>
            c.scrollWidth > c.clientWidth + 1 ||
            c.scrollHeight > c.clientHeight + 1,
        )
        .map((c) => c.textContent),
    );
    assert.deepEqual(cut, [], "a KPI context is truncated");
    await phone.close();

    const wide = await browser.newContext({
      viewport: { width: 1894, height: 1100 },
    });
    page = await freshPage(wide);
    await page.goto(`${BASE}/firewall`);
    await page.locator("tr.fw-rule").first().waitFor({ timeout: 10000 });
    const order = await page.$eval("#fw-rules .nx-tb", (tb) => {
      const x = (/** @type {Element | null} */ e) =>
        e ? Math.round(e.getBoundingClientRect().left) : -1;
      return {
        toggles: x(tb.querySelector("button[data-v='ACCEPT']")),
        search: x(tb.querySelector("input[type=search]")),
        count: x(tb.querySelector(".cf-count")),
      };
    });
    assert.ok(
      order.toggles >= 0 &&
        order.toggles < order.search &&
        order.search < order.count,
      `toolbar order ${JSON.stringify(order)}`,
    );
  } finally {
    await browser.close();
  }
});

test("invariants: redesign-config review a rule's details count the whole stack, and Move up and Disable stage the change in the stack's firewall editor", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1100 },
    });
    const page = await freshPage(context);
    const fw = await page.evaluate(async () =>
      (await fetch("/data/firewall")).json(),
    );
    /** @type {Record<string, number>} */
    const per = {};
    for (const r of fw.matrix.rules) per[r.stack] = (per[r.stack] ?? 0) + 1;
    const stack = Object.keys(per).find((n) =>
      fw.matrix.rules.some(
        (/** @type {any} */ r) => r.stack === n && r.dir === "in" && r.n > 1,
      ),
    );
    assert.ok(stack, "the fixture has no stack with a later inbound rule");
    await page.goto(`${BASE}/firewall?stack=${stack}`);
    const row = page.locator('.fw-side[data-dir="in"] tr.fw-rule').last();
    await row.waitFor({ timeout: 10000 });
    await row.click();
    const more = page.locator("tr.fw-rule-more");
    await more.waitFor({ timeout: 3000 });
    const text = await more.innerText();
    const m = /rule (\d+) of (\d+) \((\d+) inbound\)/.exec(text);
    assert.ok(m, text);
    assert.equal(Number(m[2]), per[/** @type {string} */ (stack)]);
    assert.ok(Number(m[1]) <= Number(m[2]), text);
    await more.locator("a", { hasText: "Move up" }).waitFor({ timeout: 3000 });
    await more.locator("a", { hasText: "Disable" }).click();
    await page.waitForURL(`**/stacks/${stack}/settings**`, { timeout: 5000 });
    await page.locator("tr.rule-off.rule-staged").waitFor({ timeout: 10000 });
    assert.equal(
      (await page.locator("#fw-dirty").innerText()).trim(),
      "Changed; not committed.",
    );
    assert.ok(
      !new URL(page.url()).searchParams.has("do"),
      "the address still asks to stage it again",
    );
  } finally {
    await browser.close();
  }
});

test("invariants: redesign-config review one failed read is one alert with one Try again, and Fetch now says it runs", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    const down = (/** @type {import("playwright").Route} */ r) =>
      r.fulfill({
        status: 503,
        contentType: "application/json",
        body: JSON.stringify({ error: "the host does not answer" }),
      });
    await page.route("**/data/firewall", down);
    await page.goto(`${BASE}/firewall`);
    await page.locator("#page [role=alert]").first().waitFor({
      timeout: 10000,
    });
    assert.equal(await page.locator("#page [role=alert]").count(), 1);
    assert.equal(
      await page.locator("#page button", { hasText: "Try again" }).count(),
      1,
    );
    await page.unroute("**/data/firewall");

    for (const p of ["**/data/repo", "**/data/host-settings", "**/data/tokens"])
      await page.route(p, down);
    await page.goto(`${BASE}/settings`);
    const alert = page.locator("#page [role=alert]");
    await alert
      .filter({ hasText: "the tokens" })
      .filter({ hasText: "the host settings" })
      .filter({ hasText: "the working copy" })
      .waitFor({ timeout: 10000 });
    assert.equal(await alert.count(), 1);
    for (const p of ["**/data/repo", "**/data/host-settings", "**/data/tokens"])
      await page.unroute(p);
    await page.locator("#page button", { hasText: "Try again" }).click();
    await page.locator("button[data-key]").first().waitFor({ timeout: 10000 });
    await alert.waitFor({ state: "detached", timeout: 5000 });

    // Fetch now while the remote is slow: it says so, then comes back.
    /** @type {() => void} */
    let release = () => {};
    const held = new Promise((r) => (release = () => r(undefined)));
    await page.route("**/data/repo/sync", async (r) => {
      await held;
      await r.continue().catch(() => {});
    });
    const fetchNow = page.locator("#repo-sync");
    await fetchNow.click();
    await page
      .locator("#repo-sync", { hasText: "Fetching the remote…" })
      .waitFor({ timeout: 3000 });
    assert.ok(await fetchNow.isDisabled());
    release();
    await page
      .locator("#repo-sync", { hasText: "Fetch now" })
      .waitFor({ timeout: 10000 });
  } finally {
    await browser.close();
  }
});

test("invariants: redesign-config review a Presets search marks the match inside a chip without splitting its word", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/presets`);
    await page.locator(".ps-apps .nx-chip").first().waitFor({
      timeout: 10000,
    });
    const chips = () =>
      page.$$eval(".ps-apps .nx-chip", (cs) =>
        Object.fromEntries(
          cs.map((c) => [
            c.textContent,
            Math.round(c.getBoundingClientRect().width),
          ]),
        ),
      );
    const before = await chips();
    await page.locator(".ps-tb .nx-search").fill("a");
    await page.locator(".ps-apps .nx-mark").first().waitFor({ timeout: 3000 });
    const after = await chips();
    const wider = Object.entries(after).filter(
      ([t, w]) => before[t] === undefined || Math.abs(w - before[t]) > 1,
    );
    assert.deepEqual(wider, [], "a chip's word was split by the highlight");
  } finally {
    await browser.close();
  }
});

test("invariants: Apps loads as its grouped board, its group labels are the body face at weight 500, and the legend says deploying (redesign-stacks)", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    // Hold the tiles back so the loading board can be read.
    await page.route("**/data/tiles*", async (route) => {
      await new Promise((r) => setTimeout(r, 1500));
      await route.continue();
    });
    await page.goto(`${BASE}/apps`);
    const skel = page.locator(".ap-board .ap-grp--skeleton");
    await skel.first().waitFor({ timeout: 5000 });
    assert.ok(
      (await skel.count()) > 1,
      "the loading state is not the grouped board",
    );
    const groups = page.locator(
      ".ap-board .ap-grp:not(.ap-grp--skeleton) > h2",
    );
    await groups.first().waitFor({ timeout: 10000 });
    const font = await groups.first().evaluate((e) => {
      const cs = getComputedStyle(e);
      return {
        family: cs.fontFamily,
        body: getComputedStyle(document.body).fontFamily,
        weight: cs.fontWeight,
      };
    });
    assert.equal(font.family, font.body, "a group label is not the body face");
    assert.equal(font.weight, "500");
    const text = await page.locator(".ap-page").innerText();
    assert.match(text, /deploying/);
    assert.doesNotMatch(text, /a known outage/);
    // The live status sits beside the title, not with the buttons.
    assert.equal(
      await page.locator(".nx-head .title-row > h1 + .nx-live").count(),
      1,
    );
  } finally {
    await browser.close();
  }
});

// ── redesign-stackhub (3.71.0, Kenny approved the stack hub demo
// 2026-10-03: flows/stack-hub.html + stack.html, FLOWS.md §1.3) — the
// whole-screen shape of one stack's hub, in the demo host's own data. ────

test("invariants: stack hub — the header keeps Back up · Update · Deploy (primary) · More, with b u d . and a grouped More", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/stacks/gateway`);
    const actions = page.locator(".sh-head__actions");
    await actions.waitFor({ timeout: 10000 });
    const order = await actions.evaluate((a) =>
      [...a.children].map((c) => ({
        text: (c.matches(".nx-more")
          ? c.firstElementChild
          : c
        )?.textContent?.trim(),
        primary: c.classList.contains("kp-button--primary"),
        action: /** @type {HTMLElement} */ (c).dataset.action ?? null,
      })),
    );
    assert.deepEqual(
      order.map((o) => o.text),
      ["Back up", "Update", "Deploy", "More ▾"],
    );
    assert.deepEqual(
      order.map((o) => o.primary),
      [false, false, true, false],
    );
    assert.deepEqual(
      order.slice(0, 3).map((o) => o.action),
      ["backup", "update", "deploy"],
    );
    // `.` opens More, grouped as the demo groups it.
    await page
      .locator("body")
      .click({ position: { x: 5, y: 500 }, timeout: 5000 });
    await page.keyboard.press(".");
    const groups = await page
      .locator(".sh-head .nx-menu:not([hidden]) .nx-menu__group")
      .allTextContents();
    assert.deepEqual(groups, ["Data", "Change", "Pause", "Tools", "Remove"]);
    for (const a of ["restore", "deploy-commit", "resize", "guards", "disable"])
      assert.equal(
        await page.locator(`.sh-head .nx-menu [data-action="${a}"]`).count(),
        1,
        `More has no ${a}`,
      );
    await page.keyboard.press("Escape");
    assert.equal(await page.locator(".sh-head .nx-menu").isHidden(), true);
    // redesign-openpoints-3 (the shared ui.js menu): Esc gives the focus
    // back to More, and a click outside closes it.
    assert.equal(
      await page.evaluate(
        () =>
          /** @type {HTMLElement} */ (document.activeElement)?.dataset.drive,
      ),
      "stack-more",
    );
    await page.keyboard.press("Enter");
    assert.equal(await page.locator(".sh-head .nx-menu").isVisible(), true);
    await page
      .locator("body")
      .click({ position: { x: 5, y: 500 }, timeout: 5000 });
    assert.equal(await page.locator(".sh-head .nx-menu").isHidden(), true);
    // `b` opens the Back up dialog, as the button does.
    await page.keyboard.press("b");
    const dialog = page.locator("dialog#action-dialog[open]");
    await dialog.waitFor({ timeout: 5000 });
    assert.match(
      (await dialog.locator(".kp-dialog__title").textContent()) ?? "",
      /Back up/,
    );
  } finally {
    await browser.close();
  }
});

test("invariants: stack hub — six tabs; Overview has five KPI tiles, Is it healthy? and Recent history with who did it", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/stacks/gateway`);
    // The list shows skeleton lines until the fleet is read: wait for a
    // real row, not the first line.
    await page
      .locator('#healthy .sh-checks li[data-key="apps"]')
      .waitFor({ timeout: 10000 });
    await page
      .locator(".sh-feed li[data-who]")
      .first()
      .waitFor({ timeout: 10000 });
    const r = await page.evaluate(() => ({
      tabs: [...document.querySelectorAll(".sh-tabs .kp-tab")].map((t) =>
        (t.firstChild?.textContent ?? "").trim(),
      ),
      kpis: [...document.querySelectorAll(".nx-kpis .nx-kpi__label")].map(
        (l) => l.textContent,
      ),
      links: [...document.querySelectorAll(".nx-kpis a.nx-kpi")].length,
      checks: [...document.querySelectorAll("#healthy .sh-checks li")].map(
        (l) => l.getAttribute("data-key"),
      ),
      who: [...document.querySelectorAll(".sh-feed li[data-who]")].map((l) =>
        l.getAttribute("data-who"),
      ),
      noEnv: !!document.querySelector('.nx-attention [data-key="no-env"]'),
    }));
    assert.deepEqual(r.tabs, [
      "Overview",
      "Logs",
      "Apps",
      "Backups",
      "History",
      "Settings",
    ]);
    assert.deepEqual(r.kpis, [
      "Apps up",
      "Restarts",
      "Last backup",
      "Errors in logs · 1 h",
      "Matches its files",
    ]);
    assert.equal(r.links, 4, "every tile but the drift one links into its tab");
    for (const k of [
      "online",
      "apps",
      "restarts",
      "disk",
      "backup",
      "env",
      "drift",
    ])
      assert.ok(r.checks.includes(k), `Is it healthy? has no ${k} row`);
    assert.ok(r.who.includes("claude"), "Recent history names no Claude row");
    assert.ok(r.who.includes("night"), "Recent history names no nightly row");
    assert.ok(r.noEnv, "the stack's no-env problem is not in its band");
  } finally {
    await browser.close();
  }
});

test("invariants: stack hub — Logs' side column turns an app or a level on and off with a plain click; Esc shows all again", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/stacks/gateway/logs`);
    const lines = page.locator(".sh-log .sh-log__line");
    await lines.first().waitFor({ timeout: 10000 });
    const total = await lines.count();
    assert.ok(total > 5, `only ${total} log lines`);
    assert.equal(
      await page.locator('[data-drive="stack-log-window"]').count(),
      4,
    );
    const errors = page.locator(
      '[data-drive="stack-log-filter"][data-drive-row="gateway/levels/e"]',
    );
    await errors.click({ timeout: 5000 });
    assert.equal(await errors.getAttribute("aria-pressed"), "false");
    assert.equal(await page.locator('.sh-log [data-lvl="e"]').count(), 0);
    const traefik = page.locator(
      '[data-drive="stack-log-filter"][data-drive-row="gateway/apps/traefik"]',
    );
    await traefik.click({ timeout: 5000 });
    assert.equal(await page.locator('.sh-log [data-src="traefik"]').count(), 0);
    assert.match(
      (await page.locator(".sh-count-text").textContent()) ?? "",
      new RegExp(`of ${total} lines`),
    );
    await page
      .locator("body")
      .click({ position: { x: 5, y: 900 }, timeout: 5000 });
    await page.keyboard.press("Escape");
    assert.equal(await lines.count(), total);
    assert.equal(await errors.getAttribute("aria-pressed"), "true");
  } finally {
    await browser.close();
  }
});

test("invariants: stack hub — History's Started by chips filter with a plain click, several at once", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/stacks/kp-soft/history`);
    const rows = page.locator(".sh-feed--full li[data-who]");
    await rows.first().waitFor({ timeout: 10000 });
    const all = await rows.count();
    await page
      .locator(
        '[data-drive="stack-history-who"][data-drive-row="kp-soft/claude"]',
      )
      .click({ timeout: 5000 });
    const who = await rows.evaluateAll((ls) =>
      ls.map((l) => l.getAttribute("data-who")),
    );
    assert.ok(who.length > 0 && who.every((w) => w === "claude"), who.join());
    await page
      .locator(
        '[data-drive="stack-history-who"][data-drive-row="kp-soft/night"]',
      )
      .click({ timeout: 5000 });
    const two = await rows.evaluateAll((ls) =>
      ls.map((l) => l.getAttribute("data-who")),
    );
    assert.ok(two.includes("night") && two.includes("claude"));
    assert.ok(!two.includes("you"));
    assert.match(page.url(), /by=claude%2Cnight|by=claude,night/);
    await page
      .locator("body")
      .click({ position: { x: 5, y: 900 }, timeout: 5000 });
    await page.keyboard.press("Escape");
    assert.equal(await rows.count(), all);
  } finally {
    await browser.close();
  }
});

test("invariants: stack hub — Apps and Backups are kp datatables; Backups draws 14 nights per app; Settings ends in a folded Danger zone", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/stacks/gateway/apps`);
    await page
      .locator(".sh-table[data-kp-datatable] tbody tr")
      .first()
      .waitFor({ timeout: 10000 });
    assert.equal(
      await page
        .locator(".sh-table[data-kp-sort-multi][data-kp-remember]")
        .count(),
      1,
    );
    await page.goto(`${BASE}/stacks/gateway/backups`);
    const strips = page.locator(".sh-table .sh-strip");
    await strips.first().waitFor({ timeout: 10000 });
    const cells = await strips.evaluateAll((ss) =>
      ss.map((s) => s.children.length),
    );
    assert.ok(cells.length >= 1 && cells.every((n) => n === 14), cells.join());
    assert.ok(
      (await page.locator('.sh-table [data-action="restore"]').count()) >= 1,
    );
    await page.goto(`${BASE}/stacks/gateway/settings`);
    await page.locator("#size dd").first().waitFor({ timeout: 10000 });
    const r = await page.evaluate(() => ({
      parts: [...document.querySelectorAll(".sh-grid > *")].map((e) => e.id),
      dangerOpen: /** @type {HTMLDetailsElement} */ (
        document.getElementById("danger")
      ).open,
      danger: [...document.querySelectorAll("#danger [data-action]")].map((b) =>
        b.getAttribute("data-action"),
      ),
    }));
    assert.deepEqual(r.parts, [
      "secrets",
      "size",
      "firewall",
      "files",
      "danger",
    ]);
    assert.equal(r.dangerOpen, false);
    assert.deepEqual(r.danger, ["destroy", "forget", "wipe", "prune-orphans"]);
    await page.goto(`${BASE}/stacks/gateway/settings?section=danger`);
    await page.waitForTimeout(500);
    assert.equal(
      await page.evaluate(
        () =>
          /** @type {HTMLDetailsElement} */ (document.getElementById("danger"))
            .open,
      ),
      true,
    );
  } finally {
    await browser.close();
  }
});

test("invariants: stack hub — every tab fits 390 and 1894 px in light and dark, no sideways scroll, nothing clipped inside a card", async () => {
  const browser = await launch();
  try {
    const bad = [];
    for (const theme of ["formal", "dark"])
      for (const width of [1894, 390]) {
        const context = await browser.newContext({
          viewport: { width, height: 900 },
        });
        const page = await freshPage(context);
        await page.evaluate((t) => localStorage.setItem("theme", t), theme);
        for (const tab of [
          "",
          "/logs",
          "/apps",
          "/backups",
          "/history",
          "/settings",
        ]) {
          for (const stack of tab === "/apps"
            ? ["gateway", "kp-soft"]
            : ["gateway"]) {
            await page.goto(`${BASE}/stacks/${stack}${tab}`);
            await page.waitForTimeout(1200);
            const over = await page.evaluate(
              () => document.documentElement.scrollWidth - innerWidth,
            );
            if (over > 0)
              bad.push(`${theme} ${width}px ${tab || "/"}: ${over}px`);
            // Review 3: nothing clipped INSIDE a card either — no table that
            // scrolls sideways in its own box, no button past its card's
            // edge; on a phone the KPI tiles are one column and their words
            // wrap, and a table as cards opens sorted by something.
            const inside = await page.evaluate((w) => {
              const out = [];
              for (const t of document.querySelectorAll("main .kp-table-wrap"))
                if (t.checkVisibility() && t.scrollWidth > t.clientWidth + 1)
                  out.push(`table ${t.scrollWidth} > ${t.clientWidth}`);
              for (const b of document.querySelectorAll(
                "main button, main a.kp-button",
              )) {
                const card = b.closest(".kp-card, .nx-card, .nx-kpi");
                if (!card || !b.checkVisibility()) continue;
                const r = b.getBoundingClientRect();
                const c = card.getBoundingClientRect();
                if (r.right > c.right + 1 || r.left < c.left - 1)
                  out.push(`"${b.textContent?.trim()}" past its card`);
              }
              if (w === 390) {
                const lefts = new Set(
                  [...document.querySelectorAll("main .nx-kpis > *")].map((t) =>
                    Math.round(t.getBoundingClientRect().left),
                  ),
                );
                if (lefts.size > 1) out.push(`${lefts.size} KPI columns`);
                for (const e of document.querySelectorAll(
                  "main .nx-kpi__label, main .nx-kpi__ctx",
                ))
                  if (e.scrollWidth > e.clientWidth + 1)
                    out.push(`KPI text cut: ${e.textContent}`);
                for (const sel of document.querySelectorAll(
                  "main .kp-datatable__card-sort select",
                ))
                  if (
                    /** @type {HTMLElement} */ (sel).checkVisibility() &&
                    /** @type {HTMLSelectElement} */ (sel).value === ""
                  )
                    out.push("a card sort reads None");
              }
              return out;
            }, width);
            for (const x of inside)
              bad.push(`${theme} ${width}px ${stack}${tab || "/"}: ${x}`);
          }
        }
        await context.close();
      }
    assert.deepEqual(bad, [], `clipped: ${bad.join("; ")}`);
  } finally {
    await browser.close();
  }
});

// ── senior review of redesign-stackhub (2026-10-03) ─────────────────────

// The hub's review asked for Update and its u key to open the Update;
// since the 3.71.0 merge every person's Update is the one Update flow
// (invariant 149), so both land on that stack's flow.
test("invariants: stack hub review — Update and the u key open the stack's Update flow", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    for (const how of ["button", "key"]) {
      await page.goto(`${BASE}/stacks/gateway`);
      const btn = page.locator(
        '[data-drive="stack-head"][data-drive-row="gateway/update"]',
      );
      await btn.waitFor({ timeout: 10000 });
      if (how === "button") await btn.click();
      else {
        await page.waitForTimeout(500);
        await page
          .locator("body")
          .click({ position: { x: 5, y: 500 }, timeout: 5000 });
        await page.keyboard.press("u");
      }
      await page.waitForURL("**/update?stack=gateway**", { timeout: 5000 });
      await page.locator("#page .nx-steps").waitFor({ timeout: 10000 });
      assert.match(
        await page.locator("#page h1").innerText(),
        /Update gateway/,
        `${how}: not gateway's Update flow`,
      );
    }
  } finally {
    await browser.close();
  }
});

// redesign-openpoints-2: an app's Update… in the hub's Apps tab opened the
// Update flow for the whole stack (openAction dropped the app it was
// given); it opens the flow with only that app picked.
test("invariants: redesign-openpoints-2: an app's Update… in the stack hub opens the Update flow with only that app picked", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/stacks/gateway/apps`);
    const btn = page.locator('[data-drive="stack-app-pull"]').first();
    await btn.waitFor({ timeout: 10000 });
    const row = String(await btn.getAttribute("data-drive-row"));
    const app = row.split("/").slice(1).join("/");
    assert.ok(app, `no app in the row ${row}`);
    await btn.click();
    await page.waitForURL("**/update?**", { timeout: 5000 });
    const u = new URL(page.url());
    assert.equal(u.searchParams.get("stack"), "gateway");
    assert.equal(u.searchParams.get("app"), app, "the flow names the app");
    await page.locator("#page .nx-steps").waitFor({ timeout: 10000 });
    await page
      .locator('#page [data-drive="update-pick"]')
      .first()
      .waitFor({ timeout: 10000 });
    const ticked = await page.$$eval('#page [data-drive="update-pick"]', (bs) =>
      bs
        .filter((b) => /** @type {HTMLInputElement} */ (b).checked)
        .map(
          (b) => b.closest(".uf-item")?.querySelector("strong")?.textContent,
        ),
    );
    assert.equal(ticked.length, 1, `ticked: ${ticked.join(" | ")}`);
    assert.equal(ticked[0], `gateway / ${app}`);
    assert.match(
      await page.locator("#page h1").innerText(),
      new RegExp(`Update gateway/${app}`),
    );
  } finally {
    await browser.close();
  }
});

test("invariants: stack hub review — the More menu keeps the keyboard focus across fleet pushes", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/stacks/gateway`);
    await page.locator(".sh-head__actions").waitFor({ timeout: 10000 });
    await page.waitForTimeout(1500);
    await page
      .locator("body")
      .click({ position: { x: 5, y: 500 }, timeout: 5000 });
    await page.keyboard.press(".");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowDown");
    const before = await page.evaluate(() =>
      document.activeElement?.textContent?.trim(),
    );
    // The demo host pushes the fleet every few seconds: wait through more
    // than one push with the menu open.
    const pushes = await page.evaluate(
      () =>
        new Promise((done) => {
          let n = 0;
          const t0 = Date.now();
          const tick = setInterval(() => {
            if (Date.now() - t0 > 12000) {
              clearInterval(tick);
              done(n);
            }
          }, 250);
          import("/js/store.js").then((m) =>
            m.subscribe(() => {
              n += 1;
            }),
          );
        }),
    );
    assert.ok(Number(pushes) >= 2, `only ${pushes} fleet pushes arrived`);
    const after = await page.evaluate(() => ({
      text: document.activeElement?.textContent?.trim(),
      inMenu: !!document.activeElement?.closest(".sh-head .nx-menu"),
    }));
    assert.equal(after.inMenu, true, "the focus left the menu");
    assert.equal(after.text, before);
    await page.keyboard.press("ArrowDown");
    assert.equal(
      await page.evaluate(
        () => !!document.activeElement?.closest(".sh-head .nx-menu"),
      ),
      true,
    );
  } finally {
    await browser.close();
  }
});

test("invariants: stack hub review — Settings ▸ Secrets is this stack's pane: no stack list, no Open stack, no ↑ ↓, no card in a card", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/stacks/gateway/settings`);
    const pane = page.locator("#secrets .sx--one");
    await pane.waitFor({ timeout: 10000 });
    await page.waitForTimeout(1500);
    const r = await page.evaluate(() => {
      const sec = /** @type {HTMLElement} */ (
        document.getElementById("secrets")
      );
      return {
        list: sec.querySelectorAll(".sx-stacks").length,
        open: [...sec.querySelectorAll("a")].filter((a) =>
          /open stack/i.test(a.textContent ?? ""),
        ).length,
        arrows: /↑\s*↓/.test(sec.textContent ?? ""),
        // A framed card: the kit's plain sections (no frame, the panes
        // since the 3.71.0 merge) are not cards in the card.
        cards: sec.querySelectorAll(".kp-card, .nx-card:not(.nx-card--plain)")
          .length,
      };
    });
    assert.deepEqual(r, { list: 0, open: 0, arrows: false, cards: 0 });
  } finally {
    await browser.close();
  }
});

test("invariants: stack hub review — Restarts counts 24 h with a sparkline and opens the charts; Is it healthy? has each app's health check and its web addresses", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/stacks/kp-soft`);
    const tile = page.locator(
      '[data-drive="stack-kpi"][data-drive-row="kp-soft/restarts"]',
    );
    await page
      .locator(".nx-kpi__ctx", { hasText: "in the last 24 h" })
      .waitFor({ timeout: 15000 });
    assert.equal(
      (await tile.locator(".nx-kpi__value").textContent())?.trim(),
      "3",
    );
    assert.equal(await tile.locator(".nx-kpi__spark svg").count(), 1);
    assert.equal(
      await tile.getAttribute("href"),
      "/charts?stack=kp-soft&range=24h",
    );
    const checks = page.locator(".sh-checks li");
    await page
      .locator('.sh-checks li[data-key="restarts"]', {
        hasText: "in 24 h",
      })
      .waitFor({ timeout: 10000 });
    const keys = await checks.evaluateAll((ls) =>
      ls.map((l) => l.getAttribute("data-key")),
    );
    assert.ok(
      keys.some((k) => k?.startsWith("health:")),
      `no app health row: ${keys.join()}`,
    );
    assert.ok(keys.includes("web"), `no web row: ${keys.join()}`);
    assert.match(
      (await page
        .locator('.sh-checks li[data-key^="health:"]')
        .first()
        .textContent()) ?? "",
      /not declared|healthy|unhealthy|starting/,
    );
  } finally {
    await browser.close();
  }
});

test("invariants: stack hub review — the no-env row's Push the env… opens the seal-env action", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/stacks/kp-soft`);
    const fix = page.locator(
      '[data-drive="stack-fix"][data-drive-row="kp-soft/no-env"]',
    );
    await fix.waitFor({ timeout: 10000 });
    assert.equal((await fix.textContent())?.trim(), "Push the env…");
    assert.equal(await fix.getAttribute("data-action"), "seal-env");
    await fix.click();
    const dialog = page.locator("dialog#action-dialog[open]");
    await dialog.waitFor({ timeout: 5000 });
    assert.match(
      (await dialog.locator(".kp-dialog__title").textContent()) ?? "",
      /Push the env/,
    );
  } finally {
    await browser.close();
  }
});

test("invariants: stack hub review — History says Kenny in chip and rows, dates read 30 Sep 12:14, and no incidents reads as no bundles kept", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/stacks/kp-soft/history`);
    const rows = page.locator(".sh-feed--full li[data-who]");
    await rows.first().waitFor({ timeout: 10000 });
    assert.equal(
      (
        await page
          .locator(
            '[data-drive="stack-history-who"][data-drive-row="kp-soft/you"]',
          )
          .textContent()
      )?.trim(),
      "Kenny",
    );
    const you = await page
      .locator('.sh-feed--full li[data-who="you"] .nx-chip')
      .allTextContents();
    assert.ok(
      // A CLI session reads "Kenny · CLI on wsl", as Activity names it.
      you.length > 0 &&
        you.every((t) => /^Kenny( · CLI on \S+)?$/.test(t.trim())),
      you.join(),
    );
    const times = await page
      .locator(".sh-feed--full li time")
      .allTextContents();
    for (const t of times)
      assert.match(
        t.trim(),
        /^(\d+ (s|min|h) ago|\d{1,2} [A-Z][a-z]{2}( \d{4})? \d\d:\d\d)$/,
        `a date reads ${t}`,
      );
    await page.goto(`${BASE}/stacks/notes/history`);
    const empty = page.locator(
      ".sh-stack .sh-feed:not(.sh-feed--full) .sh-feed__empty",
    );
    await empty.waitFor({ timeout: 10000 });
    assert.equal(
      (await empty.textContent())?.trim(),
      "No incident bundles kept for notes.",
    );
  } finally {
    await browser.close();
  }
});

// ── redesign-flows review (3.71.0, the senior review of 2026-10-03 and the
// coordinator's destroy rule): Deploy all changes keeps its diff inside its
// column, deploys a chosen subset and destroys only as its own confirmed
// step; the Update flow is one job on the dashboard's server with its own
// address; Help, the tour and the Inbox's details. ──

test("invariants: Deploy all changes keeps a stack's diff inside its column", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1100 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/stacks?deploy-all=1`);
    const more = page.locator(
      "#apply-section .ap-col[data-col=deploy] [data-drive=deploy-all-diff]",
    );
    await more.first().waitFor({ timeout: 20000 });
    await more.first().click();
    const diff = page.locator("#apply-section .ap-diff").first();
    await diff.locator("details").first().waitFor({ timeout: 10000 });
    for (const d of await diff.locator("details").all())
      await d.evaluate((x) => {
        /** @type {HTMLDetailsElement} */ (x).open = true;
      });
    const edges = await page.evaluate(() => {
      const col = /** @type {HTMLElement} */ (
        document.querySelector("#apply-section .ap-col[data-col=deploy]")
      );
      const next = /** @type {HTMLElement} */ (
        document.querySelector("#apply-section .ap-col[data-col=destroy]")
      );
      // The boxes that show the diff (a long line scrolls inside its own
      // box, so the lines themselves may run on under the scroll).
      const right = Math.max(
        ...[
          ...col.querySelectorAll(
            ".ap-diff, .ap-diff details, .ap-diff summary, .ap-diff pre",
          ),
        ].map((e) => e.getBoundingClientRect().right),
      );
      return {
        right,
        col: col.getBoundingClientRect().right,
        next: next.getBoundingClientRect().left,
      };
    });
    assert.ok(
      edges.right <= edges.col + 1,
      `the diff reaches ${edges.right}px, past its column's ${edges.col}px`,
    );
    assert.ok(edges.col <= edges.next, "the deploy column covers the next");
  } finally {
    await browser.close();
  }
});

test("invariants: Deploy all changes deploys the ticked subset and destroys only as its own confirmed red step", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1100 },
    });
    const page = await freshPage(context);
    // oldstack (CT 905 on the demo host) is gone from the files.
    await page.route("**/data/apply/plan", (route) =>
      route.fulfill({
        contentType: "application/json",
        body: JSON.stringify({
          plan: {
            deploy: ["alpha", "beta"],
            new: [],
            destroy: ["oldstack"],
            broken: [["gamma", "compose.yaml did not parse :: fix line 3"]],
            unchanged: [],
            ephemeral: [],
          },
          pending: 3,
          measured_at: Math.floor(Date.now() / 1000),
        }),
      }),
    );
    await page.goto(`${BASE}/stacks?deploy-all=1`);
    await page.locator("#apply-open").waitFor({ timeout: 10000 });
    await page.getByLabel("Include alpha").uncheck();
    // Deploys alone never destroy, even with a name typed.
    await page.getByLabel("Type oldstack to destroy it").fill("oldstack");
    await page.locator("#apply-open").click();
    const dialog = page.locator("dialog#action-dialog[open]");
    await dialog.waitFor({ timeout: 5000 });
    const val = (/** @type {string} */ id) =>
      dialog.locator(`#${id}`).inputValue();
    assert.equal(await val("act-leave-out"), "alpha, gamma");
    assert.equal(
      await val("act-destroy"),
      "",
      "a deploy press carries a destroy",
    );
    await page.keyboard.press("Escape");
    await dialog.waitFor({ state: "detached", timeout: 5000 }).catch(() => {});
    // The destroy is its own red step: off until it is confirmed, and while
    // a ticked deploy has not run.
    const step = page.locator("#apply-section .ap-destroy");
    const destroy = page.locator("#apply-destroy");
    assert.equal(await destroy.isDisabled(), true);
    assert.match(
      await step.evaluate((e) => getComputedStyle(e).borderColor),
      /rgb/,
    );
    const ack = page.locator("#apply-destroy-ack");
    await ack.check();
    assert.equal(
      await destroy.isDisabled(),
      true,
      "the destroy is open while a ticked deploy has not run",
    );
    await page.getByLabel("Include beta").uncheck();
    assert.equal(await destroy.isDisabled(), false);
    // Un-arming it closes it again.
    await page.getByLabel("Type oldstack to destroy it").fill("olds");
    assert.equal(await destroy.isDisabled(), true);
    await page.getByLabel("Type oldstack to destroy it").fill("oldstack");
    await ack.check();
    await destroy.click();
    await dialog.waitFor({ timeout: 5000 });
    assert.equal(await val("act-destroy"), "oldstack");
    assert.equal(await val("act-destroy-ids"), "905");
    assert.equal(await val("act-leave-out"), "alpha, beta, gamma");
    await page.keyboard.press("Escape");
    // The server refuses a destroy without its own confirmation or beside
    // a deploy, before anything runs.
    const post = (/** @type {Record<string, unknown>} */ args) =>
      page.evaluate(async (a) => {
        const r = await fetch("/data/actions/_host/apply", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify(a),
        });
        return { status: r.status, body: await r.text() };
      }, args);
    const noAck = await post({ destroy: "oldstack", destroy_ids: "905" });
    assert.equal(noAck.status, 400, noAck.body);
    assert.match(noAck.body, /confirmation/);
    const noBackup = await post({
      destroy: "oldstack",
      destroy_ids: "905",
      destroy_ack: true,
      skip_backup: true,
    });
    assert.equal(noBackup.status, 400, noBackup.body);
  } finally {
    await browser.close();
  }
});

test("invariants: Deploy all changes marks Compare done only once the compare ran, Esc and Show all reset the tile filter, and an unarmed destroy has a neutral dot", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1100 },
    });
    const page = await freshPage(context);
    await page.route("**/data/drift", (route) =>
      route.fulfill({
        contentType: "application/json",
        body: JSON.stringify({ stacks: {}, measured_at: null }),
      }),
    );
    // redesign-final (the base's failing case): since redesign-stacks-7
    // the plan read on opening IS a compare (its answer marks the list), so
    // "not compared yet" also holds the plan back until the case lets it go.
    /** @type {import("playwright").Route[]} */
    const heldPlan = [];
    await page.route("**/data/apply/plan", (route) => {
      heldPlan.push(route);
    });
    await page.goto(`${BASE}/stacks?deploy-all=1`);
    const compare = page.locator(
      "#apply-section .ap-steps li[data-step=compare]",
    );
    await compare.waitFor({ timeout: 10000 });
    await page.waitForTimeout(500);
    assert.equal(
      await compare.evaluate((e) => e.classList.contains("done")),
      false,
      "Compare is shown done before it ran",
    );
    await page.unroute("**/data/drift");
    for (const r of heldPlan) await r.continue();
    await page.unroute("**/data/apply/plan");
    await compare.locator("#drift-compare").click();
    await page.waitForFunction(
      () =>
        document
          .querySelector("#apply-section .ap-steps li[data-step=compare]")
          ?.classList.contains("done"),
      null,
      { timeout: 15000 },
    );
    // The tile filter: Show all appears while one is on; Esc resets.
    const tiles = page.locator("#apply-section .ap-tile");
    await tiles.first().waitFor({ timeout: 15000 });
    await tiles.nth(1).click();
    const showAll = page.locator("[data-drive=deploy-all-show-all]");
    await showAll.waitFor({ timeout: 3000 });
    await showAll.click();
    assert.equal(await tiles.nth(1).getAttribute("aria-pressed"), "false");
    assert.equal(await showAll.count(), 0);
    await tiles.nth(2).click();
    await page.keyboard.press("Escape");
    assert.equal(await tiles.nth(2).getAttribute("aria-pressed"), "false");
    // An unarmed destroy's dot is neutral, not the info blue.
    const dot = page.locator(
      "#apply-section .ap-item--destroy:not(.armed) .nx-sev",
    );
    if ((await dot.count()) > 0) {
      const [c, info] = await page.evaluate(() => {
        const d = /** @type {HTMLElement} */ (
          document.querySelector(
            "#apply-section .ap-item--destroy:not(.armed) .nx-sev",
          )
        );
        const probe = document.createElement("span");
        probe.className = "nx-sev nx-sev--info";
        document.body.append(probe);
        const out = [
          getComputedStyle(d).backgroundColor,
          getComputedStyle(probe).backgroundColor,
        ];
        probe.remove();
        return out;
      });
      assert.notEqual(c, info, "an unarmed destroy shows the info dot");
    }
  } finally {
    await browser.close();
  }
});

test("invariants: on a phone the Deploy all changes tiles wrap their labels and the Inbox header reads title, sentence, then its button", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 390, height: 900 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/stacks?deploy-all=1`);
    const labels = page.locator("#apply-section .ap-tile .nx-kpi__label");
    await labels.first().waitFor({ timeout: 15000 });
    for (const l of await labels.all())
      assert.ok(
        await l.evaluate((e) => e.scrollWidth <= e.clientWidth + 1),
        `a tile label is cut off: ${await l.innerText()}`,
      );
    await page.goto(`${BASE}/inbox`);
    const head = page.locator("#page .nx-head").first();
    await head.waitFor({ timeout: 10000 });
    const box = async (/** @type {string} */ sel) =>
      /** @type {{x: number, y: number, width: number, height: number}} */ (
        await head.locator(sel).first().boundingBox()
      );
    const title = await box("h1");
    const desc = await box(".nx-head-desc");
    const again = await box("[data-drive=inbox-check-again]");
    assert.ok(
      title.y < desc.y && desc.y < again.y,
      "the header does not read title, sentence, button",
    );
    assert.ok(again.width < 390 - 32 - 40, "Check again stretches the row");
  } finally {
    await browser.close();
  }
});

test("invariants: the Inbox's Worth a look carries a one-sentence description", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.goto(`${BASE}/inbox`);
    const desc = page.locator(
      "#page .inbox-worth > summary .inbox-worth__desc",
    );
    await desc.waitFor({ timeout: 10000 });
    const t = (await desc.innerText()).trim();
    assert.ok(/^[A-Z].*\.$/.test(t), `not one sentence: ${t}`);
  } finally {
    await browser.close();
  }
});

test("invariants: the Update flow has its own address, the old one redirects, and the nav marks Stacks for one stack", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1100 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/inbox?update=kp-soft`);
    await page.waitForURL(/\/update\?stack=kp-soft$/, { timeout: 5000 });
    const current = () =>
      page.locator(".kp-nav__link[aria-current=page]").innerText();
    await page.locator("#page .nx-steps").waitFor({ timeout: 10000 });
    assert.equal((await current()).trim(), "Stacks");
    await page.goto(`${BASE}/update?all=1`);
    await page.locator("#page .nx-steps").waitFor({ timeout: 10000 });
    assert.match((await current()).trim(), /^Inbox/);
    // The page is capped at the demo's width.
    const w = await page
      .locator("#page .uf-page")
      .evaluate((e) => e.getBoundingClientRect().width);
    assert.ok(w <= 1024 + 1, `the Update page is ${w}px wide`);
  } finally {
    await browser.close();
  }
});

test("invariants: the Update flow folds the file change, names each file once, and keeps running as one job when the tab leaves", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/update?all=1`);
    await page.locator("#page .uf-item").first().waitFor({ timeout: 20000 });
    await page.locator("[data-drive=update-see-impact]").click();
    const fold = page.locator("#page details.uf-fold");
    await fold.waitFor({ timeout: 15000 });
    assert.equal(
      await fold.evaluate((d) => /** @type {HTMLDetailsElement} */ (d).open),
      false,
      "the change to the stack files is unfolded",
    );
    await fold.locator("summary").first().click();
    const files = page.locator("#page .uf-diff > details");
    await files.first().waitFor({ timeout: 15000 });
    for (const f of await files.all()) {
      assert.equal(
        await f.evaluate((d) => /** @type {HTMLDetailsElement} */ (d).open),
        false,
        "a file is unfolded",
      );
      const s = await f.locator("summary").innerText();
      assert.ok(
        (s.match(/changed/g) ?? []).length <= 1,
        `a file says changed twice: ${s}`,
      );
    }
    for (const t of await page.locator("[data-drive=update-major-read]").all())
      await t.check();
    const started = page.waitForRequest(
      (r) =>
        r.method() === "POST" &&
        new URL(r.url()).pathname === "/data/actions/_host/update-apps",
      { timeout: 10000 },
    );
    await page.waitForFunction(
      () =>
        !(
          /** @type {HTMLButtonElement | null} */ (
            document.querySelector("[data-drive=update-go]")
          )?.disabled ?? true
        ),
      null,
      { timeout: 10000 },
    );
    await page.locator("[data-drive=update-go]").click();
    const req = await started;
    const job = /** @type {{job: number}} */ (
      await (await req.response())?.json()
    ).job;
    // The tab leaves: the job goes on, on the server, and is in Activity.
    await page.goto(`${BASE}/apps`);
    const end = Date.now() + 120000;
    /** @type {any} */
    let seen = null;
    while (Date.now() < end) {
      seen = await page.evaluate(async (id) => {
        const r = await fetch("/data/actions/jobs");
        const b = await r.json();
        return (b.jobs ?? []).find((/** @type {any} */ j) => j.job === id);
      }, job);
      if (seen && !["queued", "running"].includes(seen.state)) break;
      await page.waitForTimeout(1000);
    }
    assert.equal(seen?.action, "update-apps");
    assert.equal(seen?.state, "done", JSON.stringify(seen?.message));
    // Back on the flow, it shows the job's end.
    await page.goto(`${BASE}/update?all=1`);
    await page
      .locator("#page .uf-done h2")
      .waitFor({ state: "visible", timeout: 20000 });
    assert.match(
      await page.locator("#page .uf-done h2").innerText(),
      /^Updated/,
    );
    assert.match(
      await page.locator("#page .uf-done").innerText(),
      /7 days/,
      "the undo is not offered for 7 days",
    );
  } finally {
    await browser.close();
  }
});

test("invariants: Help's close button stays inside its panel, the bar's ? is the Live view control help-open, and the tour card is labelled by its step", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/apps`);
    const open = page.locator("[data-drive=help-open]");
    await open.waitFor({ timeout: 5000 });
    await open.click();
    const help = page.locator("dialog#shortcuts[open]");
    await help.waitFor({ timeout: 3000 });
    const [panel, close] = await Promise.all([
      help.boundingBox(),
      help.locator(".kp-dialog__close").first().boundingBox(),
    ]);
    assert.ok(panel && close);
    assert.ok(
      close.x + close.width <= panel.x + panel.width - 4 &&
        close.x + close.width <= 1894 - 4,
      `the close button is clipped: ${JSON.stringify({ panel, close })}`,
    );
    await help.getByText("Take the 1-minute tour").click();
    const tour = page.locator("dialog.tour");
    await tour.waitFor({ timeout: 3000 });
    assert.equal(await tour.getAttribute("aria-live"), null);
    const by = await tour.getAttribute("aria-labelledby");
    assert.ok(by, "the tour card has no label");
    assert.equal(
      await page.locator(`#${by}`).innerText(),
      await tour.locator("h3").innerText(),
    );
  } finally {
    await browser.close();
  }
});

// redesign-integrate-1 (the 3.71.0 consolidation's open item from the
// kit's review): at 390 px the Firewall's Inbound and Outbound rules
// tables scrolled sideways inside their card and cut off Ports. On a phone
// each rule is a card with its cells labelled (kp-themes' card layout),
// never a table to scroll.
test("invariants: redesign-integrate-1: on a 390 px phone the Firewall's rules tables never scroll sideways, every rule's Ports in view", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 390, height: 844 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/firewall`);
    await page.locator("tr.fw-rule").first().waitFor({ timeout: 10000 });
    const r = await page.evaluate(() => {
      const tables = [...document.querySelectorAll("table.fw-rules")];
      return {
        n: tables.length,
        sideways: tables
          .map((t) => {
            const wrap = /** @type {HTMLElement} */ (t.parentElement);
            return wrap.scrollWidth - wrap.clientWidth;
          })
          .filter((d) => d > 1),
        portsOff: [...document.querySelectorAll("tr.fw-rule")]
          .map((tr) => {
            const td = /** @type {HTMLElement} */ (tr.children[3]);
            const b = td.getBoundingClientRect();
            return b.width === 0 || b.right > innerWidth + 1
              ? (tr.getAttribute("data-rule") ?? "?")
              : null;
          })
          .filter(Boolean),
        unlabelled: [...document.querySelectorAll("tr.fw-rule > td")]
          .filter((td) => !td.getAttribute("data-label"))
          .map((td) => td.textContent ?? ""),
        page: document.documentElement.scrollWidth - innerWidth,
      };
    });
    assert.ok(r.n >= 1, "no rules table on the Firewall page");
    assert.deepEqual(r.sideways, [], "a rules table scrolls sideways by px");
    assert.deepEqual(r.portsOff, [], "rules whose Ports are cut off");
    assert.deepEqual(r.unlabelled, [], "rule cells without their column name");
    assert.ok(r.page <= 1, `the page scrolls sideways by ${r.page} px`);
  } finally {
    await browser.close();
  }
});

// ── redesign-final-gen (3.71.0's final review, 2026-10-04): one generic
// whole-screen check per finding class, over every page and every action
// dialog or drawer at 1894 and 390 px (test-e2e/layoutaudit.js says what
// each class checks). The sweep runs once; its five cases read it. ──────

/** The pages the final review walked: every area, view and hub tab. */
const AUDIT_PAGES = [
  "/apps",
  "/inbox",
  "/stacks",
  "/stacks?deploy-all=1",
  "/activity",
  "/activity?view=running",
  "/activity?view=host-log",
  "/activity?view=planned",
  "/activity?view=timeline",
  "/backups",
  "/backups?section=coverage",
  "/backups?section=removed",
  "/backups/restore?stack=kp-soft",
  "/system",
  "/host",
  "/host?section=doctor",
  "/charts",
  "/map",
  "/firewall",
  "/settings",
  "/settings?section=sign-in",
  "/presets",
  "/system/notifications",
  "/console",
  "/update?all=1",
  "/update?stack=kp-soft",
  "/stacks/kp-soft/overview",
  "/stacks/kp-soft/logs",
  "/stacks/kp-soft/apps",
  "/stacks/kp-soft/backups",
  "/stacks/kp-soft/history",
  "/stacks/kp-soft/settings",
];

/**
 * The action dialogs and drawers the final review opened: the page, then
 * the control (its Live view id, or its words).
 * @type {{path: string, drive?: string, text?: RegExp,
 *   then?: (page: import("playwright").Page) => Promise<void>}[]}
 */
const AUDIT_DIALOGS = [
  { path: "/inbox", drive: "inbox-fix" },
  { path: "/inbox", drive: "inbox-push-envs" },
  { path: "/stacks", drive: "stacks-new" },
  { path: "/activity", drive: "activity-run-again" },
  { path: "/activity?view=planned", drive: "schedule-template" },
  { path: "/backups", text: /^Back up now/ },
  { path: "/backups", drive: "backups-drill-now" },
  { path: "/host", text: /^Back up host state/ },
  { path: "/host", text: /^Back up devices/ },
  { path: "/host", text: /^Run a command/ },
  { path: "/host", text: /^Build a template/ },
  { path: "/settings", drive: "issue-token" },
  { path: "/presets", drive: "presets-import" },
  { path: "/presets", drive: "presets-new-preset" },
  { path: "/stacks/kp-soft/overview", drive: "stack-head" },
  { path: "/stacks/kp-soft/overview", drive: "stack-fix" },
  { path: "/stacks/films/overview", drive: "stack-head", text: /^Deploy/ },
  { path: "/stacks/kp-soft/apps", drive: "stack-app-publish" },
  { path: "/stacks/kp-soft/backups", drive: "stack-backups-card" },
  // The running-job drawer: a back up confirmed, then the running pill.
  {
    path: "/stacks/kp-soft/overview",
    drive: "stack-head",
    text: /^Back up/,
    then: openRunningDrawer,
  },
];

/**
 * From an open action dialog: confirm it, shut every dialog and open the
 * running pill's job drawer while the job runs.
 * @param {import("playwright").Page} page
 */
async function openRunningDrawer(page) {
  await page.locator("dialog[open] #act-run").click();
  await page.waitForTimeout(300);
  await page.evaluate(() =>
    document
      .querySelectorAll("dialog[open]")
      .forEach((d) => /** @type {HTMLDialogElement} */ (d).close()),
  );
  await page.locator("#running-pill").click({ timeout: 5000 });
  await page.locator("dialog[open]").last().waitFor({ timeout: 5000 });
}

/** @type {Promise<Record<string, string[]>> | null} */
let auditRun = null;

/**
 * Every audit finding of the sweep, by class ("a".."e"), each prefixed with
 * where it was found; run once for all five cases.
 */
function auditSweep() {
  auditRun ??= (async () => {
    const { layoutAudit } = await import("./layoutaudit.js");
    /** @type {Record<string, string[]>} */
    const all = { a: [], b: [], c: [], d: [], e: [] };
    const browser = await launch();
    try {
      for (const width of [1894, 390]) {
        const context = await browser.newContext({
          viewport: { width, height: width > 500 ? 1000 : 844 },
        });
        const page = await freshPage(context);
        const keep = (/** @type {string} */ where, r) => {
          for (const k of Object.keys(all))
            for (const f of r[k]) all[k].push(`${where} @${width}: ${f}`);
        };
        // The last open dialog, marked for the audit (a page that opens
        // one by its address, as Deploy all changes does, is audited with
        // it).
        const markDialog = () =>
          page.evaluate(() => {
            const ds = [...document.querySelectorAll("dialog[open]")];
            ds.forEach((d) => d.removeAttribute("data-audit"));
            ds[ds.length - 1]?.setAttribute("data-audit", "");
            return ds.length > 0;
          });
        for (const path of AUDIT_PAGES) {
          if (AUDIT_ONLY && !AUDIT_ONLY.test(path)) continue;
          await page.goto(`${BASE}${path}`);
          await page.waitForTimeout(1500);
          keep(path, await page.evaluate(layoutAudit, null));
          if (await markDialog())
            keep(
              `${path} › its dialog`,
              await page.evaluate(layoutAudit, "dialog[data-audit]"),
            );
        }
        for (const d of AUDIT_DIALOGS) {
          if (AUDIT_ONLY && !AUDIT_ONLY.test(d.path)) continue;
          await page.goto(`${BASE}${d.path}`);
          await page.waitForTimeout(1200);
          let btn = page.locator(
            d.drive
              ? `main [data-drive="${d.drive}"]:visible`
              : "main button:visible",
          );
          if (d.text) btn = btn.filter({ hasText: d.text });
          if ((await btn.count()) === 0) {
            all.a.push(`${d.path} @${width}: no control ${d.drive ?? d.text}`);
            continue;
          }
          await btn.first().click();
          const dlg = page.locator("dialog[open]").last();
          await dlg.waitFor({ timeout: 5000 }).catch(() => {});
          if (d.then) await d.then(page);
          await page.waitForTimeout(700);
          const where = `${d.path} › ${d.drive ?? d.text}${d.then ? " › then" : ""}`;
          await markDialog();
          keep(where, await page.evaluate(layoutAudit, "dialog[data-audit]"));
          await page.keyboard.press("Escape");
        }
        await context.close();
      }
    } finally {
      await browser.close();
    }
    if (process.env.INVARIANTS_AUDIT_OUT)
      (await import("node:fs")).writeFileSync(
        process.env.INVARIANTS_AUDIT_OUT,
        JSON.stringify(all, null, 1),
      );
    return all;
  })();
  return auditRun;
}

/** INVARIANTS_AUDIT_ONLY: audit only the pages and dialogs whose path
 * matches (a fix's own run); the gate runs them all. */
const AUDIT_ONLY = process.env.INVARIANTS_AUDIT_ONLY
  ? new RegExp(process.env.INVARIANTS_AUDIT_ONLY)
  : null;

for (const [cls, what] of /** @type {const} */ ([
  ["a", "no word wraps per letter and no text is cut or spills out of its box"],
  ["b", "no row's content runs into the next row"],
  [
    "c",
    "dates show as dd/mm/yyyy HH:MM, never with a weekday or a month's name",
  ],
  ["d", "a page has one page-level header"],
  ["e", "a problems counter never reads 0 beside red chips"],
])) {
  test(`invariants: redesign-final-gen-${cls}: on every page and action dialog at 1894 and 390 px, ${what}`, async () => {
    // redesign-final: no instance is excused (the AUDIT_KNOWN allowance is
    // gone); every finding fails.
    const found = (await auditSweep())[cls];
    assert.deepEqual(found, [], `${found.length} findings`);
  });
}

// ── redesign-final C1–C4, H1–H5 (3.71.0's final review, 2026-10-04): one
// case per finding, each failing on the reviewed tree (c6334fc9). ───────

// C4: rule 18's stack chips wrapped into row 19 and printed over its
// address; a rule's row grows with its chips.
test("invariants: redesign-final-c4: every Firewall rule row grows with its stack chips, never printing into the next rule", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/firewall`);
    await page.locator("tr.fw-rule").first().waitFor({ timeout: 10000 });
    const seg = page
      .locator(".fw-rules-body")
      .locator("..")
      .locator(".nx-seg button");
    const names = await seg.allInnerTexts();
    assert.ok(names.length >= 1, "no stack switch above the rules");
    /** @type {string[]} */
    const bad = [];
    for (let i = 0; i < names.length; i++) {
      await seg.nth(i).click();
      await page.waitForTimeout(300);
      bad.push(
        ...(await page.evaluate(() =>
          [...document.querySelectorAll("tr.fw-rule")].flatMap((tr) => {
            const next = tr.nextElementSibling;
            if (!next) return [];
            const top = next.getBoundingClientRect().top;
            const low = Math.max(
              ...[...tr.querySelectorAll("*")].map(
                (e) => e.getBoundingClientRect().bottom,
              ),
            );
            return low > top + 1
              ? [
                  `${tr.getAttribute("data-rule")}: ${Math.round(low - top)} px into the next row`,
                ]
              : [];
          }),
        )),
      );
    }
    assert.deepEqual(bad, [], "rules whose content runs into the next rule");
  } finally {
    await browser.close();
  }
});

// C1: Deploy all changes squeezed the whole plan into a ~500 px drawer:
// "WILL DEPLO Y", KPI numbers cut to "1…", the step text overlapped.
test("invariants: redesign-final-c1: Deploy all changes opens as a wide sheet whose steps, tiles and columns never break a word per letter or cut a number, at 1894 and 390 px", async () => {
  const { layoutAudit } = await import("./layoutaudit.js");
  const browser = await launch();
  try {
    for (const width of [1894, 390]) {
      const context = await browser.newContext({
        viewport: { width, height: width > 500 ? 1000 : 844 },
      });
      const page = await freshPage(context);
      await page.goto(`${BASE}/stacks?deploy-all=1`);
      const d = page.locator("dialog[open]").last();
      await d.locator(".ap-tile").first().waitFor({ timeout: 10000 });
      await page.waitForTimeout(500);
      const r = await page.evaluate(() => {
        const d = /** @type {HTMLElement} */ (
          [...document.querySelectorAll("dialog[open]")].pop()
        );
        d.setAttribute("data-audit", "");
        return {
          w: d.getBoundingClientRect().width,
          cut: [...d.querySelectorAll(".ap-tile .nx-kpi__value")]
            .filter((v) => v.scrollWidth > v.clientWidth + 1)
            .map((v) => v.textContent),
          over: [...d.querySelectorAll(".ap-steps li")].flatMap((li) => {
            const b = li.getBoundingClientRect();
            return [...li.querySelectorAll("*")]
              .filter((e) => e.getBoundingClientRect().right > b.right + 1)
              .map((e) => (e.textContent ?? "").slice(0, 30));
          }),
        };
      });
      if (width > 500)
        assert.ok(
          r.w >= 960,
          `the sheet is ${Math.round(r.w)} px wide at ${width}`,
        );
      assert.deepEqual(r.cut, [], `@${width}: tile numbers cut`);
      assert.deepEqual(r.over, [], `@${width}: step text out of its step`);
      const a = (await page.evaluate(layoutAudit, "dialog[data-audit]")).a;
      assert.deepEqual(a, [], `@${width}: words broken or text cut`);
      await context.close();
    }
  } finally {
    await browser.close();
  }
});

// C2: the running pill's drawer reused the inline panel's window-wide
// grid ("step 3", "no earlier run to go by" cut) and lacked the steps.
test("invariants: redesign-final-c2: the running pill's drawer is named after its job, lists the job's steps and keeps every fact inside its one column", async () => {
  const { layoutAudit } = await import("./layoutaudit.js");
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/stacks/kp-soft/overview`);
    await page
      .locator('main [data-drive="stack-head"]:visible')
      .filter({ hasText: /^Back up/ })
      .first()
      .click();
    await page.locator("dialog[open]").last().waitFor();
    await openRunningDrawer(page);
    const d = page.locator("dialog.job-drawer[open]");
    await d.waitFor({ timeout: 5000 });
    await page.waitForTimeout(400);
    const r = await page.evaluate(() => {
      const d = /** @type {HTMLElement} */ (
        document.querySelector("dialog[open].job-drawer")
      );
      d.setAttribute("data-audit", "");
      const panel = /** @type {HTMLElement} */ (d.querySelector(".job-panel"));
      const pr = panel.getBoundingClientRect();
      return {
        title: d.querySelector("h2")?.textContent ?? "",
        steps: d.querySelectorAll(".nx-check li").length,
        out: [...d.querySelectorAll(".job-facts dd")]
          .filter((dd) => dd.getBoundingClientRect().right > pr.right + 1)
          .map((dd) => dd.textContent),
        lefts: new Set(
          [...d.querySelectorAll(".job-facts dt")].map((dt) =>
            Math.round(dt.getBoundingClientRect().left),
          ),
        ).size,
      };
    });
    assert.match(r.title, /^Back up · kp-soft$/);
    assert.ok(r.steps >= 1, "the drawer lists no steps");
    assert.deepEqual(r.out, [], "facts cut at the drawer's edge");
    assert.equal(r.lefts, 1, "the facts are not one column");
    const a = (await page.evaluate(layoutAudit, "dialog[data-audit]")).a;
    assert.deepEqual(a, [], "text cut in the job drawer");
  } finally {
    await browser.close();
  }
});

// H2: action dialogs showed the host's raw errors ("No CLI line: cannot
// read /home/…/lxc-compose.yml (os error 2)", "No preview: exec _host",
// raw backticks) and kept Deploy armed.
test("invariants: redesign-final-h2: an action dialog that cannot run says why in one plain sentence and holds its primary back; Run a command is titled for one container", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    const raw = /\/home\/|os error|No CLI line|No plan|No preview|`|_host/;
    await page.goto(`${BASE}/inbox`);
    await page.locator('main [data-drive="inbox-fix"]:visible').first().click();
    const d = page.locator("#action-dialog[open]");
    await d.waitFor();
    await page.waitForTimeout(1200);
    const films = {
      title: await d.locator("h2").first().innerText(),
      text: await d.innerText(),
      disabled: await d.locator("#act-run").isDisabled(),
      why: await d.locator("#act-run").getAttribute("title"),
    };
    assert.match(films.title, /^Deploy · films$/);
    assert.doesNotMatch(films.text, raw, "raw words in the Deploy dialog");
    assert.ok(films.disabled, "Deploy stays armed although it cannot run");
    assert.match(films.why ?? "", /films has no stack file/);
    await page.keyboard.press("Escape");

    await page.goto(`${BASE}/host`);
    await page
      .locator("main button:visible")
      .filter({ hasText: /^Run a command/ })
      .first()
      .click();
    await d.waitFor();
    await page.waitForTimeout(1200);
    const exec = {
      title: await d.locator("h2").first().innerText(),
      text: await d.innerText(),
      code: await d.locator(".act-intro code").count(),
      disabled: await d.locator("#act-run").isDisabled(),
    };
    assert.equal(exec.title, "Run a command · one container");
    assert.doesNotMatch(exec.text, raw, "raw words in Run a command");
    assert.ok(exec.code >= 2, "the description's commands are not code");
    assert.ok(exec.disabled, "Run a command is armed without a container");
  } finally {
    await browser.close();
  }
});

// H5: Activity › Planned mounted the Schedules page with a second
// page-sized title and a key badge no other button has; /host?section=
// doctor mounted the retired Doctor page (H1 "Doctor", "Today block",
// an old table) inside Host instead of Host's own checks card.
test("invariants: redesign-final-h5: Planned is a section of Activity and the Doctor's address opens Host on its checks card", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/activity?view=planned`);
    await page.locator("#sched-new").waitFor();
    const planned = await page.evaluate(() => {
      const h1 = /** @type {HTMLElement} */ (document.querySelector("main h1"));
      const big = parseFloat(getComputedStyle(h1).fontSize) * 0.85;
      return {
        h1s: document.querySelectorAll("main h1").length,
        rivals: [...document.querySelectorAll("main h2")]
          .filter((x) => parseFloat(getComputedStyle(x).fontSize) >= big)
          .map((x) => x.textContent),
        badge: document.querySelectorAll("#sched-new kbd, #sched-new .nx-kbd")
          .length,
      };
    });
    assert.equal(planned.h1s, 1);
    assert.deepEqual(planned.rivals, [], "a second page-sized title");
    assert.equal(planned.badge, 0, "New schedule carries a key badge");

    await page.goto(`${BASE}/host?section=doctor`);
    const card = page.locator("#host-checks-card");
    await card.waitFor({ timeout: 10000 });
    await page.waitForTimeout(800);
    const host = await page.evaluate(() => ({
      h1: document.querySelector("main h1")?.textContent ?? "",
      text: document.querySelector("main")?.textContent ?? "",
      top: document.querySelector("#host-checks-card")?.getBoundingClientRect()
        .top,
    }));
    assert.match(host.h1, /^Host/);
    assert.doesNotMatch(host.text, /Today block/);
    assert.ok(
      host.top != null && host.top >= 0 && host.top < 1000,
      `the Host checks card is not in view (top ${host.top})`,
    );
  } finally {
    await browser.close();
  }
});

// H4: System was one flat grid of nine cards; the approved
// flows/system.html has four headed groups, the host's live line and the
// disaster runbook.
test("invariants: redesign-final-h4: System groups its pages under The host, The fleet as a whole, Set up and Tools, with the host's live line and the runbook", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/system`);
    await page
      .locator('[data-card="host"] .sy-kpi')
      .waitFor({ timeout: 10000 });
    await page.waitForTimeout(1500);
    const r = await page.evaluate(() => ({
      heads: [...document.querySelectorAll("main .sy-group h2")].map(
        (x) => x.textContent,
      ),
      descs: [
        ...document.querySelectorAll("main .sy-group .section-head__desc"),
      ]
        .map((x) => x.textContent?.trim())
        .filter(Boolean).length,
      line: document.querySelector('[data-card="host"] .sy-kpi')?.textContent,
      state: document.querySelector("main .sy-state")?.textContent,
      runbook: document
        .querySelector('[data-card="runbook"]')
        ?.getAttribute("download"),
    }));
    assert.deepEqual(r.heads, [
      "The host",
      "The fleet as a whole",
      "Set up",
      "Tools",
    ]);
    assert.equal(r.descs, 4, "a group without its sentence");
    assert.match(r.line ?? "", /^CPU \d+% · disk \d+% · \d+\.\d+\.\d+/);
    assert.equal(r.state, "host healthy");
    assert.equal(r.runbook, "DR_RUNBOOK.md");
  } finally {
    await browser.close();
  }
});

// H1: "newer version" meant three things: Stacks flagged kp-soft and admin
// (demo-agent, a pin in the homelab binary) while the Inbox counted only
// beta-demo's two apps and Update kp-soft offered only its moving-tag apps.
test("invariants: redesign-final-h1: Stacks, the Inbox and the Update flow name the same apps with a newer version, and a stack's flag leads to a flow that lists them", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/stacks?view=cards`);
    await page.locator(".sk-chip").first().waitFor({ timeout: 10000 });
    await page.waitForTimeout(1500);
    const flags = await page.evaluate(() => {
      /** @type {Record<string, number>} */
      const out = {};
      for (const c of document.querySelectorAll(".sk-card .sk-chip")) {
        const m = /^(\d+ )?newer versions?$/.exec((c.textContent ?? "").trim());
        if (!m) continue;
        const card = /** @type {HTMLElement} */ (c.closest(".sk-card"));
        const name = card.getAttribute("data-stack") ?? "";
        out[name] = m[1] ? Number(m[1]) : 1;
      }
      return out;
    });
    const total = Object.values(flags).reduce((a, b) => a + b, 0);
    assert.ok(
      total >= 1,
      `no stack flags a newer version (${JSON.stringify(flags)})`,
    );

    await page.goto(`${BASE}/inbox`);
    const updates = page.locator(
      "text=/\\d+ apps? (has|have) a newer version/",
    );
    await updates.first().waitFor({ timeout: 10000 });
    const title = await updates.first().innerText();
    // The Inbox counts the apps Update all lists (a stack the host does not
    // run yet, beta-demo, has no card on Stacks but its apps still count).
    await page.goto(`${BASE}/update?all=1`);
    await page.locator(".uf-item").first().waitFor({ timeout: 10000 });
    const listedAll = await page
      .locator('.uf-item:not([data-item^="pull:"])')
      .count();
    assert.equal(
      Number(/(\d+)/.exec(title)?.[1]),
      listedAll,
      `Inbox "${title}", Update all lists ${listedAll}`,
    );
    for (const [stack, n] of Object.entries(flags))
      assert.ok(
        (await page.locator(`.uf-item[data-item*=":${stack}:"]`).count()) === n,
        `Update all lists ${stack}'s apps differently from its flag (${n})`,
      );

    for (const [stack, n] of Object.entries(flags)) {
      await page.goto(`${BASE}/update?stack=${encodeURIComponent(stack)}`);
      await page.locator(".uf-item").first().waitFor({ timeout: 10000 });
      const listed = await page
        .locator('.uf-item:not([data-item^="pull:"])')
        .count();
      assert.equal(
        listed,
        n,
        `Update ${stack} lists ${listed}, its flag says ${n}`,
      );
    }
    await page.goto(`${BASE}/update?stack=kp-soft`);
    const rel = page.locator('.uf-item[data-item^="release:kp-soft:"]');
    await rel.first().waitFor({ timeout: 10000 });
    assert.match(await rel.first().innerText(), /with a homelab release/);
    assert.equal(
      await rel.locator("input").count(),
      0,
      "a release row has a tick",
    );
  } finally {
    await browser.close();
  }
});

// C3: Notification rules was never redesigned: breadcrumb "Notification
// rules" over an H1 "Notifications", generic tables with Filters, numeric
// dates, a select for the snooze, the notice list the Inbox holds now.
test("invariants: redesign-final-c3: Notification rules is the approved demo's Delivery and Per stack cards, named as its breadcrumb, without the notice list", async () => {
  const browser = await launch();
  try {
    for (const width of [1894, 390]) {
      const context = await browser.newContext({
        viewport: { width, height: 1000 },
      });
      const page = await freshPage(context);
      await page.goto(`${BASE}/system/notifications`);
      await page.locator("#notify-stacks .nr-stack").first().waitFor({
        timeout: 10000,
      });
      const r = await page.evaluate(() => {
        const crumbs = [
          ...document.querySelectorAll(
            ".nx-crumbs li, .nx-crumbs a, .nx-crumbs span",
          ),
        ]
          .map((x) => (x.textContent ?? "").trim())
          .filter(Boolean);
        const cols = document.querySelector(".nr-cols");
        return {
          h1: document.querySelector("main h1")?.textContent?.trim(),
          crumb: crumbs[crumbs.length - 1] ?? "",
          cards: [...document.querySelectorAll("main .nr-cols > section")].map(
            (s) => ({
              h: s.querySelector("h2")?.textContent?.trim(),
              d: (
                s.querySelector(".section-head__desc")?.textContent ?? ""
              ).trim(),
            }),
          ),
          tables: document.querySelectorAll("main table").length,
          selects: document.querySelectorAll("main select").length,
          seg: [...document.querySelectorAll("#delivery .nx-seg button")].map(
            (b) => b.firstChild?.textContent?.trim(),
          ),
          numeric: /\b\d{1,2}\/\d{1,2}\/\d{4}\b/.test(
            document.querySelector("main")?.textContent ?? "",
          ),
          colsN: cols
            ? getComputedStyle(cols).gridTemplateColumns.split(" ").length
            : 0,
          sideways: document.documentElement.scrollWidth - innerWidth,
        };
      });
      assert.equal(r.h1, "Notification rules");
      assert.equal(r.crumb, "Notification rules");
      assert.deepEqual(
        r.cards.map((c) => c.h),
        ["Delivery", "Per stack"],
      );
      for (const c of r.cards) assert.ok(c.d, `${c.h} has no sentence`);
      assert.equal(r.tables, 0, "a generic table on the rules page");
      assert.equal(r.selects, 0, "a native select on the rules page");
      assert.deepEqual(r.seg, ["1 h", "4 h", "Until 07:00", "1 day"]);
      assert.equal(r.numeric, false, "a numeric date");
      assert.equal(r.colsN, width > 500 ? 2 : 1, `@${width}: columns`);
      assert.ok(
        r.sideways <= 1,
        `@${width}: scrolls sideways by ${r.sideways}`,
      );
      await context.close();
    }
  } finally {
    await browser.close();
  }
});

// H3: FLOWS.md §5 approves a stepped Restore flow page; Restore… opened a
// picker dialog leading to a second dialog.
test("invariants: redesign-final-h3: Restore… opens the Restore flow page, Backups / stack / Restore, which runs app, night, confirm and the job's steps", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/backups`);
    await page.locator('main [data-drive="backups-restore"]').click();
    await page.waitForURL(/\/backups\/restore\?stack=/, { timeout: 10000 });
    assert.equal(
      await page.locator("dialog[open]").count(),
      0,
      "a dialog opened instead of the flow",
    );
    await page.goto(`${BASE}/backups/restore?stack=kp-soft`);
    const app = page.locator('[data-drive="restore-app"]').first();
    await app.waitFor({ timeout: 10000 });
    const head = await page.evaluate(() => ({
      h1: document.querySelector("main h1")?.textContent,
      crumbs: [...document.querySelectorAll(".nx-crumbs li")].map((x) =>
        (x.textContent ?? "").trim(),
      ),
      steps: [...document.querySelectorAll("main .nx-steps li")].map((x) =>
        (x.textContent ?? "").trim(),
      ),
    }));
    assert.equal(head.h1, "Restore kp-soft");
    assert.deepEqual(head.crumbs, ["Backups", "kp-soft", "Restore"]);
    assert.deepEqual(head.steps, [
      "Which app",
      "Which night",
      "Confirm",
      "Restore and check",
    ]);
    await app.click();
    const night = page.locator('[data-drive="restore-night"]').first();
    await night.waitFor({ timeout: 5000 });
    await night.click();
    assert.match(
      await page.locator("#restore-what").innerText(),
      /safety copy/,
    );
    const go = page.locator("#restore-go");
    assert.equal(
      await go.getAttribute("aria-disabled"),
      "true",
      "Restore is armed before the name",
    );
    await page.fill("#restore-confirm", "kp-soft");
    assert.equal(
      await go.getAttribute("aria-disabled"),
      "false",
      "Restore stays held after the name",
    );
    await go.click();
    await page.locator("#restore-run .nx-check li").first().waitFor({
      timeout: 10000,
    });
    assert.ok(
      await page.locator("#restore-run").isVisible(),
      "the job's steps are not shown",
    );
  } finally {
    await browser.close();
  }
});

// ── redesign-final round 2 (M1–M9, Low, X1–X7; coordinator 2026-10-04) ──

// M3: the palette offered Park and Unpark for a running stack, and a label
// wrapped ("Unpark · kp-↵soft"); it offers only the verb that applies, and
// a label keeps its own column on one line.
test("invariants: redesign-final-m3: the palette offers Park only for a stack that is not parked, never Unpark beside it, and no label wraps", async () => {
  const browser = await launch();
  try {
    for (const width of [1894, 390]) {
      const context = await browser.newContext({
        viewport: { width, height: width > 500 ? 1000 : 844 },
      });
      const page = await freshPage(context);
      await page.goto(`${BASE}/stacks`);
      await page.waitForTimeout(1200);
      const parked = await page.evaluate(async () => {
        const f = await (await fetch("/data/fleet")).json().catch(() => null);
        const s = (f?.stacks ?? f?.fleet?.stacks ?? []).find(
          (/** @type {any} */ x) => x.name === "kp-soft",
        );
        return s ? s.enabled === false : null;
      });
      await page.keyboard.press("Control+k");
      const input = page.locator("#commands input");
      await input.waitFor({ timeout: 3000 });
      await input.fill("park kp-soft");
      await page.waitForTimeout(200);
      const got = await page.evaluate(() =>
        [...document.querySelectorAll("#commands [data-kp-option]")]
          .filter((o) => !(/** @type {HTMLElement} */ (o).hidden))
          .map((o) => {
            const l = /** @type {HTMLElement} */ (
              o.querySelector(".nx-palette__label")
            );
            const r = document.createRange();
            r.selectNodeContents(l);
            const lines = new Set(
              [...r.getClientRects()].map((x) => Math.round(x.top)),
            ).size;
            return { label: l.textContent?.trim() ?? "", lines };
          }),
      );
      const labels = got.map((g) => g.label);
      if (parked !== true) {
        assert.ok(labels.includes("Park · kp-soft"), `${width}: ${labels}`);
        assert.ok(!labels.includes("Unpark · kp-soft"), `${width}: ${labels}`);
      } else {
        assert.ok(labels.includes("Unpark · kp-soft"), `${width}: ${labels}`);
        assert.ok(!labels.includes("Park · kp-soft"), `${width}: ${labels}`);
      }
      assert.deepEqual(
        got.filter((g) => g.lines > 1).map((g) => g.label),
        [],
        `${width}: a palette label wraps`,
      );
      await context.close();
    }
  } finally {
    await browser.close();
  }
});

// Low (Kenny's rule): multi-sort without Shift. A plain click on a second
// header adds it as a further key, on the dashboard's own sortable tables
// and on kp datatables alike; Reset sort clears every key.
test("invariants: redesign-final-low-sort: a plain click on a second header adds a sort key (no Shift), and Reset sort clears them", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    // The Host page's Containers table (ui.js sortableTable).
    await page.goto(`${BASE}/host`);
    const th = (/** @type {string} */ k) =>
      page.locator(
        `[data-drive="sort-host-containers"][data-drive-row="${k}"]`,
      );
    await th("status").waitFor({ timeout: 10000 });
    await th("status").click();
    await th("memory").click();
    const sorted = await page.evaluate(() =>
      [
        ...document.querySelectorAll(
          '[data-drive="sort-host-containers"][aria-sort]:not([aria-sort="none"])',
        ),
      ].map((e) => /** @type {HTMLElement} */ (e).dataset.driveRow),
    );
    assert.deepEqual(sorted.sort(), ["memory", "status"], "two keys");
    const reset = page.locator(
      '[data-drive="table-reset-sort"][data-drive-row="host-containers"]',
    );
    assert.ok(await reset.isVisible(), "no Reset sort while sorted");
    await reset.click();
    assert.equal(
      await page
        .locator(
          '[data-drive="sort-host-containers"][aria-sort]:not([aria-sort="none"])',
        )
        .count(),
      0,
      "Reset sort left a key",
    );
    // A kp datatable (the Stacks table): two plain clicks, two keys.
    await page.goto(`${BASE}/stacks?view=table`);
    const head = page.locator(".sk-table thead th[data-kp-sort]");
    await head.first().waitFor({ timeout: 10000 });
    await head.nth(0).click();
    await head.nth(1).click();
    assert.equal(
      await page
        .locator('.sk-table thead th[aria-sort]:not([aria-sort="none"])')
        .count(),
      2,
      "a plain click on a second kp datatable header did not add a key",
    );
    const kpReset = page.locator('.sk-table [data-drive="table-reset-sort"]');
    await kpReset.click();
    assert.ok(
      (await page
        .locator('.sk-table thead th[aria-sort]:not([aria-sort="none"])')
        .count()) <= 1,
      "Reset sort did not clear the kp datatable's keys",
    );
  } finally {
    await browser.close();
  }
});

// M1: Stacks drifted from flows/stacks.html: an Inbox banner duplicating
// its tile, RAM and Load tiles instead of Need you and Newer versions,
// Cards by default, a section titled "Stacks", and key hints with filter
// syntax crowding above the toolbar.
test("invariants: redesign-final-m1: Stacks is the approved demo: no Inbox banner, its five tiles, the Table by default, All stacks, key hints in the footer", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.evaluate(() => localStorage.removeItem("homelab.stacks.view"));
    await page.goto(`${BASE}/stacks`);
    await page.waitForSelector(".sk-kpis .nx-kpi:not([data-loading])", {
      timeout: 8000,
    });
    await page.waitForTimeout(800);
    const f = await page.evaluate(() => {
      const card = document.querySelector("#stack-list");
      const tb = card?.querySelector(".nx-tb");
      const before = [];
      for (let n = tb?.previousElementSibling; n; n = n.previousElementSibling)
        before.push(n.textContent ?? "");
      return {
        band: document.querySelectorAll("#page .nx-attention .kp-alert").length,
        tiles: [...document.querySelectorAll(".sk-kpis .nx-kpi__label")].map(
          (e) => e.textContent?.trim(),
        ),
        table: !!document.querySelector(".sk-table tbody tr[data-stack]"),
        cards: !!document.querySelector(".sk-cards .sk-card[data-stack]"),
        title: card?.querySelector("h2")?.textContent?.trim(),
        keysInFoot: !!card?.querySelector(".sk-foot .nx-keys"),
        aboveToolbar: before.join(" "),
      };
    });
    assert.equal(f.band, 0, "an Inbox banner beside the Need you tile");
    assert.deepEqual(f.tiles, [
      "Stacks running",
      "Need you",
      "Newer versions",
      "Host CPU",
      "Root disk",
    ]);
    assert.ok(f.table && !f.cards, "the Table is not the default view");
    assert.equal(f.title, "All stacks");
    assert.ok(f.keysInFoot, "the key hints are not in the footer");
    assert.doesNotMatch(f.aboveToolbar, /state:|flag:|move|tick/);
  } finally {
    await browser.close();
  }
});

// Low: Settings had the H1 "Settings" under the nav's and the breadcrumb's
// "Host settings"; one name.
test("invariants: redesign-final-low-settings: the Host settings page, its breadcrumb and the System tile that opens it use one name", async () => {
  const browser = await launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/settings`);
    const h1 = page.locator("#page h1");
    await h1.waitFor({ timeout: 10000 });
    const crumbs = await page.locator("#crumbs").innerText();
    assert.equal((await h1.innerText()).trim(), "Host settings");
    assert.match(crumbs, /Host settings\s*$/);
    await page.goto(`${BASE}/system`);
    await page.waitForTimeout(800);
    assert.ok(
      (await page.locator('#page a[href="/settings"]').allInnerTexts()).some(
        (t) => /Host settings/.test(t),
      ),
      "System names it otherwise",
    );
  } finally {
    await browser.close();
  }
});
