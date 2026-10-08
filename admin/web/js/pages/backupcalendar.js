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
//
// fix-214 (Kenny, Dutch: "die kan toch net als een datepicker per maand de
// dingen tonen, met controls om door de maanden te scrollen?"): the fixed
// 35-day strip only ever grew wider with the window and never let Kenny
// look at an older month, and showed no detail beyond a day's tone. Now a
// real month grid (Monday first, never more than 6 weeks), prev/next/Today
// controls, keyboard navigation (arrows move the selected day, PageUp/
// PageDown change month), and clicking (or Enter on) a day opens a detail
// panel naming every stack's own state that night. The underlying read is
// unchanged: the host's `BackupCalendar` answer already carries each
// stack's WHOLE snapshot history (not a windowed one), so changing months
// needs no new fetch — only Refresh re-asks the host.
//
// fix-224 (Kenny, Dutch: "welke doet die dan niet? waarom kan ik dat niet
// zien?" — "Stacks read: 12 of 13" named nothing): the progress line now
// names the longest-pending stack and how long it has waited, and a chip
// grid below it (`perstack.js`'s `stackChips`, painted by `dom.js`'s
// `perstackChips`) shows EVERY stack's own state — read, still reading
// (with its own elapsed time), no backups by design, or failed (the host's
// own reason, which is also where a stack that outlived its own per-stack
// timeout lands) — each failed chip with its own Retry, so one hung
// repository never needs the whole page reloaded.

import {
  calendarInputs,
  calendarWeeks,
  dayDetail,
  earliestSnapshot,
  monthDays,
  shiftMonth,
  withStackResult,
} from "../backupcalendar.js";
import { h, perstackChips, slowRead } from "../dom.js";
import { pageHeader } from "../ui.js";
import { stackChips } from "../perstack.js";
import { agoText, formatDateTime } from "../format.js";
import { current, subscribe } from "../store.js";
import { setParams } from "../urlstate.js";

// calendar-words: the month grid's column heads, not a moment's format.
const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const MONTH_NAMES = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

/**
 * `ok`: read, with its snapshot times (possibly none yet, still a stack the
 * calendar should expect a night from). `empty`: read, but the host has
 * nothing to say about it (isn't a stack it knows) — not an error, just a
 * stack the calendar leaves out entirely. `no_backup` (fix-202): the host
 * named this stack explicitly as keeping no data at all. `failed`: the read
 * itself did not finish, or the host could name no manifest for it at all —
 * also where a stack that outlived `UNREAD_GIVE_UP_MS` lands.
 * @typedef {{status: "pending"} | {status: "ok", times: number[]} |
 *   {status: "empty"} | {status: "no_backup"} |
 *   {status: "failed", reason: string}} StackResult
 */

/**
 * The fleet's stack names, waiting once for the live store's first fleet
 * snapshot if the page mounted before it arrived. Any other store change
 * (or 5 s) also lets it go, so a host that never sends one fleet snapshot
 * still reaches "no stacks" rather than sitting before its first paint
 * forever.
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

/** @param {Date} d */
function localIso(d) {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

/**
 * fix-214: the month grid's shape — headers + every day of `year`/`month`,
 * padded to whole weeks — built from the clock alone (no stack data), so
 * the skeleton paint and the final paint of the SAME month share the exact
 * geometry: no layout shift when the data replaces the pulsing cells.
 * @param {number} year
 * @param {number} month 1-12
 * @param {string} today YYYY-MM-DD
 */
function gridShape(year, month, today) {
  return calendarWeeks(monthDays({}, [], year, month, today, null));
}

/**
 * @param {(import("../backupcalendar.js").CalendarDay | null)[][]} weeks
 * @param {(day: import("../backupcalendar.js").CalendarDay | null) => Node} cell
 */
function renderGrid(weeks, cell) {
  return h(
    "div",
    { class: "backup-cal__grid", role: "grid" },
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

/**
 * fix-214: the compact "11/13" count alongside the day number, and a
 * selected/today marker — months outside the data the host returned (the
 * future, or before the fleet's oldest snapshot ever) say so plainly
 * instead of drawing a false, empty-looking red cell.
 * @param {import("../backupcalendar.js").CalendarDay | null} d
 * @param {{selected: string | null, today: string, onPick: (date: string) => void}} opts
 */
function finishedCell(d, opts) {
  if (!d) return h("div", { class: "backup-cal__cell backup-cal__cell--pad" });
  const day = d.date.slice(8);
  const outOfRange = d.tone === "future" || d.tone === "before";
  const count = outOfRange ? "" : `${d.backed_up.length}/${d.expected.length}`;
  const label = outOfRange
    ? d.tone === "future"
      ? `${d.date}: in the future — nothing to read yet`
      : `${d.date}: before the fleet's oldest known backup — no data from this far back`
    : d.expected.length
      ? `${d.date}: ${d.backed_up.length}/${d.expected.length} stacks backed up${d.missing.length ? ` — missing: ${d.missing.join(", ")}` : ""}`
      : `${d.date}: no stack keeps data`;
  const cell = h(
    "button",
    {
      type: "button",
      class: `backup-cal__cell backup-cal__cell--${d.tone}${d.date === opts.selected ? " backup-cal__cell--selected" : ""}${d.date === opts.today ? " backup-cal__cell--today" : ""}`,
      title: label,
      "aria-label": label,
      "aria-selected": String(d.date === opts.selected),
      "data-date": d.date,
      role: "gridcell",
      tabindex: d.date === opts.selected ? "0" : "-1",
    },
    h("span", { class: "backup-cal__cell-day" }, day),
    ...(count ? [h("span", { class: "backup-cal__cell-count" }, count)] : []),
  );
  cell.addEventListener("click", () => opts.onPick(d.date));
  return cell;
}

/** fix-180: how long a stack may stay "not read yet" before it is named. */
// fix-224: overridable only from a test (`globalThis.__HOMELAB_TEST_...__`),
// never read from anything a server or a URL controls — production always
// gets the real 180 s. Lets the e2e invariant for the per-stack timeout run
// in seconds instead of needing to sit through 3 real minutes.
const UNREAD_GIVE_UP_MS =
  Number(/** @type {any} */ (globalThis).__HOMELAB_TEST_UNREAD_GIVE_UP_MS__) ||
  180_000;

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
  const today = localIso(new Date());
  const params = new URLSearchParams(location.search);
  const wantedY = Number(params.get("y"));
  const wantedM = Number(params.get("m"));
  /** @type {{year: number, month: number}} */
  let ym =
    wantedY && wantedM >= 1 && wantedM <= 12
      ? { year: wantedY, month: wantedM }
      : { year: Number(today.slice(0, 4)), month: Number(today.slice(5, 7)) };
  /** @type {string | null} */
  let selected = params.get("day") || null;

  const heading = h(
    "h2",
    { id: "backup-cal-heading" },
    `${MONTH_NAMES[ym.month - 1]} ${ym.year}`,
  );
  const prevBtn = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--sm",
      "aria-label": "Previous month",
    },
    "‹ Prev",
  );
  const nextBtn = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--sm",
      "aria-label": "Next month",
    },
    "Next ›",
  );
  const todayBtn = h(
    "button",
    { type: "button", class: "kp-button kp-button--sm" },
    "Today",
  );
  const chipsWrap = h("div", { class: "backup-cal__chips-wrap" });
  // fix-202/fix-214: the skeleton grid is laid out at once, synchronously,
  // in the SAME markup `root.replaceChildren` paints — never left for the
  // async load to fill in later, which is exactly what used to show a
  // blank area while `fleetStackNames()`'s own await was still pending.
  const grid = h(
    "div",
    {
      class: "backup-cal",
      "aria-label": "Backup calendar",
      role: "grid",
      "aria-labelledby": "backup-cal-heading",
    },
    renderGrid(gridShape(ym.year, ym.month, today), skeletonCell),
  );
  const detail = h("div", {
    class: "backup-cal__detail",
    "aria-live": "polite",
  });
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
    pageHeader({
      title: "Backup calendar",
      desc: "One month at a time: every stack that keeps data is expected to have at least one restic snapshot each night. Reads restic directly over the network, one stack at a time, so a slow repository only ever holds up its own chip — never the whole page.",
      primary: refresh,
    }).el,
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
      "no stack keeps data  ",
      h("span", { class: "backup-cal__cell backup-cal__cell--before" }, ""),
      "before any backup history  ",
      h("span", { class: "backup-cal__cell backup-cal__cell--future" }, ""),
      "in the future",
    ),
    chipsWrap,
    h(
      "div",
      { class: "backup-cal__nav", role: "group", "aria-label": "Month" },
      prevBtn,
      heading,
      nextBtn,
      todayBtn,
    ),
    h("div", { class: "backup-cal__layout" }, grid, detail),
    status,
  );

  const abort = new AbortController();
  /** @type {Record<string, StackResult>} */
  let results = {};
  /** @type {Record<string, number>} */
  const startedAt = {};
  /** @type {Record<string, number | null>} */
  const measuredAt = {};
  /** @type {string[]} */
  let names = [];
  /** @type {number | undefined} */
  let tickTimer;

  const writeUrl = () => {
    const search = setParams(location.search, {
      y: String(ym.year),
      m: String(ym.month),
      day: selected ?? null,
    });
    if (search !== location.search)
      history.replaceState(history.state, "", location.pathname + search);
  };

  const paintDetail = () => {
    if (!selected) {
      detail.replaceChildren(
        h(
          "p",
          { class: "backup-cal__detail-empty measured" },
          "Pick a day to see every stack's own state that night.",
        ),
      );
      return;
    }
    const rows = dayDetail(results, selected);
    const words = {
      backed_up: "backed up",
      missing: "no backup that night",
      no_backup: "no backups by design",
      not_read: "not read",
    };
    detail.replaceChildren(
      h("h3", null, selected),
      h(
        "ul",
        { class: "backup-cal__detail-list" },
        ...rows.map((r) =>
          h(
            "li",
            {
              class: `backup-cal__detail-row backup-cal__detail-row--${r.state}`,
            },
            h("span", { class: "backup-cal__detail-stack" }, r.stack),
            h(
              "span",
              { class: "backup-cal__detail-state" },
              r.times.length
                ? r.times.map((t) => formatDateTime(t)).join(", ")
                : words[r.state],
            ),
          ),
        ),
      ),
    );
  };

  const paintGrid = () => {
    heading.textContent = `${MONTH_NAMES[ym.month - 1]} ${ym.year}`;
    const shape = gridShape(ym.year, ym.month, today);
    const { stacks, expected } = calendarInputs(results);
    const anyOk = Object.values(results).some((r) => r.status === "ok");
    if (!anyOk) {
      grid.replaceChildren(renderGrid(shape, skeletonCell));
      return;
    }
    const earliest = earliestSnapshot(stacks);
    const days = monthDays(
      stacks,
      expected,
      ym.year,
      ym.month,
      today,
      earliest,
    );
    grid.replaceChildren(
      renderGrid(calendarWeeks(days), (d) =>
        finishedCell(d, { selected, today, onPick: pickDay }),
      ),
    );
  };

  /** @param {string} date */
  function pickDay(date) {
    selected = date;
    writeUrl();
    paintGrid();
    paintDetail();
  }

  /** @param {number} delta */
  const moveMonth = (delta) => {
    ym = shiftMonth(ym, delta);
    writeUrl();
    paintGrid();
  };
  prevBtn.addEventListener("click", () => moveMonth(-1));
  nextBtn.addEventListener("click", () => moveMonth(1));
  todayBtn.addEventListener("click", () => {
    ym = { year: Number(today.slice(0, 4)), month: Number(today.slice(5, 7)) };
    selected = today;
    writeUrl();
    paintGrid();
    paintDetail();
  });

  /** fix-214: arrows move the selected day, PageUp/PageDown change month. */
  grid.addEventListener("keydown", (e) => {
    const deltas = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -7, ArrowDown: 7 };
    if (e.key in deltas) {
      e.preventDefault();
      const base = selected ?? today;
      const d = new Date(`${base}T00:00:00`);
      d.setDate(
        d.getDate() + deltas[/** @type {keyof typeof deltas} */ (e.key)],
      );
      const iso = localIso(d);
      ym = { year: d.getFullYear(), month: d.getMonth() + 1 };
      pickDay(iso);
      /** @type {HTMLElement | null} */ (
        root.querySelector(`.backup-cal__cell[data-date="${iso}"]`)
      )?.focus();
      return;
    }
    if (e.key === "PageUp" || e.key === "PageDown") {
      e.preventDefault();
      moveMonth(e.key === "PageUp" ? -1 : 1);
      return;
    }
    if (e.key === "Enter" && selected) {
      e.preventDefault();
      paintDetail();
    }
  });

  /**
   * fix-224: the plain-words line names the longest-pending stack, not
   * only a fraction — "Stacks read: 12 of 13 — waiting for inbox (37s)".
   */
  const paintStatus = () => {
    const chips = stackChips(results, startedAt, Date.now());
    const loaded = chips.filter((c) => c.state !== "reading").length;
    const reading = chips
      .filter((c) => c.state === "reading")
      .sort((a, b) => (b.seconds ?? 0) - (a.seconds ?? 0));
    const waiting = reading.length
      ? ` — waiting for ${reading[0].stack}${reading.length > 1 ? ` (+${reading.length - 1} more)` : ""} (${reading[0].seconds}s)`
      : "";
    const readings = Object.values(measuredAt).filter(
      (t) => typeof t === "number",
    );
    const oldest = readings.length ? Math.min(...readings) : null;
    const ago =
      !waiting && oldest != null
        ? ` · ${agoText("read", oldest, Math.floor(Date.now() / 1000))}`
        : "";
    status.textContent = names.length
      ? `Stacks read: ${loaded} of ${names.length}${waiting}${ago}`
      : "";
    perstackChipsRepaint(chips);
  };

  /** @param {ReturnType<typeof stackChips>} chips */
  const perstackChipsRepaint = (chips) => {
    chipsWrap.replaceChildren(
      perstackChips(chips, { onRetry: (name) => void retryOne(name) }),
    );
  };

  const ensureTicking = () => {
    const anyPending = Object.values(results).some(
      (r) => r.status === "pending",
    );
    if (anyPending && tickTimer == null) {
      tickTimer = window.setInterval(paintStatus, 2000);
    } else if (!anyPending && tickTimer != null) {
      window.clearInterval(tickTimer);
      tickTimer = undefined;
    }
  };

  const repaint = () => {
    paintStatus();
    paintGrid();
    paintDetail();
    ensureTicking();
  };

  /**
   * One stack's read, with fix-180's "not read yet" retry loop and
   * fix-202's terminal-name shortcut. Shared by the initial fleet-wide load
   * and a single chip's own Retry (fix-224), so a hung repository never
   * needs the whole page reloaded.
   * @param {string} name
   * @param {boolean} force
   * @returns {Promise<StackResult>}
   */
  const readStack = async (name, force) => {
    startedAt[name] = Date.now();
    const started = startedAt[name];
    try {
      for (let attempt = 0; ; attempt++) {
        const qs = force && attempt === 0 ? "&refresh=1" : "";
        const r = await slowRead(
          `/data/backup-calendar?stack=${encodeURIComponent(name)}${qs}`,
          `${name}'s backup calendar`,
          abort.signal,
        );
        if (!r.ok) return { status: "failed", reason: r.error.why };
        if (r.body.no_backup?.includes(name)) {
          measuredAt[name] =
            r.body.measured_at?.[name] ?? Math.floor(Date.now() / 1000);
          return { status: "no_backup" };
        }
        const terminalReason = r.body.reasons?.[name];
        if (terminalReason) return { status: "failed", reason: terminalReason };
        const read = r.body.measured_at?.[name] ?? null;
        if (read === null && Date.now() - started < UNREAD_GIVE_UP_MS) {
          await sleep(unreadRetryMs(attempt), abort.signal);
          if (abort.signal.aborted) return { status: "pending" };
          continue;
        }
        if (read === null) {
          return {
            status: "failed",
            reason: `did not answer within ${Math.round(UNREAD_GIVE_UP_MS / 1000)}s`,
          };
        }
        measuredAt[name] = read;
        const times = r.body.stacks?.[name];
        return times ? { status: "ok", times } : { status: "empty" };
      }
    } catch (e) {
      if (abort.signal.aborted) return { status: "pending" };
      return { status: "failed", reason: String(e) };
    }
  };

  /**
   * fix-224: one chip's own Retry — re-reads only that stack.
   * @param {string} name
   */
  async function retryOne(name) {
    if (abort.signal.aborted) return;
    results = withStackResult(results, name, { status: "pending" });
    repaint();
    const outcome = await readStack(name, true);
    if (abort.signal.aborted) return;
    results = withStackResult(results, name, outcome);
    repaint();
  }

  /**
   * @param {boolean} force true on an explicit Refresh click: asks the host
   *   to read restic again in the background; this call still answers at
   *   once from whatever is cached.
   */
  const load = async (force = false) => {
    status.textContent = "Reading the fleet…";
    const fetched = await fleetStackNames();
    if (abort.signal.aborted) return;
    names = fetched;

    if (names.length === 0) {
      status.textContent = "";
      results = {};
      paintGrid();
      return;
    }

    results = Object.fromEntries(names.map((n) => [n, { status: "pending" }]));
    repaint();

    await Promise.allSettled(
      names.map(async (name) => {
        const outcome = await readStack(name, force);
        if (abort.signal.aborted) return;
        results = withStackResult(results, name, outcome);
        repaint();
      }),
    );
    if (abort.signal.aborted) return;
    repaint();
  };

  refresh.addEventListener("click", () => void load(true).catch(() => {}));
  void load().catch(() => {});
  return () => {
    abort.abort();
    if (tickTimer != null) window.clearInterval(tickTimer);
  };
}
