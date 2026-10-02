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
    await page.goto(`${BASE}/backupcalendar`);
    const excluded = page.locator(".backup-cal__excluded");
    await excluded.first().waitFor({ timeout: 5000 });
    const text = await excluded.first().textContent();
    assert.ok(
      text && /oldstack/.test(text) && /no backups by design/.test(text),
      `oldstack was not named as excluded: ${text}`,
    );
    // The progress bar must reach "3 of 3" promptly — not stuck waiting on
    // oldstack the way a false "not read yet" would stall it.
    await page.waitForFunction(
      () => {
        const p = document.querySelector(".backup-cal__progress");
        return p && /3 of 3/.test(p.textContent ?? "");
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
