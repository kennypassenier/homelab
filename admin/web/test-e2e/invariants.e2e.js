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
import { chromium } from "playwright";
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
  await logIn(context);
  const page = await context.newPage();
  await page.goto(`${BASE}/stacks`);
  return page;
}

test("invariants: the nav bar stays inline, with brand Homelab and the version beside the search box, at 1600/1920/2560 CSS px and at 1894 in the widest themes", async () => {
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.goto(`${BASE}/stacks/films`);
    await page.waitForTimeout(600);
    // H8 (per-stack enabled flag): "Park" ("Disable" before 3.71.0, the
    // approved rename) needs only the stack name, so it runs end to end
    // against the demo host without a working copy of the repository
    // (deploy/backup both refuse without one there).
    await page
      .getByRole("button", { name: "Park", exact: true })
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
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    // feat-shell-1: the notices table is Notification rules' page under
    // System since 3.71.0 (`/notifications` itself goes to the Inbox).
    await page.goto(`${BASE}/system/notifications`);
    const row = page
      .locator("[data-kp-expandable] tbody tr:not([data-kp-detail])")
      .first();
    await row.waitFor({ timeout: 10000 });
    const toggle = row.locator("[data-kp-row-toggle]");
    const before = await toggle.getAttribute("aria-expanded");
    // A plain cell: the last text cell that holds no control of its own.
    const plain = row.locator("td:not([data-kp-expand-cell])").nth(1);
    await plain.click();
    const after = await toggle.getAttribute("aria-expanded");
    assert.notEqual(
      after,
      before,
      "a click in the row must toggle it (Kenny, 2026-10-02)",
    );
    await plain.click();
    assert.equal(
      await toggle.getAttribute("aria-expanded"),
      before,
      "a second click must close it again",
    );
    // A control of the row's own keeps doing its own thing.
    const control = row
      .locator("td button:not([data-kp-row-toggle]), td a")
      .first();
    if ((await control.count()) > 0) {
      const href = await control.getAttribute("href");
      if (href === null) {
        await control.click();
        assert.equal(
          await toggle.getAttribute("aria-expanded"),
          before,
          "a click on the row's own button must not toggle the row",
        );
      }
    }
  } finally {
    await browser.close();
  }
});

test("invariants: the kit's Status and Clients pages are switched off, Passkeys stays", async () => {
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
  } finally {
    await browser.close();
  }
});

// ── fix-202: the backup calendar (Kenny, 2026-10-02 verbatim: "na een paar
// minuten zie ik nog altijd niks van data laden. nog altijd 11/13") ────────

test("invariants: the backup calendar paints a stack's own cells as soon as it answers, without waiting for the rest", async () => {
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
      await page.locator(".topology__svg").count(),
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
    await page.waitForSelector(".topology__svg", { timeout: 10000 });
    assert.equal(
      await page.locator(".topology__svg").count(),
      1,
      "the fleet view must draw exactly one topology",
    );
    const toggle = page.locator("#fleetview-traffic");
    assert.equal(
      await page.locator(".topology__traffic-ring").count(),
      0,
      "a traffic ring is drawn before the toggle is on",
    );
    await toggle.check();
    await page.waitForFunction(
      () => document.querySelectorAll(".topology__traffic-ring").length > 0,
      { timeout: 5000 },
    );
  } finally {
    await browser.close();
  }
});

// ── fix-203: fleet view's topology (Kenny, verbatim: "waarom is bv almanac
// met niks gelinked?") ──────────────────────────────────────────────────

test("invariants: fleet view shows a legend and an edge for a stack whose own files name another stack's address", async () => {
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.goto(`${BASE}/fleetview`);
    await page.waitForTimeout(600);
    const legendItems = await page
      .locator(".topology__legend .topology__key")
      .count();
    assert.ok(legendItems > 0, "the topology has no legend");
    // scripts/invariants-run.sh's fixture working copy: alpha-demo names
    // beta-demo's address (10.10.10.91:8080) directly in its own manifest,
    // with no firewall rule involved at all (fix-203's `named` edge kind).
    // An SVG <path>'s own visibility heuristic can read "hidden" when its
    // bounding box has zero width (two nodes placed directly above each
    // other on the circle layout draw a dead-straight vertical curve) —
    // present in the DOM is what matters here, not CSS visibility.
    await page.waitForFunction(
      () => document.querySelectorAll(".topology__edge--named").length > 0,
      { timeout: 5000 },
    );
    const named = page.locator(".topology__edge--named");
    assert.ok(
      (await named.count()) > 0,
      "no named edge for alpha-demo -> beta-demo",
    );
  } finally {
    await browser.close();
  }
});

test("invariants: every page keeps clear vertical spacing between its top-level sections and is drawn in kp-themes styling", async () => {
  const browser = await chromium.launch();
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

test("invariants: the topology's legend gives every stack its own colour", async () => {
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.goto(`${BASE}/fleetview`);
    await page.waitForSelector(".topology__stack-key", { timeout: 10000 });
    const hues = await page
      .locator(".topology__stack-key")
      .evaluateAll((els) =>
        els.map((el) => el.style.getPropertyValue("--stack-hue")),
      );
    assert.ok(
      hues.length >= 2,
      `expected at least two stacks, got ${hues.length}`,
    );
    assert.equal(
      new Set(hues).size,
      hues.length,
      `every stack's legend swatch must have its own hue, got ${hues}`,
    );
    // Each node's own hue (in the graph) matches its legend entry.
    const nodeHues = await page
      .locator(".topology__node")
      .evaluateAll((els) =>
        els.map((el) => [
          el.getAttribute("data-stack"),
          el.style.getPropertyValue("--stack-hue"),
        ]),
      );
    const byStack = Object.fromEntries(
      await page
        .locator(".topology__stack-key")
        .evaluateAll((els) =>
          els.map((el) => [
            el.textContent.trim(),
            el.style.getPropertyValue("--stack-hue"),
          ]),
        ),
    );
    for (const [stack, hue] of nodeHues) {
      assert.equal(
        byStack[stack],
        hue,
        `${stack}'s node hue must match its legend swatch`,
      );
    }
  } finally {
    await browser.close();
  }
});

test("invariants: hovering a stack in the topology isolates its own edges, leaving restores them", async () => {
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.goto(`${BASE}/fleetview`);
    await page.waitForSelector(".topology__stack-key", { timeout: 10000 });
    const stacks = await page
      .locator(".topology__stack-key")
      .evaluateAll((els) => els.map((el) => el.textContent.trim()));
    assert.ok(
      stacks.length >= 2,
      "need at least two stacks to prove isolation",
    );
    const target = stacks[0];
    const before = await page.locator(".topology__edge--dim").count();
    assert.equal(before, 0, "nothing is dimmed before any hover");
    await page
      .locator(".topology__stack-key", { hasText: target })
      .first()
      .hover();
    await page.waitForTimeout(50);
    const edgeStates = await page
      .locator(".topology__edge")
      .evaluateAll((els) =>
        els.map((el) => ({
          from: el.getAttribute("data-from"),
          to: el.getAttribute("data-to"),
          dim: el.classList.contains("topology__edge--dim"),
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
      await page.locator(".topology__edge--dim").count(),
      0,
      "leaving the stack must restore every edge",
    );
  } finally {
    await browser.close();
  }
});

test("invariants: a stack whose firewall the host enforces is shown as enforced even when the repository disagrees", async () => {
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    // scripts/invariants-run.sh's fixture declares "gateway" with its
    // firewall off; the demo host (admin/src/shell/demo.rs) always answers
    // that the first such stack is enforced on pve anyway, with the
    // mismatch flagged — Kenny's own "5 van de 11" case.
    await page.goto(`${BASE}/fleetview`);
    await page.waitForSelector(".topology__node", { timeout: 10000 });
    const mismatchRings = await page
      .locator(".topology__fw-ring--mismatch")
      .count();
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

test("invariants: Overview's apply section loads only after it is opened, and shows a loading state first", async () => {
  const browser = await chromium.launch();
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
    await page.goto(`${BASE}/overview`);
    await page.waitForSelector("#apply-section", { timeout: 5000 });
    assert.equal(
      planRequests.length,
      0,
      "the apply plan was read before its section was ever opened",
    );
    const openBefore = await page.$eval(
      "#apply-section",
      (d) => /** @type {HTMLDetailsElement} */ (d).open,
    );
    assert.equal(openBefore, false, "the apply section starts open");

    await page.click("#apply-section > summary");
    // Right after opening, before the delayed plan has answered: a
    // skeleton row must already be on screen. `waitFor` (not a bare
    // `count()`) because a `<details>` fires its own "toggle" event as a
    // queued task, not synchronously with the click — this still proves
    // the point as long as the skeleton attaches well within the 1200ms
    // the plan read is held back by.
    await page
      .locator(".apply-row--skeleton")
      .first()
      .waitFor({ state: "attached", timeout: 1000 });
    assert.ok(
      planRequests.length > 0,
      "opening the section never triggered the plan read",
    );

    // Once the delayed read lands, the skeleton is gone and the real plan
    // is drawn.
    await page.waitForSelector(".apply-row--skeleton", {
      state: "detached",
      timeout: 5000,
    });
  } finally {
    await browser.close();
  }
});

test("invariants: /apply lands on Stacks with Deploy all changes open", async () => {
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    // feat-shell-1: Overview is Stacks since 3.71.0 and its Apply section
    // is "Deploy all changes" (`?deploy-all=1`), the approved redirect.
    await page.goto(`${BASE}/apply`);
    await page.waitForURL("**/stacks?deploy-all=1", { timeout: 5000 });
    const open = await page.$eval(
      "#apply-section",
      (d) => /** @type {HTMLDetailsElement} */ (d).open,
    );
    assert.equal(open, true, "/apply did not land with its section open");
    const headingText = await page
      .locator("#apply-section > summary")
      .innerText();
    assert.ok(
      headingText.includes("Apply the whole fleet"),
      "the opened section is not the apply section",
    );
  } finally {
    await browser.close();
  }
});

test("invariants: every data-loading page shows a loading indicator before its data (or error) arrives", async () => {
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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

test("invariants: opening Restore from an app's row never asks for the app again and shows dated snapshots with the latest preselected", async () => {
  // fix-216 (Kenny, 2026-10-02: "als ik daar bv op kyu restore pak, dan
  // vraagt die nog altijd welke app, terwijl ik restore al bij een app
  // selecteerde? ... ik heb toch geen idee wat die snapshot is?").
  // kp-soft's fixture stack declares two apps (kp-soft, jobtracker), so its
  // Backups rows are a real case of "the row already said which app" — the
  // demo host answers GetBackups with a few nights of made-up snapshots per
  // app (admin/src/shell/demo.rs).
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.goto(`${BASE}/backups`);
    const row = page.locator('tr[data-kp-row-key="kp-soft-jobtracker"]');
    await row.waitFor({ timeout: 15000 });
    await row.getByRole("button", { name: /Restore/ }).click();
    const dialog = page.locator("dialog#action-dialog");
    await dialog.waitFor({ timeout: 5000 });

    // The app is shown read-only, with the row's own choice ("jobtracker")
    // — never a select box asking which app, all over again.
    const appLocked = dialog.locator('[data-field="app-locked"]');
    await appLocked.waitFor({ timeout: 5000 });
    assert.match(await appLocked.innerText(), /jobtracker/);
    assert.equal(
      await dialog.locator('select[name="app"]').isVisible(),
      false,
      "the app select must stay hidden behind the locked summary until Change is pressed",
    );

    // The snapshot field is a picker: dated rows, newest first, the first
    // one marked "latest" and preselected.
    const rows = dialog.locator(".act-snapshot-row");
    await rows.first().waitFor({ timeout: 5000 });
    const count = await rows.count();
    assert.ok(count >= 2, `expected several snapshot rows, got ${count}`);
    const firstText = await rows.first().innerText();
    assert.match(firstText, /\d{2}\/\d{2}\/\d{4} \d{2}:\d{2}/, "a dated row");
    assert.match(firstText, /ago/);
    assert.match(firstText, /latest/);
    assert.equal(
      await rows.first().locator('input[type="radio"]').isChecked(),
      true,
      "the newest snapshot is preselected",
    );
    assert.equal(
      await rows.nth(1).locator('input[type="radio"]').isChecked(),
      false,
    );
    // Never a bare restic id as the only thing shown — the short id is
    // there, but only beside the date, never alone.
    const idText = await rows
      .first()
      .locator(".act-snapshot-row__id")
      .innerText();
    assert.match(idText, /^demo/);
  } finally {
    await browser.close();
  }
});

test("invariants: no /charts series label matches a raw-id pattern and every chart has a description", async () => {
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.goto(`${BASE}/charts`, { waitUntil: "load" });
    // The host's own charts (no ?stack=) carry the temperature/SMART/
    // network/memory panels the demo metrics server answers with raw ids.
    await page.waitForSelector(".chart, .chart--health", { timeout: 5000 });
    await page.waitForTimeout(300);
    const found = await page.evaluate(() => {
      /** @type {string[]} */
      const labels = [];
      for (const li of document.querySelectorAll(".chart__key"))
        labels.push(li.textContent ?? "");
      for (const td of document.querySelectorAll(
        ".chart--health td:first-child",
      ))
        labels.push(td.textContent ?? "");
      /** @type {string[]} */
      const noDesc = [];
      for (const fig of document.querySelectorAll(".chart, .chart--health")) {
        const cap = fig.querySelector("figcaption")?.textContent?.trim();
        const desc = fig.querySelector(".chart__desc")?.textContent?.trim();
        if (!desc || desc.length < 10) noDesc.push(cap ?? "(no caption)");
      }
      return { labels, noDesc };
    });
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
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.goto(`${BASE}/charts`, { waitUntil: "load" });
    await page.waitForSelector(".chart--health", { timeout: 5000 });
    const rows = await page.evaluate(() => {
      const table = [...document.querySelectorAll(".chart--health")].find((f) =>
        f.querySelector("figcaption")?.textContent?.includes("Drive health"),
      );
      return [...(table?.querySelectorAll("tbody tr") ?? [])].map((tr) => ({
        device: tr.children[0]?.textContent?.trim() ?? "",
        state: tr.children[1]?.textContent?.trim() ?? "",
      }));
    });
    assert.ok(
      rows.length >= 2,
      `expected at least 2 drives, got: ${JSON.stringify(rows)}`,
    );
    assert.ok(
      rows.some((r) => r.state === "not ok"),
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
      if (!(await btn.isVisible()) || !(await btn.isEnabled())) continue;
      await btn.click();
      const dialog = page.locator("dialog#action-dialog[open]");
      if (!(await dialog.isVisible().catch(() => false))) {
        await page.waitForTimeout(300);
        if (!(await dialog.isVisible().catch(() => false))) {
          for (const close of await page
            .locator("dialog[open] .kp-dialog__close")
            .all())
            await close.click().catch(() => {});
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
        await close.click().catch(() => {});
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
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    for (const tab of ["system", "traffic"]) {
      await page.goto(`${BASE}/charts?tab=${tab}`);
      const grid = page.locator(".chart-grid").first();
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1920, height: 1080 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/fleetview`);
    await page.waitForTimeout(600);
    const blocks = await page.evaluate(() =>
      [...document.querySelectorAll("#page > section")].map((s) => {
        const head = s.querySelector(":scope > .section-head h2");
        const desc = s.querySelector(
          ":scope > .section-head .section-head__desc",
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
    assert.ok(blocks.length >= 5, `expected 5 blocks, got ${blocks.length}`);
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
  const browser = await chromium.launch();
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
      hasText: "updated with a homelab release, not from here",
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

    // The row marked as a major jump: the dialog names from and to, warns,
    // and keeps Confirm off until the release notes are ticked as read.
    const major = page.locator(
      'section[aria-label="Stale images"] button[data-pin-update][data-major]',
    );
    assert.ok((await major.count()) >= 1, "no major-jump row in the demo");
    const from = (await major.first().getAttribute("data-from")) ?? "";
    const to = (await major.first().getAttribute("data-to")) ?? "";
    assert.match(await major.first().innerText(), /Update to/);
    await major.first().click();
    const dialog = page.locator("dialog#action-dialog[open]");
    await dialog.waitFor({ timeout: 5000 });
    const confirm = dialog.locator("button[data-pin-confirm]");
    await confirm.waitFor({ timeout: 10000 });
    const text = await dialog.innerText();
    assert.ok(
      text.includes(from),
      `the dialog does not name the pinned ${from}`,
    );
    assert.ok(text.includes(to), `the dialog does not name the target ${to}`);
    assert.match(text, /major/i);
    const tick = dialog.getByLabel(
      "I read the release notes for this major version",
    );
    assert.equal(await tick.isChecked(), false);
    assert.equal(
      await confirm.isEnabled(),
      false,
      "Confirm must stay off until the release notes are ticked",
    );
    await tick.check();
    assert.equal(
      await confirm.isEnabled(),
      true,
      "the tick must enable Confirm",
    );
    await page.keyboard.press("Escape");
    await dialog.waitFor({ state: "hidden", timeout: 5000 }).catch(() => {});

    // A minor move asks no tick: Confirm is on at once.
    const minor = page.locator(
      'section[aria-label="Stale images"] button[data-pin-update]:not([data-major])',
    );
    assert.ok((await minor.count()) >= 1, "no minor-jump row in the demo");
    await minor.first().click();
    await dialog.waitFor({ timeout: 5000 });
    const confirm2 = dialog.locator("button[data-pin-confirm]");
    await confirm2.waitFor({ timeout: 10000 });
    assert.equal(
      await dialog
        .getByLabel("I read the release notes for this major version")
        .count(),
      0,
    );
    assert.equal(await confirm2.isEnabled(), true);
  } finally {
    await browser.close();
  }
});

test("invariants: every link on every drivable page has an absolute http(s) href or a same-site path that resolves", async () => {
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
test("invariants: every button that opens a dialog or runs an action is reachable through Live view", async (t) => {
  const { default: SPEC } = await import("../js/formspec.json", {
    with: { type: "json" },
  });
  const forms = new Set(SPEC.forms);
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    /** @type {string[]} */
    const changes = [];
    await page.route("**/data/**", (r) => {
      const req = r.request();
      if (req.method() !== "GET" && !req.url().includes("/data/drive/")) {
        changes.push(`${req.method()} ${new URL(req.url()).pathname}`);
        return r.abort();
      }
      return r.continue();
    });
    const paths = [
      ...DRIVABLE_PATHS.map((p) => `/${p}`),
      ...[
        "",
        "/apps",
        "/history",
        "/logs",
        "/checks",
        "/settings",
        "/firewall",
      ].map((t) => `/stacks/beta-demo${t}`),
    ];
    const SEL =
      "#page button:not([disabled]), #page [role=button], #page [role=switch]:not([disabled])";
    /** Every candidate on screen, outside a dialog, as the page marks it. */
    const describe = () =>
      page.$$eval(SEL, (els) =>
        els
          .filter((e) => e.getClientRects().length > 0 && !e.closest("dialog"))
          .map((e) => {
            const el = /** @type {HTMLElement} */ (e);
            const label = (
              el.getAttribute("aria-label") ||
              el.textContent ||
              ""
            )
              .trim()
              .replace(/\s+/g, " ")
              .slice(0, 60);
            return {
              key: `${label.replace(/\d+/g, "#")}|${el.dataset.drive ?? ""}`,
              label,
              drive: el.dataset.drive ?? null,
              row: el.dataset.driveRow ?? null,
              form:
                el.dataset.action ??
                /** @type {HTMLElement | null} */ (
                  el.closest("[data-drive-form]")
                )?.dataset.driveForm ??
                null,
            };
          }),
      );
    const visibleHandles = async () => {
      const out = [];
      for (const e of await page.$$(SEL))
        if (
          await e.evaluate(
            (x) => x.getClientRects().length > 0 && !x.closest("dialog"),
          )
        )
          out.push(e);
      return out;
    };
    // The page's own declarations, from the module the dashboard loaded.
    const declared = new Set(
      await page.evaluate(async () => {
        const m = await import("/js/drivable.js").catch(() => null);
        return m ? m.controls().map((/** @type {any} */ c) => c.id) : [];
      }),
    );
    const effect = async () => {
      for (let i = 0; i < 12; i += 1) {
        const open = await page.$$eval("dialog[open]", (d) => d.length);
        if (open || changes.length) break;
        await page.waitForTimeout(100);
      }
      const dialogs = await page.$$eval("dialog[open]", (d) =>
        d.map((x) =>
          (x.querySelector(".kp-dialog__title")?.textContent ?? "").trim(),
        ),
      );
      return [
        ...dialogs.map((t) => `opens "${t}"`),
        ...changes.map((c) => `sends ${c}`),
      ].join(", ");
    };
    const closeDialogs = () =>
      page.evaluate(() =>
        document
          .querySelectorAll("dialog[open]")
          .forEach((d) => /** @type {HTMLDialogElement} */ (d).close()),
      );
    /** @type {string[]} */
    const unreachable = [];
    /** @type {Map<string, {path: string, row: string | null}>} the path is
     * where the walk found it, for the failure message only */
    const toDrive = new Map();
    let walked = 0;
    for (const path of paths) {
      await page.goto(`${BASE}${path}`);
      await page.waitForTimeout(1500);
      const seen = new Set();
      for (const c of await describe()) {
        if (seen.has(c.key)) continue;
        seen.add(c.key);
        walked += 1;
        if (new URL(page.url()).pathname !== path) {
          await page.goto(`${BASE}${path}`);
          await page.waitForTimeout(1500);
        }
        const i = (await describe()).findIndex((x) => x.key === c.key);
        if (i < 0) continue;
        changes.length = 0;
        const handles = await visibleHandles();
        await handles[i]?.click({ timeout: 2000 }).catch(() => {});
        const did = await effect();
        await closeDialogs();
        if (!did) continue;
        const viaForm = c.form != null && forms.has(c.form);
        const viaControl = c.drive != null && declared.has(c.drive);
        if (!viaForm && !viaControl)
          unreachable.push(`${path}: "${c.label}" ${did}`);
        if (viaControl && c.drive && !toDrive.has(c.drive))
          toDrive.set(c.drive, { path, row: c.row });
        // A change was held back: the page may show its error; start clean.
        if (changes.length) {
          await page.goto(`${BASE}${path}`);
          await page.waitForTimeout(1500);
        }
      }
    }
    assert.ok(
      walked > 50,
      `only ${walked} buttons were walked: the pages did not draw`,
    );
    assert.deepEqual(
      unreachable,
      [],
      `buttons Live view cannot reach (declare them in drivable.js, or mark the form that reaches them): ${unreachable.join("; ")}`,
    );
    assert.ok(
      toDrive.has("pin-update"),
      "the stale image's Update was never found to drive",
    );

    // Every page control found, driven through the Live view driver.
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
    /** @type {string[]} */
    const failed = [];
    try {
      for (const [control, at] of toDrive) {
        // From another page: the tab goes to the control's own page itself
        // (its declaration names it), so a page that moves keeps working.
        await page.goto(`${BASE}/jobs`);
        await page.waitForTimeout(1000);
        changes.length = 0;
        const r = await step({
          do: "click",
          control,
          ...(at.row == null ? {} : { row: at.row }),
        });
        const did = await effect();
        if (!r.ok)
          failed.push(
            `${control} (found on ${at.path}): refused, ${r.refusal?.why}; ${r.refusal?.fix}`,
          );
        else if (!did)
          failed.push(`${control}: taken, but nothing opened or was sent`);
        if (r.state?.page_dialog) await step({ do: "close" });
        await closeDialogs();
      }
    } finally {
      await step({ do: "done" });
    }
    t.diagnostic(
      `walked ${walked} buttons; drove ${toDrive.size} page controls through Live view: ${[...toDrive.keys()].join(", ")}`,
    );
    assert.deepEqual(
      failed,
      [],
      `page controls Live view could not drive: ${failed.join("; ")}`,
    );
  } finally {
    await browser.close();
  }
});

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
      if (!(await btn.isVisible()) || !(await btn.isEnabled())) continue;
      await btn.click();
      const dialog = page.locator("dialog#action-dialog[open]");
      await dialog.waitFor({ timeout: 1500 }).catch(() => {});
      if (await dialog.isVisible().catch(() => false))
        await measure(`${path} ${action}`, dialog);
      for (const close of await page
        .locator("dialog[open] .kp-dialog__close")
        .all())
        await close.click().catch(() => {});
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
test("invariants: a collapsible block's heading stays beside its chevron, on desktop and phone", async () => {
  const browser = await chromium.launch();
  try {
    const bad = [];
    let seen = 0;
    for (const width of [1894, 390]) {
      const context = await browser.newContext({
        viewport: { width, height: 1000 },
      });
      const page = await freshPage(context);
      for (const path of ["overview", "health"]) {
        await page.goto(`${BASE}/${path}`);
        await page.locator("details.health-block").first().waitFor();
        const found = await page.evaluate(() =>
          [...document.querySelectorAll("details.health-block > summary")]
            .filter((s) => /** @type {HTMLElement} */ (s).offsetParent)
            .map((s) => {
              const head = s.querySelector("h2, h3");
              if (!head) return null;
              const st = getComputedStyle(s);
              const contentLeft =
                s.getBoundingClientRect().left +
                parseFloat(st.paddingInlineStart) +
                parseFloat(st.borderInlineStartWidth);
              return {
                name: (head.textContent ?? "").trim(),
                indent: Math.round(
                  head.getBoundingClientRect().left - contentLeft,
                ),
              };
            })
            .filter((x) => x !== null),
        );
        for (const f of found) {
          seen++;
          // The chevron is 0.5rem plus its margins: a heading beside it
          // starts at least ~12 px in; one wrapped under it starts at 0.
          if (f.indent < 10)
            bad.push(
              `${width}px /${path}: "${f.name}" wrapped under its chevron`,
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
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1894, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/stacks/films`);
    await page.waitForTimeout(600);
    // "Park": "Disable" before 3.71.0 (the approved rename).
    await page
      .getByRole("button", { name: "Park", exact: true })
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
        .waitForSelector(".chart__svg text, .timeline svg text", {
          timeout: 8000,
        })
        .catch(() => {});
      await page.waitForTimeout(400);
      const found = await page.evaluate(() => {
        const out = [];
        let n = 0;
        for (const t of document.querySelectorAll(
          ".chart__svg text, .timeline svg text",
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
  const browser = await chromium.launch();
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
    await page.goto(`${BASE}/charts`);
    await page.waitForSelector(".chart__svg", { timeout: 8000 });
    await page.waitForTimeout(300);
    const charts = await page.evaluate(() =>
      [...document.querySelectorAll(".chart:not(.chart--health)")]
        .filter((f) => f.querySelector(".chart__svg"))
        .map((f) => ({
          title: f.querySelector("figcaption")?.textContent ?? "",
          series: f.querySelectorAll(".chart__key").length,
          points: [...f.querySelectorAll(".chart__point")].filter((c) => {
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
        [...document.querySelectorAll(".chart")]
          .map((f) => ({
            title: f.querySelector("figcaption")?.textContent ?? "",
            ticks: [...f.querySelectorAll(".chart__tick--y")].map(
              (t) => t.textContent ?? "",
            ),
          }))
          .filter((c) => new Set(c.ticks).size !== c.ticks.length),
      );
    const dupOne = await dupAt();
    await page.unroute("**/data/charts*");
    await page.goto(`${BASE}/charts`);
    await page.waitForSelector(".chart__svg", { timeout: 8000 });
    await page.waitForTimeout(300);
    const yTicks = await page.locator(".chart__tick--y").count();
    assert.ok(yTicks > 0, "no y tick labels found (.chart__tick--y)");
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
  const browser = await chromium.launch();
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
    await page.waitForSelector("table caption", { timeout: 8000 });
    await page.waitForTimeout(300);
    const cells = await page.evaluate(() => {
      const t = [...document.querySelectorAll("table")].find((x) =>
        x.querySelector("caption")?.textContent?.includes("client"),
      );
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
    const named = cells.find((c) =>
      c.text.includes("not logged by the gateway"),
    );
    assert.ok(
      named,
      `no "not logged by the gateway" row: ${JSON.stringify(cells)}`,
    );
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
      await page.goto(`${BASE}/doctor`);
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
          document
            .querySelector('[data-kp-remember="doctor"]')
            ?.closest("details") ??
          document.querySelector("main") ??
          document.body;
        return {
          verdict: [...main.querySelectorAll(".state")]
            .map((s) => s.textContent ?? "")
            .join(" "),
          spinners: [...main.querySelectorAll(".kp-spinner")].filter(shown)
            .length,
          notRead: [...main.querySelectorAll("p, span")].some(
            (e) => shown(e) && (e.textContent ?? "").trim() === "not read yet",
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
        () =>
          document
            .querySelector('[data-kp-remember="doctor"]')
            ?.closest("details")
            ?.querySelector(".state")
            ?.textContent?.includes("overall"),
        null,
        { timeout: 10000 },
      );
      const after = await page.evaluate(
        () =>
          document
            .querySelector('[data-kp-remember="doctor"]')
            ?.closest("details")
            ?.querySelector(".state")?.textContent ?? "",
      );
      assert.match(after ?? "", /overall ok/);
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
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
    });
    const page = await freshPage(context);
    await page.goto(`${BASE}/fleetview`);
    await page.locator(".topology__node").nth(1).waitFor({ timeout: 10000 });
    await page.waitForTimeout(500);
    const stacks = await page.evaluate(() => {
      /** @type {any} */ (window).__clicks = [];
      const nodes = [...document.querySelectorAll(".topology__node")];
      /** @type {any} */ (window).__nodes = nodes;
      document.addEventListener(
        "click",
        (e) => {
          const g = /** @type {Element} */ (e.target).closest?.(
            ".topology__node",
          );
          /** @type {any} */ (window).__clicks.push(
            g ? g.getAttribute("data-stack") : null,
          );
        },
        true,
      );
      return nodes.map((n) => n.getAttribute("data-stack"));
    });
    const dots = page.locator(".topology__node .topology__dot");
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
      same: [...document.querySelectorAll(".topology__node")].every(
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
    // Enter starts it: the action's own dialog opens (nothing runs yet).
    await first("update kp-soft");
    await page.keyboard.press("Enter");
    const dialog = page.locator("dialog#action-dialog[open]");
    await dialog.waitFor({ timeout: 5000 });
    const title = await dialog.locator(".kp-dialog__title").first().innerText();
    assert.match(title, /Update/, `the dialog that opened: ${title}`);
  } finally {
    await browser.close();
  }
});

test("invariants: the kp-themes theme picker is in the bar on every page, desktop and phone", async () => {
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
      assert.equal((await badge.innerText()).trim(), "12", `${width}px`);
      const rows = await page.locator("#page .nx-inbox__row").count();
      assert.equal(rows, 12, `${width}px: the Inbox shows ${rows} rows`);
      assert.match(await page.title(), /^● 12 in the Inbox/);
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
    const tip = page.locator(".bk-tip[role=tooltip]");
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
          kpis: [...document.querySelectorAll(".hk-kpi")].map(
            (k) => /** @type {HTMLElement} */ (k).dataset.key,
          ),
          kpiText: [...document.querySelectorAll(".hk-kpi")].map(
            (k) => k.textContent ?? "",
          ),
          primary: [
            ...document.querySelectorAll(".hk-head__actions .kp-button"),
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

test("invariants: the Host page's Containers filter toggles with a plain click and / focuses its search", async () => {
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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

// ── redesign-config (3.71.0): Firewall, Settings (with Sign-in), Presets ──
// Kenny's approved demos (~/.local/share/homelab/redesign-3.71/
// firewall.html, settings.html, presets.html), implemented exactly; these
// pin what each page must always do.

test("invariants: redesign-config Firewall shows its totals, one attention row per unprotected stack, and lights stacks up with plain clicks", async () => {
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
    const named = page.locator("tr.fw-rule:has(.cf-chip)").first();
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
  const browser = await chromium.launch();
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
      (await page.locator(".cf-head__actions .cf-chip").innerText()).trim(),
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
    await page
      .locator(".cf-head__actions button", { hasText: "Discard" })
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
  const browser = await chromium.launch();
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
      for (const path of ["firewall", "settings", "presets"]) {
        await page.route("**/data/**", async (r) => {
          await new Promise((res) => setTimeout(res, 2500));
          await r.continue().catch(() => {});
        });
        await page.goto(`${BASE}/${path}`);
        await page.waitForTimeout(400);
        for (const [sel, n] of shapes[path]) {
          const got = await page.locator(`#page ${sel}`).count();
          if (got < n)
            bad.push(
              `${width}px /${path}: ${got} of ${n} ${sel} while loading`,
            );
        }
        await page.unroute("**/data/**");
        await page.goto(`${BASE}/${path}`);
        await page.waitForTimeout(1500);
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
  const browser = await chromium.launch();
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
    for (const path of ["firewall", "settings", "presets"]) {
      await page.goto(`${BASE}/${path}`);
      await page.waitForTimeout(2000);
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
              (e) => !e.closest("[data-kp-datatable], .nx-crumbs, .cf-tip"),
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
