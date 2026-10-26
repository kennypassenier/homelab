// feat-retired-1: the Retired page — every `HostState::retired` entry (a
// stack, app or native unit the host still KEEPS everything of, ask-9),
// what it keeps, and a "Wipe" button that opens the existing wipe dialog
// (`ActionKind::Wipe`) on it.
//
// Kenny, 2026-10-02: "is there a way, from the admin dashboard, to delete
// backups of services we no longer use? That seems desirable." Before this
// page, `ActionKind::Wipe` was reachable only from the stack's own page (the
// "Retire" group next to Destroy/Forget) — unreachable the moment the stack
// itself is gone, which is exactly when a wipe is wanted. This page lists
// every retired key regardless of whether its stack still has a page, and
// `homelab ui open wipe <key>` (Live view) now opens the same dialog on a
// key that is not a live stack (`core::drive`'s `targets_retired`).
//
// It is its own page in the Configure group rather than a section of
// Backups (Kenny asked which reads better): Backups is built around "every
// managed stack's repositories", one row per owning app of a STACK THAT
// EXISTS; a retired entry has no stack page to link to and no live manifest
// to read apps/units from, so folding it into that per-stack loop would mean
// a second, different kind of row in the same table. A dedicated page keeps
// both models simple, and sits naturally beside Secrets/Settings/Firewall as
// "what the fleet's lifecycle looks like", not as a kind of backup status.

import {
  badgeCell,
  fetchJson,
  h,
  labelCells,
  selectCell,
  tableBlock,
  td,
} from "../dom.js";
import { declare, drivable } from "../drivable.js";
import { formatDateTime, humanDuration } from "../format.js";
import {
  ensureStyle,
  kpiStrip,
  pageHeader,
  section,
  segSwitch,
} from "../ui.js";
import { humanMb } from "../fleet.js";
import { openAction, openBatch, reopenBatch } from "../actiondialog.js";
import { send } from "../act.js";
import { listen } from "../store.js";
import {
  changesRetired,
  operationLabel,
  repoTotals,
  splitInUse,
  wipeStatus,
} from "../retiredview.js";
import { setBarCell } from "/static/kp/js/barcell.js";
import { openConfirmation } from "/static/kp/js/components.js";
import { attachDataTables } from "/static/kp/js/datatable.js";
import { attachIdmarks } from "/static/kp/js/idmark.js";
import { bindSelection } from "../selectbind.js";
import { setSelectionBusy } from "/static/kp/js/selection-bar.js";
import { setStackbar } from "/static/kp/js/stackbar.js";

const NOTHING = "Nothing retired is kept right now.";

/**
 * The retired table's columns; redesign-backups: shared with the Backups page.
 * @type {import("../dom.js").Column[]}
 */
export const RETIRED_COLUMNS = [
  { label: "Kind", sort: "text", filter: "choice" },
  { label: "Key", sort: "text" },
  { label: "Retired by", sort: "text", filter: "choice" },
  { label: "Age", sort: "text" },
  { label: "Restic", sort: "text" },
  { label: "/appdata", sort: "text" },
  { label: "Vault", sort: "text" },
  { label: "Kept", sort: "text", filter: "choice" },
  { label: "", sort: "none" },
];

/**
 * @param {number | null | undefined} unixSeconds
 */
function age(unixSeconds) {
  if (unixSeconds == null) return "—";
  return humanDuration(Date.now() / 1000 - unixSeconds);
}

/**
 * @param {ReturnType<typeof repoTotals>} totals
 */
function reposCell(totals) {
  if (totals.repoCount === 0) return "none";
  const parts = [
    `${totals.snapshotCount} snapshot(s)`,
    humanMb(totals.sizeBytes / (1024 * 1024)),
  ];
  if (totals.newestTime != null) {
    parts.push(
      `newest ${humanDuration(Date.now() / 1000 - totals.newestTime)} ago`,
    );
  }
  if (totals.unread > 0) parts.push(`${totals.unread} not read yet`);
  return parts.join(", ");
}

/**
 * @param {string[]} removed
 * @param {string[]} kept
 * @param {string} noun
 */
function pathsCell(removed, kept, noun) {
  if (removed.length === 0 && kept.length === 0) return "none";
  const parts = [];
  if (removed.length > 0) parts.push(`${removed.length} ${noun}`);
  if (kept.length > 0) parts.push(`${kept.length} kept (in use elsewhere)`);
  return parts.join(", ");
}

/**
 * One retired entry's row; redesign-backups: the Backups page's "Kept from retired
 * stacks" section draws the same rows.
 * @param {import("../retiredview.js").RetiredEntry} entry
 */
export function retiredRow(entry) {
  const status = wipeStatus(entry);
  // live-finding 2026-10-02: a refused key is NOT "0 removable, 0 kept" —
  // it is the whole record still in use, so every cell says "still in use"
  // rather than running the partial-keep split meant for D25's narrower
  // case (one repo or dir in use while the rest of the same record is
  // genuinely gone). Showing a per-path breakdown here would read as "wipe
  // would remove N things", which is exactly the false safety this guard
  // exists to prevent.
  const tr = h(
    "tr",
    { "data-kp-row-key": entry.key },
    td(entry.kind, "mono"),
    h("td", null, h("strong", null, entry.key)),
    td(operationLabel(entry.kind)),
    td(age(entry.retired_at)),
    td(
      status === "refused"
        ? "still in use"
        : reposCell(repoTotals(entry.repos)),
    ),
    td(
      status === "refused"
        ? "still in use"
        : pathsCell(...splitHalves(entry.appdata, entry.in_use), "dir(s)"),
    ),
    td(
      status === "refused"
        ? "still in use"
        : pathsCell(...splitHalves(entry.vault, entry.in_use), "file(s)"),
    ),
    badgeCell(statusBadge(status)),
    h(
      "td",
      null,
      status === "refused" ? refusedNote(entry) : wipeButton(entry),
    ),
  );
  return tr;
}

/**
 * @param {string[]} paths
 * @param {string[]} inUse
 * @returns {[string[], string[]]}
 */
function splitHalves(paths, inUse) {
  const s = splitInUse(paths, inUse);
  return [s.removed, s.kept];
}

/**
 * @param {"removable" | "partial" | "refused"} status
 */
function statusBadge(status) {
  switch (status) {
    case "refused":
      return { label: "still in use — not retired", tone: "bad" };
    case "partial":
      return { label: "partly kept in use", tone: "warn" };
    default:
      return { label: "fully removable", tone: "good" };
  }
}

/**
 * The refusal in place of a Wipe button: `wipe_plan`'s own reason, so the
 * page never explains a refusal in words of its own that could drift from
 * what an actual wipe attempt would say.
 * @param {import("../retiredview.js").RetiredEntry} entry
 */
function refusedNote(entry) {
  return h("span", { class: "measured" }, entry.refused ?? "still in use");
}

/**
 * @param {import("../retiredview.js").RetiredEntry} entry
 */
function wipeButton(entry) {
  const btn = h(
    "button",
    {
      class: "kp-button kp-button--sm kp-button--destructive",
      type: "button",
      "data-action": "wipe",
    },
    "Wipe",
  );
  btn.addEventListener("click", () => void openAction(entry.key, "wipe"));
  return btn;
}

// ── the page, on kp-themes 11 (feat-kp11demo-12.6) ────────────────────────
const AGE = declare({
  id: "retired-age",
  page: "retired",
  opens: "view",
  row: "all|old",
  what: "show every retired stack, or only those retired more than 90 days ago",
});
const TICK = declare({
  id: "retired-tick",
  page: "retired",
  opens: "view",
  row: "<key>",
  selects: true,
  what: "tick or untick one retired entry to wipe it",
});
const TICK_ALL = declare({
  id: "retired-tick-all",
  page: "retired",
  opens: "view",
  what: "tick every retired entry that can be wiped now",
});
const UNTICK = declare({
  id: "retired-untick",
  page: "retired",
  opens: "view",
  what: "untick every ticked retired entry",
  shows: "while a retired entry is ticked",
  reach: [{ do: "click", control: "retired-tick", row: "*" }],
});
const WIPE = declare({
  id: "retired-wipe",
  page: "retired",
  opens: "dialog",
  what: "wipe the ticked retired entries for good: one asks for its name, several for 'wipe N stacks'",
  shows: "while a retired entry is ticked",
  reach: [{ do: "click", control: "retired-tick", row: "*" }],
});

const OLD_DAYS = 90;
const GB = 1024 ** 3;
/** @param {number} bytes */
const gbText = (bytes) => `${(bytes / GB).toFixed(1)} GB`;
/** @param {number} n @param {string} one */
const plural = (n, one) => `${n} ${n === 1 ? one : `${one}s`}`;

/**
 * Whole days since a retirement.
 * @param {number} at unix seconds
 */
const daysSince = (at) =>
  Math.max(0, Math.floor((Date.now() / 1000 - at) / 86400));

/**
 * The size an entry keeps in its restic repositories, in bytes.
 * @param {import("../retiredview.js").RetiredEntry} e
 */
const sizeOf = (e) => repoTotals(e.repos).sizeBytes;

/**
 * The Retired kit: the key figure and share bar, the age control, the
 * selection bar and the table with Wipe. The Backups page's "Kept from
 * retired stacks" section and the page's own mount both draw it; whoever
 * draws it feeds it with `set` and hears `onRetry`.
 * @param {{onRetry: () => void}} opts
 */
export function retiredKit(opts) {
  ensureStyle("/css/pages/retired.css");
  /** @type {import("../retiredview.js").RetiredEntry[]} */
  let entries = [];
  /** @type {"loading" | "failed" | "ready"} */
  let status = "loading";
  /** @type {"all" | "old"} */
  let age = "all";

  const table = tableBlock({
    remember: "retired",
    caption:
      "Every stack, app or native unit the host keeps a retired record of; tick the ones to wipe",
    captionHidden: true,
    search: "Search retired keys",
    state: "loading",
    nothing: NOTHING,
    pageSize: 50,
    pageSizes: "25,50,100,250",
    select: {
      label: "Tick every retired entry that can be wiped",
      actions: [],
    },
    columns: [
      { label: "Stack", sort: "text" },
      { label: "Retired on", sort: "number" },
      { label: "Age", sort: "number", cls: "kp-text-end" },
      { label: "Size", sort: "number" },
      { label: "Snapshots", sort: "number", cls: "kp-text-end" },
      { label: "Newest snapshot", sort: "number" },
      { label: "Can it be wiped now?", sort: "text", filter: "choice" },
      { label: "Retired by", sort: "text", filter: "choice" },
      { label: "Vault", sort: "text" },
    ],
  });
  table.wrap.querySelector("[data-kp-datatable-actions]")?.remove();
  table.wrap.setAttribute("data-kp-column-menu", "");
  table.wrap.id = "rt-table";
  const ths = /** @type {HTMLElement[]} */ ([
    ...table.wrap.querySelectorAll("thead th"),
  ]);
  ths[1].setAttribute("data-kp-column-locked", "");
  // Retired by and Vault start hidden; one tick in Columns brings them back.
  ths[8].setAttribute("data-kp-column-hidden", "");
  ths[9].setAttribute("data-kp-column-hidden", "");
  const selectAll = table.wrap.querySelector("[data-kp-select-all]");
  if (selectAll instanceof HTMLElement) drivable(selectAll, TICK_ALL);

  const kpis = kpiStrip(
    [{ key: "reclaimable", label: "Reclaimable", value: "…" }],
    { loading: true, label: "Retired data" },
  );
  kpis.el.classList.add("rt-kpis");
  const split = h("figure", {
    class: "kp-stackbar rt-split",
    "data-kp-stackbar": "",
  });
  const head = h("div", { class: "rt-head" }, kpis.el, split);

  const ages = segSwitch({
    label: "Age",
    value: age,
    items: [
      { value: "all", label: "All", hint: "Every retired entry" },
      {
        value: "old",
        label: "Older than 90 days",
        hint: "Only entries retired more than 90 days ago",
      },
    ],
    onChange: (v) => {
      age = v === "old" ? "old" : "all";
      paint();
    },
    mark: (b, v) => void drivable(b, AGE, v),
  });
  const controls = h(
    "div",
    { class: "rt-ctl" },
    h("b", { class: "rt-ctl__label" }, "Age"),
    h("div", { class: "rt-ctl__control" }, ages.el),
  );
  const readout = h("p", { class: "rt-readout", "aria-live": "polite" });
  const bar = h(
    "div",
    {
      class: "kp-selection rt-bar",
      "data-kp-selection": "",
      "data-kp-selection-for": "rt-table",
      "data-kp-placement": "sticky-top",
      id: "rt-bar",
      "aria-label": "Selected retired entries",
    },
    h(
      "div",
      { class: "kp-selection__actions" },
      drivable(
        h(
          "button",
          {
            type: "button",
            class: "kp-button kp-button--destructive",
            "data-kp-selection-action": "wipe",
            "data-action": "wipe",
            title: "Delete what the ticked entries keep, for good",
          },
          "Wipe",
        ),
        WIPE,
      ),
    ),
  );
  const el = h(
    "div",
    { class: "rt-kit" },
    head,
    controls,
    readout,
    bar,
    table.wrap,
  );
  const detachTables = attachDataTables(el);
  const sel = bindSelection(
    el,
    bar,
    () => ({}),
    undefined,
    (c) => drivable(c, UNTICK),
  );
  const detachMarks = attachIdmarks(el);

  /** @param {import("../retiredview.js").RetiredEntry} e */
  const refusedWhy = (e) =>
    wipeStatus(e) === "refused" ? (e.refused ?? "still in use") : "";

  /** @param {import("../retiredview.js").RetiredEntry} e */
  const rowOf = (e) => {
    const t = repoTotals(e.repos);
    const why = refusedWhy(e);
    const biggest = Math.max(1, ...entries.map(sizeOf));
    const sizeEl = h("span", { class: "kp-bar-cell" });
    setBarCell(sizeEl, {
      value: sizeOf(e) / biggest,
      text: humanMb(sizeOf(e) / (1024 * 1024)),
      sortValue: sizeOf(e),
      tone: null,
      label: "of the biggest retired entry",
    });
    const tick = selectCell(e.key, `Tick ${e.key}`);
    const box = /** @type {HTMLInputElement} */ (tick.querySelector("input"));
    drivable(box, TICK, e.key);
    if (why) {
      box.disabled = true;
      box.title = `Cannot be wiped now: ${why}`;
      box.setAttribute("data-kp-disabled-reason", why);
    }
    const status = wipeStatus(e);
    const tr = h(
      "tr",
      { "data-kp-row-key": e.key, "data-kp-row-label": e.key },
      tick,
      h(
        "td",
        { "data-kp-sort-value": e.key },
        h(
          "span",
          { class: "rt-name" },
          h("span", {
            class: "kp-idmark kp-idmark--sm",
            "data-kp-idmark": e.key,
            "aria-hidden": "true",
          }),
          h("b", null, e.key),
          e.kind === "stack"
            ? null
            : h("span", { class: "kp-timestamp" }, e.kind),
        ),
      ),
      h(
        "td",
        { class: "rt-nowrap", "data-kp-sort-value": String(e.retired_at) },
        formatDateTime(e.retired_at),
      ),
      h(
        "td",
        {
          class: "kp-text-end",
          "data-kp-sort-value": String(daysSince(e.retired_at)),
        },
        plural(daysSince(e.retired_at), "day"),
      ),
      h("td", null, sizeEl),
      h(
        "td",
        { class: "kp-text-end", "data-kp-sort-value": String(t.snapshotCount) },
        String(t.snapshotCount),
      ),
      h(
        "td",
        { class: "rt-nowrap", "data-kp-sort-value": String(t.newestTime ?? 0) },
        t.newestTime ? formatDateTime(t.newestTime) : "none",
      ),
      h(
        "td",
        {
          "data-kp-filter-value":
            status === "refused"
              ? "not now"
              : status === "partial"
                ? "partly"
                : "yes",
        },
        status === "refused"
          ? h(
              "span",
              null,
              h("span", { class: "kp-badge kp-badge--warning" }, "Not now"),
              " ",
              why,
            )
          : status === "partial"
            ? h(
                "span",
                { class: "kp-badge kp-badge--info" },
                "Yes, partly: some is kept in use",
              )
            : h("span", { class: "kp-badge kp-badge--success" }, "Yes"),
      ),
      td(operationLabel(e.kind)),
      td(`${e.vault.length} file${e.vault.length === 1 ? "" : "s"}`),
    );
    return tr;
  };

  const shown = () =>
    entries.filter((e) => age === "all" || daysSince(e.retired_at) > OLD_DAYS);

  const paint = () => {
    ages.set(age);
    const list = shown();
    const total = list
      .filter((e) => wipeStatus(e) !== "refused")
      .reduce((t, e) => t + sizeOf(e), 0);
    const state =
      status === "loading"
        ? "loading"
        : status === "failed"
          ? "error"
          : list.length
            ? "ready"
            : "empty";
    kpis.tiles.get("reclaimable")?.set(
      state === "ready"
        ? {
            loading: false,
            value: (total / GB).toFixed(1),
            unit: "GB",
            ctx: `${plural(list.length, "entry")}${age === "old" ? ", older than 90 days" : ""}`,
          }
        : state === "loading"
          ? { loading: true }
          : {
              loading: false,
              value: "—",
              ctx: state === "error" ? "not read" : "nothing retired",
            },
    );
    setStackbar(split, {
      title: `Retired data keeps ${gbText(total)} in backups`,
      format: (v) => `${(v / GB).toFixed(1)} GB`,
      legend: true,
      label: "What retired data keeps, per entry",
      other: 0.03,
      parts: list
        .filter((e) => sizeOf(e) > 0)
        .map((e) => ({ key: e.key, label: e.key, value: sizeOf(e) })),
      state,
      message:
        state === "error"
          ? "The retired list could not be read."
          : "Nothing is retired.",
    });
    if (status === "loading") {
      table.loading({ words: "Reading the retired list from the host…" });
      return;
    }
    if (status === "failed") return;
    table.tbody.replaceChildren(...list.map(rowOf));
    labelCells(table.tbody, () => [
      "",
      "Stack",
      "Retired on",
      "Age",
      "Size",
      "Snapshots",
      "Newest snapshot",
      "Can it be wiped now?",
      "Retired by",
      "Vault",
    ]);
    table.ready();
    summary(ticked());
  };

  /** @returns {string[]} */
  const ticked = () =>
    [...table.tbody.querySelectorAll("input[data-kp-select-row]:checked")].map(
      (b) => /** @type {HTMLInputElement} */ (b).value,
    );
  /** @param {string[]} keys */
  const summary = (keys) => {
    if (status !== "ready") {
      readout.textContent = "";
      return;
    }
    const chosen = entries.filter((e) => keys.includes(e.key));
    readout.textContent = chosen.length
      ? `${plural(chosen.length, "entry")} ticked keep ${gbText(chosen.reduce((t, e) => t + sizeOf(e), 0))} that would be gone for good.`
      : `${plural(entries.length, "retired entry")} keep ${gbText(entries.reduce((t, e) => t + sizeOf(e), 0))} in all.`;
  };
  bar.addEventListener("kp-selection-change", (e) =>
    summary(/** @type {CustomEvent} */ (e).detail.keys),
  );

  bar.addEventListener("kp-selection-action", (e) => {
    const { keys, button } = /** @type {CustomEvent} */ (e).detail;
    if (keys.length === 1) {
      // One entry: the wipe dialog asks for its name.
      void openAction(keys[0], "wipe");
      return;
    }
    const chosen = entries.filter((x) => keys.includes(x.key));
    openConfirmation(button, {
      phrase: `Wipe ${plural(chosen.length, "retired entry")} for good? This cannot be undone.`,
      typeName: `wipe ${chosen.length} stacks`,
      loses: chosen.map((x) => {
        const t = repoTotals(x.repos);
        return `${x.key}: ${humanMb(t.sizeBytes / (1024 * 1024))} of backups (${plural(t.snapshotCount, "snapshot")}), ${x.appdata.length} app data ${x.appdata.length === 1 ? "directory" : "directories"} and ${plural(x.vault.length, "vault file")}`;
      }),
      acceptLabel: "Wipe",
      onAccept: () => {
        setSelectionBusy(bar, `Wiping ${chosen.length}…`);
        void send(
          "POST",
          "/data/actions/batch",
          {
            action: "wipe",
            stacks: keys,
            args: {},
            confirms: Object.fromEntries(
              keys.map((/** @type {string} */ k) => [k, k]),
            ),
          },
          `wipe ${chosen.length} entries`,
        ).then((r) => {
          setSelectionBusy(bar, null);
          if (r.ok) void reopenBatch(r.body.batch);
          else void openBatch("wipe", keys);
        });
      },
    });
  });

  el.addEventListener("kp-datatable-retry", opts.onRetry);
  split.addEventListener("kp-stackbar-retry", opts.onRetry);
  paint();
  return {
    el,
    /**
     * @param {{status: "loading" | "failed" | "ready", entries?: import("../retiredview.js").RetiredEntry[], error?: import("../doctor.js").RouteError}} m
     */
    set: (m) => {
      status = m.status;
      if (m.entries) entries = m.entries;
      paint();
      if (m.status === "failed" && m.error) table.failed(m.error);
    },
    stop: () => {
      detachTables();
      sel.stop();
      detachMarks();
    },
  };
}

/**
 * The page: its header, the kit in a card, and the host's retired list read
 * (and read again when a wipe, destroy, forget or deploy ends).
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const abort = new AbortController();
  const kit = retiredKit({ onRetry: () => retry() });
  const sec = section({
    title: "What retired stacks still keep",
    desc: "Lists the retired stacks with the size of what is left of each (backups), adds it up, and wipes the ticked ones for good: the name is typed, the host may still refuse, and an entry that cannot be wiped cannot be ticked.",
    id: "rt-keep",
    body: [kit.el],
    foot: ["Read from the host · nothing here is deleted on its own"],
  });
  sec.el.classList.add("rt-card");
  root.replaceChildren(
    pageHeader({
      title: "Removed stacks",
      desc: "Every stack, app or native unit a destroy, forget or deploy retired: nothing here is deleted on its own. Wipe deletes exactly what one entry kept: its restic repositories, /appdata directories and vault copies, after you type its name to confirm; what a managed stack still uses is kept even then.",
    }).el,
    sec.el,
  );
  /** @param {boolean} [quiet] a re-read after a job keeps the rows up */
  const load = async (quiet = false) => {
    if (!quiet) kit.set({ status: "loading" });
    const r = await fetchJson(
      "/data/retired",
      "the retired list",
      abort.signal,
    );
    if (abort.signal.aborted) return;
    if (!r.ok) {
      kit.set({ status: "failed", error: r.error });
      return;
    }
    kit.set({ status: "ready", entries: r.body?.retired ?? [] });
  };
  const retry = () => void load().catch(() => {});
  retry();
  // fix-wipe-1: a finished wipe, destroy, forget, apply or deploy changes
  // this list; read it again so a wiped key leaves without a refresh.
  const off = listen("action", (j) => {
    if (changesRetired(j)) void load(true).catch(() => {});
  });
  return () => {
    abort.abort();
    off();
    kit.stop();
  };
}
