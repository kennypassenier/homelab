// The Stacks page (redesign-stacks, release 3.71.0; Kenny approved the demo
// 2026-10-03: ~/.local/share/homelab/redesign-3.71/overview.html + .js,
// under the IA of FLOWS.md §1 row 3 — "the fleet list with a host strip on
// top, Deploy all changes, New stack; each row opens the stack hub").
// Top to bottom: the header (live status, Deploy all changes with its
// count, New stack… as the one primary action), the attention band (one
// sentence when the Inbox holds something, absent otherwise), the host
// strip (six KPI tiles, each a link), and the Stacks card: a key row, the
// §12 toolbar (search with key:value tokens · "Only" chips with their
// counts · the count, the cards' sort and the Table / Cards switch), the
// batch bar while stacks are ticked, and the stacks as cards or as a kp
// datatable — the same rows, the same selection. A row opens its stack
// hub; its own controls (the tick, its one action) do only their own
// thing. Deploy all changes and New stack open side panels.
//
// Data: the fleet store (live), /data/fleet-trend (the sparklines: the
// last day, one point per five minutes), /data/drift (which stacks differ
// from their files, the count on Deploy all changes), /data/backup-calendar
// (each stack's last backup), /data/stale-images (newer versions; the
// fleet check's own kept run) and the Inbox (the verdict and its tile).

import { openAction, openBatch } from "../actiondialog.js";
import { errorBox, fetchJson, h, slowRead } from "../dom.js";
import { declare, dialogControl, drivable, viaForm } from "../drivable.js";
import { register } from "../drivehooks.js";
import { gb } from "../fleet.js";
import { openImport } from "../importstack.js";
import { inboxNow, onInbox } from "../inbox.js";
import { openNewStack } from "../newstack.js";
import { updateHrefFor } from "../updateflow.js";
import { stackHref } from "../router.js";
import { current, subscribe } from "../store.js";
import {
  ONLY,
  SORTS,
  deployCount,
  hostStrip,
  newerByStack,
  onlyCounts,
  parseQuery,
  rowMatches,
  sortRows,
  stackRows,
} from "../stacksview.js";
import {
  art,
  drawer,
  emptyState,
  ensureStyle,
  keyRow,
  kpiStrip,
  pageHeader,
  section,
  segSwitch,
  skeletonBlock,
  sparkline,
  stackMark,
  toggleChips,
  toolbar,
} from "../ui.js";
import { setParams } from "../urlstate.js";
import { mount as mountPlan } from "./apply.js";
import { attachDataTables, dataTable } from "/static/kp/js/datatable.js";

/** @typedef {{navigate: (href: string) => void}} Ctx */

/**
 * A host tile as ui.js draws it: stacksview's used-of-limit number is the
 * inline meter at the start of the context line, its status dot the
 * context's tone.
 * @param {import("../stacksview.js").HostKpi} t
 * @returns {import("../ui.js").Kpi}
 */
const tileKpi = ({ meter, dot, tone, ...t }) => ({
  ...t,
  tone: tone === "ok" ? "" : tone,
  ctxMeter:
    meter == null ? null : { pct: meter, tone: tone === "ok" ? "" : tone },
  ctxTone: dot ?? null,
});
/** @typedef {import("../stacksview.js").Row} Row */

// Live view (invariant 39): every control on this page that opens a panel
// or changes the view is declared here; the batch buttons, a row's Deploy
// and Back up, and the panels' New stack / Import / Deploy reach their
// dialogs as catalog forms.
const DEPLOY_ALL = declare({
  id: "stacks-deploy-all",
  page: "overview",
  opens: "dialog",
  what: "open Deploy all changes: compare every stack with its files, read the plan, then deploy",
});
const NEW_STACK = declare({
  id: "stacks-new",
  page: "overview",
  opens: "dialog",
  what: "open New stack: from a preset, from a bundle or empty",
});
const VIEW = declare({
  id: "stacks-view",
  page: "overview",
  opens: "view",
  row: "cards|table",
  what: "show the stacks as cards or as a table",
});
const ONLY_CHIP = declare({
  id: "stacks-only",
  page: "overview",
  opens: "view",
  row: "newer|problems|drift",
  what: "turn one 'Only' filter on or off (newer version, problems, differs from files)",
});
const SORT = declare({
  id: "stacks-sort",
  page: "overview",
  opens: "view",
  row: "vmid|name|cpu",
  what: "order the cards by vmid, by name or busiest first",
  shows: "in the cards view",
  reach: [{ do: "click", control: "stacks-view", row: "cards" }],
});
const CLEAR = declare({
  id: "stacks-untick",
  page: "overview",
  opens: "view",
  what: "untick every ticked stack",
  shows: "while a stack is ticked",
  reach: [{ do: "click", control: "stacks-tick", row: "*" }],
});
const SEARCH = declare({
  id: "stacks-search",
  page: "overview",
  opens: "view",
  what: "the search box: find a stack or app by name, vmid, state: or flag:",
});
const TICK = declare({
  id: "stacks-tick",
  page: "overview",
  opens: "view",
  row: "<stack>",
  what: "tick or untick one stack for a batch action",
});
const TICK_ALL = declare({
  id: "stacks-tick-all",
  page: "overview",
  opens: "view",
  what: "tick every stack shown (click again to untick them)",
  shows: "in the table view",
  reach: [{ do: "click", control: "stacks-view", row: "table" }],
});
const BATCH = declare({
  id: "stacks-batch",
  page: "overview",
  opens: "dialog",
  row: "backup|update|deploy",
  what: "act on the ticked stacks: back up or deploy them in one batch dialog, or update them (each stack's Update dialog in turn)",
  shows: "while a stack is ticked",
  reach: [{ do: "click", control: "stacks-tick", row: "*" }],
});
const OPEN = declare({
  id: "stacks-open",
  page: "overview",
  opens: "view",
  row: "<stack>",
  what: "open one stack's hub (its name, its card or its › button)",
});
const ROW_ACTION = declare({
  id: "stacks-row-action",
  page: "overview",
  opens: "dialog",
  row: "<stack>",
  what: "a stack's one suggested action that opens its dialog: Deploy or Back up",
  shows: "in the table view",
  reach: [{ do: "click", control: "stacks-view", row: "table" }],
});
// redesign-integrate-8: a row's Update opens the one Update flow
// (redesign-flows-1), an address, not a dialog: its own control, so Live
// view does not wait for a dialog that never comes.
const ROW_UPDATE = declare({
  id: "stacks-row-update",
  page: "overview",
  opens: "view",
  row: "<stack>",
  what: "a stack's suggested Update: open the Update flow for that stack",
  shows: "in the table view",
  reach: [{ do: "click", control: "stacks-view", row: "table" }],
});
const ROW_LOGS = declare({
  id: "stacks-row-logs",
  page: "overview",
  opens: "view",
  row: "<stack>",
  what: "a stack's suggested Logs: open its hub's Logs tab",
  shows: "in the table view",
  reach: [{ do: "click", control: "stacks-view", row: "table" }],
});
const KPI = declare({
  id: "stacks-kpi",
  page: "overview",
  opens: "view",
  row: "online|newer|cpu|disk",
  what: "open the detail behind one tile of the strip (stacks running, newer versions, host CPU, root disk)",
});
// redesign-final M1: the attention band that duplicated the Inbox tile is
// gone; its Inbox button's id is the strip's Need you tile now.
const INBOX = declare({
  id: "stacks-open-inbox",
  page: "overview",
  opens: "view",
  what: "the strip's Need you tile: open the Inbox",
});
const SHOW_ALL = declare({
  id: "stacks-show-all",
  page: "overview",
  opens: "view",
  what: "clear the search and the Only filters (the empty list's Show every stack)",
  shows: "when the search and the Only filters match no stack",
});
const HUB_BUTTON = declare({
  id: "stacks-hub-button",
  page: "overview",
  opens: "view",
  row: "<stack>",
  what: "a table row's › button: open that stack's hub",
  shows: "in the table view",
  reach: [{ do: "click", control: "stacks-view", row: "table" }],
});
const EMPTY_NEW = declare({
  id: "stacks-empty-new",
  page: "overview",
  opens: "dialog",
  what: "the empty fleet's New stack…",
  shows: "while the fleet has no stack",
});

/** The view a person last chose, kept in this browser only. */
const VIEW_KEY = "homelab.stacks.view";

/** @param {string} tone */
const dotClass = (tone) => `sk-dot sk-dot--${tone || "none"}`;

/**
 * A flag chip in its tone.
 * @param {import("../stacksview.js").Flag} f
 */
const flagChip = (f) =>
  h(
    "span",
    {
      class: `sk-chip${f.tone ? ` sk-chip--${f.tone}` : ""}`,
      title: f.title,
      "data-flag": f.key,
    },
    f.label,
  );

/**
 * @param {HTMLElement} root
 * @param {Ctx} ctx
 * @returns {() => void}
 */
export function mount(root, ctx) {
  ensureStyle("/css/pages/stacks.css");
  root.classList.add("sk-page");
  const abort = new AbortController();
  /** @type {(() => void)[]} */
  const stops = [];
  const q0 = new URLSearchParams(location.search);

  // ── state ────────────────────────────────────────────────────────────
  /** @type {{q: string, only: Set<string>, view: string, sort: string}} */
  const ui = {
    q: q0.get("q") ?? "",
    only: new Set(
      (q0.get("only") ?? "")
        .split(",")
        .filter((v) => ONLY.some((o) => o.value === v)),
    ),
    view: q0.get("view") ?? readView(),
    sort: SORTS.some((s) => s.key === q0.get("sort"))
      ? /** @type {string} */ (q0.get("sort"))
      : "vmid",
  };
  // redesign-final M1: the demo opens on the Table.
  if (ui.view !== "cards") ui.view = "table";
  /** @type {Set<string>} */
  const ticked = new Set();
  /** @type {string | null} the j/k cursor, a stack's name */
  let cursor = null;
  /** @type {any} */
  let drift = null;
  /** @type {number | null} what the plan itself counted, once read */
  let planCount = null;
  /** @type {any} */
  let calendar = null;
  /** @type {Map<string, number>} */
  let newer = new Map();
  /** @type {any} */
  let trend = null;
  /** @type {Row[]} */
  let rows = [];

  const writeUrl = () => {
    const search = setParams(location.search, {
      q: ui.q || null,
      only: ui.only.size ? [...ui.only].join(",") : null,
      view: ui.view === "table" ? null : ui.view,
      sort: ui.sort === "vmid" ? null : ui.sort,
    });
    if (search !== location.search)
      history.replaceState(history.state, "", location.pathname + search);
  };

  // ── header ───────────────────────────────────────────────────────────
  const deployBadge = h("span", { class: "sk-badge", hidden: "" });
  const deployAll = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button",
        id: "deploy-all",
        title:
          "Make every stack match its files in one confirmed batch; each stack is backed up before it changes",
      },
      "Deploy all changes",
      deployBadge,
    ),
    DEPLOY_ALL,
  );
  deployAll.addEventListener("click", () => openDeployAll());
  const newStack = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--primary",
        id: "new-stack",
        title: "Add a stack from a preset or from an exported bundle",
      },
      "New stack…",
    ),
    NEW_STACK,
  );
  newStack.addEventListener("click", () => openNew());
  const head = pageHeader({
    title: "Stacks",
    desc: "Every stack the host runs, live. Click one to open its hub — its state, logs, backups and settings together — or tick several to act on them at once.",
    live: "updated",
    actions: [deployAll],
    primary: newStack,
  });
  // Both demos put "updated … ago" beside the title (pageHeader's one
  // slot since redesign-final X1).
  head.el.classList.add("sk-head");

  // ── the strip (redesign-final M1: the demo's five tiles; no attention
  // band beside it, the Need you tile says what the Inbox holds) ────────
  const strip = kpiStrip(
    [
      { key: "online", label: "Stacks running", href: "/stacks" },
      { key: "inbox", label: "Need you", href: "/inbox" },
      { key: "newer", label: "Newer versions", href: "/update?all=1" },
      { key: "cpu", label: "Host CPU", href: "/host" },
      { key: "disk", label: "Root disk", href: "/host" },
    ],
    { loading: !current().fleet, label: "The host at a glance" },
  );
  strip.el.classList.add("sk-kpis");
  for (const [key, k] of strip.tiles)
    if (key === "inbox") drivable(k.el, INBOX);
    else drivable(k.el, KPI, key);

  // ── the stacks card ─────────────────────────────────────────────────
  const count = h("span", { class: "sk-count", "aria-live": "polite" });
  // The cards' order: every option shown, the active one pressed (the
  // table sorts by its own headers).
  const sorts = segSwitch({
    label: "Sort the cards",
    value: ui.sort,
    items: SORTS.map((o) => ({
      value: o.key,
      label: o.label,
      hint: `Order the cards by ${o.key === "cpu" ? "CPU, busiest first" : o.key === "name" ? "name" : "vmid"}`,
    })),
    onChange: (v) => {
      ui.sort = v;
      writeUrl();
      paint();
    },
    mark: (b, v) => void drivable(b, SORT, v),
  });
  sorts.el.classList.add("sk-sorts");
  const views = segSwitch({
    label: "View",
    value: ui.view,
    items: [
      {
        value: "table",
        label: "Table",
        hint: "Every stack as a table row; click a header to sort, another to sort by it next",
      },
      {
        value: "cards",
        label: "Cards",
        hint: "Every stack as a card with its CPU line",
      },
    ],
    onChange: (v) => {
      ui.view = v;
      try {
        localStorage.setItem(VIEW_KEY, v);
      } catch {
        // a private window: the choice lives in the address only
      }
      writeUrl();
      paint();
    },
    mark: (b, v) => void drivable(b, VIEW, v),
  });
  const chips = toggleChips({
    label: "Only",
    chips: ONLY.map((o) => ({ value: o.value, label: o.label, hint: o.hint })),
    selected: ui.only,
    onChange: (sel) => {
      ui.only = sel;
      writeUrl();
      paint();
    },
    drive: { id: ONLY_CHIP },
  });
  const tb = toolbar({
    search: {
      placeholder: "Find a stack or app",
      label: "Find a stack or app",
      value: ui.q,
      onInput: (v) => {
        ui.q = v;
        writeUrl();
        paint();
      },
    },
    groups: [chips.el],
    state: [count, sorts.el, views.el],
  });
  if (tb.search) drivable(tb.search, SEARCH);

  // The batch bar: between the toolbar and the list while stacks are ticked.
  const batchCount = h("strong", null, "");
  const batchNames = h("span", { class: "sk-muted" }, "");
  /**
   * @param {string} action
   * @param {string} label
   * @param {string} title
   * @param {boolean} [primary]
   */
  const batchBtn = (action, label, title, primary = false) => {
    const b = drivable(
      viaForm(
        h(
          "button",
          {
            type: "button",
            class: `kp-button kp-button--sm${primary ? " kp-button--primary" : ""}`,
            "data-batch": action,
            title,
          },
          label,
        ),
        "batch",
      ),
      BATCH,
      action,
    );
    b.addEventListener("click", () => void openBatch(action, [...ticked]));
    return b;
  };
  const batchDeploy = batchBtn(
    "deploy",
    "Deploy",
    "Deploy every ticked stack, one after the other, each backed up first",
    true,
  );
  // Update… is the one Update flow (invariant 149), not a blind batch: the
  // ticked stacks' apps with a newer version, seen before anything runs.
  const batchUpdate = drivable(
    viaForm(
      h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--sm",
          "data-batch": "update",
          title:
            "Update the ticked stacks in the Update flow: see what changes, then back up, update and verify",
        },
        "Update…",
      ),
      "update",
    ),
    BATCH,
    "update",
  );
  // redesign-integrate-5: the one Update flow (invariant 149) for every
  // ticked stack at once.
  batchUpdate.addEventListener("click", () =>
    ctx.navigate(updateHrefFor([...ticked])),
  );
  const untick = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm kp-button--ghost sk-iconbtn",
        "aria-label": "Untick all",
        title: "Untick every stack (Esc)",
      },
      "✕",
    ),
    CLEAR,
  );
  untick.addEventListener("click", () => {
    ticked.clear();
    paint();
  });
  const batch = h(
    "div",
    {
      class: "sk-batch",
      role: "region",
      "aria-label": "Act on the ticked stacks",
      hidden: "",
    },
    batchCount,
    batchNames,
    h(
      "div",
      { class: "sk-batch__acts" },
      batchBtn("backup", "Back up", "Back up every ticked stack now"),
      batchUpdate,
      batchDeploy,
      untick,
    ),
  );

  // Cards and table: two views of the same rows.
  const cards = h("div", {
    class: "sk-cards",
    role: "list",
    "aria-label": "Stacks",
  });
  const tbody = h("tbody");
  const selectAll = /** @type {HTMLInputElement} */ (
    h("input", {
      type: "checkbox",
      class: "kp-field__check",
      "aria-label": "Tick every stack shown",
      title: "Tick every stack shown; click again to untick them",
    })
  );
  drivable(selectAll, TICK_ALL);
  selectAll.addEventListener("change", () => {
    const shown = visible();
    if (selectAll.checked) for (const r of shown) ticked.add(r.name);
    else for (const r of shown) ticked.delete(r.name);
    paint();
  });
  /**
   * @param {string} label
   * @param {string} [kind] kp sort kind
   * @param {string} [cls]
   */
  const th = (label, kind = "text", cls = "") =>
    h("th", { "data-kp-sort": kind, ...(cls ? { class: cls } : {}) }, label);
  const tableWrap = h(
    "div",
    {
      class: "kp-datatable sk-table",
      "data-kp-datatable": "",
      "data-kp-sort-multi": "",
      "data-kp-remember": "stacks",
      "data-kp-page-sizes": "none",
    },
    h(
      "div",
      { class: "kp-table-wrap" },
      h(
        "table",
        { class: "kp-table" },
        h(
          "thead",
          null,
          h(
            "tr",
            null,
            h("th", { class: "sk-tick" }, selectAll),
            th("Stack", "text", "sk-id"),
            th("State", "text", "sk-state"),
            th("Apps up", "number", "num sk-apps"),
            th("Restarts", "number", "num sk-wide"),
            th("RAM used of limit", "number", "sk-wide"),
            th("CPU · 24 h", "number", "sk-wide"),
            th("Last backup", "text", "sk-mid"),
            th("Attention", "text", "sk-mid"),
            h("th", { class: "sk-right" }, "Actions"),
          ),
        ),
        tbody,
      ),
    ),
  );
  // Live view's `ui select` flashes this, whichever view is on.
  const listBox = h(
    "div",
    { class: "sk-list", "data-drive-list": "stacks" },
    cards,
    tableWrap,
  );
  const empty = h("div", { class: "sk-empty-slot", hidden: "" });
  const card = section({
    title: "All stacks",
    desc: "Click a row to open it; click the box at the start of a row to tick it (click again to untick).",
    id: "stack-list",
  });
  // redesign-final M1: the key hints sit in the card's footer, as the demo
  // draws them; the search's filter words are its own hint.
  const keys = keyRow([
    [["/"], "find"],
    [["j", "k"], "move"],
    [["x"], "tick"],
    [["Enter"], "open"],
    [["Esc"], "untick all"],
  ]);
  const foot = h(
    "div",
    { class: "sk-foot" },
    h("span", null, "Read from the host, live · CPU lines: the last 24 hours"),
    keys,
  );
  // One line per source that could not be read (the comparison, the
  // backups, the newer versions, the CPU lines): what, why, what to do.
  const errs = h("div", { class: "sk-errors", "aria-live": "polite" });
  /** @type {Map<string, HTMLElement>} */
  const errBySource = new Map();
  /**
   * @param {string} source
   * @param {import("../doctor.js").RouteError | null} e null: it read fine
   */
  const sourceError = (source, e) => {
    if (e) errBySource.set(source, errorBox(e));
    else errBySource.delete(source);
    errs.replaceChildren(...errBySource.values());
    errs.hidden = errBySource.size === 0;
  };
  errs.hidden = true;
  card.body.append(tb.el, errs, batch, listBox, empty, foot);

  root.replaceChildren(head.el, strip.el, card.el);

  const detach = attachDataTables(root);
  const table = dataTable(tableWrap);

  // ── painting ─────────────────────────────────────────────────────────
  const visible = () => {
    const q = parseQuery(ui.q);
    return rows.filter((r) => rowMatches(r, q, ui.only));
  };

  /** @param {Row} r */
  const tickBox = (r) => {
    const box = /** @type {HTMLInputElement} */ (
      h("input", {
        type: "checkbox",
        class: "kp-field__check sk-tickbox",
        "aria-label": `Tick ${r.name}`,
        title: `Tick ${r.name}; click again to untick`,
        "data-tick": r.name,
      })
    );
    drivable(box, TICK, r.name);
    box.checked = ticked.has(r.name);
    box.addEventListener("change", () => {
      if (box.checked) ticked.add(r.name);
      else ticked.delete(r.name);
      cursor = r.name;
      paint();
    });
    return box;
  };

  /** @param {Row} r */
  const actionEl = (r) => {
    const a = r.action;
    // Deploy, Back up and Update open their dialogs; Update through the
    // same `openAction(stack, "update")` the Update flow routes.
    if (a.kind === "deploy" || a.kind === "backup" || a.kind === "update") {
      const b = h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--sm",
          "data-action": a.kind,
          title: a.title,
        },
        a.label,
      );
      b.addEventListener("click", () => void openAction(r.name, a.kind));
      return drivable(b, a.kind === "update" ? ROW_UPDATE : ROW_ACTION, r.name);
    }
    // Logs is the hub's second tab.
    return drivable(
      h(
        "a",
        {
          class: "kp-button kp-button--sm",
          href: stackHref(r.name, "logs"),
          "data-action": "logs",
          title: a.title,
        },
        a.label,
      ),
      ROW_LOGS,
      r.name,
    );
  };

  /** @param {Row} r */
  const sparkCell = (r) =>
    h(
      "span",
      { class: "sk-cpu" },
      r.spark.length > 1
        ? sparkline(r.spark)
        : h("span", { class: "sk-muted" }, "no line yet"),
      h(
        "span",
        { class: "sk-num" },
        r.cpuPct == null ? "—" : `${Math.round(r.cpuPct)}%`,
      ),
    );

  /** @param {Row} r */
  const cardEl = (r) => {
    const link = drivable(
      h("a", { class: "sk-card__name", href: stackHref(r.name) }, r.name),
      OPEN,
      r.name,
    );
    const el = h(
      "article",
      {
        class: `sk-card${ticked.has(r.name) ? " is-ticked" : ""}${cursor === r.name ? " is-cursor" : ""}`,
        role: "listitem",
        "data-stack": r.name,
      },
      tickBox(r),
      h(
        "div",
        { class: "sk-card__id" },
        stackMark(r.name, 22),
        link,
        h("span", { class: "sk-card__vmid sk-mono" }, String(r.vmid)),
      ),
      h("span", { class: dotClass(r.state.tone) }, r.state.label),
      h(
        "div",
        { class: "sk-card__meta" },
        h(
          "span",
          null,
          h("b", null, `${r.appsUp}/${r.appsTotal}`),
          r.appsTotal ? " apps up" : " apps",
        ),
        h("span", null, h("b", null, String(r.restarts)), " restarts"),
        h(
          "span",
          { title: "The last backup" },
          h(
            "span",
            { class: `sk-backup sk-backup--${r.backup.tone || "none"}` },
            r.backup.state === "ok"
              ? `backed up ${r.backup.text}`
              : r.backup.text,
          ),
        ),
      ),
      // Its own row, kept (empty) when there is no flag, so every card has
      // the same height in every state.
      h(
        "div",
        { class: "sk-card__flags", "aria-label": "Flags" },
        ...r.flags.map(flagChip),
      ),
      h("div", { class: "sk-card__bar" }, h("span", null, "CPU"), sparkCell(r)),
    );
    return el;
  };

  /** @param {Row} r */
  const rowEl = (r) => {
    const ram =
      r.ramPct == null
        ? h("span", { class: "sk-muted" }, "not measured")
        : h(
            "span",
            { class: "sk-ram" },
            meter(r.ramPct),
            h(
              "span",
              { class: "sk-num" },
              `${gb(r.ramUsed)} of ${gb(r.ramMax)} GB`,
            ),
          );
    return h(
      "tr",
      {
        class: `sk-row${ticked.has(r.name) ? " is-ticked" : ""}${cursor === r.name ? " is-cursor" : ""}`,
        "data-stack": r.name,
        "data-kp-row-key": r.name,
      },
      h("td", { class: "sk-tick" }, tickBox(r)),
      h(
        "td",
        { class: "sk-id" },
        h(
          "span",
          { class: "sk-id__wrap" },
          stackMark(r.name, 24),
          h(
            "span",
            null,
            // On a phone the State column folds into this dot.
            h("span", {
              class: `${dotClass(r.state.tone)} sk-phone-dot`,
              role: "img",
              "aria-label": r.state.label,
              title: r.state.label,
            }),
            drivable(h("a", { href: stackHref(r.name) }, r.name), OPEN, r.name),
            h("small", { class: "sk-mono" }, `vmid ${r.vmid}`),
          ),
        ),
      ),
      h(
        "td",
        { class: "sk-state" },
        h("span", { class: dotClass(r.state.tone) }, r.state.label),
      ),
      h("td", { class: "num sk-apps" }, `${r.appsUp} of ${r.appsTotal}`),
      h("td", { class: "num sk-wide" }, String(r.restarts)),
      h("td", { class: "sk-wide" }, ram),
      h("td", { class: "sk-wide" }, sparkCell(r)),
      h(
        "td",
        { class: "sk-mid" },
        h(
          "span",
          { class: `sk-backup sk-backup--${r.backup.tone || "none"}` },
          r.backup.text,
        ),
      ),
      h(
        "td",
        { class: "sk-mid" },
        h("span", { class: "sk-flags" }, ...r.flags.map(flagChip)),
      ),
      h(
        "td",
        { class: "sk-right sk-acts" },
        actionEl(r),
        drivable(
          h(
            "a",
            {
              class: "kp-button kp-button--sm kp-button--ghost sk-iconbtn",
              href: stackHref(r.name),
              "aria-label": `Open ${r.name}'s hub`,
              title: `Open ${r.name}'s hub`,
            },
            "›",
          ),
          HUB_BUTTON,
          r.name,
        ),
      ),
    );
  };

  /** @param {number} pct */
  const meter = (pct) => {
    const fill = h("span");
    fill.style.inlineSize = `${Math.max(0, Math.min(100, pct))}%`;
    return h(
      "span",
      {
        class: `sk-meter${pct >= 90 ? " sk-meter--bad" : pct >= 80 ? " sk-meter--warn" : ""}`,
        role: "meter",
        "aria-valuenow": String(pct),
        "aria-valuemin": "0",
        "aria-valuemax": "100",
        "aria-label": `${pct}% of its limit`,
      },
      fill,
    );
  };

  /** @type {Map<string, {sig: string, el: HTMLElement}>} built rows, by view and stack */
  const built = new Map();
  /** @param {string} kind */
  const forget = (kind) => {
    for (const k of [...built.keys()])
      if (k.startsWith(`${kind}:`)) built.delete(k);
  };
  /**
   * Draw `shown` into `box`, rebuilding only the rows whose content
   * changed; the others keep their node (and so their focus and hover).
   * `ordered`: the order is ours (cards); the table keeps its own sort.
   * @param {HTMLElement} box
   * @param {Row[]} shown
   * @param {(r: Row) => HTMLElement} make
   * @param {string} kind
   * @param {boolean} ordered
   * @returns {boolean} whether anything changed
   */
  const patch = (box, shown, make, kind, ordered) => {
    const want = shown.map((r) => {
      const sig = JSON.stringify([r, ticked.has(r.name), cursor === r.name]);
      const key = `${kind}:${r.name}`;
      const prev = built.get(key);
      if (prev && prev.sig === sig && prev.el.isConnected) return prev.el;
      const el = make(r);
      built.set(key, { sig, el });
      return el;
    });
    const cur = /** @type {HTMLElement[]} */ ([...box.children]);
    const sameRows =
      cur.length === want.length &&
      (ordered
        ? cur.every((c, i) => c.dataset.stack === want[i].dataset.stack)
        : want.every((w) =>
            cur.some((c) => c.dataset.stack === w.dataset.stack),
          ));
    if (!sameRows) {
      box.replaceChildren(...want);
      return true;
    }
    let changed = false;
    for (const w of want) {
      const c = cur.find((x) => x.dataset.stack === w.dataset.stack);
      if (c && c !== w) {
        // A row whose content moved (a live CPU figure) keeps its node, so
        // its hover stays; its contents and the focus inside are renewed.
        morph(c, w);
        built.set(`${kind}:${c.dataset.stack}`, {
          sig: /** @type {any} */ (built.get(`${kind}:${c.dataset.stack}`)).sig,
          el: c,
        });
        changed = true;
      }
    }
    return changed;
  };
  /**
   * `c` takes `w`'s attributes and children; the element that had the
   * focus inside `c` gets it back in the new children (by its Live view
   * id and row).
   * @param {HTMLElement} c the node on screen
   * @param {HTMLElement} w the freshly built one
   */
  const morph = (c, w) => {
    const f = document.activeElement;
    const was =
      f instanceof HTMLElement && c.contains(f) && f.dataset.drive
        ? `[data-drive="${CSS.escape(f.dataset.drive)}"]${f.dataset.driveRow == null ? "" : `[data-drive-row="${CSS.escape(f.dataset.driveRow)}"]`}`
        : null;
    for (const a of [...c.attributes])
      if (!w.hasAttribute(a.name)) c.removeAttribute(a.name);
    for (const a of [...w.attributes]) c.setAttribute(a.name, a.value);
    c.replaceChildren(...w.childNodes);
    if (was)
      /** @type {HTMLElement | null} */ (c.querySelector(was))?.focus({
        preventScroll: true,
      });
  };

  /** Loading: skeletons in the final geometry of whichever view is on. */
  const paintLoading = () => {
    built.clear();
    const tableOn = ui.view === "table";
    // The skeleton card has the filled card's rows (id, meta, flags, CPU
    // line), so nothing moves when the fleet arrives.
    const bone = (/** @type {string} */ cls) =>
      h("span", { class: `kp-skeleton ${cls}` });
    cards.replaceChildren(
      ...(tableOn
        ? []
        : Array.from({ length: 6 }, () =>
            h(
              "div",
              {
                class: "sk-card sk-card--skeleton",
                role: "status",
                "aria-label": "Reading the fleet",
                "data-kp-state": "loading",
              },
              h("span", { class: "sk-card__tickbone" }),
              h("div", { class: "sk-card__id" }, bone("sk-bone--id")),
              bone("sk-bone--state"),
              h("div", { class: "sk-card__meta" }, bone("sk-bone--meta")),
              h("div", { class: "sk-card__flags" }, bone("sk-bone--flag")),
              h("div", { class: "sk-card__bar" }, bone("sk-bone--bar")),
            ),
          )),
    );
    if (!tableOn) {
      tbody.replaceChildren();
      count.textContent = "reading…";
      return;
    }
    tbody.replaceChildren(
      ...Array.from({ length: 4 }, () =>
        h(
          "tr",
          { "data-kp-skeleton-row": "" },
          h(
            "td",
            { colspan: "10" },
            skeletonBlock("1.6rem", "Reading the fleet"),
          ),
        ),
      ),
    );
    count.textContent = "reading…";
  };

  const paint = () => {
    const f = current().fleet;
    const tableOn = ui.view === "table";
    cards.hidden = tableOn;
    tableWrap.hidden = !tableOn;
    sorts.el.hidden = tableOn;
    views.set(ui.view);
    sorts.set(ui.sort);
    if (!f) {
      paintLoading();
      return;
    }
    // Forget ticks of stacks that are gone.
    for (const n of [...ticked])
      if (!rows.some((r) => r.name === n)) ticked.delete(n);
    const shown = sortRows(visible(), ui.sort);
    const counts = onlyCounts(rows);
    chips.setChips(
      ONLY.map((o) => ({
        value: o.value,
        label: o.label,
        hint: o.hint,
        count: counts[o.value],
      })),
    );
    count.textContent = `${shown.length} of ${rows.length}`;
    // Keep the focused control across a live repaint: only rows whose
    // content changed are rebuilt (by key), and a control that was
    // rebuilt anyway gets its focus back by its Live view id and row.
    const focus = /** @type {HTMLElement | null} */ (
      document.activeElement instanceof HTMLElement &&
      listBox.contains(document.activeElement)
        ? document.activeElement
        : null
    );
    const focusDrive = focus?.dataset.drive ?? null;
    const focusRow = focus?.dataset.driveRow ?? null;
    // Only the view that is on is drawn, so a Live view lookup never
    // lands on a hidden twin.
    if (tableOn) {
      cards.replaceChildren();
      forget("card");
      if (patch(tbody, shown, rowEl, "row", false)) table?.refresh();
    } else {
      tbody.replaceChildren();
      forget("row");
      patch(cards, shown, cardEl, "card", true);
    }
    if (focus && document.activeElement !== focus && focusDrive)
      /** @type {HTMLElement | null} */ (
        listBox.querySelector(
          `[data-drive="${CSS.escape(focusDrive)}"]${focusRow == null ? "" : `[data-drive-row="${CSS.escape(focusRow)}"]`}`,
        )
      )?.focus();
    selectAll.checked =
      shown.length > 0 && shown.every((r) => ticked.has(r.name));
    selectAll.indeterminate =
      !selectAll.checked && shown.some((r) => ticked.has(r.name));
    // Empty: why, and what fills it — in the same box as the list.
    if (rows.length === 0) {
      empty.replaceChildren(
        emptyState({
          art: art.noStacks(),
          title: "The host runs no stacks yet",
          text: "A stack is one container with the apps it runs. Start one from a preset or from a bundle another homelab exported.",
          action: (() => {
            const b = h(
              "button",
              { type: "button", class: "kp-button kp-button--primary" },
              "New stack…",
            );
            b.addEventListener("click", () => openNew());
            return drivable(b, EMPTY_NEW);
          })(),
        }),
      );
    } else if (shown.length === 0) {
      const clear = h(
        "button",
        {
          type: "button",
          class: "kp-button",
          title: "Clear the search and the Only filters (Esc)",
        },
        "Show every stack",
      );
      clear.addEventListener("click", () => resetFilters());
      drivable(clear, SHOW_ALL);
      empty.replaceChildren(
        emptyState({
          art: art.noMatch(),
          title: `No stack matches${ui.q ? ` “${ui.q}”` : " these filters"}`,
          text: "Clear the search or the Only filters to see every stack.",
          action: clear,
        }),
      );
    }
    empty.hidden = shown.length > 0;
    listBox.hidden = shown.length === 0;
    // The batch bar says what is ticked; it is absent when nothing is.
    const names = rows.filter((r) => ticked.has(r.name)).map((r) => r.name);
    batch.hidden = names.length === 0;
    batchCount.textContent = `${names.length} ticked`;
    batchNames.textContent = names.join(", ");
    batchDeploy.textContent = `Deploy ${names.length} stack${names.length === 1 ? "" : "s"}`;
  };

  const resetFilters = () => {
    ui.q = "";
    if (tb.search) tb.search.value = "";
    chips.reset();
    ui.only = new Set();
    writeUrl();
    paint();
  };

  const repaintHost = () => {
    const f = current().fleet;
    if (!f) return;
    const { items } = inboxNow();
    const tiles = hostStrip(
      f,
      trend,
      {
        count: items.length,
        urgent: items.filter((i) => i.severity === "bad").length,
      },
      newer,
    );
    for (const t of tiles) {
      const k = strip.tiles.get(t.key);
      if (!k) continue;
      k.set({ ...tileKpi(t), loading: false });
    }
    head.live?.set(f.measured_at);
  };

  const recompute = () => {
    const f = current().fleet;
    if (!f) {
      paint();
      return;
    }
    rows = stackRows(f, {
      newer,
      drift: drift?.stacks ?? {},
      calendar,
      trend,
      now: Date.now() / 1000,
    });
    paint();
    repaintHost();
    const n = planCount ?? deployCount(drift);
    deployBadge.hidden = n == null || n === 0;
    deployBadge.textContent = n == null ? "" : String(n);
    deployAll.title =
      n == null
        ? "Make every stack match its files in one confirmed batch (not compared yet: the panel compares first)"
        : n === 0
          ? "Every stack matched its files at the last comparison; open to compare again"
          : `${n} ${n === 1 ? "stack differs" : "stacks differ"} from ${n === 1 ? "its" : "their"} files (new and removed ones included); deploy them in one confirmed batch, each backed up first`;
  };

  // ── side panels ──────────────────────────────────────────────────────
  /** @type {ReturnType<typeof drawer> | null} */
  let panel = null;

  /** Deploy all changes: compare, read the plan, deploy (FLOWS.md task 13). */
  const openDeployAll = () => {
    panel?.close();
    const compareNote = h("p", { class: "sk-hint", "aria-live": "polite" });
    const compareBtn = dialogControl(
      h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--sm",
          id: "drift-compare",
          title:
            "Check every stack against its files again now and mark the ones that differ in the list",
        },
        "Compare again",
      ),
      "compare-now",
    );
    /** @type {{pending: number, broken: number} | null} */
    let planRead = null;
    // redesign-flows-4/5: the plan's own steps (apply.js) carry the
    // compare as step 1, done once a compare has run.
    /** @type {Set<() => void>} */
    const compareSubs = new Set();
    const compare = {
      button: h("div", { class: "ap-compare" }, compareBtn, compareNote),
      at: () =>
        planRead != null || deployCount(drift) != null
          ? (drift?.measured_at ?? Date.now() / 1000)
          : null,
      /** @param {() => void} f */
      onChange: (f) => {
        compareSubs.add(f);
        return () => compareSubs.delete(f);
      },
    };
    const paintCompare = () => {
      const n = planRead ? planRead.pending : deployCount(drift);
      const stacks = (/** @type {number} */ k) =>
        `${k} ${k === 1 ? "stack" : "stacks"}`;
      compareNote.textContent =
        n == null
          ? "Comparing every stack with its files…"
          : n === 0
            ? "Every stack matches its files."
            : `${stacks(n)} ${n === 1 ? "differs" : "differ"} from ${n === 1 ? "its" : "their"} files, new and removed ones included · marked in the list.`;
      compareSubs.forEach((f) => f());
    };
    paintCompare();
    compareBtn.addEventListener("click", () => {
      compareBtn.disabled = true;
      compareNote.textContent = "Comparing every stack with its files…";
      void readDrift(true)
        .catch(unlessAborted)
        .finally(() => {
          compareBtn.disabled = false;
          paintCompare();
        });
    });
    const planBody = h("div", { class: "sk-plan", id: "apply-section" });
    let stopPlan = () => {};
    panel = drawer({
      // redesign-final-c1: a wide sheet (stacks.css), the plan's steps,
      // tiles and columns at the approved apply.html's size.
      cls: "sk-drawer sk-drawer--plan",
      title: "Deploy all changes",
      desc: "Make every stack match its files in the repository, in one confirmed batch. Each stack is backed up before it changes.",
      body: [planBody],
      onClose: () => {
        stopPlan();
        panel = null;
        const s = setParams(location.search, { "deploy-all": null });
        if (s !== location.search)
          history.replaceState(history.state, "", location.pathname + s);
      },
    });
    panel.open();
    stopPlan = mountPlan(planBody, {
      compare,
      // The plan's own count is the exact one: it goes on the header
      // button and into the compare line, and the list's flags follow.
      onRead: (read) => {
        planRead = read;
        planCount = read.pending;
        paintCompare();
        recompute();
        void readDrift(false).catch(unlessAborted);
      },
    });
    const s = setParams(location.search, { "deploy-all": "1" });
    if (s !== location.search)
      history.replaceState(history.state, "", location.pathname + s);
  };

  /** New stack: the three routes, each ending on the same review. */
  const openNew = () => {
    panel?.close();
    /**
     * @param {string} title
     * @param {string} text
     * @param {string} form
     * @param {() => void} run
     * @param {string} [name] its Live view name in the panel (the form's
     *   own by default)
     */
    const route = (title, text, form, run, name = form) => {
      const b = viaForm(
        dialogControl(
          h(
            "button",
            { type: "button", class: "nx-action sk-route" },
            h("strong", null, title),
            h("span", null, text),
          ),
          name,
        ),
        form,
      );
      b.addEventListener("click", () => {
        panel?.close();
        run();
      });
      return b;
    };
    panel = drawer({
      cls: "sk-drawer",
      title: "New stack",
      desc: "A stack is one container with the apps it runs. Pick how to start; the wizard asks only what it cannot work out itself.",
      body: [
        h(
          "div",
          { class: "nx-actions sk-routes" },
          route(
            "From a preset",
            "Jellyfin, Vaultwarden, a static site… — tested settings; you pick a name and a disk size",
            "new-stack",
            () => void openNewStack(ctx.navigate),
          ),
          route(
            "From a bundle",
            "A stack exported from another homelab (a .tar.gz from Export bundle)",
            "import",
            () => void openImport(ctx.navigate),
          ),
          route(
            "Empty",
            "Start from a blank stack and add apps yourself, from its hub",
            "new-stack",
            () => void openNewStack(ctx.navigate, { empty: true }),
            "new-empty",
          ),
        ),
        h(
          "p",
          { class: "sk-hint" },
          "Every route ends on the same review: what will be created, which address it gets, and a deploy you confirm. ",
          dialogControl(
            h("a", { href: "/presets" }, "See every preset"),
            "see-presets",
          ),
        ),
      ],
      onClose: () => {
        panel = null;
      },
    });
    panel.open();
  };

  // ── the row's own click, the keys ────────────────────────────────────
  // A click anywhere on a card or a row opens its hub; its own controls
  // (the tick, its action, a link) do only their own thing (invariant 12).
  listBox.addEventListener("click", (e) => {
    const t = /** @type {Element} */ (e.target);
    if (t.closest("a, button, input, label")) return;
    const el = /** @type {HTMLElement | null} */ (t.closest("[data-stack]"));
    if (el?.dataset.stack) ctx.navigate(stackHref(el.dataset.stack));
  });
  /** @param {KeyboardEvent} e */
  const onKey = (e) => {
    if (e.defaultPrevented || e.ctrlKey || e.metaKey || e.altKey) return;
    if (document.querySelector("dialog[open]")) return;
    const a = document.activeElement;
    if (
      a instanceof HTMLElement &&
      (/^(INPUT|TEXTAREA|SELECT)$/.test(a.tagName) || a.isContentEditable) &&
      !(a instanceof HTMLInputElement && a.type === "checkbox")
    )
      return;
    const shown = sortRows(visible(), ui.sort);
    const i = shown.findIndex((r) => r.name === cursor);
    if (e.key === "j" || e.key === "k") {
      e.preventDefault();
      const next = Math.max(
        0,
        Math.min(shown.length - 1, i + (e.key === "j" ? 1 : -1)),
      );
      cursor = shown[next]?.name ?? null;
      paint();
      listBox
        .querySelector(
          `${ui.view === "table" ? "tbody" : ".sk-cards"} .is-cursor`,
        )
        ?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "x" && cursor) {
      e.preventDefault();
      if (ticked.has(cursor)) ticked.delete(cursor);
      else ticked.add(cursor);
      paint();
    } else if (
      e.key === "Enter" &&
      cursor &&
      !(a instanceof HTMLButtonElement || a instanceof HTMLAnchorElement)
    ) {
      e.preventDefault();
      ctx.navigate(stackHref(cursor));
    } else if (e.key === "Escape") {
      if (ticked.size) {
        ticked.clear();
        paint();
      } else if (ui.q || ui.only.size) resetFilters();
    }
  };
  document.addEventListener("keydown", onKey);

  // Live view (owner decision 2026-09-30): `homelab ui select` ticks stacks
  // here exactly as a click would, so a batch action opened right after it
  // acts on the same stacks.
  stops.push(
    register("overview", {
      select: (/** @type {string[]} */ stacks) => {
        ticked.clear();
        for (const s of stacks) ticked.add(s);
        paint();
      },
    }),
  );

  // ── reads ────────────────────────────────────────────────────────────
  /** @param {boolean} fresh */
  async function readDrift(fresh) {
    const r = await fetchJson(
      `/data/drift${fresh ? "?fresh=1" : ""}`,
      "the comparison with the files",
      abort.signal,
    );
    sourceError("drift", r.ok ? null : r.error);
    if (r.ok) {
      drift = r.body;
      recompute();
    }
  }
  const readTrend = async () => {
    const r = await fetchJson(
      "/data/fleet-trend",
      "the fleet's last day (the CPU lines)",
      abort.signal,
    );
    sourceError("trend", r.ok ? null : r.error);
    if (r.ok) {
      trend = r.body;
      recompute();
    }
  };
  /** An abort (the page closed) is not an error to show. @param {unknown} e */
  const unlessAborted = (e) => {
    if (!abort.signal.aborted) throw e;
  };
  // FLOWS.md §3 #13: the count on Deploy all changes shows before the
  // click, so the comparison is read with the page (the server reuses a
  // recent one rather than asking latch again).
  void readDrift(true).catch(unlessAborted);
  void readTrend().catch(unlessAborted);
  const trendTimer = setInterval(
    () => void readTrend().catch(unlessAborted),
    300_000,
  );
  void slowRead(
    "/data/backup-calendar",
    "the backups",
    abort.signal,
    (last) => {
      calendar = last;
      recompute();
    },
  )
    .then((r) => {
      sourceError("calendar", r.ok ? null : r.error);
      calendar = r.ok ? r.body : { stacks: {}, no_backup: [] };
      recompute();
    })
    .catch(unlessAborted);
  void slowRead(
    "/data/stale-images",
    "newer versions",
    abort.signal,
    (last) => {
      newer = newerByStack(last.images ?? []);
      recompute();
    },
  )
    .then((r) => {
      sourceError("newer", r.ok ? null : r.error);
      if (r.ok) newer = newerByStack(r.body.images ?? []);
      recompute();
    })
    .catch(unlessAborted);

  stops.push(subscribe(recompute));
  stops.push(onInbox(repaintHost));
  recompute();
  // `/stacks?deploy-all=1` (Deploy all changes, `g d`, the old `/apply`)
  // opens its panel at once.
  if (q0.get("deploy-all") === "1") openDeployAll();

  return () => {
    abort.abort();
    clearInterval(trendTimer);
    document.removeEventListener("keydown", onKey);
    panel?.close();
    detach();
    for (const s of stops) s();
    root.classList.remove("sk-page");
  };
}

/** The view this browser last chose (per viewer; the address wins). */
function readView() {
  try {
    return localStorage.getItem(VIEW_KEY) ?? "table";
  } catch {
    return "table";
  }
}
