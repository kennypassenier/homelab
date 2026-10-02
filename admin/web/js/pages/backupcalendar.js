// feat-overview-10: the backup calendar page. Its own module (`../js/backupcalendar.js`
// for the pure grid math) and its own route (`/data/backup-calendar`, a
// dedicated `BackupCalendar` host command) — kept apart from the Backups
// page another helper is building in the same milestone, so the two merge
// without either touching the other's files.
//
// fix-177: the host answered every stack in one call, sequentially, each
// restic read capped at 120 s — on a real fleet that could, and on CT 120
// measured 2026-10-02 did, run past the dashboard's own ask budget and
// answer one bare 502 for the whole page, with nothing to say which stack
// it was stuck on. The page now already knows the fleet's stack names
// (`store.js`'s `current().fleet.stacks`, same as the Backups page), asks
// for each one's calendar on its own (`?stack=`), and fills the grid in
// once every one of them has answered — ok, empty, or failed, each named.
// One slow or hung repository (rclone to a remote gdrive, say) now times
// out alone; the rest of the fleet's answers still show.
//
// Kenny's call on the loading shape (2026-10-02): the real grid is laid
// out at once with every cell pulsing, same size as the finished one — no
// spinner, nothing that changes shape when the data lands.
//
// fix-202 (Kenny, 2026-10-02, live on the real fleet): the grid used to stay
// a full skeleton until EVERY stack had answered, even once most of them
// already had — measured sitting at "11 of 13" for minutes with nothing but
// pulsing cells to show for it. It now repaints from whatever subset of
// stacks has answered "ok" on every settle, not only at the end; a day's
// ratio can shift as the rest trickle in (the progress bar says how many
// are left), which beats a frozen skeleton. A stack the host already named
// as terminal — keeps no data by design, or could never be read at all
// (no manifest on record, not a known stack) — is named at once instead of
// being retried for minutes and then blamed on "not read yet".

import {
  calendarDays,
  calendarInputs,
  calendarNoBackup,
  calendarProgress,
  calendarWeeks,
  withStackResult,
} from "../backupcalendar.js";
import { h, progressGroup, slowRead } from "../dom.js";
import { agoText } from "../format.js";
import { current, subscribe } from "../store.js";

const WINDOW_DAYS = 35;
const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/**
 * The fleet's stack names, waiting once for the live store's first fleet
 * snapshot if the page mounted before it arrived (a direct link to this
 * page, opened before `store.start()`'s own `/data/fleet` read lands). Any
 * other store change (or 5 s) also lets it go, so a host that never sends
 * one fleet snapshot (the link is down) still reaches "no stacks" rather
 * than sitting before its first paint forever.
 * @returns {Promise<string[]>}
 */
async function fleetStackNames() {
  if (!current().fleet) {
    await new Promise((resolve) => {
      const stop = () => {
        off();
        clearTimeout(t);
        resolve(undefined);
      };
      const off = subscribe(stop);
      const t = setTimeout(stop, 5000);
    });
  }
  return (current().fleet?.stacks ?? []).map((s) => s.name).sort();
}

/**
 * The real grid's shape (headers + `WINDOW_DAYS` cells, padded to whole
 * weeks) built once from the clock alone, so the skeleton paint and the
 * final paint share the exact same geometry — no layout shift when the
 * data replaces the pulsing placeholders.
 * @param {number} now unix seconds
 */
function gridShape(now) {
  return calendarWeeks(calendarDays({}, [], WINDOW_DAYS, now));
}

/**
 * @param {(import("../backupcalendar.js").CalendarDay | null)[][]} weeks
 * @param {(day: import("../backupcalendar.js").CalendarDay | null) => Node} cell
 */
function renderGrid(weeks, cell) {
  return h(
    "div",
    { class: "backup-cal__grid" },
    ...WEEKDAYS.map((w) => h("div", { class: "backup-cal__head" }, w)),
    ...weeks.flatMap((week) => week.map(cell)),
  );
}

/** @param {import("../backupcalendar.js").CalendarDay | null} d */
function skeletonCell(d) {
  return d
    ? h("div", {
        class: "backup-cal__cell backup-cal__cell--skeleton",
        "aria-hidden": "true",
      })
    : h("div", { class: "backup-cal__cell backup-cal__cell--pad" });
}

/** @param {import("../backupcalendar.js").CalendarDay | null} d */
function finishedCell(d) {
  if (!d) return h("div", { class: "backup-cal__cell backup-cal__cell--pad" });
  const label = d.expected.length
    ? `${d.date}: ${d.backed_up.length}/${d.expected.length} stacks backed up${d.missing.length ? ` — missing: ${d.missing.join(", ")}` : ""}`
    : `${d.date}: no stack keeps data`;
  return h(
    "div",
    {
      class: `backup-cal__cell backup-cal__cell--${d.tone}`,
      title: label,
      "aria-label": label,
    },
    d.date.slice(8),
  );
}

/**
 * @param {HTMLElement} wrap
 * @param {ReturnType<typeof calendarProgress>} progress
 * @param {number} stackCount
 */
function paintProgress(wrap, progress, stackCount) {
  if (!stackCount) {
    wrap.replaceChildren();
    return;
  }
  wrap.replaceChildren(
    progressGroup([
      {
        label: "Stacks read",
        pct: progress.pct,
        value: `${progress.loaded} of ${progress.total}`,
      },
    ]),
  );
}

/**
 * @param {HTMLElement} ul
 * @param {{stack: string, reason: string}[]} failed
 */
function paintFailures(ul, failed) {
  ul.replaceChildren(
    ...failed.map((f) =>
      h(
        "li",
        { class: "backup-cal__error" },
        h("strong", null, f.stack),
        `: not read — ${f.reason}`,
      ),
    ),
  );
}

/**
 * fix-202: stacks the host has already named as keeping no data at all —
 * its own line, named at once, never left looking like a stuck read.
 * @param {HTMLElement} ul
 * @param {string[]} names
 */
function paintNoBackup(ul, names) {
  ul.replaceChildren(
    ...names.map((name) =>
      h(
        "li",
        { class: "backup-cal__excluded" },
        h("strong", null, name),
        ": no backups by design — every mount is excluded",
      ),
    ),
  );
}

/** fix-180: how long a stack may stay "not read yet" before it is named. */
const UNREAD_GIVE_UP_MS = 180_000;

/**
 * Backoff between asks for a stack the host has not read yet: 2 s, 4 s,
 * 8 s, then every 10 s.
 * @param {number} attempt 0-based
 */
export function unreadRetryMs(attempt) {
  return Math.min(10_000, 2_000 * 2 ** attempt);
}

/**
 * @param {number} ms
 * @param {AbortSignal} signal
 * @returns {Promise<void>}
 */
function sleep(ms, signal) {
  return new Promise((resolve) => {
    const t = setTimeout(resolve, ms);
    signal.addEventListener(
      "abort",
      () => {
        clearTimeout(t);
        resolve();
      },
      { once: true },
    );
  });
}

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */

export function mount(root) {
  const now = Math.floor(Date.now() / 1000);
  const shape = gridShape(now);

  const progressWrap = h("div", { class: "backup-cal__progress" });
  const failures = h("ul", {
    class: "backup-cal__errors",
    "aria-live": "polite",
  });
  const noBackup = h("ul", {
    class: "backup-cal__excluded-list",
    "aria-live": "polite",
  });
  const grid = h(
    "div",
    { class: "backup-cal", "aria-label": "Backup calendar" },
    renderGrid(shape, skeletonCell),
  );
  const status = h(
    "p",
    { class: "measured", role: "status" },
    "Reading the fleet…",
  );
  const refresh = h(
    "button",
    { type: "button", class: "kp-button kp-button--primary" },
    "Refresh",
  );
  root.replaceChildren(
    h("div", { class: "title-row" }, h("h1", null, "Backup calendar"), refresh),
    h(
      "p",
      { class: "page-intro" },
      `The last ${WINDOW_DAYS} nights, one cell per day: every stack that keeps data is expected to have at least one restic snapshot that night. Reads restic directly over the network, one stack at a time, so a slow repository only ever holds up its own cell.`,
    ),
    h(
      "div",
      { class: "backup-cal__legend" },
      h("span", { class: "backup-cal__cell backup-cal__cell--ok" }, ""),
      "every stack  ",
      h("span", { class: "backup-cal__cell backup-cal__cell--warn" }, ""),
      "some stacks  ",
      h("span", { class: "backup-cal__cell backup-cal__cell--bad" }, ""),
      "no stack  ",
      h("span", { class: "backup-cal__cell backup-cal__cell--muted" }, ""),
      "no stack keeps data",
    ),
    progressWrap,
    failures,
    noBackup,
    grid,
    status,
  );
  const abort = new AbortController();

  /**
   * fix-180: the host now answers from its own snapshot cache rather than
   * reading restic on the request path, so a stack's own reply carries
   * `measured_at` — when restic was actually last read for it, which can be
   * minutes old, not "now". The oldest of the shown stacks' readings is
   * what the status line reports, so "measured just now" is never claimed
   * for data that is, in truth, however old the cache says it is.
   * @param {boolean} force true on an explicit Refresh click: asks the host
   *   to read restic again in the background; this call still answers at
   *   once from whatever is cached.
   */
  const load = async (force = false) => {
    grid.replaceChildren(renderGrid(shape, skeletonCell));
    progressWrap.replaceChildren();
    failures.replaceChildren();
    noBackup.replaceChildren();
    status.textContent = "Reading the fleet…";

    const names = await fleetStackNames();
    if (abort.signal.aborted) return;

    if (names.length === 0) {
      status.textContent = "";
      grid.replaceChildren(renderGrid(shape, finishedCell));
      return;
    }

    /** @type {Record<string, import("../backupcalendar.js").StackResult>} */
    let results = Object.fromEntries(
      names.map((n) => [n, { status: "pending" }]),
    );
    /** @type {Record<string, number | null>} */
    const measuredAt = {};
    // fix-202: Kenny measured the page sitting on a full skeleton grid for
    // minutes while the status line already said "11 of 13" — every stack
    // that HAD answered was thrown away until every last one had. The grid
    // now repaints from whatever subset of stacks has already answered
    // "ok", on every settle, not only once the whole fleet is in; a day's
    // ratio can still shift as more stacks arrive (the progress bar says
    // how many are left), but that beats a frozen skeleton. Cells stay
    // skeletons only until the very first stack answers "ok" — before that,
    // an empty `expected` would paint every day "no stack keeps data",
    // which is simply untrue while the reads are still in flight.
    const repaint = () => {
      const progress = calendarProgress(results);
      paintProgress(progressWrap, progress, names.length);
      paintFailures(failures, progress.failed);
      paintNoBackup(noBackup, calendarNoBackup(results));
      if (Object.values(results).some((r) => r.status === "ok")) {
        const { stacks, expected } = calendarInputs(results);
        const days = calendarDays(stacks, expected, WINDOW_DAYS, now);
        grid.replaceChildren(renderGrid(calendarWeeks(days), finishedCell));
      }
    };
    status.textContent = `Reading each stack's restic snapshots from the host — 0 of ${names.length} so far…`;
    repaint();

    await Promise.allSettled(
      names.map(async (name) => {
        /** @type {import("../backupcalendar.js").StackResult} */
        let outcome;
        try {
          // fix-180: a stack the host's snapshot cache has not read yet
          // answers at once with no `measured_at`; that is "not read yet",
          // never "no backups", so it stays pending and is asked again
          // (the host is reading it in the background) until it has been
          // read or UNREAD_GIVE_UP_MS has passed.
          //
          // fix-202: that retry loop must never run for a name the host has
          // already named as terminal — `no_backup` (keeps no data, by
          // design: `StackManifest::backs_up_nothing`) or `reasons` (no
          // manifest on record yet, or not a known stack at all). Before
          // this fix those names got no `measured_at` entry either, so they
          // were indistinguishable from "cache still warming up" and sat
          // retrying for the full 3 minutes before a generic "press
          // Refresh" — the live "stuck at 11 of 13" symptom (Kenny,
          // 2026-10-02).
          const started = Date.now();
          for (let attempt = 0; ; attempt++) {
            const qs = force && attempt === 0 ? "&refresh=1" : "";
            const r = await slowRead(
              `/data/backup-calendar?stack=${encodeURIComponent(name)}${qs}`,
              `${name}'s backup calendar`,
              abort.signal,
            );
            if (!r.ok) {
              outcome = { status: "failed", reason: r.error.why };
              break;
            }
            if (r.body.no_backup?.includes(name)) {
              measuredAt[name] =
                r.body.measured_at?.[name] ?? Math.floor(Date.now() / 1000);
              outcome = { status: "no_backup" };
              break;
            }
            const terminalReason = r.body.reasons?.[name];
            if (terminalReason) {
              outcome = { status: "failed", reason: terminalReason };
              break;
            }
            const read = r.body.measured_at?.[name] ?? null;
            if (read === null && Date.now() - started < UNREAD_GIVE_UP_MS) {
              await sleep(unreadRetryMs(attempt), abort.signal);
              if (abort.signal.aborted) return;
              continue;
            }
            if (read === null) {
              outcome = {
                status: "failed",
                reason:
                  "the host has not read this stack's restic repository yet; press Refresh in a minute",
              };
              break;
            }
            measuredAt[name] = read;
            const times = r.body.stacks?.[name];
            outcome = times ? { status: "ok", times } : { status: "empty" };
            break;
          }
        } catch (e) {
          if (abort.signal.aborted) return;
          outcome = { status: "failed", reason: String(e) };
        }
        if (abort.signal.aborted) return;
        results = withStackResult(results, name, outcome);
        const progress = calendarProgress(results);
        repaint();
        status.textContent = `Reading each stack's restic snapshots from the host — ${progress.loaded} of ${progress.total} so far…`;
      }),
    );
    if (abort.signal.aborted) return;

    const { stacks, expected } = calendarInputs(results);
    const days = calendarDays(stacks, expected, WINDOW_DAYS, now);
    grid.replaceChildren(renderGrid(calendarWeeks(days), finishedCell));
    const progress = calendarProgress(results);
    const readings = Object.values(measuredAt).filter(
      (t) => typeof t === "number",
    );
    const oldest = readings.length ? Math.min(...readings) : null;
    const nowS = Math.floor(Date.now() / 1000);
    status.textContent = `${expected.length} of ${names.length} stacks · ${agoText("read", oldest, nowS)}${progress.failed.length ? ` · ${progress.failed.length} not read` : ""}`;
  };

  refresh.addEventListener("click", () => void load(true).catch(() => {}));
  void load().catch(() => {});
  return () => abort.abort();
}
