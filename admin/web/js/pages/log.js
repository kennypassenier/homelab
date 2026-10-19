// redesign-console (3.71.0, the Console demo Kenny approved on 2026-10-03):
// the host-log explorer. Every line the host prints for every operation,
// whoever started it (this dashboard, a CLI or TUI on a workstation, the
// nightly round), live. Console shows it with the shell under it; Activity
// shows it as its Host log view (`/activity?view=host-log`; the old `/log`
// redirects there).
//
// A log explorer, so the filters are a side column (DESIGN_LANGUAGE §12):
// Source (each a plain-click toggle with its count, "only" beside it) and
// Level. The toolbar above the lines keeps its three zones (fix for
// Kenny's "alle items lijken gewoon links aligned naast elkaar gezet"):
// "Lines containing" at its own width on the left, the count, the follow
// switch and the key hints against the right edge. The lines scroll inside
// their own region; the page never grows with them.

import { act } from "../act.js";
import { ownerOf } from "../activityview.js";
import { fetchJson, h, progressBar } from "../dom.js";
import { declare, drivable } from "../drivable.js";
import {
  LEVELS,
  defaultLevels,
  BUCKET_S,
  bucketStart,
  counts,
  countText,
  densityBuckets,
  inRange,
  keyAction,
  lineShown,
  logFilterFromParams,
  logFilterToParams,
  mergeLines,
  narrowed,
  sourceLabel,
  sourceList,
  sourceOn,
  toggleSource,
} from "../hostlogview.js";
import { logLine } from "../jobs.js";
import { logText, saveLogLevels } from "../logs.js";
import { openJobDialog } from "../jobpanel.js";
import { transferView } from "../parity.js";
import { current, listen, subscribe } from "../store.js";
import { countOf, emptyState, kbd, stackMark, toolbar } from "../ui.js";
import { setParams } from "../urlstate.js";
import { attachLogs } from "/static/kp/js/log.js";
import { gridRow, formGrid } from "../gridrow.js";
import { formatClock, formatDateTime } from "../format.js";
import {
  TOGGLE_CHANGED_EVENT,
  attachToggles,
  toggle as toggleOf,
} from "/static/kp/js/toggle.js";
import {
  SEGMENTED_EVENT,
  attachSegmented,
  setSegmentedValues,
} from "/static/kp/js/segmented.js";
import {
  UPTIME_PICK_EVENT,
  attachUptimes,
  setUptime,
} from "/static/kp/js/uptime.js";
import {
  HEAT_PICK_EVENT,
  HEAT_ROW_EVENT,
  attachHeats,
  setHeat,
  setHeatPick,
  setHeatRows,
} from "/static/kp/js/heat.js";
import { attachHoverCards } from "/static/kp/js/hovercard.js";

/** Lines kept in the page, as the server's ring. */
const KEEP = 2000;

// feat-logs-1: Copy for the whole host log, declared here because Activity's
// Host log and Console both draw this explorer.
const COPY_ALL = declare({
  id: "host-log-copy-all",
  page: "log",
  opens: "run",
  what: "copy every line of the host log to the clipboard",
});

/**
 * @typedef {{follow: string, source: string, only: string, level: string,
 *   reset: string, line: string, openJob: string, copy: string,
 *   everything: string, onlyLine: string, newLines: string,
 *   search: string, retry: string, download?: string}} DriveIds the Live
 *   view controls the host page
 *   declared for this explorer (drivable.js)
 * @typedef {{drive: DriveIds, followInBar: boolean, foot?: HTMLElement,
 *   onFollow?: (on: boolean) => void, label: string}} ExplorerOpts
 */

/**
 * @param {HTMLElement} root
 * @param {ExplorerOpts} opts
 * @returns {{cleanup: () => void, setFollow: (on: boolean) => void,
 *   following: () => boolean, download: () => void,
 *   addBlock: (el: HTMLElement) => void, focusSearch: () => void}}
 */
export function mountHostLog(root, opts) {
  const params = new URLSearchParams(location.search);
  let f = logFilterFromParams(params);
  /** @type {import("../parity.js").HostLine[]} */
  let lines = [];
  /** @type {Set<string>} */
  const seen = new Set();
  let lastSeq = 0;
  /** Live lines that arrived while the snapshot was being read. */
  /** @type {import("../parity.js").HostLine[]} */
  let early = [];
  let loaded = false;
  /** @type {string | null} */
  let failure = null;
  let follow = true;
  let unseen = 0;
  /** @type {number | null} */
  let open = null;
  /** The five minutes (start, unix seconds) the lines are narrowed to. @type {number | null} */
  let range = null;
  /** The picture over the lines: sources by time (opens first) or the strip. @type {"grid" | "strip"} */
  let picture = "grid";

  const side = h("aside", {
    class: "hl-side",
    "aria-label": "Which lines",
  });
  const countEl = h("span", { class: "hl-count", role: "status" });
  const followBtn = drivable(
    h("button", {
      type: "button",
      class: "kp-button kp-button--ghost kp-button--sm hl-follow",
      title: "Stop following: new lines wait below a marker (Space)",
    }),
    opts.drive.follow,
  );
  followBtn.addEventListener("click", () => setFollow(!follow));
  const copyAll = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--ghost kp-button--sm hl-copyall",
        title: "Copy every line the page holds to the clipboard",
      },
      "Copy",
    ),
    COPY_ALL,
  );
  copyAll.addEventListener("click", () => {
    void navigator.clipboard
      ?.writeText(logText(lines))
      .then(() => (copyAll.textContent = "Copied"))
      .catch(() => (copyAll.textContent = "Press Ctrl C"))
      .finally(() =>
        setTimeout(() => {
          if (copyAll.isConnected) copyAll.textContent = "Copy";
        }, 1500),
      );
  });
  const hints = h(
    "span",
    { class: "hl-hints" },
    kbd("Space"),
    " pause · ",
    kbd("End"),
    " tail",
  );
  const tb = toolbar({
    search: {
      placeholder: "Lines containing…",
      label: "Lines containing",
      value: f.q,
      onInput: (q) => {
        f = { ...f, q };
        changed();
      },
    },
    groups: [],
    state: [countEl, copyAll, ...(opts.followInBar ? [followBtn] : []), hints],
  });
  tb.el.classList.add("hl-bar");
  if (tb.search) drivable(tb.search, opts.drive.search);
  const transfers = h("div", { class: "hl-xfers" });
  // The kp log component: its grid holds the lines (gap-kplog-1).
  const list = h("div", { class: "kp-log hl-lines", "data-kp-arrive": "none" });
  // redesign-final (exact counts): "N of M lines" is the lines drawn.
  countOf(countEl, list, ".kp-log__line");
  const blocks = h("div", { class: "hl-blocks" });
  const newPill = h("button", {
    type: "button",
    class: "kp-button kp-button--sm kp-button--primary hl-newpill",
    hidden: "",
  });
  drivable(newPill, opts.drive.newLines);
  newPill.addEventListener("click", () => setFollow(true));
  const out = h(
    "div",
    {
      class: "hl-out",
      role: "log",
      "aria-live": "polite",
      "aria-label": opts.label,
      tabindex: "0",
      id: "host-log",
    },
    transfers,
    list,
    blocks,
  );
  // ── the levels as chips, exact counts (feat-kp11demo-7) ───────────────
  const levelBtn = (/** @type {string} */ value, /** @type {string} */ label) =>
    drivable(
      h(
        "button",
        {
          class: "kp-button",
          type: "button",
          "data-kp-toggle": "",
          "data-kp-look": "chip",
          "data-kp-toggle-value": value,
          "data-kp-count": "0",
          "aria-pressed": String(f.levels.has(value)),
          title: `Show or hide the ${label.toLowerCase()} lines; several levels can be on`,
        },
        label,
      ),
      opts.drive.level,
      value,
    );
  const levelsEl = h(
    "div",
    {
      class: "kp-toggles hl-levels",
      role: "group",
      "aria-label": "Level",
      "data-kp-toggle-group": "level",
    },
    ...LEVELS.map((lv) => levelBtn(lv.value, lv.label)),
  );
  const levelRead = h("span", { class: "hl-levelread", role: "status" });
  /** @param {Record<string, number>} byLevel */
  const paintLevels = (byLevel) => {
    for (const b of /** @type {HTMLElement[]} */ ([
      ...levelsEl.querySelectorAll("button"),
    ])) {
      const v = b.getAttribute("data-kp-toggle-value") ?? "";
      b.setAttribute("data-kp-count", String(byLevel[v] ?? 0));
      toggleOf(b)?.setCount(byLevel[v] ?? 0);
      b.setAttribute("aria-pressed", String(f.levels.has(v)));
    }
    levelRead.textContent = loaded
      ? `${Object.values(byLevel)
          .reduce((a, n) => a + n, 0)
          .toLocaleString("en-GB")} lines in the window`
      : "";
  };
  levelsEl.addEventListener(TOGGLE_CHANGED_EVENT, (e) => {
    const keys = /** @type {CustomEvent} */ (e).detail.pressedKeys ?? [];
    const levels = new Set(keys.length ? keys : LEVELS.map((x) => x.value));
    f = { ...f, levels };
    saveLogLevels(levels);
    changed();
  });

  // ── where in the window did it happen (feat-kp11demo-7) ───────────────
  const pictureSeg = h(
    "div",
    {
      class: "kp-segmented",
      "data-kp-segmented": "single",
      role: "radiogroup",
      "aria-label": "Picture of the window",
    },
    h(
      "button",
      {
        type: "button",
        class: "kp-segmented__item",
        role: "radio",
        "aria-checked": "true",
        "data-value": "grid",
      },
      "Sources by time",
    ),
    h(
      "button",
      {
        type: "button",
        class: "kp-segmented__item",
        role: "radio",
        "aria-checked": "false",
        "data-value": "strip",
      },
      "The strip",
    ),
  );
  const pictureRead = h(
    "span",
    { class: "hl-levelread", role: "status" },
    "Press a cell to look at those five minutes.",
  );
  const stripEl = h("div", {
    class: "kp-uptime hl-strip",
    "data-kp-uptime": "",
    "data-kp-uptime-fit": "scroll",
  });
  const gridEl = h("div", {
    class: "kp-heat hl-grid",
    "data-kp-heat": "",
    "data-kp-heat-toggle": "",
  });
  const pictureBox = h("div", { class: "hl-picture" }, gridEl, stripEl);
  const pictureRetry = h(
    "button",
    { type: "button", class: "kp-button", hidden: "" },
    "Try again",
  );
  pictureRetry.addEventListener("click", () => void read());
  const density = formGrid(
    [
      gridRow({
        label: "Where",
        hideLabel: true,
        control: pictureSeg,
        value: pictureRead,
      }),
      h(
        "div",
        { "data-kp-formgrid-span": "all", class: "hl-picturewrap" },
        pictureBox,
        pictureRetry,
      ),
    ],
    "hl-density",
  );
  const levelsRow = formGrid(
    [
      gridRow({
        label: "Levels",
        control: levelsEl,
        value: levelRead,
        wide: false,
      }),
    ],
    "hl-levelsrow",
  );
  const qualify = (/** @type {number} */ ts) => formatDateTime(ts);
  const paintDensity = () => {
    const buckets = loaded && !failure ? densityBuckets(lines) : [];
    const sources = allSources();
    stripEl.hidden = picture !== "strip";
    gridEl.hidden = picture !== "grid";
    pictureRetry.hidden = !failure;
    setSegmentedValues(pictureSeg, [picture]);
    const state = !loaded
      ? "loading"
      : failure
        ? "error"
        : buckets.length
          ? "ready"
          : "empty";
    const every = Math.max(1, Math.round(buckets.length / 12));
    const colour = (/** @type {number} */ i) => `var(--chart-${(i % 5) + 1})`;
    setUptime(stripEl, {
      label: "Lines in the window",
      periods: buckets.map((b) => ({
        key: String(b.start),
        label: formatClock(b.start),
        state:
          b.worst === "critical"
            ? "down"
            : b.worst === "warning"
              ? "degraded"
              : b.count
                ? "up"
                : "none",
        tip: `${qualify(b.start)}: ${b.count.toLocaleString("en-GB")} ${b.count === 1 ? "line" : "lines"}${b.worst === "critical" ? ", errors" : b.worst === "warning" ? ", warnings" : ""}`,
      })),
      from: buckets.length ? qualify(buckets[0].start) : "",
      to: buckets.length
        ? qualify(buckets[buckets.length - 1].start + BUCKET_S)
        : "",
      count: Math.max(buckets.length, 1),
      state,
      error: failure ?? undefined,
    });
    setHeat(gridEl, {
      columns: buckets.map((b, i) => ({
        key: String(b.start),
        label: i % every === 0 ? formatClock(b.start) : "",
        minor: i % every !== 0,
      })),
      rows: sources.map((src, i) => ({
        key: src,
        label: sourceLabel(src),
        mark: src === "HOST" ? "var(--muted-foreground)" : colour(i),
        pressed: !sourceOn(src, f),
        cells: buckets.map((b) => {
          const x = b.bySource.get(src);
          const tone = !x
            ? "muted"
            : x.worst === "critical"
              ? "bad"
              : x.worst === "warning"
                ? "warn"
                : "ok";
          return {
            tone: /** @type {any} */ (tone),
            label: x
              ? `${x.count.toLocaleString("en-GB")} ${x.count === 1 ? "line" : "lines"}${x.worst === "critical" ? ", errors" : x.worst === "warning" ? ", warnings" : ""}`
              : "nothing logged",
            tip: `${sourceLabel(src)} · ${qualify(b.start)}: ${x ? `${x.count.toLocaleString("en-GB")} lines` : "nothing logged"}`,
          };
        }),
      })),
      toggle: true,
      every,
      state,
      message:
        failure ??
        "No lines yet: the host has printed nothing since the dashboard started.",
      skeleton: { rows: Math.max(sources.length, 3), columns: 24 },
      label: "Sources against five-minute columns",
      picked: range == null ? null : String(range),
    });
    // sources shown, as the side column says
    setHeatRows(
      gridEl,
      sources.filter((s2) => !sourceOn(s2, f)),
    );
  };
  stripEl.addEventListener(UPTIME_PICK_EVENT, (e) => {
    const key = Number(/** @type {CustomEvent} */ (e).detail.key);
    const b = densityBuckets(lines).find((x) => x.start === key);
    if (!b || b.first == null) return;
    const el = list.querySelector(`.kp-log__line[data-seq="${b.first}"]`);
    setFollow(false);
    el?.scrollIntoView({ block: "start" });
    pictureRead.textContent = el
      ? `Jumped to ${formatClock(b.start)}, line ${b.first}.`
      : `${formatClock(b.start)}: its lines are hidden by the filters.`;
  });
  gridEl.addEventListener(HEAT_PICK_EVENT, (e) => {
    const d = /** @type {CustomEvent} */ (e).detail;
    range = d.picked ? Number(d.colKey) : null;
    pictureRead.textContent = d.picked
      ? `Showing the lines of ${formatClock(Number(d.colKey))} to ${formatClock(Number(d.colKey) + BUCKET_S)} only.`
      : "Press a cell to look at those five minutes.";
    paintLines();
  });
  gridEl.addEventListener(HEAT_ROW_EVENT, (e) => {
    const off = new Set(/** @type {CustomEvent} */ (e).detail.keys);
    f = {
      ...f,
      off: new Set(allSources().filter((x) => off.has(x))),
      only: null,
    };
    changed();
  });
  pictureSeg.addEventListener(SEGMENTED_EVENT, (e) => {
    picture =
      /** @type {CustomEvent} */ (e).detail.value === "strip"
        ? "strip"
        : "grid";
    paintDensity();
  });
  void bucketStart;

  const term = h(
    "section",
    { class: "hl-term", "aria-label": opts.label },
    tb.el,
    levelsRow,
    density,
    h("div", { class: "hl-outwrap" }, out, newPill),
    ...(opts.foot ? [opts.foot] : []),
  );
  root.replaceChildren(h("div", { class: "hl" }, side, term));
  const detachKp = [
    attachToggles(root),
    attachSegmented(root),
    attachUptimes(root),
    attachHeats(root),
    attachHoverCards(root),
  ];

  const fleetNames = () => (current().fleet?.stacks ?? []).map((s) => s.name);
  const allSources = () => sourceList(fleetNames(), seen);

  const toUrl = () =>
    history.replaceState(
      history.state,
      "",
      location.pathname + setParams(location.search, logFilterToParams(f)),
    );

  /** @param {string} s */
  const swatch = (s) =>
    s === "HOST" || !fleetNames().includes(s)
      ? h("span", { class: "hl-sw", "aria-hidden": "true" })
      : stackMark(s, 12);

  // The side column is built once and updated in place (senior review,
  // finding 3): rebuilding it for every arriving line took keyboard focus
  // away and made the "only" link flicker. A source seen for the first
  // time gets its row inserted at its place; nothing else moves.
  const everythingIn = /** @type {HTMLInputElement} */ (
    drivable(h("input", { type: "checkbox" }), opts.drive.everything)
  );
  everythingIn.addEventListener("change", () => {
    f = { ...f, off: new Set(), only: null };
    changed();
  });
  const everythingCount = h("small");
  const srcOpts = h(
    "div",
    { class: "hl-opts" },
    h(
      "label",
      { class: "hl-opt", title: "Show every source again" },
      everythingIn,
      h("span"),
      h("span", null, "Everything"),
      everythingCount,
    ),
  );
  /** @type {Map<string, {label: HTMLElement, input: HTMLInputElement,
   *   count: HTMLElement}>} */
  const srcRows = new Map();
  /** @param {string} s */
  const sourceRow = (s) => {
    const input = /** @type {HTMLInputElement} */ (
      drivable(
        h("input", {
          type: "checkbox",
          "aria-label": `Show ${sourceLabel(s)}`,
        }),
        opts.drive.source,
        s,
      )
    );
    input.addEventListener("change", () => {
      f = toggleSource(f, s, allSources());
      changed();
    });
    const only = drivable(
      h(
        "button",
        {
          type: "button",
          class: "hl-only",
          title: `Show only ${sourceLabel(s)}`,
        },
        "only",
      ),
      opts.drive.only,
      s,
    );
    only.addEventListener("click", (e) => {
      e.preventDefault();
      f = { ...f, only: s, off: new Set() };
      changed();
    });
    const count = h("small");
    const label = h(
      "label",
      {
        class: "hl-opt",
        title: `Show or hide ${s === "HOST" ? "the host's own" : `${s}'s`} lines; each click turns it on or off`,
      },
      input,
      swatch(s),
      h("span", { class: "hl-opt__name" }, sourceLabel(s), " ", only),
      count,
    );
    return { label, input, count };
  };
  const reset = drivable(
    h(
      "button",
      { type: "button", class: "kp-button kp-button--ghost kp-button--sm" },
      "Reset the filters",
    ),
    opts.drive.reset,
  );
  reset.addEventListener("click", () => {
    f = { off: new Set(), only: null, levels: defaultLevels(), q: "" };
    saveLogLevels(f.levels);
    if (tb.search) tb.search.value = "";
    changed();
  });
  // The demo's words: the hint while nothing is narrowed, the reset
  // while something is (senior review, finding 16).
  const hint = h("p", { class: "hl-hint" }, "Hover a source for “only”.");
  side.replaceChildren(
    h("div", { class: "hl-group" }, h("h3", null, "Source"), srcOpts),
    h("div", { class: "hl-sidefoot" }, reset, hint),
  );

  const paintSide = () => {
    const c = counts(lines);
    const srcs = allSources();
    srcs.forEach((s, i) => {
      if (srcRows.has(s)) return;
      const row = sourceRow(s);
      srcRows.set(s, row);
      // After the "Everything" row, before the next source already drawn.
      const next = srcs.slice(i + 1).find((n) => srcRows.has(n) && n !== s);
      srcOpts.insertBefore(
        row.label,
        next ? (srcRows.get(next)?.label ?? null) : null,
      );
    });
    everythingIn.checked = f.only == null && f.off.size === 0;
    everythingCount.textContent = String(lines.length);
    for (const [s, row] of srcRows) {
      const on = sourceOn(s, f);
      row.input.checked = on;
      row.label.classList.toggle("hl-opt--off", !on);
      row.count.textContent = String(c.bySource.get(s) ?? 0);
    }
    paintLevels(c.byLevel);
    const n = narrowed(f);
    reset.hidden = !n;
    hint.hidden = n;
  };

  /** @param {import("../parity.js").HostLine} l */
  const lineEls = (l, fresh = false) => {
    const x = logLine(/** @type {any} */ ({ ...l, job: 0 }));
    const isOpen = open === l.seq;
    // The line is `display: contents` (kp's grid), so the button is its
    // message cell; a click anywhere on the line still reaches the line.
    const message = drivable(
      h(
        "span",
        {
          class: "kp-log__message hl-msg",
          role: "button",
          tabindex: "0",
          "aria-expanded": String(isOpen),
          title: "Click for when, who and the request it belongs to",
        },
        ...mark(l.msg),
        l.by
          ? h(
              "em",
              null,
              `  · ${l.by === "admin" ? "this dashboard" : ownerOf(l.by)}`,
            )
          : "",
      ),
      opts.drive.line,
      String(l.seq),
    );
    const row = h(
      "p",
      {
        class: `kp-log__line${fresh ? " is-new" : ""}`,
        "data-kp-severity": x.severity,
        ...(isOpen ? { "data-kp-current": "" } : {}),
        "data-seq": String(l.seq),
      },
      h("time", { class: "kp-log__time" }, x.time),
      h(
        "span",
        { class: "kp-log__source", "data-kp-source": l.source },
        swatch(l.source),
        sourceLabel(l.source),
      ),
      h("span", { class: "kp-log__level" }, x.level),
      message,
    );
    const toggle = () => {
      open = isOpen ? null : l.seq;
      paintLines();
      // The repainted row keeps focus, so the next key reaches it.
      /** @type {HTMLElement | null} */ (
        list.querySelector(`.kp-log__line[data-seq="${l.seq}"] [role=button]`)
      )?.focus({ preventScroll: true });
    };
    row.addEventListener("click", toggle);
    message.addEventListener("keydown", (e) => {
      // A role=button row answers Space like Enter (finding 12).
      if (e.target === message && (e.key === "Enter" || e.key === " ")) {
        e.preventDefault();
        toggle();
      }
    });
    if (!isOpen) return [row];
    const job =
      l.req == null
        ? null
        : act.jobs.find((j) => j.reqs?.includes(l.req ?? -1));
    const onlyBtn = h(
      "button",
      { type: "button", class: "kp-button kp-button--sm" },
      `Only ${sourceLabel(l.source)}`,
    );
    drivable(onlyBtn, opts.drive.onlyLine);
    onlyBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      f = { ...f, only: l.source, off: new Set() };
      open = null;
      changed();
    });
    const copy = drivable(
      h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--sm kp-button--ghost",
          title: "Copy this line to the clipboard",
        },
        "Copy line",
      ),
      opts.drive.copy,
    );
    copy.addEventListener("click", (e) => {
      e.stopPropagation();
      void navigator.clipboard
        ?.writeText(`${x.time} ${x.level} ${l.source} ${l.msg}`)
        .then(() => (copy.textContent = "Copied"))
        .catch(() => (copy.textContent = "Press Ctrl C"))
        // Back to its own name after a moment (finding 14).
        .finally(() =>
          setTimeout(() => {
            if (copy.isConnected) copy.textContent = "Copy line";
          }, 1500),
        );
    });
    const more = h(
      "div",
      { class: "hl-more" },
      h(
        "dl",
        { class: "kp-kv" },
        h(
          "div",
          { class: "kp-kv__row" },
          h("dt", null, "When"),
          h("dd", null, formatDateTime(l.ts)),
        ),
        h(
          "div",
          { class: "kp-kv__row" },
          h("dt", null, "Started by"),
          h(
            "dd",
            null,
            l.by
              ? l.by === "admin"
                ? "Kenny, from this dashboard"
                : `${ownerOf(l.by)} (the token “${l.by}”)`
              : "the host (the nightly round or its own work)",
          ),
        ),
        h(
          "div",
          { class: "kp-kv__row" },
          h("dt", null, "Line"),
          h(
            "dd",
            { class: "mono" },
            `#${l.seq}${l.req != null ? ` · request ${l.req}` : ""}`,
          ),
        ),
      ),
      h(
        "div",
        { class: "hl-more__acts" },
        onlyBtn,
        ...(job
          ? [
              (() => {
                const b = drivable(
                  h(
                    "button",
                    {
                      type: "button",
                      class: "kp-button kp-button--sm kp-button--ghost",
                      title: "Open the job this line belongs to",
                    },
                    "Open the job",
                  ),
                  opts.drive.openJob,
                  String(job.job),
                );
                b.addEventListener("click", (e) => {
                  e.stopPropagation();
                  openJobDialog(job.job);
                });
                return b;
              })(),
            ]
          : []),
        copy,
      ),
    );
    return [row, more];
  };

  /** @param {string} text @returns {(string | Node)[]} */
  const mark = (text) => {
    const q = f.q.trim().toLowerCase();
    if (!q) return [text];
    const i = text.toLowerCase().indexOf(q);
    if (i < 0) return [text];
    return [
      text.slice(0, i),
      h("mark", null, text.slice(i, i + q.length)),
      text.slice(i + q.length),
    ];
  };

  const shownLines = () =>
    lines.filter((l) => lineShown(l, f) && inRange(l, range));

  const paintCount = () => {
    countEl.textContent = loaded
      ? countText(shownLines().length, lines.length)
      : "Reading the lines…";
    followBtn.textContent = follow ? "Pause" : "Resume";
    followBtn.title = follow
      ? "Stop following: new lines wait below a marker (Space)"
      : "Follow the newest lines again (Space)";
    followBtn.setAttribute("aria-pressed", String(!follow));
    newPill.hidden = follow || unseen === 0;
    newPill.textContent = `${unseen} new ${unseen === 1 ? "line" : "lines"} · back to the tail`;
    root.dataset.following = String(follow);
  };

  const toTail = () => {
    out.scrollTop = out.scrollHeight;
  };

  const paintLines = () => {
    if (!loaded) {
      list.replaceChildren(
        ...Array.from({ length: 12 }, () =>
          h(
            "div",
            { class: "hl-sk", "data-kp-state": "loading" },
            ...[5, 6, 3, 80].map((w, i) =>
              h("span", {
                class: "kp-skeleton",
                style: `inline-size:${w}${i === 3 ? "%" : "rem"}`,
              }),
            ),
          ),
        ),
      );
    } else if (failure) {
      const again = h(
        "button",
        { type: "button", class: "kp-button kp-button--sm" },
        "Try again",
      );
      drivable(again, opts.drive.retry);
      again.addEventListener("click", () => void read());
      list.replaceChildren(
        h(
          "div",
          { class: "kp-alert kp-alert--destructive hl-error", role: "alert" },
          h(
            "span",
            { class: "kp-alert__body" },
            `The host's lines could not be read: ${failure}`,
          ),
          again,
        ),
      );
    } else {
      const v = shownLines();
      list.replaceChildren(
        ...(v.length
          ? v.flatMap((l) => lineEls(l))
          : [
              emptyState(
                lines.length
                  ? {
                      title: "No line matches",
                      text: "Lines that match the filter appear here as they arrive.",
                    }
                  : {
                      title: "No lines yet",
                      text: "The host has printed nothing since the dashboard started; its lines appear here as it works.",
                    },
              ),
            ]),
      );
    }
    attachLogs(list);
    paintCount();
    paintDensity();
    if (follow) toTail();
  };

  let sideQueued = false;
  const queueSide = () => {
    if (sideQueued) return;
    sideQueued = true;
    requestAnimationFrame(() => {
      sideQueued = false;
      paintSide();
      paintDensity();
    });
  };

  const changed = () => {
    toUrl();
    paintSide();
    paintLines();
  };

  /** @param {boolean} on @param {boolean} [quiet] */
  const setFollow = (on, quiet = false) => {
    follow = on;
    if (on) {
      unseen = 0;
      toTail();
    }
    paintCount();
    opts.onFollow?.(on);
    if (!quiet && on) toTail();
  };

  /** The seq of the line holding focus in this log, or null. */
  const focusedSeq = () => {
    const at = document.activeElement?.closest?.(".kp-log__line[data-seq]");
    return at && list.contains(at)
      ? Number(/** @type {HTMLElement} */ (at).dataset.seq)
      : null;
  };

  /** @param {import("../parity.js").HostLine} l */
  const add = (l) => {
    // Until the snapshot is in, a live line waits: the two merge by number
    // when it lands (finding 1).
    if (!loaded) {
      early.push(l);
      if (early.length > KEEP) early = early.slice(early.length - KEEP);
      return;
    }
    if (l.seq <= lastSeq) return;
    lastSeq = l.seq;
    lines.push(l);
    if (lines.length > KEEP) {
      // The ring drops its oldest lines, but never the open one or the one
      // holding focus: a repaint (Space on that line) must still find it,
      // or Space opens a different line and the focus falls to the page.
      const held = new Set([open, focusedSeq()]);
      const cut = lines.length - KEEP;
      lines = lines.filter((x, i) => i >= cut || held.has(x.seq));
    }
    seen.add(l.source);
    queueSide();
    if (!loaded || !lineShown(l, f) || !inRange(l, range)) {
      paintCount();
      return;
    }
    if (!follow) unseen += 1;
    if (list.querySelector(".nx-empty")) list.replaceChildren();
    const els = lineEls(l, true);
    list.append(...els);
    // Only the new line: the colour of a source is written per element.
    for (const el of els) attachLogs(el);
    // The oldest line is dropped to keep the page at the server's ring, but
    // never the one holding focus: losing it would send the next Space to
    // the page's pause shortcut instead of the line.
    let oldest = list.firstElementChild;
    while (oldest && list.childElementCount > KEEP) {
      const next = oldest.nextElementSibling;
      const seq = Number(/** @type {HTMLElement} */ (oldest).dataset.seq);
      if (!oldest.contains(document.activeElement) && seq !== open)
        oldest.remove();
      oldest = next;
    }
    if (follow) toTail();
    paintCount();
  };

  /** @type {Map<string, any>} */
  const live = new Map();
  const paintTransfers = () => {
    const now = Date.now() / 1000;
    for (const [k, t] of live) if (now - t.at > 30) live.delete(k);
    transfers.replaceChildren(
      ...[...live.values()].map((t) => {
        const v = transferView(t);
        return h(
          "div",
          { class: "hl-xfer" },
          h("span", null, h("strong", null, t.op), ` · ${t.label}`),
          progressBar(v.label, v.pct, "hl-xfer__bar"),
          h("span", { class: "hl-hint" }, v.text),
        );
      }),
    );
  };

  // Scrolling up is the TUI's SCROLL mode: following stops until the tail.
  out.addEventListener("scroll", () => {
    const atTail = out.scrollHeight - out.scrollTop - out.clientHeight < 24;
    if (atTail && !follow) setFollow(true, true);
    else if (!atTail && follow && loaded) {
      follow = false;
      paintCount();
      opts.onFollow?.(false);
    }
  });

  /** @param {KeyboardEvent} e */
  const onKey = (e) => {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    // A line row already answered this key (Space or Enter opens it).
    if (e.defaultPrevented) return;
    if (document.querySelector("dialog[open]")) return;
    const a = document.activeElement;
    const what = keyAction(
      e.key,
      a?.tagName ?? "",
      a?.getAttribute("role") ?? null,
    );
    if (!what) return;
    e.preventDefault();
    if (what === "search") tb.search?.focus();
    else if (what === "pause") setFollow(!follow);
    else setFollow(true);
  };
  document.addEventListener("keydown", onKey);

  const abort = new AbortController();
  const read = async () => {
    loaded = false;
    failure = null;
    paintLines();
    const r = await fetchJson(
      "/data/host-log",
      "the host's lines",
      abort.signal,
    );
    loaded = true;
    // The page's own lines, the snapshot and the live lines that came
    // while it was read: merged by number, capped at KEEP (finding 1).
    lines = mergeLines(
      mergeLines(lines, r.ok ? (r.body.lines ?? []) : [], KEEP),
      early,
      KEEP,
    );
    early = [];
    if (!r.ok) {
      failure = r.error.why;
      paintLines();
      return;
    }
    for (const s of r.body.sources ?? []) seen.add(s);
    for (const l of lines) seen.add(l.source);
    lastSeq = lines.length ? lines[lines.length - 1].seq : lastSeq;
    for (const t of r.body.transfers ?? []) live.set(`${t.op}|${t.label}`, t);
    paintTransfers();
    paintSide();
    paintLines();
  };

  const offLine = listen("host_log", (l) => add(l));
  const offTransfer = listen("transfer", (t) => {
    live.set(`${t.op}|${t.label}`, t);
    paintTransfers();
  });
  const unsub = subscribe(queueSide);
  paintSide();
  paintLines();
  void read();
  const timer = setInterval(paintTransfers, 5000);

  const download = () => {
    const text = shownLines()
      .map((l) => {
        const x = logLine(/** @type {any} */ ({ ...l, job: 0 }));
        return `${new Date(l.ts * 1000).toISOString()}  ${x.level.toUpperCase().padEnd(5)}  ${l.source.padEnd(12)}  ${l.msg}`;
      })
      .join("\n");
    const url = URL.createObjectURL(
      new Blob([`${text}\n`], { type: "text/plain" }),
    );
    const a = h("a", { href: url, download: "host-log.txt" });
    document.body.append(a);
    a.click();
    a.remove();
    // The browser has the file once the click ran (finding 14).
    setTimeout(() => URL.revokeObjectURL(url), 0);
  };

  return {
    cleanup: () => {
      abort.abort();
      offLine();
      offTransfer();
      unsub();
      clearInterval(timer);
      document.removeEventListener("keydown", onKey);
      for (const d of detachKp) d();
    },
    setFollow,
    following: () => follow,
    download,
    addBlock: (el) => {
      blocks.append(el);
      setFollow(true);
    },
    focusSearch: () => tb.search?.focus(),
  };
}
