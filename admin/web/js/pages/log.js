// redesign-console (3.71.0, the Console demo Kenny approved on 2026-10-03):
// the host-log explorer. Every line the host prints for every operation,
// whoever started it (this dashboard, a CLI or TUI on a workstation, the
// nightly round), live. Console shows it with the shell under it; Activity
// shows it as its Host log view (`/activity?view=host-log`; the old `/log`
// redirects there).
//
// feat-kpadopt-2 (kp-themes 11.1.1): the lines are kp's log viewer
// (`logviewer.js`: find, wrap, time column, Copy, the ring buffer, the
// "N new lines" pill and the loading, empty and failed states), the
// filters are two kp facet groups (Source, Level) and Copy line flashes its
// answer (`flash.js`). The page owns which lines the viewer is handed (the
// facets and the five-minute pick narrow them), the details of the chosen
// line under the lines, and the Live view marks.

import { act } from "../act.js";
import { ownerOf } from "../activityview.js";
import { fetchJson, h, progressBar } from "../dom.js";
import { declare, drivable } from "../drivable.js";
import {
  LEVELS,
  defaultLevels,
  BUCKET_S,
  counts,
  densityBuckets,
  inRange,
  lineShown,
  logFilterFromParams,
  logFilterToParams,
  mergeLines,
  narrowed,
  sourceLabel,
  sourceList,
  sourceOn,
} from "../hostlogview.js";
import { logLine } from "../jobs.js";
import { levelBucket, saveLogLevels } from "../logs.js";
import { openJobDialog } from "../jobpanel.js";
import { transferView } from "../parity.js";
import { current, listen, subscribe } from "../store.js";
import { kbd } from "../ui.js";
import { setParams } from "../urlstate.js";
import { facetGroup } from "../logfacets.js";
import { gridRow, formGrid } from "../gridrow.js";
import { formatClock, formatDateTime } from "../format.js";
import { sourceColour } from "../logcolour.js";
import {
  LOGVIEWER_EVENT,
  LOGVIEWER_RETRY_EVENT,
  appendLogLines,
  attachLogViewers,
  logViewerText,
  setLogViewer,
} from "/static/kp/dist/kp-themes.min.js";
import { FOLLOW_EVENT, newPill } from "/static/kp/dist/kp-themes.min.js";
import {
  FACET_EVENT,
  attachFacets,
  setFacetCounts,
  setFacetItems,
  setFacetValues,
} from "/static/kp/dist/kp-themes.min.js";
import { runFlashButton } from "/static/kp/dist/kp-themes.min.js";
import {
  SEGMENTED_EVENT,
  attachSegmented,
  setSegmentedValues,
} from "/static/kp/dist/kp-themes.min.js";
import {
  UPTIME_PICK_EVENT,
  attachUptimes,
  setUptime,
} from "/static/kp/dist/kp-themes.min.js";
import {
  HEAT_PICK_EVENT,
  HEAT_ROW_EVENT,
  attachHeats,
  setHeat,
  setHeatRows,
} from "/static/kp/dist/kp-themes.min.js";
import { attachHoverCards } from "/static/kp/dist/kp-themes.min.js";
import { attachFootprints } from "/static/kp/dist/kp-themes.min.js";
import { attachDisabledReasons } from "/static/kp/dist/kp-themes.min.js";

/** Lines kept in the page, as the server's ring. */
const KEEP = 2000;

// feat-logs-1: Copy for the whole host log, declared here because Activity's
// Host log and Console both draw this explorer. It is the viewer's own Copy
// (the lines the filters show).
const COPY_ALL = declare({
  id: "host-log-copy-all",
  page: "log",
  opens: "run",
  what: "copy every line of the host log to the clipboard",
});

/** The swatch of a level's facet row. @type {Record<string, string>} */
const LEVEL_SWATCH = {
  debug: "var(--muted-foreground)",
  info: "var(--chart-1, var(--primary))",
  warning: "var(--warning-foreground)",
  critical: "var(--destructive)",
};

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
 *   focusSearch: () => void}}
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
  /** The line whose details are open. @type {number | null} */
  let open = null;
  /** The five minutes (start, unix seconds) the lines are narrowed to. @type {number | null} */
  let range = null;
  /** The picture over the lines: sources by time (opens first) or the strip. @type {"grid" | "strip"} */
  let picture = "grid";

  // ── the filters: kp facets, one group per dimension ────────────────────
  const sourceGroup = facetGroup(
    "Source",
    "Everything",
    "No source has printed a line yet.",
  );
  const levelGroup = facetGroup("Level", "Every level", "No levels.");
  /** The rows already made, so a row that is there keeps its state. @type {Set<string>} */
  const rowsMade = new Set();
  const side = h("aside", { class: "hl-side", "aria-label": "Which lines" });
  const reset = drivable(
    h(
      "button",
      { type: "button", class: "kp-button kp-button--ghost kp-button--sm" },
      "Reset the filters",
    ),
    opts.drive.reset,
  );
  // The demo's words: the hint while nothing is narrowed, the reset
  // while something is (senior review, finding 16).
  const hint = h("p", { class: "hl-hint" }, "Hover a source for “only”.");
  side.replaceChildren(
    h("div", { class: "hl-group" }, h("h3", null, "Source"), sourceGroup),
    h("div", { class: "hl-group" }, h("h3", null, "Level"), levelGroup),
    h("div", { class: "hl-sidefoot" }, reset, hint),
  );

  // ── the lines: kp's log viewer ─────────────────────────────────────────
  const tools = ["search", "wrap", "time", "copy"];
  if (opts.followInBar) tools.push("follow");
  const viewer = h("div", {
    class: "kp-logviewer hl-viewer",
    "data-kp-logviewer": "",
    "data-kp-log-max": String(KEEP),
    "data-kp-log-tools": tools.join(","),
    "data-kp-log-filename": "host-log",
    "data-kp-log-search-only": "on",
    "data-kp-log-hint":
      "The host has printed nothing since the dashboard started; its lines appear here as it works.",
    "data-kp-state": "loading",
    "aria-label": opts.label,
    id: "host-log",
  });
  const transfers = h("div", { class: "hl-xfers" });
  // The chosen line's details stand under the lines in one fixed slot, so
  // choosing one moves nothing.
  const detail = h("div", {
    class: "hl-detail",
    role: "region",
    "aria-label": "The chosen line",
    "aria-live": "polite",
  });
  const hints = h(
    "p",
    { class: "hl-hints" },
    kbd("↑"),
    kbd("↓"),
    " choose a line · ",
    kbd("Space"),
    " pause · ",
    kbd("End"),
    " tail · ",
    kbd("/"),
    " search",
  );

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

  const term = h(
    "section",
    { class: "hl-term", "aria-label": opts.label },
    transfers,
    density,
    viewer,
    detail,
    hints,
    ...(opts.foot ? [opts.foot] : []),
  );
  root.replaceChildren(h("div", { class: "hl" }, side, term));
  const detachKp = [
    attachFacets(root),
    attachLogViewers(root),
    attachSegmented(root),
    attachUptimes(root),
    attachHeats(root),
    attachHoverCards(root),
    attachDisabledReasons(root),
    // the heat grid and the strip keep their loaded height in every state
    attachFootprints(root),
  ];
  const logEl = /** @type {HTMLElement} */ (
    viewer.querySelector(".kp-logviewer__log")
  );
  setLogViewer(viewer, { search: f.q, searchOnly: true, state: "loading" });

  const fleetNames = () => (current().fleet?.stacks ?? []).map((s) => s.name);
  const allSources = () => sourceList(fleetNames(), seen);

  const toUrl = () =>
    history.replaceState(
      history.state,
      "",
      location.pathname + setParams(location.search, logFilterToParams(f)),
    );

  /** Whether the filters let a line into the viewer. @param {import("../parity.js").HostLine} l */
  const visible = (l) => lineShown(l, { ...f, q: "" }) && inRange(l, range);

  /** A host line as the viewer keeps it. @param {import("../parity.js").HostLine} l */
  const kpLine = (l) => ({
    id: String(l.seq),
    ts: l.ts * 1000,
    source: sourceLabel(l.source),
    level: levelBucket(l.level),
    message: l.msg,
  });

  // ── the Live view marks on what kp draws ──────────────────────────────
  const stampDrive = () => {
    /** Runs `mark` on the element while it has no Live view mark. @param {Element | null} el @param {(e: HTMLElement) => void} mark */
    const onFresh = (el, mark) => {
      if (el instanceof HTMLElement && !el.dataset.drive) mark(el);
    };
    for (const row of sourceGroup.querySelectorAll(
      ".kp-facet:not([data-kp-facet-all])",
    )) {
      const v = /** @type {HTMLElement} */ (row).dataset.value ?? "";
      onFresh(row.querySelector(".kp-facet__check"), (e) =>
        drivable(e, opts.drive.source, v),
      );
      onFresh(row.querySelector(".kp-facet__only"), (e) =>
        drivable(e, opts.drive.only, v),
      );
    }
    onFresh(
      sourceGroup.querySelector("[data-kp-facet-all] .kp-facet__check"),
      (e) => drivable(e, opts.drive.everything),
    );
    for (const row of levelGroup.querySelectorAll(
      ".kp-facet:not([data-kp-facet-all])",
    ))
      onFresh(row.querySelector(".kp-facet__check"), (e) =>
        drivable(
          e,
          opts.drive.level,
          /** @type {HTMLElement} */ (row).dataset.value ?? "",
        ),
      );
    onFresh(viewer.querySelector(".kp-logviewer__find"), (e) =>
      drivable(e, opts.drive.search),
    );
    onFresh(viewer.querySelector("[data-kp-logviewer-copy]"), (e) =>
      drivable(e, COPY_ALL),
    );
    onFresh(viewer.querySelector("[data-kp-follow-toggle]"), (e) =>
      drivable(e, opts.drive.follow),
    );
    onFresh(viewer.querySelector("[data-kp-logviewer-retry]"), (e) =>
      drivable(e, opts.drive.retry),
    );
    onFresh(viewer.querySelector(".kp-newpill"), (e) =>
      drivable(e, opts.drive.newLines),
    );
    for (const row of logEl.querySelectorAll(".kp-log__line:not([data-drive])"))
      onFresh(row, (e) => drivable(e, opts.drive.line, e.dataset.id ?? ""));
  };
  let stampQueued = false;
  const queueStamp = () => {
    if (stampQueued) return;
    stampQueued = true;
    requestAnimationFrame(() => {
      stampQueued = false;
      stampDrive();
    });
  };
  const watcher = new MutationObserver(queueStamp);
  watcher.observe(root, { childList: true, subtree: true });

  // ── the filters ───────────────────────────────────────────────────────
  /** Bring the facet rows in step with the filter (after a reset or a heat row). */
  const syncFacets = () => {
    setFacetValues(
      sourceGroup,
      allSources().filter((s) => sourceOn(s, f)),
    );
    setFacetValues(levelGroup, [...f.levels]);
  };

  const paintSide = () => {
    if (!loaded) {
      for (const g of [sourceGroup, levelGroup])
        g.setAttribute("aria-busy", "true");
      return;
    }
    const c = counts(lines);
    const srcs = allSources();
    setFacetItems(
      sourceGroup,
      srcs.map((s) => ({
        value: s,
        label: sourceLabel(s),
        count: c.bySource.get(s) ?? 0,
        swatch: sourceColour(sourceLabel(s)),
        pattern: s === "HOST" ? "" : s,
        ...(rowsMade.has(`s:${s}`) ? {} : { checked: sourceOn(s, f) }),
      })),
    );
    for (const s of srcs) rowsMade.add(`s:${s}`);
    setFacetItems(
      levelGroup,
      LEVELS.map((lv) => ({
        value: lv.value,
        label: lv.label,
        count: c.byLevel[lv.value] ?? 0,
        swatch: LEVEL_SWATCH[lv.value],
        ...(rowsMade.has(`l:${lv.value}`)
          ? {}
          : { checked: f.levels.has(lv.value) }),
      })),
    );
    for (const lv of LEVELS) rowsMade.add(`l:${lv.value}`);
    setFacetCounts(
      sourceGroup,
      Object.fromEntries(srcs.map((s) => [s, c.bySource.get(s) ?? 0])),
      { all: lines.length },
    );
    setFacetCounts(
      levelGroup,
      Object.fromEntries(LEVELS.map((lv) => [lv.value, c.byLevel[lv.value]])),
      { all: lines.length },
    );
    const n = narrowed(f) || range != null;
    reset.hidden = !n;
    hint.hidden = n;
    queueStamp();
  };

  sourceGroup.addEventListener(FACET_EVENT, (e) => {
    const values = new Set(/** @type {CustomEvent} */ (e).detail.values);
    f = {
      ...f,
      only: null,
      off: new Set(allSources().filter((s) => !values.has(s))),
    };
    changed();
  });
  levelGroup.addEventListener(FACET_EVENT, (e) => {
    const values = /** @type {string[]} */ (
      /** @type {CustomEvent} */ (e).detail.values
    );
    // Nothing ticked reads as every level, as the chips did.
    if (!values.length) setFacetValues(levelGroup, null);
    const levels = new Set(values.length ? values : LEVELS.map((x) => x.value));
    f = { ...f, levels };
    saveLogLevels(levels);
    changed();
  });
  reset.addEventListener("click", () => {
    f = { off: new Set(), only: null, levels: defaultLevels(), q: "" };
    saveLogLevels(f.levels);
    range = null;
    setLogViewer(viewer, { search: "" });
    syncFacets();
    changed();
  });

  // ── the chosen line ───────────────────────────────────────────────────
  const markCurrent = () => {
    for (const r of logEl.querySelectorAll(".kp-log__line[data-kp-current]"))
      if (/** @type {HTMLElement} */ (r).dataset.id !== String(open))
        r.removeAttribute("data-kp-current");
    if (open != null)
      logEl
        .querySelector(`.kp-log__line[data-id="${open}"]`)
        ?.setAttribute("data-kp-current", "");
  };

  const paintDetail = () => {
    if (failure) {
      // The reason stands in the viewer's failed plate (kp 11.2.0 `message`).
      detail.replaceChildren(h("p", { class: "hl-detail__note" }, ""));
      return;
    }
    const l = open == null ? null : lines.find((x) => x.seq === open);
    if (!l) {
      open = null;
      detail.replaceChildren(
        h(
          "p",
          { class: "hl-detail__note" },
          loaded
            ? "Press a line to see when it was written, who started it and the request it belongs to."
            : "",
        ),
      );
      return;
    }
    const x = logLine(/** @type {any} */ ({ ...l, job: 0 }));
    const job =
      l.req == null
        ? null
        : act.jobs.find((j) => j.reqs?.includes(l.req ?? -1));
    const onlyBtn = drivable(
      h(
        "button",
        { type: "button", class: "kp-button kp-button--sm" },
        `Only ${sourceLabel(l.source)}`,
      ),
      opts.drive.onlyLine,
    );
    onlyBtn.addEventListener("click", () => {
      f = { ...f, only: l.source, off: new Set() };
      open = null;
      syncFacets();
      changed();
    });
    const copy = drivable(
      h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--sm kp-button--ghost",
          title: "Copy this line to the clipboard",
          "data-kp-flash-words": "Copied,Press Ctrl C",
        },
        "Copy line",
      ),
      opts.drive.copy,
    );
    copy.addEventListener("click", () => {
      void runFlashButton(copy, async () => {
        try {
          await navigator.clipboard.writeText(
            `${x.time} ${x.level} ${l.source} ${l.msg}`,
          );
          return "Copied";
        } catch {
          return { error: "Press Ctrl C", word: "Press Ctrl C" };
        }
      });
    });
    const jobBtn = job
      ? drivable(
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
        )
      : null;
    jobBtn?.addEventListener("click", () => openJobDialog(job?.job ?? 0));
    const kv = (/** @type {string} */ k, /** @type {string} */ v, cls = "") =>
      h(
        "div",
        { class: "kp-kv__row" },
        h("dt", null, k),
        h("dd", { class: cls }, v),
      );
    detail.replaceChildren(
      h(
        "dl",
        { class: "kp-kv hl-detail__kv" },
        kv("When", formatDateTime(l.ts)),
        kv(
          "Started by",
          l.by
            ? l.by === "admin"
              ? "Kenny, from this dashboard"
              : `${ownerOf(l.by)} (the token “${l.by}”)`
            : "the host (the nightly round or its own work)",
        ),
        kv(
          "Line",
          `#${l.seq}${l.req != null ? ` · request ${l.req}` : ""}`,
          "mono",
        ),
      ),
      h(
        "div",
        { class: "hl-detail__acts" },
        onlyBtn,
        ...(jobBtn ? [jobBtn] : []),
        copy,
      ),
    );
  };

  /** @param {number | null} seq */
  const choose = (seq) => {
    open = seq;
    markCurrent();
    paintDetail();
  };

  viewer.addEventListener("click", (e) => {
    const row =
      e.target instanceof Element
        ? e.target.closest(".kp-log__line[data-id]")
        : null;
    if (!row || !logEl.contains(row)) return;
    // Selecting text to copy it is not choosing the line.
    if (getSelection()?.toString()) return;
    const seq = Number(/** @type {HTMLElement} */ (row).dataset.id);
    choose(seq === open ? null : seq);
  });
  logEl.addEventListener("keydown", (e) => {
    if (e.target !== logEl || e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.key === "Escape" && open != null) {
      e.preventDefault();
      choose(null);
      return;
    }
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    const rows = /** @type {HTMLElement[]} */ ([
      ...logEl.querySelectorAll(".kp-log__line[data-id]:not([hidden])"),
    ]);
    if (!rows.length) return;
    e.preventDefault();
    const at = rows.findIndex((r) => r.dataset.id === String(open));
    const next =
      e.key === "ArrowDown"
        ? rows[at < 0 ? 0 : Math.min(at + 1, rows.length - 1)]
        : rows[at < 0 ? rows.length - 1 : Math.max(at - 1, 0)];
    // Looking back stops the following; the pill counts what comes meanwhile.
    if (e.key === "ArrowUp") newPill(logEl)?.pause();
    choose(Number(next.dataset.id));
    next.scrollIntoView({ block: "nearest" });
  });
  viewer.addEventListener(LOGVIEWER_EVENT, () => {
    // The viewer's own find is the text filter; the address remembers it.
    const q = viewer.getAttribute("data-kp-log-search") ?? "";
    if (q !== f.q) {
      f = { ...f, q };
      toUrl();
      paintSide();
    }
    markCurrent();
    queueStamp();
  });
  viewer.addEventListener(LOGVIEWER_RETRY_EVENT, () => void read());

  // ── what is handed to the viewer ──────────────────────────────────────
  /** @returns {"loading" | "error" | "ready"} */
  const viewState = () => (!loaded ? "loading" : failure ? "error" : "ready");

  const feed = () => {
    setLogViewer(viewer, {
      lines: loaded && !failure ? lines.filter(visible).map(kpLine) : [],
      state: viewState(),
      ...(failure
        ? { message: `The host's lines could not be read: ${failure}` }
        : {}),
    });
    markCurrent();
    paintDetail();
    paintDensity();
    queueStamp();
  };

  // ── the picture of the window ─────────────────────────────────────────
  const qualify = (/** @type {number} */ ts) => formatDateTime(ts);
  let lastStrip = "";
  let lastHeat = "";
  const paintDensity = () => {
    const buckets = loaded && !failure ? densityBuckets(lines) : [];
    const sources = allSources();
    stripEl.hidden = picture !== "strip";
    gridEl.hidden = picture !== "grid";
    pictureRetry.hidden = !failure;
    setSegmentedValues(pictureSeg, [picture]);
    /** @type {"loading" | "error" | "ready" | "empty"} */
    const state = !loaded
      ? "loading"
      : failure
        ? "error"
        : buckets.length
          ? "ready"
          : "empty";
    const every = Math.max(1, Math.round(buckets.length / 12));
    const colour = (/** @type {number} */ i) => `var(--chart-${(i % 5) + 1})`;
    /** @type {import("/static/kp/dist/kp-themes.min.js").UptimeSpec} */
    const stripSpec = {
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
    };
    // Drawn only when what it shows changed: a live line that moves nothing
    // here must not repaint the picture (and a state the package drew in it).
    const stripKey = JSON.stringify(stripSpec);
    if (stripKey !== lastStrip) {
      lastStrip = stripKey;
      setUptime(stripEl, stripSpec);
    }
    const heatSpec = {
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
    };
    const heatKey = JSON.stringify(heatSpec);
    if (heatKey !== lastHeat) {
      lastHeat = heatKey;
      setHeat(gridEl, heatSpec);
    }
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
    const el = logEl.querySelector(`.kp-log__line[data-id="${b.first}"]`);
    newPill(logEl)?.pause();
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
    paintSide();
    feed();
  });
  gridEl.addEventListener(HEAT_ROW_EVENT, (e) => {
    const off = new Set(/** @type {CustomEvent} */ (e).detail.keys);
    f = {
      ...f,
      off: new Set(allSources().filter((x) => off.has(x))),
      only: null,
    };
    syncFacets();
    changed();
  });
  pictureSeg.addEventListener(SEGMENTED_EVENT, (e) => {
    picture =
      /** @type {CustomEvent} */ (e).detail.value === "strip"
        ? "strip"
        : "grid";
    paintDensity();
  });

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
    feed();
  };

  // ── following: the viewer's pill and Pause are kp's (newpill.js) ──────
  /** @type {boolean | null} */
  let lastFollowing = null;
  logEl.addEventListener(FOLLOW_EVENT, (e) => {
    const on = /** @type {CustomEvent} */ (e).detail.following !== false;
    root.dataset.following = String(on);
    if (on === lastFollowing) return;
    lastFollowing = on;
    opts.onFollow?.(on);
  });
  /** @param {boolean} on */
  const setFollow = (on) => {
    const pill = newPill(logEl);
    if (on) pill?.resume();
    else pill?.pause();
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
      // The ring drops its oldest lines, but never the chosen one.
      const cut = lines.length - KEEP;
      lines = lines.filter((x, i) => i >= cut || x.seq === open);
    }
    seen.add(l.source);
    queueSide();
    if (failure || !visible(l)) return;
    // A poll or a push may hand a line twice: the viewer keeps it once.
    appendLogLines(viewer, [kpLine(l)]);
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

  const abort = new AbortController();
  const read = async () => {
    loaded = false;
    failure = null;
    paintSide();
    feed();
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
      paintSide();
      feed();
      return;
    }
    for (const s of r.body.sources ?? []) seen.add(s);
    for (const l of lines) seen.add(l.source);
    lastSeq = lines.length ? lines[lines.length - 1].seq : lastSeq;
    for (const t of r.body.transfers ?? []) live.set(`${t.op}|${t.label}`, t);
    paintTransfers();
    paintSide();
    feed();
  };

  const offLine = listen("host_log", (l) => add(l));
  const offTransfer = listen("transfer", (t) => {
    live.set(`${t.op}|${t.label}`, t);
    paintTransfers();
  });
  const unsub = subscribe(queueSide);
  paintSide();
  feed();
  void read();
  const timer = setInterval(paintTransfers, 5000);

  const download = () => {
    const url = URL.createObjectURL(
      new Blob([`${logViewerText(viewer)}\n`], { type: "text/plain" }),
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
      watcher.disconnect();
      for (const d of detachKp) d();
    },
    setFollow,
    following: () => newPill(logEl)?.following() ?? true,
    download,
    focusSearch: () =>
      /** @type {HTMLElement | null} */ (
        viewer.querySelector(".kp-logviewer__find")
      )?.focus(),
  };
}
