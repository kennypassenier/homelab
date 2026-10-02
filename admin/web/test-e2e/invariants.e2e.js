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
import { DRIVABLE_PATHS, route } from "../js/router.js";

const BASE = process.env.INVARIANTS_BASE_URL ?? "http://127.0.0.1:8099";
const TOKEN = process.env.INVARIANTS_TOKEN ?? "test-invariants-token-1234";

/** Logs in on a fresh page and leaves it on /overview. */
async function freshPage(context) {
  const page = await context.newPage();
  await page.goto(`${BASE}/login`);
  await page.fill("#token", TOKEN);
  await Promise.all([
    page.waitForURL("**/overview", { timeout: 5000 }).catch(() => {}),
    page.click("button[type=submit]"),
  ]);
  await page.goto(`${BASE}/overview`);
  return page;
}

test("invariants: the nav bar stays inline, with brand Homelab and the version beside Go to…, at 1600/1920/2560 CSS px and at 1894 in the widest themes", async () => {
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
    // H8 (per-stack enabled flag): "Disable" needs only the stack name, so
    // it runs end to end against the demo host without a working copy of
    // the repository (deploy/backup both refuse without one there).
    await page
      .getByRole("button", { name: "Disable", exact: true })
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
    await page.goto(`${BASE}/notifications`);
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
    // the app sends it on to Health — never the old page.
    const old = await page.goto(`${BASE}/status`);
    if (old && old.status() !== 404) {
      await page.waitForURL("**/health", { timeout: 5000 });
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
    await page.goto(`${BASE}/login`);
    await page.fill("#token", TOKEN);
    await Promise.all([
      page.waitForURL("**/overview", { timeout: 5000 }).catch(() => {}),
      page.click("button[type=submit]"),
    ]);
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
    const link = page.locator("a", { hasText: "Fleet view topology" });
    assert.equal(await link.count(), 1, "no link to the Fleet view topology");
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
    const skip = new Set(["apply", "shell", "log", "passkeys"]);
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
    await page.goto(`${BASE}/firewall`);
    await page.waitForSelector("table", { timeout: 10000 });
    const bodyText = await page.locator("body").innerText();
    assert.ok(
      bodyText.includes("in force (repo differs)"),
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

test("invariants: /apply lands on Overview with its section open", async () => {
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext();
    const page = await freshPage(context);
    await page.goto(`${BASE}/apply`);
    await page.waitForURL("**/overview?section=apply", { timeout: 5000 });
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
    // own page (pre-existing gap, noted in the fix-206 test).
    const skip = new Set(["apply", "shell", "log", "passkeys"]);
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
    await page.waitForSelector("#host-disk-facts", { timeout: 5000 });
    await page.waitForTimeout(300);
    const diskFactsText = await page.evaluate(
      () => document.querySelector("#host-disk-facts")?.textContent ?? "",
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
      () =>
        document.querySelector('[data-kp-remember="host-top-dirs"]')
          ?.textContent ?? "",
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
    const skip = new Set(["apply", "shell", "log", "passkeys"]);
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
