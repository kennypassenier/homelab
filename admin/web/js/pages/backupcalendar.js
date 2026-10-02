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
// spinner, nothing that changes shape when the data lands. A day's tone is
// only ever painted once every stack has answered (a day's ratio needs all
// of them), so the skeleton cells repaint in one pass, in place.

import {
  calendarDays,
  calendarInputs,
  calendarProgress,
  calendarWeeks,
  withStackResult,
} from "../backupcalendar.js";
import { h, progressGroup, slowRead } from "../dom.js";
import { formatDateTime } from "../format.js";
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
    grid,
    status,
  );
  const abort = new AbortController();

  const load = async () => {
    grid.replaceChildren(renderGrid(shape, skeletonCell));
    progressWrap.replaceChildren();
    failures.replaceChildren();
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
    const repaint = () => {
      const progress = calendarProgress(results);
      paintProgress(progressWrap, progress, names.length);
      paintFailures(failures, progress.failed);
    };
    status.textContent = `Reading each stack's restic snapshots from the host — 0 of ${names.length} so far…`;
    repaint();

    await Promise.allSettled(
      names.map(async (name) => {
        /** @type {import("../backupcalendar.js").StackResult} */
        let outcome;
        try {
          const r = await slowRead(
            `/data/backup-calendar?stack=${encodeURIComponent(name)}`,
            `${name}'s backup calendar`,
            abort.signal,
          );
          if (!r.ok) outcome = { status: "failed", reason: r.error.why };
          else {
            const times = r.body.stacks?.[name];
            outcome = times ? { status: "ok", times } : { status: "empty" };
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
    status.textContent = `${expected.length} of ${names.length} stacks · measured ${formatDateTime(Math.floor(Date.now() / 1000))}${progress.failed.length ? ` · ${progress.failed.length} not read` : ""}`;
  };

  refresh.addEventListener("click", () => void load().catch(() => {}));
  void load().catch(() => {});
  return () => abort.abort();
}
