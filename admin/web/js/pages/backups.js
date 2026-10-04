// feat-backup-1/2/3, redesign-backups (Backups redesign 3.71, approved by Kenny
// 2026-10-03; the demo is `redesign-3.71/backups.html`): the Backups page.
//
// Top to bottom: the header (Refresh, Back up now…, Restore… as the one
// primary action); five KPI tiles (last night, newest snapshot,
// repositories, restore drills — a tile that filters the table — and what
// retired stacks still keep); the attention band; the nightly coverage
// heatmap (every stack × 30 nights, with a whole-fleet row; hover a night
// for its snapshot time and ids, click to pin it — into the address as
// `?night=` — and read it in the side panel); the repositories grouped per
// stack (sort, `/` filter, a 7-night tick strip, size, drill status, a
// click shows every snapshot with its own Restore); and "Kept from retired
// stacks", folded, last (was the Retired page).
//
// Data: `/data/backups/<stack>` (repositories, sizes, drill verdicts, every
// snapshot — the host's snapshot cache, fix-180) and
// `/data/backup-calendar?stack=<stack>` (which stacks keep no data by
// design, and each stack's snapshot nights), one stack at a time so a slow
// repository holds up only its own row (fix-177, fix-224); `/data/retired`.
// Restore, Verify restore (fix-237) and the drills are the existing action
// dialogs; "Show a file…" (fix-241) is the existing read-only dialog.
//
// The shared pieces (header, KPI strip, attention band, cards, hover card,
// sort headers, filter chips) are ui.js's; the page's root carries
// `bk-page`, under which app.css measures them the way the demo does.

import { fetchJson, h, slowRead, tableBlock } from "../dom.js";
import {
  formatClock,
  formatDateTime,
  formatDay,
  humanDuration,
} from "../format.js";
import { openAction, openBatch } from "../actiondialog.js";
import { openDialog } from "../actui.js";
import { snapshotPickerRows } from "../snapshotpicker.js";
import { restoreHref } from "../restoreflow.js";
import { snapshotFileUrl, snapshotFileView } from "../snapshotfile.js";
import {
  declare,
  declareField,
  drivable,
  fieldId,
  viaForm,
} from "../drivable.js";
import { stackHref } from "../router.js";
import { current, subscribe } from "../store.js";
import { setParams } from "../urlstate.js";
import { stackHues } from "../topology.js";
import { RETIRED_COLUMNS, retiredRow } from "./retired.js";
import { attachDataTables } from "/static/kp/js/datatable.js";
import {
  allRepos,
  cellState,
  drillState,
  drillWords,
  fleetNight,
  humanBytes,
  kpis,
  nightRange,
  nightsNow,
  offsetFor,
  repoGroups,
  searchFromView,
  stackNight,
  stackWhy,
  ticks,
  viewFromSearch,
} from "../backupsview.js";
import {
  countOf,
  attentionBand,
  button,
  filterChip,
  hoverCard,
  hueColour,
  kbd,
  kpiStrip,
  PHONE,
  pageHeader,
  section,
  skeleton,
  sortHead,
  swatch,
} from "../ui.js";

/** A stack's colour square, in the topology's hue. @param {number} hue */
const stackMark = (hue) => swatch(hueColour(hue));

// review M5: every page field Live view may set is declared (drivable.js
// `declareField`); the client and the dashboard refuse any other.
const BK_FILTER = declareField({
  id: "bk-filter",
  page: "backups",
  what: "filter the repositories by stack or app",
});
const SNAPFILE = declareField({
  id: "snapfile",
  page: "backups",
  what: "the Show a file dialog: its snapshot (-snap) and path (-path)",
  row: "<stack>-<owner>-snap|path",
});

/**
 * @typedef {import("../backupsview.js").StackRead} StackRead
 * @typedef {import("../backupsview.js").Repo} Repo
 * @typedef {import("../backupsview.js").SortKey} SortKey
 */

// ── Live view (fix-239, invariant 39): every control of this page is
// reachable with `homelab ui click <control> [row]`. The dialogs that are
// catalog actions (Restore, Restore (native), Verify restore, Wipe) and the
// batch (Back up now) are reached through their forms instead.
const REFRESH = declare({
  id: "backups-refresh",
  page: "backups",
  opens: "view",
  what: "read every stack's repositories from the host again",
});
const RESTORE_PICK = declare({
  id: "backups-restore",
  page: "backups",
  opens: "view",
  what: "open the Restore flow: pick an app and a night, then restore it",
});
const DRILL_NOW = declare({
  id: "backups-drill-now",
  page: "backups",
  opens: "dialog",
  what: "list every repository never restore-drilled, each with Verify restore",
});
const UNDRILLED = declare({
  id: "backups-undrilled-filter",
  page: "backups",
  opens: "view",
  what: "show only the repositories never restore-drilled, or all again",
});
const NIGHT = declare({
  id: "backup-night",
  page: "backups",
  opens: "view",
  row: "<stack>/<YYYY-MM-DD>",
  what: "pin one stack's night of the coverage heatmap",
});
const FLEET_NIGHT = declare({
  id: "backup-fleet-night",
  page: "backups",
  opens: "view",
  row: "<YYYY-MM-DD>",
  what: "pin one night of the coverage heatmap from the whole-fleet row",
});
const UNPIN = declare({
  id: "backup-night-unpin",
  page: "backups",
  opens: "view",
  what: "unpin the night and show last night again",
  shows: "while a night is pinned",
  reach: [{ do: "click", control: "backup-fleet-night", row: "*" }],
});
const EARLIER = declare({
  id: "backup-nights-earlier",
  page: "backups",
  opens: "view",
  what: "show the nights before the ones on screen",
});
const LATER = declare({
  id: "backup-nights-later",
  page: "backups",
  opens: "view",
  what: "show the nights after the ones on screen",
  shows: "once the strip shows earlier nights",
  reach: [{ do: "click", control: "backup-nights-earlier" }],
});
const TODAY = declare({
  id: "backup-nights-today",
  page: "backups",
  opens: "view",
  what: "go back to the latest nights and unpin",
});
const STACK_FILTER = declare({
  id: "backup-stack-filter",
  page: "backups",
  opens: "view",
  row: "<stack>",
  what: "turn one stack on or off in the heatmap and the table",
});
const CLEAR = declare({
  id: "backup-clear-filter",
  page: "backups",
  opens: "view",
  row: "<filter>",
  what: "turn one active filter off (stack:<name>, text, drills, sort, all)",
  shows: "while a filter is on",
  reach: [{ do: "click", control: "backups-undrilled-filter" }],
});
const FOLD = declare({
  id: "backup-repo-group",
  page: "backups",
  opens: "view",
  row: "<stack>",
  what: "fold or unfold one stack's repositories",
});
const OPEN_REPO = declare({
  id: "backup-repo",
  page: "backups",
  opens: "view",
  row: "<stack>/<app>",
  what: "show or hide every snapshot of one repository",
});
const SORT = declare({
  id: "backup-repo-sort",
  page: "backups",
  opens: "view",
  row: "<app|age|snaps|size|drill>",
  what: "sort the repositories by a column (again: descending, then none)",
});
const RETRY = declare({
  id: "backup-stack-retry",
  page: "backups",
  opens: "view",
  row: "<stack>",
  what: "read one stack's repositories again after its read failed",
  shows: "on a stack whose repositories could not be read",
});
// fix-239/241: "Show a file…" opens a page dialog (no catalog action).
const SNAPSHOT_FILE = declare({
  id: "snapshot-file",
  page: "backups",
  opens: "dialog",
  row: "<stack>/<app>",
  what: "read one file of a snapshot without restoring anything",
});

/** A read that never settles is given up after this long (fix-224). */
const UNREAD_GIVE_UP_MS = 180_000;

const nowS = () => Math.floor(Date.now() / 1000);

/** @param {number} t unix seconds @returns {string} local HH:MM */
const hhmm = (t) => formatClock(t);

/**
 * A night (YYYY-MM-DD, a civil date) as every page writes a day: "Sat 3
 * Oct" (redesign-final X4: one date format).
 * @param {string} n
 */
const shortDate = (n) =>
  formatDay(Date.parse(`${n}T12:00:00Z`) / 1000, { timeZone: "UTC" });
const longDate = shortDate;

/**
 * The restore action a stack's repositories need.
 * @param {boolean} native
 */
const restoreKind = (native) => (native ? "restore-native" : "restore");

/**
 * What a restore opened from a known repository and snapshot already
 * knows (fix-216: the dialog never asks again).
 * @param {boolean} native
 * @param {string} app
 * @param {string} snapshot short id
 * @returns {Record<string, string>}
 */
const restorePreset = (native, app, snapshot) =>
  native ? { snapshot } : { app, snapshot };

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const fromUrl = viewFromSearch(location.search);
  const S = {
    /** @type {string | null} */ night: fromUrl.night,
    offset: 0,
    stacks: fromUrl.stacks,
    undrilled: fromUrl.undrilled,
    q: fromUrl.q,
    /** @type {SortKey[]} */ sort: fromUrl.sort,
    /** @type {Set<string>} */ collapsed: new Set(),
    /** @type {string | null} */ open: null,
  };
  /** @type {string[]} */
  let names = [];
  /** @type {Record<string, StackRead>} */
  let reads = {};
  /** @type {Record<string, number>} */
  const startedAt = {};
  /** @type {any[] | null} */
  let retired = null;
  /** @type {string | null} */
  let retiredError = null;
  /** @type {number | null} */
  let readAt = null;
  const abort = new AbortController();
  const tip = hoverCard();
  root.classList.add("bk-page");
  /** @type {(() => void)[]} */
  const cleanups = [() => tip.stop(), () => root.classList.remove("bk-page")];
  const N = () => (matchMedia(PHONE).matches ? 14 : 30);
  /** @type {Map<string, number>} */
  let hues = new Map();
  const hueOf = (/** @type {string} */ s) => hues.get(s) ?? 210;

  const writeUrl = () => {
    const search = setParams(location.search, searchFromView(S));
    if (search !== location.search)
      history.replaceState(history.state, "", location.pathname + search);
  };

  // ── header ────────────────────────────────────────────────────────────
  const refreshBtn = drivable(
    button("Refresh", {
      cls: "kp-button--secondary",
      title: "Read every stack's repositories from the host again",
      onClick: () => void loadAll(true),
    }),
    REFRESH,
  );
  const backupBtn = viaForm(
    button("Back up now…", {
      cls: "kp-button--secondary",
      title:
        "Run a backup now of every stack that keeps data (or only the stacks you turned on below); the dialog lists them first",
      onClick: () => {
        const want = names.filter((n) => {
          const r = reads[n];
          return (
            r?.status === "ok" &&
            !r.noBackup &&
            (!S.stacks.size || S.stacks.has(n))
          );
        });
        void openBatch("backup", want);
      },
    }),
    "batch",
  );
  const restoreBtn = drivable(
    button("Restore…", {
      cls: "kp-button--primary",
      title: "Open the Restore flow: pick an app and a night, then restore it",
      onClick: () => openRestorePicker(),
    }),
    RESTORE_PICK,
  );
  const head = pageHeader({
    title: "Backups",
    desc: "Every night each stack that keeps data writes a restic snapshot. See which nights are covered, open any repository, and bring data back with Restore.",
    live: "read",
    actions: [refreshBtn, backupBtn],
    primary: restoreBtn,
  });

  // ── KPI strip ─────────────────────────────────────────────────────────
  const strip = kpiStrip(
    [
      { key: "last", label: "Last night" },
      { key: "newest", label: "Newest snapshot" },
      { key: "repos", label: "Repositories" },
      {
        key: "drills",
        label: "Restore drills",
        title:
          "Click to show only the repositories never drilled in the table; click again to show all",
        toggle: {
          pressed: S.undrilled,
          onToggle: () => {
            S.undrilled = !S.undrilled;
            writeUrl();
            paintKpis();
            paintRepos();
          },
          drive: { id: UNDRILLED },
        },
      },
      { key: "retired", label: "Kept from retired" },
    ],
    { loading: true },
  );
  const tile = (/** @type {string} */ k) =>
    /** @type {ReturnType<typeof import("../ui.js").kpi>} */ (
      strip.tiles.get(k)
    );

  const band = attentionBand([]);

  // ── coverage heatmap + night detail ───────────────────────────────────
  const heat = h("div", {
    class: "bk-heat",
    role: "grid",
    "aria-label": "Snapshots per stack per night",
    "aria-describedby": "bk-cov-keys",
  });
  const range = h("span", { class: "bk-range" });
  const covActive = h("div", { class: "bk-active", "aria-live": "polite" });
  const earlier = drivable(
    button("‹ Earlier", {
      cls: "kp-button--sm kp-button--ghost",
      onClick: () => {
        S.offset += N();
        paintHeat();
      },
    }),
    EARLIER,
  );
  const later = drivable(
    button("Later ›", {
      cls: "kp-button--sm kp-button--ghost",
      onClick: () => {
        S.offset = Math.max(0, S.offset - N());
        paintHeat();
      },
    }),
    LATER,
  );
  const today = drivable(
    button("Today", {
      cls: "kp-button--sm kp-button--secondary",
      title: "Back to the latest nights and unpin (Home)",
      onClick: () => {
        S.offset = 0;
        pin(null);
      },
    }),
    TODAY,
  );
  const legendSwatch = (/** @type {string} */ cls, /** @type {string} */ w) =>
    h("span", null, h("i", { class: `bk-legend__sw ${cls}` }), w);
  const legend = h(
    "div",
    { class: "bk-legend", id: "bk-cov-keys" },
    legendSwatch("bk-legend__ok", "backed up"),
    legendSwatch("bk-legend__miss", "missed"),
    legendSwatch("bk-legend__before", "before any history"),
    legendSwatch("bk-legend__none", "keeps no data by design"),
    h(
      "span",
      { class: "bk-legend__keys" },
      kbd("← → ↑ ↓"),
      " move · ",
      kbd("Enter"),
      " pin · ",
      kbd("Esc"),
      " unpin · ",
      kbd("PgUp"),
      " earlier",
    ),
  );
  const cov = section({
    id: "bk-coverage",
    title: "Nightly coverage",
    desc: "Each square is one stack on one night, filled when it made a snapshot. Hover for the details, click to pin a night.",
    tools: [earlier, range, later, today],
    cls: "bk-span-8",
  });
  cov.body.append(covActive, heat, legend);
  const detail = h("div", { class: "bk-night", id: "bk-night" });
  const side = section({
    id: "bk-night-card",
    tag: "aside",
    title: "Night detail",
    desc: "Which stack wrote its snapshot that night, and when; restore any of them from here.",
    cls: "bk-span-4",
  });
  side.body.append(detail);
  const grid = h("div", { class: "bk-grid" }, cov.el, side.el);
  let focusCell = { r: 0, c: N() - 1 };

  // ── repositories ──────────────────────────────────────────────────────
  const search = /** @type {HTMLInputElement} */ (
    h("input", {
      class: "bk-search nx-search",
      id: BK_FILTER,
      name: "bk-filter",
      type: "search",
      placeholder: "Filter stacks or apps",
      "aria-label": "Filter repositories by stack or app",
      autocomplete: "off",
    })
  );
  search.value = S.q;
  search.addEventListener("input", () => {
    S.q = search.value;
    writeUrl();
    paintRepos();
  });
  search.addEventListener("keydown", (/** @type {KeyboardEvent} */ e) => {
    if (e.key === "Escape" && search.value) {
      e.stopPropagation();
      search.value = "";
      S.q = "";
      writeUrl();
      paintRepos();
    }
  });
  const repoActive = h("div", { class: "bk-active", "aria-live": "polite" });
  const repoBox = h("div", { class: "bk-table-wrap" });
  const repos = section({
    id: "bk-repos",
    title: "Repositories",
    desc: "One restic repository per app that keeps data (a native service has its own), grouped by stack. The ticks show the last 7 nights.",
    // The `/` hint sits inside the box, so the two never part.
    tools: [
      h("div", { class: "nx-tb__search bk-searchbox" }, search, kbd("/")),
    ],
  });
  repos.body.append(repoActive, repoBox);
  const readState = h("span", { class: "bk-readstate" });
  repos.foot.hidden = false;
  repos.foot.append(
    readState,
    h(
      "span",
      null,
      kbd("j"),
      " ",
      kbd("k"),
      " move · ",
      kbd("Enter"),
      " open · ",
      kbd("/"),
      " filter",
    ),
  );

  // ── kept from retired stacks ──────────────────────────────────────────
  const retiredCount = h("span", { class: "bk-count" }, "reading…");
  const ret = section({
    id: "bk-retired",
    title: "Kept from retired stacks",
    desc: "When a stack, app or native unit leaves the files, its backups, /appdata and vault copies stay here until you wipe them (was the Retired page).",
    collapsible: true,
    open: fromUrl.section === "removed",
    badge: retiredCount,
  });

  root.replaceChildren(head.el, strip.el, band.el, grid, repos.el, ret.el);

  // ── painting ──────────────────────────────────────────────────────────
  const paintLive = () => {
    const done = names.filter((n) => reads[n]?.status !== "pending").length;
    const failed = names.filter((n) => reads[n]?.status === "failed");
    if (!current().fleet)
      head.live?.set(null, "waiting for the host's report of the fleet…");
    else if (done < names.length)
      head.live?.set(null, `reading ${done} of ${names.length} stacks…`);
    else head.live?.set(readAt);
    readState.textContent = !current().fleet
      ? "Waiting for the host's report of the fleet…"
      : `Read from the host's snapshot cache · ${done - failed.length} of ${names.length} stacks answered${failed.length ? ` · ${failed.join(", ")} could not be read` : ""}`;
    // redesign-final (exact counts): "N of M stacks answered" is the
    // coverage rows of the stacks that answered.
    countOf(readState, heat, '.bk-heat__label[data-read="ok"]');
    readState.dataset.bkRead =
      names.length > 0 && done === names.length ? "all" : "some";
  };

  const paintKpis = () => {
    const k = kpis(reads, nowS(), retired ? retired.length : null);
    const loading = !current().fleet || !k.settled;
    for (const t of strip.tiles.values()) t.set({ loading });
    if (loading) {
      tile("drills").set({
        toggle: { ...tileToggle(), pressed: S.undrilled },
      });
      return;
    }
    const ln = k.lastNight;
    tile("last").set({
      value: ln.expected ? String(ln.covered) : "—",
      unit: ln.expected ? `/ ${ln.expected}` : "",
      ctx: !ln.expected
        ? "no stack keeps data"
        : ln.missed.length
          ? `${ln.missed.length} missed: ${ln.missed.join(", ")}`
          : "every stack covered",
      ctxTone: !ln.expected ? null : ln.missed.length ? "bad" : "ok",
      tone: ln.missed.length ? "bad" : null,
      title: `The night of ${longDate(ln.night)}`,
    });
    tile("newest").set(
      k.newest
        ? {
            value: `${humanDuration(nowS() - k.newest.time)} ago`,
            ctx: k.newest.where,
            title: formatDateTime(k.newest.time),
          }
        : { value: "—", ctx: "no snapshot yet" },
    );
    const rp = k.repositories;
    tile("repos").set({
      value: String(rp.count),
      ctx: `${rp.snapshots} snapshots · ${rp.stacks} ${rp.stacks === 1 ? "stack" : "stacks"}`,
    });
    const d = k.drills;
    tile("drills").set({
      value: String(d.passed),
      unit: `/ ${d.total}`,
      ctx: d.failed
        ? `${d.failed} failed${d.never ? ` · ${d.never} never drilled` : ""}`
        : d.never
          ? `${d.never} never drilled`
          : d.total
            ? "every repository passed"
            : "no repository yet",
      ctxTone: d.failed ? "bad" : d.never ? "warn" : d.total ? "ok" : null,
      tone: d.failed ? "bad" : d.passed < d.total ? "warn" : null,
      toggle: { ...tileToggle(), pressed: S.undrilled },
    });
    tile("retired").set(
      retired
        ? {
            value: String(retired.length),
            ctx: retired.length
              ? `${retired.length} waiting to be wiped`
              : "nothing waiting to be wiped",
          }
        : retiredError
          ? { value: "—", ctx: "could not be read" }
          : { loading: true },
    );
    paintBand(k);
  };
  const tileToggle = () => ({
    pressed: S.undrilled,
    onToggle: () => {
      S.undrilled = !S.undrilled;
      writeUrl();
      paintKpis();
      paintRepos();
    },
  });

  /** @param {ReturnType<typeof kpis>} k */
  const paintBand = (k) => {
    /** @type {import("../ui.js").Attention[]} */
    const items = [];
    if (k.lastNight.missed.length)
      items.push({
        key: "missed",
        tone: "bad",
        title: `${k.lastNight.missed.length} of ${k.lastNight.expected} stacks made no snapshot last night`,
        text: `${k.lastNight.missed.join(", ")} — pin the night in the coverage below to see each one, or back them up now.`,
      });
    if (k.drills.never)
      items.push({
        key: "drills",
        tone: "warn",
        title: `${k.drills.never} of ${k.drills.total} repositories were never restore-drilled`,
        text: "A drill restores the newest snapshot into a scratch directory and compares it; nothing live is touched.",
        action: drivable(
          button("Drill them now…", {
            cls: "kp-button--sm kp-button--secondary",
            title:
              "List every repository that never had a drill, each with its own Verify restore",
            onClick: () => openDrills(),
          }),
          DRILL_NOW,
        ),
      });
    band.set(items);
  };

  /** @param {string} s @param {string} night */
  const cellTip = (s, night) => {
    const r = reads[s];
    if (!r) return null;
    const n = stackNight(r, night, nightsNow(nowS()));
    /** @type {Node[]} */
    const out = [h("b", null, `${s} · ${shortDate(night)}`)];
    const dot = (/** @type {string} */ tone, /** @type {string} */ w) =>
      h("span", { class: `nx-dot nx-dot--${tone}` }, w);
    if (n.state === "ok")
      out.push(
        dot("ok", `snapshot at ${n.times.map(hhmm).join(", ")}`),
        ...(n.ids.length
          ? [
              h(
                "span",
                { class: "bk-tip__ids mono" },
                n.ids.map((x) => `${x.owner} ${x.short_id}`).join(" · "),
              ),
            ]
          : []),
        h(
          "span",
          { class: "bk-hint" },
          "Click to pin this night · Enter on the keyboard",
        ),
      );
    else if (n.state === "miss") out.push(dot("bad", "no snapshot this night"));
    else if (n.state === "none")
      out.push(h("span", null, "keeps no data by design"));
    else if (n.state === "before")
      out.push(h("span", null, "before this stack had any backup history"));
    else if (n.state === "wait")
      out.push(h("span", null, "tonight's round has not run yet"));
    else if (n.state === "unread")
      out.push(
        dot(
          "bad",
          r.status === "failed" ? `not read: ${r.reason}` : "not read",
        ),
      );
    else out.push(h("span", null, "reading…"));
    return out;
  };

  const paintHeat = () => {
    tip.hide();
    const n = N();
    const nn = nightsNow(nowS());
    const nights = nightRange(nn.current, n, S.offset);
    heat.style.setProperty("--n", String(n));
    const rows = names.filter((s) => !S.stacks.size || S.stacks.has(s));
    if (focusCell.c >= n) focusCell = { r: focusCell.r, c: n - 1 };
    if (focusCell.r >= rows.length) focusCell = { r: 0, c: focusCell.c };
    const fleetRow = h("div", { class: "bk-heat__row bk-heat__fleet" });
    for (const night of nights) {
      const f = fleetNight(reads, night, nn);
      const loading =
        !current().fleet || names.some((s) => reads[s]?.status === "pending");
      const cls = [
        "bk-heat__cell",
        loading
          ? "bk-heat__cell--load"
          : f.before
            ? "bk-heat__cell--before"
            : f.wait
              ? "bk-heat__cell--wait"
              : f.expected && f.ok === 0
                ? "bk-heat__cell--miss"
                : f.pct < 100 && f.expected
                  ? "bk-heat__cell--warn"
                  : "",
        night === S.night ? "is-col" : "",
      ]
        .filter(Boolean)
        .join(" ");
      const b = drivable(
        h(
          "button",
          {
            type: "button",
            class: cls,
            tabindex: "-1",
            style: `--r: ${f.pct}%`,
            "aria-label": `${night}: ${f.ok} of ${f.expected} stacks backed up`,
          },
          h("i"),
        ),
        FLEET_NIGHT,
        night,
      );
      b.addEventListener("click", () => pin(night));
      tip.attach(b, () => [
        h("b", null, shortDate(night)),
        f.before
          ? h("span", null, "before any backup history")
          : f.wait
            ? h("span", null, "tonight's round has not run yet")
            : h(
                "span",
                {
                  class: `nx-dot nx-dot--${f.ok === f.expected ? "ok" : f.ok ? "warn" : "bad"}`,
                },
                `${f.ok} of ${f.expected} stacks backed up`,
              ),
        h("span", { class: "bk-hint" }, "Click to pin this night"),
      ]);
      fleetRow.append(b);
    }
    /** @type {Node[]} */
    const body = [
      h(
        "span",
        { class: "bk-heat__label bk-heat__label--fleet" },
        h("span", null, "Whole fleet"),
      ),
      fleetRow,
      h("div", { class: "bk-heat__sep" }),
    ];
    rows.forEach((s, ri) => {
      const r = reads[s] ?? { status: "pending" };
      const label = drivable(
        h(
          "button",
          {
            type: "button",
            class: "bk-heat__label",
            "data-stack": s,
            "data-read": r.status,
            "aria-pressed": String(S.stacks.has(s)),
            title: `Click to show only ${s} (each click turns a stack on or off; Show every stack resets)`,
          },
          stackMark(hueOf(s)),
          h(
            "span",
            { class: r.status === "ok" && r.noBackup ? "bk-muted" : "" },
            s,
          ),
        ),
        STACK_FILTER,
        s,
      );
      label.addEventListener("click", () => toggleStack(s));
      const row = h("div", { class: "bk-heat__row", role: "row" });
      nights.forEach((night, ci) => {
        const st = cellState(r, night, nn);
        const cell = drivable(
          h("button", {
            type: "button",
            class: `bk-heat__cell bk-heat__cell--${st}${night === S.night ? " is-col" : ""}`,
            role: "gridcell",
            tabindex: ri === focusCell.r && ci === focusCell.c ? "0" : "-1",
            "data-r": String(ri),
            "data-c": String(ci),
            "aria-label": `${s}, ${shortDate(night)}: ${st === "ok" ? "backed up" : st === "miss" ? "missed" : st}`,
          }),
          NIGHT,
          `${s}/${night}`,
        );
        cell.addEventListener("click", () => {
          focusCell = { r: ri, c: ci };
          pin(night);
        });
        tip.attach(cell, () => cellTip(s, night));
        row.append(cell);
      });
      body.push(label, row);
    });
    body.push(
      h("span"),
      h(
        "div",
        { class: "bk-heat__axis", "aria-hidden": "true" },
        ...nights.map((night, i) =>
          h(
            "span",
            null,
            i % 3 === 0 || i === n - 1 ? String(Number(night.slice(8))) : "",
          ),
        ),
      ),
    );
    heat.replaceChildren(...body);
    range.textContent = `${shortDate(nights[0])} – ${shortDate(nights[n - 1])}`;
    later.toggleAttribute("disabled", S.offset === 0);
    if (S.stacks.size) {
      const all = drivable(
        h(
          "button",
          { type: "button", class: "bk-linkbtn" },
          "Show every stack",
        ),
        CLEAR,
        "all-stacks",
      );
      all.addEventListener("click", () => {
        S.stacks.clear();
        writeUrl();
        paintAll();
      });
      covActive.replaceChildren(
        h("span", null, "Showing"),
        ...[...S.stacks].sort().map((s) =>
          filterChip(s, () => toggleStack(s), {
            id: CLEAR,
            row: `stack:${s}`,
          }),
        ),
        all,
      );
    } else
      covActive.replaceChildren(
        h(
          "span",
          null,
          "Click stack names to show only them; each click turns one on or off.",
        ),
      );
  };

  /** @param {string} s */
  const toggleStack = (s) => {
    if (S.stacks.has(s)) S.stacks.delete(s);
    else S.stacks.add(s);
    writeUrl();
    paintAll();
  };

  // keyboard: roving focus in the grid (DESIGN_LANGUAGE §10, calendars)
  heat.addEventListener("keydown", (/** @type {KeyboardEvent} */ e) => {
    const c = /** @type {HTMLElement | null} */ (
      /** @type {HTMLElement} */ (e.target).closest(".bk-heat__cell[data-r]")
    );
    if (!c) return;
    let r = Number(c.dataset.r);
    let col = Number(c.dataset.c);
    const rowsN = heat.querySelectorAll(".bk-heat__row[role=row]").length;
    const n = N();
    if (e.key === "ArrowRight") col = Math.min(n - 1, col + 1);
    else if (e.key === "ArrowLeft") col = Math.max(0, col - 1);
    else if (e.key === "ArrowDown") r = Math.min(rowsN - 1, r + 1);
    else if (e.key === "ArrowUp") r = Math.max(0, r - 1);
    else if (e.key === "PageUp" || e.key === "PageDown" || e.key === "Home") {
      e.preventDefault();
      if (e.key === "PageUp") S.offset += n;
      else if (e.key === "PageDown") S.offset = Math.max(0, S.offset - n);
      else {
        S.offset = 0;
        pin(null);
      }
      paintHeat();
      focusAt();
      return;
    } else if (e.key === "Escape") {
      e.preventDefault();
      pin(null);
      focusAt();
      return;
    } else return;
    e.preventDefault();
    focusCell = { r, c: col };
    focusAt();
  });
  const focusAt = () => {
    heat
      .querySelectorAll(".bk-heat__cell[data-r]")
      .forEach((x) =>
        /** @type {HTMLElement} */ (x).setAttribute("tabindex", "-1"),
      );
    const el = /** @type {HTMLElement | null} */ (
      heat.querySelector(
        `.bk-heat__cell[data-r="${focusCell.r}"][data-c="${focusCell.c}"]`,
      )
    );
    if (!el) return;
    el.setAttribute("tabindex", "0");
    el.focus();
  };

  const paintDetail = () => {
    const nn = nightsNow(nowS());
    const n = S.night ?? nn.last;
    const settled =
      names.length > 0 &&
      names.every((s) => reads[s] && reads[s].status !== "pending");
    const expected = names.filter((s) =>
      ["ok", "miss"].includes(
        cellState(reads[s] ?? { status: "pending" }, n, nn),
      ),
    );
    const ok = expected.filter(
      (s) => cellState(reads[s] ?? { status: "pending" }, n, nn) === "ok",
    );
    const unpin = S.night
      ? drivable(
          button("Unpin ×", {
            cls: "kp-button--sm kp-button--ghost",
            title: "Unpin and go back to last night (Esc)",
            onClick: () => pin(null),
          }),
          UNPIN,
        )
      : h("span");
    detail.replaceChildren(
      h(
        "div",
        { class: "bk-night__top" },
        h(
          "div",
          null,
          h(
            "div",
            { class: "bk-hint bk-night__label" },
            S.night ? "Pinned night" : "Last night (click any night to pin it)",
          ),
          h("div", { class: "bk-night__date" }, longDate(n)),
          h(
            "div",
            { class: "bk-night__big" },
            settled ? `${ok.length}` : skeleton("2ch"),
            h(
              "small",
              null,
              ` of ${settled ? expected.length : "…"} stacks backed up`,
            ),
          ),
        ),
        unpin,
      ),
      h(
        "ul",
        null,
        ...names.map((s) => {
          const r = reads[s] ?? { status: "pending" };
          const sn = stackNight(r, n, nn);
          /** @type {Node} */
          let state;
          if (sn.state === "load") state = skeleton("6ch");
          else if (sn.state === "ok")
            state = h(
              "span",
              { class: "nx-dot nx-dot--ok mono" },
              sn.times.length > 1
                ? `${hhmm(sn.times[0])}–${hhmm(sn.times[sn.times.length - 1])}`
                : hhmm(sn.times[0]),
            );
          else if (sn.state === "none")
            state = h("span", { class: "bk-muted" }, "keeps no data");
          else if (sn.state === "before")
            state = h("span", { class: "bk-muted" }, "no history yet");
          else if (sn.state === "wait")
            state = h("span", { class: "bk-muted" }, "not run yet");
          else if (sn.state === "unread")
            state = h("span", { class: "nx-dot nx-dot--bad" }, "not read");
          else state = h("span", { class: "nx-dot nx-dot--bad" }, "missed");
          /** @type {Node} */
          let act = h("span");
          if (sn.state === "ok" && r.status === "ok" && r.repos.length) {
            const one = r.native || r.repos.length === 1;
            const first = sn.ids[0];
            act = viaForm(
              button("Restore…", {
                cls: "kp-button--sm kp-button--ghost",
                title: `Restore ${s} to the snapshot of this night`,
                attrs: { "data-action": restoreKind(r.native) },
                onClick: () =>
                  void openAction(s, restoreKind(r.native), {
                    preset:
                      one && first
                        ? restorePreset(r.native, first.owner, first.short_id)
                        : {},
                  }),
              }),
              restoreKind(r.native),
            );
          }
          return h(
            "li",
            { "data-stack": s },
            stackMark(hueOf(s)),
            h("span", null, s),
            state,
            act,
          );
        }),
      ),
    );
  };

  /** @param {string | null} n */
  const pin = (n) => {
    S.night = n;
    if (n) {
      const off = offsetFor(nightsNow(nowS()).current, n, N());
      if (S.offset > off || off >= S.offset + N()) S.offset = off;
    }
    writeUrl();
    paintHeat();
    paintDetail();
  };

  // ── repositories table ────────────────────────────────────────────────
  /** @param {Repo & {stack: string, native: boolean}} r */
  const drillTip = (r) => {
    const d = drillState(r);
    return [
      h("b", null, "Restore drill"),
      h(
        "span",
        null,
        d === "never"
          ? "This repository was never restored into scratch to prove it works."
          : d === "failed"
            ? `Last drill ${formatDateTime(r.drill?.last_attempt)} failed: ${r.drill?.last_error}`
            : `Last passed ${formatDateTime(r.drill?.last_pass)}.`,
      ),
      h(
        "span",
        { class: "bk-hint" },
        "The nightly round drills one repository in turn, the one drilled longest ago.",
      ),
    ];
  };

  const paintRepos = () => {
    const now = nowS();
    const last = nightsNow(now).last;
    const groups = repoGroups(
      Object.fromEntries(
        names.map((n) => [n, reads[n] ?? { status: "pending" }]),
      ),
      S,
      now,
    );
    const tbody = h("tbody");
    for (const g of groups) {
      const s = g.stack;
      const r = g.read;
      const collapsed = S.collapsed.has(s);
      const native = r.status === "ok" && r.native;
      const groupRestore =
        r.status === "ok" && g.total
          ? viaForm(
              button("Restore…", {
                cls: "kp-button--sm kp-button--ghost",
                title: `Restore one of ${s}'s apps`,
                attrs: { "data-action": restoreKind(native) },
                onClick: () => void openAction(s, restoreKind(native)),
              }),
              restoreKind(native),
            )
          : r.status === "failed"
            ? drivable(
                button("Retry", {
                  cls: "kp-button--sm kp-button--secondary",
                  title: `Read ${s}'s repositories again`,
                  onClick: () => void retryOne(s),
                }),
                RETRY,
                s,
              )
            : h("span");
      const gtr = drivable(
        h(
          "tr",
          {
            class: "bk-repo-group",
            tabindex: "0",
            "data-stack": s,
            ...(g.total
              ? {
                  "aria-expanded": String(!collapsed),
                  title: "Click to fold or unfold this stack",
                }
              : {}),
          },
          h(
            "td",
            null,
            h(
              "span",
              { class: "bk-group-id" },
              g.total
                ? h("span", { class: "bk-caret" })
                : h("span", { class: "bk-caret-pad" }),
              stackMark(hueOf(s)),
              h("a", { href: stackHref(s), title: `Open ${s}'s page` }, s),
            ),
          ),
          h(
            "td",
            { class: "bk-meta", colspan: "5" },
            r.status === "pending" ? skeleton("30%") : stackWhy(r),
          ),
          h("td", { class: "bk-row-actions" }, groupRestore),
        ),
        FOLD,
        s,
      );
      gtr.addEventListener("click", (/** @type {MouseEvent} */ e) => {
        if (/** @type {HTMLElement} */ (e.target).closest("button, a, input"))
          return;
        if (!g.total) return;
        if (collapsed) S.collapsed.delete(s);
        else S.collapsed.add(s);
        paintRepos();
      });
      tbody.append(gtr);
      if (r.status === "pending") {
        tbody.append(
          h(
            "tr",
            { class: "bk-repo bk-repo--load" },
            ...[30, 40, 50, 20, 30, 50, 40].map((w) =>
              h("td", null, skeleton(`${w}%`)),
            ),
          ),
        );
        continue;
      }
      if (collapsed) continue;
      for (const repo of g.rows) {
        const key = `${s}/${repo.owner}`;
        const open = S.open === key;
        const newest = repo.newest_snapshot;
        const fresh = newest != null && now - newest.time < 30 * 3600;
        const d = drillState(repo);
        const size = humanBytes(repo.size_bytes);
        const full = { ...repo, stack: s, native };
        const tr = drivable(
          h(
            "tr",
            {
              class: "bk-repo",
              tabindex: "0",
              "aria-expanded": String(open),
              title: "Click to see every snapshot",
              "data-kp-row-key": `${s}-${repo.owner}`,
            },
            h("td", { class: "mono bk-app" }, repo.owner),
            h(
              "td",
              { "data-label": "Newest" },
              newest
                ? h("span", { class: "bk-idchip mono" }, newest.short_id)
                : h("span", { class: "bk-muted" }, repo.error ?? "—"),
            ),
            h(
              "td",
              { "data-label": "Age" },
              newest
                ? h(
                    "span",
                    { class: `nx-dot nx-dot--${fresh ? "ok" : "bad"}` },
                    `${humanDuration(now - newest.time)} ago`,
                  )
                : h("span", { class: "bk-muted" }, "—"),
            ),
            h(
              "td",
              { class: "bk-num", "data-label": "Snapshots" },
              `${repo.snapshot_count}`,
              h(
                "span",
                { class: "bk-ticks", "aria-hidden": "true" },
                ...ticks(repo, last).map((t) =>
                  h("i", t ? null : { class: "gap" }),
                ),
              ),
            ),
            h(
              "td",
              { class: "bk-num", "data-label": "Size" },
              size ??
                h(
                  "span",
                  {
                    class: "bk-muted",
                    title: "The host could not measure this repository's size",
                  },
                  "—",
                ),
            ),
            h(
              "td",
              { "data-label": "Restore drill" },
              h(
                "span",
                {
                  class: `nx-dot nx-dot--${d === "never" ? "warn" : d === "failed" ? "bad" : "ok"} bk-drill`,
                  tabindex: "0",
                },
                drillWords(d),
              ),
            ),
            h(
              "td",
              { class: "bk-row-actions" },
              ...rowActions(s, native, repo),
            ),
          ),
          OPEN_REPO,
          key,
        );
        if (newest) {
          const chip = /** @type {HTMLElement} */ (tr.children[1].firstChild);
          chip.tabIndex = 0;
          tip.attach(chip, () => [
            h("b", null, `snapshot ${newest.short_id}`),
            h("span", { class: "mono" }, newest.id),
            h("span", null, formatDateTime(newest.time)),
          ]);
        }
        tip.attach(
          /** @type {HTMLElement} */ (tr.querySelector(".bk-drill")),
          () => drillTip(full),
        );
        tr.addEventListener("click", (/** @type {MouseEvent} */ e) => {
          if (/** @type {HTMLElement} */ (e.target).closest("button, a, input"))
            return;
          S.open = open ? null : key;
          paintRepos();
        });
        tbody.append(tr);
        if (open)
          tbody.append(
            h(
              "tr",
              { class: "bk-snaps" },
              h(
                "td",
                { colspan: "7" },
                h(
                  "div",
                  { class: "bk-hint bk-snaps__head" },
                  `All ${repo.snapshot_count} snapshots of ${key}, newest first`,
                ),
                h(
                  "ol",
                  null,
                  ...(repo.snapshots ?? []).map((x) =>
                    h(
                      "li",
                      null,
                      h("span", { class: "bk-idchip mono" }, x.short_id),
                      h(
                        "span",
                        null,
                        `${formatDateTime(x.time)} · ${humanDuration(now - x.time)} ago`,
                      ),
                      viaForm(
                        button("Restore", {
                          cls: "kp-button--sm kp-button--ghost",
                          title: `Restore ${repo.owner} to ${x.short_id}`,
                          attrs: { "data-action": restoreKind(native) },
                          onClick: () =>
                            void openAction(s, restoreKind(native), {
                              preset: restorePreset(
                                native,
                                repo.owner,
                                x.short_id,
                              ),
                            }),
                        }),
                        restoreKind(native),
                      ),
                    ),
                  ),
                ),
              ),
            ),
          );
      }
    }
    if (!tbody.children.length) {
      const clear = drivable(
        h(
          "button",
          { type: "button", class: "bk-linkbtn" },
          "Clear the filters",
        ),
        CLEAR,
        "all",
      );
      clear.addEventListener("click", resetAll);
      tbody.append(
        h(
          "tr",
          null,
          h(
            "td",
            { colspan: "7" },
            h(
              "div",
              { class: "bk-empty" },
              h(
                "strong",
                null,
                names.length ? "No repository matches" : "No stack to show yet",
              ),
              ...(names.length ? [clear] : []),
            ),
          ),
        ),
      );
    }
    /** @param {string} label @param {SortKey["key"]} key @param {string} [cls] */
    const head = (label, key, cls) =>
      sortHead({
        label,
        key,
        sort: S.sort,
        onSort: (next) => {
          S.sort = /** @type {SortKey[]} */ (next);
          writeUrl();
          paintRepos();
        },
        cls,
        drive: { id: SORT, row: key },
      });
    repoBox.replaceChildren(
      h(
        "table",
        { class: "bk-table" },
        h(
          "thead",
          null,
          h(
            "tr",
            null,
            head("App", "app"),
            h("th", { scope: "col" }, "Newest"),
            head("Age", "age"),
            head("Snapshots", "snaps", "bk-num"),
            head("Size", "size", "bk-num"),
            head("Restore drill", "drill"),
            h("th", { scope: "col" }, h("span", { class: "nx-vh" }, "Actions")),
          ),
        ),
        tbody,
      ),
    );
    /** @type {Node[]} */
    const act = [];
    const chipOf = (
      /** @type {string} */ label,
      /** @type {string} */ row,
      /** @type {() => void} */ clear,
    ) => filterChip(label, clear, { id: CLEAR, row });
    if (S.q.trim())
      act.push(
        chipOf(`“${S.q.trim()}”`, "text", () => {
          S.q = "";
          search.value = "";
          writeUrl();
          paintRepos();
        }),
      );
    if (S.undrilled)
      act.push(
        chipOf("never drilled", "drills", () => {
          S.undrilled = false;
          writeUrl();
          paintAll();
        }),
      );
    for (const s of [...S.stacks].sort())
      act.push(chipOf(s, `stack:${s}`, () => toggleStack(s)));
    if (S.sort.length)
      act.push(
        chipOf(
          `sorted by ${S.sort.map((x) => x.key + (x.dir > 0 ? " ↑" : " ↓")).join(", ")}`,
          "sort",
          () => {
            S.sort = [];
            writeUrl();
            paintRepos();
          },
        ),
      );
    if (act.length) {
      const all = drivable(
        h("button", { type: "button", class: "bk-linkbtn" }, "Reset all"),
        CLEAR,
        "all",
      );
      all.addEventListener("click", resetAll);
      repoActive.replaceChildren(h("span", null, "Active:"), ...act, all);
    } else
      repoActive.replaceChildren(
        h(
          "span",
          null,
          "Click a header to sort; each header clicked is a further key (again reverses it, a third time takes it out) · click a stack to fold it · click a row for every snapshot",
        ),
      );
  };

  const resetAll = () => {
    S.q = "";
    search.value = "";
    S.undrilled = false;
    S.open = null;
    S.stacks.clear();
    S.sort = [];
    S.collapsed.clear();
    writeUrl();
    paintAll();
  };

  /**
   * Restore… (the row's own app and newest snapshot, fix-216), Verify
   * restore… (fix-237) and Show a file… (fix-241).
   * @param {string} stack
   * @param {boolean} native
   * @param {Repo} r
   * @returns {Node[]}
   */
  const rowActions = (stack, native, r) => {
    const newest = r.newest_snapshot;
    if (!newest) return [];
    const restore = viaForm(
      button("Restore…", {
        cls: "kp-button--sm kp-button--secondary",
        title:
          "Restore this app; the newest snapshot is picked, change it in the dialog",
        attrs: { "data-action": restoreKind(native) },
        onClick: () =>
          void openAction(stack, restoreKind(native), {
            preset: restorePreset(native, r.owner, newest.short_id),
          }),
      }),
      restoreKind(native),
    );
    const verify = viaForm(
      button("Verify restore…", {
        cls: "kp-button--sm kp-button--ghost",
        title:
          "Restore this snapshot into the drill's scratch directory to prove it restores; live data is not touched",
        attrs: { "data-action": "verify-restore" },
        onClick: () =>
          void openAction(stack, "verify-restore", {
            preset: { app: r.owner, snapshot: newest.short_id },
          }),
      }),
      "verify-restore",
    );
    /** @type {Node[]} */
    const out = [restore, verify];
    // A native unit's snapshot is one tar stream, not a tree of files, so
    // its row has no Show a file (restore is the way back for it).
    if (!native) {
      const file = drivable(
        button("Show a file…", {
          cls: "kp-button--sm kp-button--ghost",
          title:
            "Read one file as this snapshot holds it, without restoring anything",
          onClick: () => openFileDialog(stack, r),
        }),
        SNAPSHOT_FILE,
        `${stack}/${r.owner}`,
      );
      out.push(file);
    }
    return [h("span", { class: "bk-actions" }, ...out)];
  };

  rowKeys(repos.el);

  // ── retired ───────────────────────────────────────────────────────────
  /** @type {(() => void) | null} */
  let detachRetired = null;
  const paintRetired = () => {
    // redesign-final (exact counts): "N entries" is the retired table's rows.
    countOf(retiredCount, ret.body, "tbody > tr:not([data-kp-skeleton-row])");
    retiredCount.textContent = retired
      ? `${retired.length} ${retired.length === 1 ? "entry" : "entries"}`
      : retiredError
        ? "not read"
        : "reading…";
    detachRetired?.();
    detachRetired = null;
    if (retiredError) {
      ret.body.replaceChildren(
        h(
          "div",
          { class: "kp-alert kp-alert--destructive" },
          h(
            "div",
            { class: "kp-alert__body" },
            `Could not read the retired list: ${retiredError}`,
          ),
        ),
      );
      return;
    }
    if (!retired) {
      ret.body.replaceChildren(skeleton("60%"), skeleton("40%"));
      return;
    }
    if (!retired.length) {
      ret.body.replaceChildren(
        h(
          "div",
          { class: "bk-retired-empty" },
          h(
            "div",
            null,
            h("strong", null, "Nothing retired is kept"),
            "Destroy, forget or a deploy that drops an app puts an entry here.",
          ),
          h(
            "div",
            null,
            h("strong", null, "Wipe is per entry"),
            "Wipe deletes exactly what one entry kept, after you type its key.",
          ),
          h(
            "div",
            null,
            h("strong", null, "In-use data is safe"),
            "Anything a managed stack still uses is kept even then.",
          ),
        ),
      );
      return;
    }
    const table = tableBlock({
      remember: "retired",
      caption:
        "Every stack, app or native unit the host keeps a retired record of",
      captionHidden: true,
      search: "Search retired keys",
      nothing: "Nothing retired is kept right now.",
      pageSize: 25,
      pageSizes: "25,50,100",
      columns: RETIRED_COLUMNS,
    });
    ret.body.replaceChildren(table.wrap);
    detachRetired = attachDataTables(ret.body);
    table.tbody.replaceChildren(...retired.map(retiredRow));

    table.ready();
  };
  cleanups.push(() => detachRetired?.());

  const paintAll = () => {
    paintLive();
    paintKpis();
    paintHeat();
    paintDetail();
    paintRepos();
  };

  // ── dialogs ───────────────────────────────────────────────────────────
  /**
   * Restore… in the header: pick the stack, the app and the snapshot here,
   * then the Restore action's own dialog opens with all three filled in.
   */
  // redesign-final-h3: Restore… opens the Restore flow (FLOWS.md §5), on
  // the stack the view is narrowed to, or the first with a snapshot.
  const openRestorePicker = () => {
    const withRepos = names.filter(
      (n) =>
        reads[n]?.status === "ok" && /** @type {any} */ (reads[n]).repos.length,
    );
    const first =
      (S.stacks.size ? withRepos.find((n) => S.stacks.has(n)) : null) ??
      withRepos[0] ??
      null;
    history.pushState(null, "", restoreHref(first));
    dispatchEvent(new PopStateEvent("popstate"));
  };

  /** "Drill them now…": every repository never drilled, each its own Verify restore. */
  const openDrills = () => {
    const never = allRepos(reads).filter(
      (r) => drillState(r) === "never" && r.newest_snapshot,
    );
    /** @type {{close: () => void} | null} */
    let dlg = null;
    const list = h(
      "ul",
      { class: "bk-drill-list" },
      ...never.map((r) => {
        const b = viaForm(
          button("Verify restore…", {
            cls: "kp-button--sm kp-button--secondary",
            title:
              "Restore the newest snapshot into the drill's scratch directory and judge it",
            attrs: { "data-action": "verify-restore" },
            onClick: () => {
              dlg?.close();
              void openAction(r.stack, "verify-restore", {
                preset: {
                  app: r.owner,
                  snapshot: /** @type {any} */ (r.newest_snapshot).short_id,
                },
              });
            },
          }),
          "verify-restore",
        );
        return h(
          "li",
          null,
          stackMark(hueOf(r.stack)),
          h("span", { class: "mono" }, `${r.stack}/${r.owner}`),
          h(
            "span",
            { class: "bk-hint" },
            `newest ${humanDuration(nowS() - (r.newest_snapshot?.time ?? nowS()))} ago`,
          ),
          b,
        );
      }),
    );
    dlg = openDialog({
      title: "Drill the repositories never drilled",
      description:
        "Verify restore restores a repository's newest snapshot into the drill's scratch directory, judges it as the nightly drill does, and empties the scratch again; nothing live is touched. Each runs as its own job.",
      body: [
        list,
        h(
          "p",
          { class: "bk-hint" },
          "The Restore drills tile counts the nightly round's own drills, one repository a night in turn; a verify you start here reports in its job.",
        ),
      ],
      wide: true,
      id: "bk-drills",
    });
  };

  // ── loading ───────────────────────────────────────────────────────────
  /**
   * One stack: its repositories and its nights, read side by side.
   * @param {string} name
   * @param {boolean} force
   * @returns {Promise<StackRead>}
   */
  const readOne = async (name, force) => {
    const q = force ? "?refresh=1" : "";
    const enc = encodeURIComponent(name);
    try {
      const [b, c] = await Promise.all([
        fetchJson(
          `/data/backups/${enc}${q}`,
          `${name}'s backups`,
          abort.signal,
        ),
        slowRead(
          `/data/backup-calendar?stack=${enc}${force ? "&refresh=1" : ""}`,
          `${name}'s backup nights`,
          abort.signal,
        ).catch(() => ({ ok: false, error: null })),
      ]);
      if (!b.ok)
        return {
          status: "failed",
          reason: b.error.fix ? `${b.error.why} — ${b.error.fix}` : b.error.why,
        };
      /** @type {Repo[]} */
      const repoList = b.body?.repos ?? [];
      const cal = c.ok ? /** @type {any} */ (c).body : null;
      const calTimes = /** @type {number[] | undefined} */ (
        cal?.stacks?.[name]
      );
      const repoTimes = repoList.flatMap((r) =>
        (r.snapshots ?? []).map((s) => s.time),
      );
      return {
        status: "ok",
        native: b.body?.native === true,
        repos: repoList,
        noBackup: (cal?.no_backup ?? []).includes(name),
        reason: cal?.reasons?.[name] ?? null,
        times: [...new Set([...(calTimes ?? []), ...repoTimes])],
      };
    } catch (e) {
      if (abort.signal.aborted) return { status: "pending" };
      return { status: "failed", reason: String(e) };
    }
  };

  /** A repository the host has not read yet is asked again, a while later. */
  const unread = (/** @type {StackRead} */ r) =>
    r.status === "ok" && r.repos.some((x) => x.error === "not read yet");

  /**
   * @param {string} name
   * @param {boolean} force
   */
  const readAndPaint = async (name, force) => {
    startedAt[name] ??= Date.now();
    let r = await readOne(name, force);
    let wait = 2000;
    while (
      !abort.signal.aborted &&
      unread(r) &&
      Date.now() - startedAt[name] < UNREAD_GIVE_UP_MS
    ) {
      reads = { ...reads, [name]: r };
      paintAll();
      await new Promise((res) => setTimeout(res, wait));
      wait = Math.min(wait * 2, 15000);
      r = await readOne(name, false);
    }
    if (abort.signal.aborted) return;
    reads = { ...reads, [name]: r };
    readAt = nowS();
    paintAll();
  };

  /** @param {string} name */
  const retryOne = async (name) => {
    reads = { ...reads, [name]: { status: "pending" } };
    delete startedAt[name];
    paintAll();
    await readAndPaint(name, true);
  };

  let running = false;
  let again = false;
  /** @param {boolean} force */
  const loadAll = async (force) => {
    if (running) {
      again = true;
      return;
    }
    running = true;
    try {
      const fleet = current().fleet;
      names = (fleet?.stacks ?? [])
        .map((/** @type {any} */ s) => s.name)
        .sort((/** @type {string} */ a, /** @type {string} */ b) =>
          a.localeCompare(b),
        );
      hues = stackHues(names.map((stack) => ({ stack })));
      for (const s of [...S.stacks])
        if (fleet && !names.includes(s)) S.stacks.delete(s);
      reads = Object.fromEntries(
        names.map((n) => [n, { status: /** @type {const} */ ("pending") }]),
      );
      for (const n of names) delete startedAt[n];
      if (S.night)
        S.offset = offsetFor(nightsNow(nowS()).current, S.night, N());
      paintAll();
      if (!fleet) return;
      void loadRetired();
      await Promise.allSettled(names.map((n) => readAndPaint(n, force)));
    } finally {
      running = false;
      if (again && !abort.signal.aborted) {
        again = false;
        void loadAll(false);
      }
    }
  };

  const loadRetired = async () => {
    const r = await fetchJson(
      "/data/retired",
      "the retired list",
      abort.signal,
    ).catch(() => null);
    if (abort.signal.aborted || !r) return;
    if (r.ok) {
      retired = r.body?.retired ?? [];
      retiredError = null;
    } else retiredError = r.error.why;
    paintRetired();
    paintKpis();
  };

  // fix-204: one read at a time; a store notice reloads only when the set
  // of stacks itself changed.
  const namesKey = () =>
    (current().fleet?.stacks ?? [])
      .map((/** @type {any} */ s) => s.name)
      .sort()
      .join(",");
  let lastNames = namesKey();
  const off = subscribe(() => {
    const k = namesKey();
    if (k !== lastNames) {
      lastNames = k;
      void loadAll(false);
    }
  });
  cleanups.push(off);

  // `/` focuses the filter (DESIGN_LANGUAGE §8); Esc unpins the night when
  // nothing else has it (a dialog, a field).
  /** @param {KeyboardEvent} e */
  const onKey = (e) => {
    if (e.defaultPrevented && e.key !== "/") return;
    const t = /** @type {HTMLElement | null} */ (document.activeElement);
    const inField = !!t && /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName);
    if (document.querySelector("dialog[open]")) return;
    if (e.key === "/" && !inField && !e.ctrlKey && !e.metaKey) {
      e.preventDefault();
      search.focus();
    } else if (e.key === "Escape" && !inField && S.night) pin(null);
  };
  document.addEventListener("keydown", onKey);
  cleanups.push(() => document.removeEventListener("keydown", onKey));

  // Rotating to a phone (or back) changes how many nights fit.
  let lastN = N();
  const onResize = () => {
    if (N() === lastN) return;
    lastN = N();
    if (S.night) S.offset = offsetFor(nightsNow(nowS()).current, S.night, N());
    else S.offset = 0;
    paintHeat();
  };
  addEventListener("resize", onResize);
  cleanups.push(() => removeEventListener("resize", onResize));

  ret.el.addEventListener("toggle", () => {
    if (/** @type {HTMLDetailsElement} */ (ret.el).open) paintRetired();
  });
  paintRetired();
  if (fromUrl.section === "coverage") cov.el.scrollIntoView({ block: "start" });
  if (fromUrl.section === "removed") ret.el.scrollIntoView({ block: "start" });

  void loadAll(false);
  return () => {
    abort.abort();
    for (const c of cleanups) c();
  };
}

/**
 * j / k (and the arrows) move between the table's rows, Enter opens one.
 * @param {HTMLElement} card
 */
function rowKeys(card) {
  card.addEventListener("keydown", (e) => {
    if (!["j", "k", "ArrowDown", "ArrowUp", "Enter"].includes(e.key)) return;
    const t = /** @type {HTMLElement} */ (e.target);
    if (/^(INPUT|TEXTAREA|SELECT|BUTTON|A)$/.test(t.tagName)) return;
    const items = /** @type {HTMLElement[]} */ ([
      ...card.querySelectorAll("tr[tabindex]"),
    ]).filter((x) => x.offsetParent);
    const i = items.indexOf(
      /** @type {HTMLElement} */ (document.activeElement),
    );
    if (e.key === "Enter") {
      if (i >= 0) {
        e.preventDefault();
        items[i].click();
      }
      return;
    }
    e.preventDefault();
    const step = e.key === "j" || e.key === "ArrowDown" ? 1 : -1;
    items[Math.max(0, Math.min(items.length - 1, i + step))]?.focus();
  });
}

/**
 * fix-241: "Show a file…" — one file of one snapshot of this app,
 * read-only.
 * @param {string} stack
 * @param {Repo} r
 */
function openFileDialog(stack, r) {
  const now = Math.floor(Date.now() / 1000);
  const id = `${stack}-${r.owner}`;
  const snap = /** @type {HTMLSelectElement} */ (
    h("select", {
      class: "kp-field__input",
      id: fieldId(SNAPFILE, `${id}-snap`),
    })
  );
  snap.append(
    ...snapshotPickerRows(r.snapshots ?? [], now).map((row) =>
      h(
        "option",
        { value: row.value },
        `${row.shortId} · ${row.when} · ${row.ago}${row.latest ? " (latest)" : ""}`,
      ),
    ),
  );
  const path = /** @type {HTMLInputElement} */ (
    h("input", {
      class: "kp-field__input",
      type: "text",
      id: fieldId(SNAPFILE, `${id}-path`),
      autocomplete: "off",
      spellcheck: "false",
      placeholder: "config/settings.xml",
      "aria-describedby": `${id}-path-hint`,
    })
  );
  const show = h(
    "button",
    { class: "kp-button kp-button--primary", type: "submit" },
    "Show the file",
  );
  const status = h(
    "p",
    { class: "snapfile__status", role: "status", "aria-live": "polite" },
    "Pick a snapshot, type a path, then Show the file.",
  );
  const out = h("pre", { class: "snapfile__out mono", tabindex: "0" });
  const form = h(
    "form",
    { class: "snapfile" },
    h(
      "div",
      { class: "kp-field" },
      h("label", { class: "kp-field__label", for: `${id}-snap` }, "Snapshot"),
      snap,
    ),
    h(
      "div",
      { class: "kp-field" },
      h("label", { class: "kp-field__label", for: `${id}-path` }, "File"),
      path,
      h(
        "p",
        { class: "kp-field__help", id: `${id}-path-hint` },
        "Relative to this app's backed-up directory, or the absolute path under it.",
      ),
    ),
    h(
      "div",
      { class: "snapfile__go" },
      show,
      h(
        "span",
        { class: "kp-field__help" },
        "Reads the file from the backup on the host; nothing is restored or written.",
      ),
    ),
    status,
    out,
  );
  /** @type {AbortController | null} */
  let reading = null;
  form.addEventListener("submit", (ev) => {
    ev.preventDefault();
    const typed = path.value.trim();
    if (!typed) {
      status.textContent = "Type the path of the file first.";
      return;
    }
    reading?.abort();
    const abort = new AbortController();
    reading = abort;
    status.textContent = `Reading ${typed} from the snapshot… (up to 30 s; the backup lives on the remote)`;
    out.textContent = "";
    void fetchJson(
      snapshotFileUrl(stack, r.owner, snap.value, typed),
      "the file from the snapshot",
      abort.signal,
    )
      .then((res) => {
        if (abort.signal.aborted) return;
        if (!res.ok) {
          status.textContent = res.error.fix
            ? `Could not read it: ${res.error.why} — ${res.error.fix}`
            : `Could not read it: ${res.error.why}`;
          return;
        }
        const v = snapshotFileView(/** @type {any} */ (res.body));
        status.textContent = v.notes.join(" ");
        status.dataset.tone = v.tone;
        out.textContent = v.text;
      })
      .catch((e) => {
        if (!abort.signal.aborted)
          status.textContent = `Could not read it: ${String(e)}`;
      });
  });
  const d = openDialog({
    title: `A file from ${r.owner}'s backup`,
    description: `Shows one file of ${stack}/${r.owner} as the chosen snapshot holds it (up to 1 MiB, and says so when cut), without restoring anything.`,
    body: [form],
    wide: true,
    id,
  });
  void d.closed.then(() => reading?.abort());
  path.focus();
}
