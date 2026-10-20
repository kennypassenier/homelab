// The Firewall page (redesign-firewall, release 3.71.0; rebuilt on kp-themes
// 11.0.0 from the demo Kenny approved on 2026-10-19, app-homelab-backups,
// feat-kp11demo-3). It lives under System. Top to bottom: the header
// ("working copy <commit>", Topology), what needs a look (one alert per
// stack whose firewall is not in force, each with its Compare… and its
// fix), four totals (In force; No firewall, Live differs from files and Open
// paths filter the stacks and light the matrix), the stacks as one datatable,
// "Who can reach whom" as a matrix you can read live, from the files or as
// the differences, a hop-by-hop trace of one pair, the rules as one grouped table, and the guard in front of editing a
// stack whose management is open.
//
// feat-firewall-2 still holds: everything is read from the working copy's
// stack files (`/data/firewall`), and what the host says pve enforces wins
// over the declaration (fix-207). Edits go through each stack hub's Settings
// (FLOWS.md: the stack's Firewall tab merged there).
//
// `fwSummary` (fwview.js, re-exported here) is the one-stack summary the
// stack hub's Settings tab draws.

import { registerShortcuts } from "/static/kp/js/shortcuts.js";
import { GROUP_FIREWALL, unlessDialog } from "../shortcuts.js";
import { agoEl, setAgo } from "../ago.js";
import { bindTableUrl, ensureStyle, fetchJson, h } from "../dom.js";
import { declare, declareField, drivable } from "../drivable.js";
import { failSummary } from "../settingsview.js";
import {
  allPorts,
  cellIn,
  cellLabel,
  cellOf,
  cellWords,
  diffWords,
  fwAttention,
  fwKpis,
  fwSets,
  fwState,
  pairDiffers,
  portsText,
  reachCounts,
  ruleHits,
  rulesFor,
  ruleOrderText,
  traceHops,
} from "../fwview.js";
import { gridRow, formGrid } from "../gridrow.js";
import { stackHref } from "../router.js";
import { listen } from "../store.js";
import {
  attentionBand,
  button,
  failBand,
  kpiStrip,
  pageHeader,
} from "../ui.js";
import { setParams } from "../urlstate.js";
import {
  VIEW_EVENT,
  attachDataTables,
  dataTable,
} from "/static/kp/js/datatable.js";
import {
  MATRIX_LIGHT_EVENT,
  MATRIX_PICK_EVENT,
  attachMatrices,
  setHeatLegend,
  setMatrix,
  setMatrixLit,
  setMatrixPick,
} from "/static/kp/js/heat.js";
import { attachFootprints } from "/static/kp/js/footprint.js";
import { attachIdmarks } from "/static/kp/js/idmark.js";
import { attachHoverCards } from "/static/kp/js/hovercard.js";
import { attachKeys } from "/static/kp/js/keys.js";
import {
  SEGMENTED_EVENT,
  attachSegmented,
  setSegmentedValues,
} from "/static/kp/js/segmented.js";
import {
  TRACE_RETRY_EVENT,
  TRACE_RULE_EVENT,
  TRACE_SWAP_EVENT,
  attachTraces,
  setTrace,
  trace,
  traceSummary,
} from "/static/kp/js/path-trace.js";
import {
  CHANGE_EVENT,
  attachComboboxes,
  combobox,
} from "/static/kp/js/combobox.js";
import {
  TOGGLE_CHANGED_EVENT,
  attachToggles,
  toggle as toggleOf,
} from "/static/kp/js/toggle.js";
import { ACK_EVENT, ackGate, attachAckGates } from "/static/kp/js/ack-gate.js";
import {
  CARD_RETRY_EVENT,
  attachCardStates,
  setCardState,
} from "/static/kp/js/card-state.js";
import { attachDisabledReasons } from "/static/kp/js/reason.js";

export { fwSummary } from "../fwview.js";

/**
 * @typedef {import("../fwview.js").StackFw} StackFw
 * @typedef {import("../fwview.js").Matrix} Matrix
 * @typedef {import("../fwview.js").Cell} Cell
 * @typedef {import("../fwview.js").RuleRow} RuleRow
 * @typedef {import("../fwview.js").FirewallRead} FirewallRead
 */

// Live view (invariant 39): every control here only changes what the page
// shows; declared so `homelab ui click` reaches each one.
const LIGHT = declare({
  id: "firewall-light",
  page: "firewall",
  opens: "view",
  row: "<stack>",
  what: "light one stack up in the matrix (again turns it off; several may be on)",
});
const SHOW_ALL = declare({
  id: "firewall-show-all",
  page: "firewall",
  opens: "view",
  what: "turn every lit stack off again",
  shows: "while a stack is lit",
  reach: [{ do: "click", control: "firewall-light", row: "*" }],
});
const CELL = declare({
  id: "firewall-cell",
  page: "firewall",
  opens: "view",
  row: "<from>><to>",
  what: "pin one square of the matrix: who may reach whom, and the rules that decide it",
});
const RULE_STACK = declare({
  id: "firewall-rules-stack",
  page: "firewall",
  opens: "view",
  row: "<stack>",
  what: "show only one stack's rules in the Rules table (again shows every stack)",
});
const RULE = declare({
  id: "firewall-rule",
  page: "firewall",
  opens: "view",
  row: "<stack>/<in|out>/<n>",
  what: "open or close one rule's details",
});
const DECIDED_BY = declare({
  id: "firewall-decided-by",
  page: "firewall",
  opens: "view",
  row: "<stack>/<in|out>/<n>",
  what: "jump from the traced pair to the rule that decides it",
});
const CLEAR_RULES = declare({
  id: "firewall-clear-rule-filter",
  page: "firewall",
  opens: "view",
  what: "clear the Rules table's search and action filter",
  shows: "while the rule search or filter narrows the rules",
});
const TOPOLOGY = declare({
  id: "firewall-topology",
  page: "firewall",
  opens: "view",
  what: "go to the Map's topology with the measured traffic on",
});
const EDIT_RULES = declare({
  id: "firewall-edit-rules",
  page: "firewall",
  opens: "view",
  what: "go to the chosen stack's firewall on its Settings (Edit rules…, guarded when its management is open)",
});
const EDIT_THESE = declare({
  id: "firewall-edit-these-rules",
  page: "firewall",
  opens: "view",
  what: "go to the Rules table's stack's firewall on its Settings",
});
const FIX = declare({
  id: "firewall-fix",
  page: "firewall",
  opens: "view",
  row: "<stack>",
  what: "go to a stack's firewall on its Settings from its attention row (Declare… or Fix…)",
});
const COMPARE = declare({
  id: "firewall-compare",
  page: "firewall",
  opens: "view",
  row: "<stack>",
  what: "light a stack in the matrix with the Differences reading on (Compare…)",
});
const OPEN_STACK = declare({
  id: "firewall-open-stack",
  page: "firewall",
  opens: "view",
  row: "<stack>",
  what: "open a stack's hub from the Stacks table",
});
const EDIT_STACK = declare({
  id: "firewall-edit-stack",
  page: "firewall",
  opens: "view",
  row: "<stack>",
  what: "go to a stack's firewall on its Settings from its row (Edit or Declare…)",
});
const LIGHT_COL = declare({
  id: "firewall-light-column",
  page: "firewall",
  opens: "view",
  row: "<stack>",
  what: "light a stack up from its column name on the matrix",
});
const LIGHT_ROW = declare({
  id: "firewall-light-row",
  page: "firewall",
  opens: "view",
  row: "<stack>",
  what: "light a stack up from its row name on the matrix",
});
const UNLIGHT = declare({
  id: "firewall-unlight",
  page: "firewall",
  opens: "view",
  row: "<stack>",
  what: "turn one lit stack off from the reading above the matrix",
  shows: "while a stack is lit",
  reach: [{ do: "click", control: "firewall-light", row: "*" }],
});
const TRY_AGAIN = declare({
  id: "firewall-try-again",
  page: "firewall",
  opens: "view",
  what: "read what failed again",
  shows: "after the firewall could not be read",
});
const ACTION_FILTER = declare({
  id: "firewall-action-filter",
  page: "firewall",
  opens: "view",
  row: "all|ACCEPT|DROP",
  what: "show only ACCEPT or only DROP rules (again shows every rule), or All",
});
const RULE_SEARCH = declare({
  id: "firewall-rule-search",
  page: "firewall",
  opens: "view",
  what: "the Rules table's search box (address, port, stack or why)",
});
const EDIT_RULE = declare({
  id: "firewall-edit-rule",
  page: "firewall",
  opens: "view",
  row: "<stack>/<in|out>/<n>",
  what: "go to an opened rule's stack firewall on its Settings",
  shows: "in an opened rule",
  reach: [{ do: "click", control: "firewall-rule", row: "*" }],
});
const RULE_UP = declare({
  id: "firewall-rule-move-up",
  page: "firewall",
  opens: "view",
  row: "<stack>/<in|out>/<n>",
  what: "move an opened rule one place up: its stack's firewall editor opens with the move staged for review",
  shows: "in an opened rule that is not the first",
  reach: [{ do: "click", control: "firewall-rule", row: "*" }],
});
const RULE_OFF = declare({
  id: "firewall-rule-disable",
  page: "firewall",
  opens: "view",
  row: "<stack>/<in|out>/<n>",
  what: "switch an opened rule off (or back on): its stack's firewall editor opens with the change staged for review",
  shows: "in an opened rule",
  reach: [{ do: "click", control: "firewall-rule", row: "*" }],
});
const SORT = declare({
  id: "firewall-sort",
  page: "firewall",
  opens: "view",
  row: "stack|state|rules",
  what: "sort the Stacks table by a column (again flips it, a third time clears)",
});
const KPI = declare({
  id: "firewall-total",
  page: "firewall",
  opens: "view",
  row: "none|differs|open",
  what: "show only the stacks a total counts, and light them in the matrix (again shows every stack)",
});
const COMPARE_MODE = declare({
  id: "firewall-compare-mode",
  page: "firewall",
  opens: "view",
  row: "live|files|differences",
  what: "what the matrix shows: what pve enforces now, what the stack files declare, or only the pairs where they differ",
});
const TRACE_FROM = declare({
  id: "firewall-trace-from",
  page: "firewall",
  opens: "view",
  what: "the stack a traced connection starts at",
});
const TRACE_TO = declare({
  id: "firewall-trace-to",
  page: "firewall",
  opens: "view",
  what: "the stack a traced connection goes to",
});
const TRACE_PORT = declare({
  id: "firewall-trace-port",
  page: "firewall",
  opens: "view",
  what: "the port of the traced connection, or any port the rules allow",
});
// review M5: the page's own form fields (the controls above are what Live
// view presses; these are the ids the inputs carry in the page).
declareField({
  id: "fw-trace-from",
  page: "firewall",
  what: "the stack a traced connection starts at",
});
declareField({
  id: "fw-trace-to",
  page: "firewall",
  what: "the stack a traced connection goes to",
});
declareField({
  id: "fw-trace-port",
  page: "firewall",
  what: "the port of the traced connection",
});
const TRACE_COPY = declare({
  id: "firewall-trace-copy",
  page: "firewall",
  opens: "run",
  what: "copy the trace as text",
});
const TRACE_SWAP_BTN = declare({
  id: "firewall-trace-swap",
  page: "firewall",
  opens: "view",
  what: "turn the traced pair round (From and To change places; key S)",
});
const GUARD_STACK = declare({
  id: "firewall-guard-stack",
  page: "firewall",
  opens: "view",
  row: "<stack>",
  what: "choose the stack whose rules Edit rules… opens",
});

/** The settings address of a stack's firewall. @param {string} s */
const fwHref = (s) => `${stackHref(s, "settings")}?section=firewall`;

/** @param {string} href */
const go = (href) => {
  history.pushState(null, "", href);
  dispatchEvent(new PopStateEvent("popstate"));
};

/** @param {string} k */
const idmark = (k) =>
  h("span", {
    class: "kp-idmark kp-idmark--pattern",
    "data-kp-idmark": k,
    "aria-hidden": "true",
  });

/** The badge tone of a firewall state. */
const BADGE = /** @type {const} */ ({
  ok: "success",
  warn: "warning",
  bad: "destructive",
});

/**
 * One panel of the page: a form grid whose first row names the part (the
 * label track), says in one sentence what it does (the control track) and
 * carries the part's live reading (the value track); the part itself spans
 * every track under it.
 * @param {{title: string, desc: string, id?: string, span?: string}} spec
 */
function panel(spec) {
  const read = h("p", {
    class: "kp-formgrid__value fw-panel__read",
    role: "status",
  });
  const el = h(
    "section",
    {
      class: `fw-panel kp-formgrid${spec.span ? ` fw-c${spec.span}` : ""}`,
      "data-kp-formgrid-label": "fit",
      id: spec.id ?? null,
      "aria-label": spec.title,
    },
    h(
      "div",
      { class: "kp-formgrid__row fw-panel__head" },
      h("h2", { class: "kp-formgrid__label" }, spec.title),
      h("p", { class: "kp-formgrid__control" }, spec.desc),
      read,
    ),
  );
  return { el, read };
}

/**
 * A kp card around an area that loads, comes up empty or fails: the busy
 * card, the empty plate and the failed plate with a normal-size retry.
 * @param {string} label
 * @param {Node[]} body
 */
function card(label, body) {
  const data = h("div", { class: "kp-card__data" }, ...body);
  const el = h(
    "section",
    {
      class: "kp-card fw-card",
      "data-kp-state": "loading",
      "aria-label": label,
    },
    h("div", { class: "kp-card__stage" }, data),
  );
  return { el, data };
}

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  ensureStyle("/css/pages/config.css");
  ensureStyle("/css/pages/firewall.css");
  root.classList.add("cf-page", "fw-page");
  const abort = new AbortController();
  /** @type {(() => void)[]} */
  const cleanups = [() => root.classList.remove("cf-page", "fw-page")];
  const q0 = new URLSearchParams(location.search);

  /** @type {FirewallRead | null} */
  let data = null;
  /** @type {"loading" | "ready" | "empty" | "error"} */
  let state = "loading";
  let failure = "";
  const S = {
    /** @type {Set<string>} lit stacks */
    lit: new Set((q0.get("lit") ?? "").split(",").filter(Boolean)),
    /** the rules table's stack, "" for all */
    stack: q0.get("stack") ?? "",
    /** the pinned cell, "from>to" */
    cell: q0.get("cell") ?? "",
    /** @type {"live" | "files" | "differences"} */
    mode: ["live", "files", "differences"].includes(q0.get("compare") ?? "")
      ? /** @type {any} */ (q0.get("compare"))
      : "live",
    /** @type {"calm" | "caution"} how a pair that can connect is coloured */
    tone: "calm",
    /** @type {Set<string>} totals pressed */
    totals: new Set(),
    from: q0.get("from") ?? "",
    to: q0.get("to") ?? "",
    port: "any",
    /** @type {"first-refusal" | "all"} */
    stop: "first-refusal",
    /** the stack the guard has chosen */
    guard: "",
  };
  const keepUrl = () => {
    const next = setParams(location.search, {
      lit: [...S.lit].join(","),
      stack: S.stack,
      cell: S.cell,
      compare: S.mode === "live" ? null : S.mode,
    });
    history.replaceState(history.state, "", `${location.pathname}${next}`);
  };

  // ── header ────────────────────────────────────────────────────────────
  const commit = h("span", { class: "cf-mono" }, "…");
  const live = h("span", { class: "cf-live" }, "working copy ", commit);
  const head = pageHeader({
    title: "Firewall",
    desc: "What each container's Proxmox firewall lets through, read from the stack files. See who can reach whom, spot what is unprotected, and jump to a stack's rules to change them.",
    titleMeta: [live],
    actions: [
      drivable(
        h(
          "a",
          {
            class: "kp-button kp-button--secondary",
            href: "/map?traffic=1",
            title: "The same connections drawn as a graph on the Map",
          },
          "Topology",
        ),
        TOPOLOGY,
      ),
    ],
  });
  const fail = failBand({ drive: { id: TRY_AGAIN } });

  // ── what needs a look ─────────────────────────────────────────────────
  const attention = attentionBand([]);
  const needs = panel({
    title: "Needs a look",
    desc: "Stacks whose live firewall is not what the file says, worst first, each with its fix.",
    id: "fw-needs",
    span: "12",
  });
  needs.el.append(attention.el);

  // ── totals ────────────────────────────────────────────────────────────
  const totals = panel({
    title: "Totals",
    desc: "Press a tile to show only the stacks it counts.",
    id: "fw-totals",
    span: "12",
  });
  const kpis = kpiStrip(
    [
      {
        key: "protected",
        label: "In force",
        ctx: "stacks with a firewall that pve enforces",
      },
      {
        key: "none",
        label: "No firewall",
        ctx: "",
        toggle: {
          pressed: false,
          onToggle: () => pressTotal("none"),
          drive: { id: KPI, row: "none" },
        },
      },
      {
        key: "differs",
        label: "Live differs from files",
        ctx: "",
        toggle: {
          pressed: false,
          onToggle: () => pressTotal("differs"),
          drive: { id: KPI, row: "differs" },
        },
      },
      {
        key: "open",
        label: "Open paths",
        ctx: "",
        toggle: {
          pressed: false,
          onToggle: () => pressTotal("open"),
          drive: { id: KPI, row: "open" },
        },
      },
    ],
    { loading: true, label: "Firewall at a glance" },
  );
  totals.el.append(kpis.el);

  // ── the stacks ────────────────────────────────────────────────────────
  const stacks = panel({
    title: "The stacks",
    desc: "Every stack with its firewall state, its default policies and what it reaches. Press a row to light the stack in the matrix.",
    id: "fw-stacks",
    span: "12",
  });
  const ago = agoEl("read");
  const stTable = h(
    "div",
    {
      class: "kp-datatable fw-stacks-dt",
      id: "fw-stack-table",
      "data-kp-datatable": "",
      "data-kp-cards": "",
      "data-kp-page-size": "100",
      "data-kp-page-sizes": "none",
      "data-kp-sort-multi": "",
      "data-kp-remember": "firewall-stacks",
      "data-kp-busy-overlay": "",
      "data-kp-state": "loading",
    },
    h(
      "div",
      { class: "kp-datatable__bar" },
      h("input", {
        class: "kp-datatable__search",
        type: "search",
        "data-kp-datatable-search": "",
        placeholder: "Search the stacks",
        "aria-label": "Search the stacks",
      }),
    ),
    // The failed plate is the package's own: fail(reason) draws it with its Retry.
    h(
      "div",
      { class: "kp-table-wrap" },
      h(
        "table",
        { class: "kp-table" },
        h("caption", { class: "kp-sr-only" }, "Stacks and their firewalls"),
        h(
          "thead",
          null,
          h(
            "tr",
            null,
            h("th", { scope: "col", "data-kp-sort": "text" }, "Stack"),
            h(
              "th",
              {
                scope: "col",
                "data-kp-sort": "text",
                "data-kp-filter": "choice",
                "data-kp-sort-order":
                  "no firewall,declared, switched off,live differs from files,in force",
              },
              "Firewall",
            ),
            h("th", { scope: "col", "data-kp-sort": "text" }, "Inbound"),
            h("th", { scope: "col", "data-kp-sort": "text" }, "Outbound"),
            h(
              "th",
              { scope: "col", "data-kp-sort": "number", class: "kp-text-end" },
              "Rules",
            ),
            h(
              "th",
              { scope: "col", "data-kp-sort": "number", class: "kp-text-end" },
              "Reaches",
            ),
            h(
              "th",
              { scope: "col", "data-kp-sort": "number", class: "kp-text-end" },
              "Reached by",
            ),
            h("th", { scope: "col" }, "Actions"),
          ),
        ),
        h("tbody"),
      ),
    ),
    h(
      "div",
      { class: "kp-empty", "data-kp-datatable-empty": "", hidden: "" },
      h(
        "div",
        { "data-kp-datatable-empty-none": "" },
        h("p", { class: "kp-empty__title" }, "No stack files"),
        h(
          "p",
          null,
          "The working copy holds no stack files, so there is no firewall to show.",
        ),
      ),
      h(
        "div",
        { "data-kp-datatable-empty-nomatch": "", hidden: "" },
        h("p", { class: "kp-empty__title" }, "No stack matches"),
        h(
          "button",
          {
            type: "button",
            class: "kp-button kp-button--secondary",
            "data-kp-datatable-clear": "",
          },
          "Clear the search and filters",
        ),
      ),
    ),
    h(
      "div",
      { class: "kp-datatable__bar" },
      h("p", {
        class: "kp-datatable__status",
        "data-kp-datatable-status": "",
        role: "status",
        "aria-live": "polite",
      }),
      h("div", { class: "kp-datatable__pager", "data-kp-datatable-pager": "" }),
    ),
  );
  const stBody = /** @type {HTMLElement} */ (stTable.querySelector("tbody"));
  stacks.el.append(h("div", { class: "fw-table" }, stTable));
  const foot = h("p", { class: "fw-foot" }, ago);

  // ── the matrix ──────────────────────────────────────────
  const mxp = panel({
    title: "Who can reach whom",
    desc: "From a row to a column: the ports a connection may use. Press a cell to read it and to trace it below.",
    id: "fw-matrix",
    span: "12",
  });
  const modeItem = (/** @type {string} */ v, /** @type {string} */ label) =>
    drivable(
      h(
        "button",
        {
          type: "button",
          class: "kp-segmented__item",
          role: "radio",
          "aria-checked": String(S.mode === v),
          "data-value": v,
        },
        label,
      ),
      COMPARE_MODE,
      v,
    );
  const modeSeg = h(
    "div",
    {
      class: "kp-segmented",
      "data-kp-segmented": "single",
      role: "radiogroup",
      "aria-label": "Compare",
    },
    modeItem("live", "Live"),
    modeItem("files", "Files"),
    modeItem("differences", "Differences"),
  );
  const mxAgo = agoEl("read");
  const lit = h("p", { class: "kp-formgrid__control fw-lit", role: "status" });
  const showAll = drivable(
    button("Show all", {
      cls: "kp-button--sm kp-button--ghost",
      title: "Turn every lit stack off (Esc)",
      onClick: () => {
        S.lit.clear();
        paintAll();
      },
    }),
    SHOW_ALL,
  );
  const mxEl = h("div", {
    class: "kp-matrix",
    "data-kp-matrix": "",
  });
  const mxCard = card("Who can reach whom", [mxEl]);
  // kp's heat legend, under the matrix it explains.
  setHeatLegend(
    mxEl,
    [
      { tone: "ok", label: "some ports" },
      { tone: "bad", label: "every port (target unprotected)" },
      { tone: "muted", label: "nothing" },
      { tone: "warn", label: "differs from the files" },
    ],
    {
      keys: [
        [["←", "→", "↑", "↓"], "move"],
        ["Enter", "pin the cell"],
        ["S", "swap the traced pair"],
        ["Esc", "show every stack"],
      ],
    },
  );
  mxp.el.append(
    gridRow({
      label: "Compare",
      hideLabel: true,
      control: modeSeg,
      value: mxAgo,
    }),
    gridRow({ label: "Lit", control: lit, actions: showAll }),
    h("div", { class: "fw-matrix", "data-fw-span": "all" }, mxCard.el),
  );

  // ── the trace ─────────────────────────────────────────────────────────
  const trp = panel({
    title: "Can this stack reach that one?",
    desc: "Pick a source, a target and a port, and see where the connection goes through or stops, with the rule that decides.",
    id: "fw-trace",
    span: "12",
  });
  /** @param {string} id */
  function comboBox(id) {
    return h(
      "div",
      { class: "kp-combobox", "data-kp-combobox": "" },
      h("input", {
        class: "kp-combobox__input",
        id,
        type: "text",
        role: "combobox",
        autocomplete: "off",
        "aria-expanded": "false",
        "aria-controls": `${id}-list`,
      }),
      h("ul", {
        class: "kp-combobox__list",
        id: `${id}-list`,
        role: "listbox",
        hidden: "",
      }),
      h("p", {
        class: "kp-combobox__status",
        "data-kp-combobox-status": "",
        role: "status",
        "aria-live": "polite",
      }),
    );
  }
  const fromBox = comboBox("fw-trace-from");
  const toBox = comboBox("fw-trace-to");
  const portSel = /** @type {HTMLSelectElement} */ (
    h("select", {
      class: "kp-field__input",
      id: "fw-trace-port",
      "data-kp-select": "",
    })
  );
  const traceEl = h("div", {
    class: "kp-trace",
    "data-kp-trace": "",
    "data-kp-trace-copy": "",
    "data-kp-trace-swap": "",
    "data-kp-trace-stop": S.stop,
  });
  const traceCard = card("The path", [traceEl]);
  trp.el.append(
    gridRow({ label: "From", labelFor: "fw-trace-from", control: fromBox }),
    gridRow({ label: "To", labelFor: "fw-trace-to", control: toBox }),
    gridRow({ label: "Port", labelFor: "fw-trace-port", control: portSel }),
    h("div", { class: "fw-tracebox", "data-fw-span": "all" }, traceCard.el),
  );

  // ── the rules ─────────────────────────────────────────────────────────
  const rup = panel({
    title: "The rules",
    desc: "Inbound rules guard a stack, outbound rules let it out. Open a rule to see which pairs it decides.",
    id: "fw-rules",
    span: "12",
  });
  const toggleBtn = (/** @type {string} */ v) =>
    drivable(
      h(
        "button",
        {
          class: "kp-button",
          type: "button",
          "data-kp-toggle": "",
          "data-kp-look": "chip",
          "data-kp-toggle-value": v,
          "data-kp-count": "0",
        },
        v,
      ),
      ACTION_FILTER,
      v,
    );
  const toggles = h(
    "div",
    {
      class: "kp-toggles",
      role: "group",
      "aria-label": "Action",
      "data-kp-toggle-group": "action",
      "data-kp-toggle-clear": "",
    },
    h(
      "button",
      {
        class: "kp-button",
        type: "button",
        "data-kp-toggle": "",
        "data-kp-toggle-all": "",
        "data-kp-look": "chip",
        "aria-pressed": "true",
        "data-kp-count": "0",
      },
      "All",
    ),
    toggleBtn("ACCEPT"),
    toggleBtn("DROP"),
  );
  const editThese = drivable(
    button("Edit these rules", {
      cls: "kp-button--sm",
      title: "Edit the shown stack's rules on its Settings",
      onClick: () => S.stack && go(fwHref(S.stack)),
    }),
    EDIT_THESE,
  );
  const ruTable = h("div", { class: "fw-table", "data-fw-rules-host": "" });
  rup.el.append(
    gridRow({
      label: "Action",
      hideLabel: true,
      control: toggles,
      actions: editThese,
    }),
    ruTable,
  );

  // ── the guard in front of editing ─────────────────────────────────────
  const gdp = panel({
    title: "Edit its rules",
    desc: "Choose a stack, then open its rules on its Settings. A stack whose management is open asks for one tick first.",
    id: "fw-guard",
    span: "12",
  });
  const chips = h("div", {
    class: "kp-toggles fw-chips",
    role: "group",
    "aria-label": "Stack",
    "data-kp-toggle-group": "stack",
    "data-kp-toggle-mode": "single",
  });
  const warn = h("div", { class: "kp-alert__body" });
  const ack = h(
    "div",
    {
      class: "kp-alert kp-alert--warning",
      "data-kp-ack": "",
      "data-kp-ack-for": "#fw-edit-go",
      "data-kp-ack-key": "mgmt",
      "data-kp-ack-label":
        "I understand that a change here reaches the whole LAN",
    },
    warn,
  );
  const editGo = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--primary",
        id: "fw-edit-go",
      },
      "Edit rules…",
    ),
    EDIT_RULES,
  );
  const guardCard = card("Edit its rules", [
    formGrid([
      gridRow({ label: "Stack", hideLabel: true, control: chips, wide: true }),
      h("div", { "data-kp-formgrid-span": "all" }, ack),
      gridRow({
        buttons: true,
        control: h("div", { class: "kp-formgrid__buttons" }, editGo),
      }),
    ]),
  ]);
  gdp.el.append(h("div", { "data-fw-span": "all" }, guardCard.el));

  root.replaceChildren(
    head.el,
    fail.el,
    h(
      "div",
      { class: "fw-grid" },
      needs.el,
      totals.el,
      stacks.el,
      mxp.el,
      trp.el,
      rup.el,
      gdp.el,
      foot,
    ),
  );

  cleanups.push(
    attachSegmented(root),
    attachMatrices(root),
    attachTraces(root),
    attachToggles(root),
    attachAckGates(root),
    attachCardStates(root),
    attachHoverCards(root),
    attachFootprints(root),
    attachKeys(root),
    attachDisabledReasons(root),
  );
  const tables = attachDataTables(root, {
    detail: (row) => ruleDetail(row.dataset.kpRowKey ?? ""),
    decorate: (part, info) => {
      if (info.kind === "row-toggle" && info.value)
        drivable(part, RULE, info.value);
    },
  });
  cleanups.push(tables);
  const comboHandles = attachComboboxes(root);
  cleanups.push(comboHandles);
  // What Live view reaches that the package draws itself.
  const toggleAll = toggles.querySelector("[data-kp-toggle-all]");
  if (toggleAll instanceof HTMLElement)
    drivable(toggleAll, ACTION_FILTER, "all");
  const fromInput = fromBox.querySelector("input");
  if (fromInput) drivable(fromInput, TRACE_FROM);
  const toInput = toBox.querySelector("input");
  if (toInput) drivable(toInput, TRACE_TO);
  drivable(portSel, TRACE_PORT);
  /** The package may redraw its parts after a call returns; mark them again. */
  const watch = (
    /** @type {HTMLElement} */ el,
    /** @type {() => void} */ fn,
  ) => {
    const mo = new MutationObserver(() => fn());
    mo.observe(el, { childList: true, subtree: true });
    cleanups.push(() => mo.disconnect());
  };
  const markTrace = () => {
    const copy = traceEl.querySelector(".kp-trace__copy");
    const swap = traceEl.querySelector(".kp-trace__swap");
    if (copy instanceof HTMLElement) drivable(copy, TRACE_COPY);
    if (swap instanceof HTMLElement) drivable(swap, TRACE_SWAP_BTN);
    for (const chip of /** @type {HTMLElement[]} */ ([
      ...traceEl.querySelectorAll(".kp-trace__rule"),
    ])) {
      const text = (chip.textContent ?? "").trim();
      const r = ruleOfChip(text);
      drivable(chip, DECIDED_BY, r ? `${r.stack}/${r.dir}/${r.n}` : text);
    }
  };
  /** Live view reaches a header or a square by its name. */
  const markMatrix = () => {
    for (const head of /** @type {HTMLElement[]} */ ([
      ...mxEl.querySelectorAll("button.kp-matrix__col, button.kp-matrix__row"),
    ])) {
      const k = head.dataset.key ?? "";
      drivable(
        head,
        head.classList.contains("kp-matrix__col") ? LIGHT_COL : LIGHT_ROW,
        k,
      );
    }
    for (const c of /** @type {HTMLElement[]} */ ([
      ...mxEl.querySelectorAll(".kp-matrix__cell[data-row-key][data-col-key]"),
    ]))
      drivable(c, CELL, `${c.dataset.rowKey}>${c.dataset.colKey}`);
  };
  /** The package adds its Clear button to a toggle group when one is pressed. */
  const markClear = () => {
    for (const b of /** @type {HTMLElement[]} */ ([
      ...root.querySelectorAll(".kp-toggle__clear"),
    ]))
      drivable(b, CLEAR_RULES);
  };
  watch(root, markClear);
  markClear();
  watch(mxEl, markMatrix);
  watch(traceEl, markTrace);
  const stDt = () => dataTable(stTable);
  const ruDt = () => {
    const t = ruTable.querySelector(".kp-datatable");
    return t ? dataTable(t) : null;
  };
  cleanups.push(bindTableUrl(stDt(), "firewall-stacks"));

  // ── data views ────────────────────────────────────────────────────────
  const names = () =>
    data?.matrix?.stacks ?? data?.stacks.map((s) => s.stack) ?? [];

  /** A cell in the page's colours. @param {Cell} c */
  const cellView = (c) => {
    const words = cellWords(c);
    if (c.state === "open")
      return {
        tone: /** @type {const} */ ("bad"),
        text: "all",
        label: words,
        tip: words,
      };
    if (c.state === "some")
      return {
        tone:
          S.tone === "calm"
            ? /** @type {const} */ ("ok")
            : /** @type {const} */ ("warn"),
        text: cellLabel(c),
        label: words,
        tip: words,
        ringed: c.stopped_at_source.length > 0,
      };
    return c.stopped_at_source.length
      ? {
          tone: /** @type {const} */ ("bad"),
          label: words,
          tip: words,
          ringed: true,
        }
      : { tone: /** @type {const} */ ("muted"), label: words, tip: words };
  };

  /** The cell of a pair in the reading on show. @param {string} r @param {string} c */
  const cellFor = (r, c) => {
    if (!data || r === c) return undefined;
    if (S.mode === "differences") {
      if (!pairDiffers(data, r, c))
        return {
          tone: /** @type {const} */ ("muted"),
          label: "same live and in the files",
          tip: "Live and files agree.",
        };
      const words = diffWords(data, r, c);
      return {
        tone: /** @type {const} */ ("warn"),
        text: "≠",
        label: words,
        tip: words,
      };
    }
    const cell = cellIn(data, r, c, S.mode === "files" ? "files" : "live");
    return cell ? cellView(cell) : undefined;
  };

  /** @param {string} stack */
  const toggleLit = (stack) => {
    if (S.lit.has(stack)) S.lit.delete(stack);
    else S.lit.add(stack);
  };

  const paintHeader = () => {
    if (!data) return;
    commit.replaceChildren(String(data.head?.commit ?? "").slice(0, 10) || "—");
    live.title = data.head?.subject ?? "";
  };

  // totals ----------------------------------------------------------------
  const paintTotals = () => {
    if (!data) {
      for (const t of kpis.tiles.values()) t.set({ loading: true });
      return;
    }
    const k = fwKpis(data);
    const sets = fwSets(data);
    kpis.tiles.get("protected")?.set({
      value: String(k.protected),
      unit: `of ${k.total}`,
      tone: k.protected < k.total ? "warn" : null,
      meter: { pct: k.total ? (k.protected / k.total) * 100 : 0 },
      ctx: "stacks with a firewall that pve enforces",
    });
    kpis.tiles.get("none")?.set({
      value: String(sets.none.length),
      tone: sets.none.length ? "bad" : null,
      ctx: sets.none.join(", ") || "every stack declares one",
      toggle: {
        pressed: S.totals.has("none"),
        onToggle: () => pressTotal("none"),
      },
    });
    kpis.tiles.get("differs")?.set({
      value: String(sets.differs.length),
      tone: sets.differs.length ? "warn" : null,
      ctx: sets.differs.join(", ") || "live matches the files",
      toggle: {
        pressed: S.totals.has("differs"),
        onToggle: () => pressTotal("differs"),
      },
    });
    kpis.tiles.get("open")?.set({
      value: String(k.open),
      tone: k.open ? "warn" : null,
      ctx: sets.openTargets.length
        ? `to ${sets.openTargets.join(" and ")}`
        : "no target lets everything in",
      toggle: {
        pressed: S.totals.has("open"),
        onToggle: () => pressTotal("open"),
      },
    });
  };
  /** The stacks the pressed totals count (none pressed: no filter). */
  const totalKeys = () => {
    if (!data || !S.totals.size) return [];
    const sets = fwSets(data);
    const map = {
      none: sets.none,
      differs: sets.differs,
      open: sets.openTargets,
    };
    return [
      ...new Set(
        [...S.totals].flatMap(
          (k) => map[/** @type {keyof typeof map} */ (k)] ?? [],
        ),
      ),
    ];
  };
  /** @param {"none" | "differs" | "open"} k */
  function pressTotal(k) {
    if (S.totals.has(k)) S.totals.delete(k);
    else S.totals.add(k);
    applyTotals();
    paintTotals();
  }
  const applyTotals = () => {
    const keys = totalKeys();
    stDt()?.filter(0, keys.length ? keys : null);
    paintMx();
    const n = names().length;
    totals.read.textContent = !data
      ? ""
      : keys.length
        ? `Showing ${keys.length.toLocaleString("en-GB")} of ${n.toLocaleString("en-GB")} stacks: ${keys.join(", ")}.`
        : `Showing all ${n.toLocaleString("en-GB")} stacks.`;
  };

  // needs a look ----------------------------------------------------------
  const paintNeeds = () => {
    if (!data) return attention.set([]);
    const items = fwAttention(data.stacks).map((a) => ({
      key: a.stack,
      tone: a.tone,
      title: a.title,
      text: a.text,
      action: h(
        "span",
        { class: "fw-fixes" },
        drivable(
          button("Compare…", {
            cls: "kp-button--sm",
            title:
              "Show the stack in the matrix with only the pairs where live and files differ",
            onClick: () => {
              S.mode = "differences";
              S.lit = new Set([a.stack]);
              paintAll();
              mxp.el.scrollIntoView({ block: "start", behavior: "smooth" });
            },
          }),
          COMPARE,
          a.stack,
        ),
        drivable(
          h(
            "a",
            {
              class: "kp-button kp-button--sm kp-button--secondary",
              href: fwHref(a.stack),
              title: a.hint,
            },
            a.fix === "Compare…" ? "Fix…" : a.fix,
          ),
          FIX,
          a.stack,
        ),
      ),
    }));
    attention.set(items);
    needs.read.textContent = items.length
      ? `${items.length.toLocaleString("en-GB")} ${items.length === 1 ? "stack needs" : "stacks need"} a look.`
      : "";
    needs.el.hidden = items.length === 0;
  };

  // the stacks table -------------------------------------------------------
  /** @type {StackFw[] | null} */
  let stDrawn = null;
  const paintStacks = () => {
    const dt = stDt();
    if (!dt) return;
    if (!data) return;
    const m = data.matrix;
    if (stDrawn !== data.stacks) {
      stDrawn = data.stacks;
      stBody.replaceChildren(
        ...data.stacks.map((s) => {
          const st = fwState(s);
          const rc = m ? reachCounts(m, s.stack) : { reaches: 0, reachedBy: 0 };
          const tr = h(
            "tr",
            {
              "data-kp-row-key": s.stack,
              "aria-selected": String(S.lit.has(s.stack)),
            },
            h(
              "td",
              { "data-label": "Stack", "data-kp-filter-value": s.stack },
              idmark(s.stack),
              " ",
              drivable(
                h(
                  "a",
                  { href: stackHref(s.stack), title: `Open ${s.stack}` },
                  h("b", null, s.stack),
                ),
                OPEN_STACK,
                s.stack,
              ),
              " ",
              h(
                "span",
                {
                  class: "fw-ct",
                  ...(s.management_open
                    ? {
                        "data-kp-hovercard": `Management open: ${s.management_open}`,
                      }
                    : {}),
                },
                `CT ${s.vmid}`,
              ),
            ),
            h(
              "td",
              { "data-label": "Firewall", "data-kp-filter-value": st.word },
              h(
                "span",
                { class: `kp-badge kp-badge--${BADGE[st.tone]}` },
                st.word,
              ),
            ),
            h("td", { "data-label": "Inbound" }, s.policy_in ?? "—"),
            h("td", { "data-label": "Outbound" }, s.policy_out ?? "—"),
            h(
              "td",
              { "data-label": "Rules", class: "kp-text-end kp-numeric" },
              String(s.rules),
            ),
            h(
              "td",
              { "data-label": "Reaches", class: "kp-text-end kp-numeric" },
              String(rc.reaches),
            ),
            h(
              "td",
              { "data-label": "Reached by", class: "kp-text-end kp-numeric" },
              String(rc.reachedBy),
            ),
            h(
              "td",
              { "data-label": "Actions" },
              h(
                "div",
                { class: "kp-row-actions" },
                drivable(
                  h(
                    "a",
                    {
                      class: "kp-button kp-button--sm",
                      href: fwHref(s.stack),
                      title: s.declared
                        ? "Edit on the stack's Settings"
                        : "Declare a firewall for this stack on its Settings",
                    },
                    s.declared ? "Edit" : "Declare…",
                  ),
                  EDIT_STACK,
                  s.stack,
                ),
              ),
            ),
          );
          drivable(tr, LIGHT, s.stack);
          tr.addEventListener("click", (e) => {
            if (/** @type {Element} */ (e.target).closest("a,button,input"))
              return;
            toggleLit(s.stack);
            S.stack = s.stack;
            paintAll();
          });
          return tr;
        }),
      );
      attachIdmarks(stTable);
      dt.refresh();
      const ths = /** @type {HTMLElement[]} */ ([
        ...stTable.querySelectorAll("thead th[data-kp-sort]"),
      ]);
      ["stack", "state", "", "", "rules"].forEach((k, i) => {
        if (k && ths[i]) drivable(ths[i], SORT, k);
      });
    }
    for (const tr of stBody.querySelectorAll("tr[data-kp-row-key]"))
      tr.setAttribute(
        "aria-selected",
        String(
          S.lit.has(/** @type {HTMLElement} */ (tr).dataset.kpRowKey ?? ""),
        ),
      );
  };

  // the matrix --------------------------------------------------------------
  const paintMx = () => {
    if (!data) return;
    const m = data.matrix;
    const ns = names();
    const nodes = ns.map((k) => ({ key: k, label: k }));
    if (!m) {
      setMatrix(mxEl, {
        rows: [],
        columns: [],
        state: "empty",
        message:
          "The host's answer carried no reachability matrix; it needs at least one stack with an address.",
      });
      return;
    }
    const sets = fwSets(data);
    const label =
      S.mode === "files"
        ? "What the stack files declare"
        : S.mode === "differences"
          ? "Pairs where live and files differ"
          : "What pve enforces now";
    setMatrix(mxEl, {
      corner: "from ↓ to →",
      rows: nodes,
      columns: nodes,
      label,
      state: ns.length ? "ready" : "empty",
      message: "No stack has a firewall declared yet.",
      cell: cellFor,
      lit: [...new Set([...S.lit, ...totalKeys()])],
    });
    markMatrix();
    const pick = pinnedPair();
    if (pick) setMatrixPick(mxEl, { row: pick[0], column: pick[1] });
    const diff = ns.length
      ? ns.flatMap((a) =>
          ns.filter(
            (b) => a !== b && pairDiffers(data ?? { stacks: [] }, a, b),
          ),
        ).length
      : 0;
    mxp.read.textContent =
      S.mode === "differences"
        ? `${diff.toLocaleString("en-GB")} ${diff === 1 ? "pair differs" : "pairs differ"} between live and the files.`
        : pick
          ? cellReading(pick)
          : "Press a cell to read it in words.";
    lit.textContent = S.lit.size
      ? `Lit up: ${[...S.lit].join(", ")}.`
      : "Press stack names to light them up; each press turns one on or off.";
    showAll.hidden = S.lit.size === 0;
  };

  /** @returns {[string, string] | null} the pinned pair */
  const pinnedPair = () => {
    if (!data?.matrix) return null;
    if (S.cell.includes(">")) {
      const [a, b] = S.cell.split(">");
      if (cellOf(data.matrix, a, b)) return [a, b];
    }
    return null;
  };
  /** @param {[string, string]} p */
  const cellReading = (p) => {
    const c = data
      ? cellIn(data, p[0], p[1], S.mode === "files" ? "files" : "live")
      : null;
    return c ? cellWords(c) : "";
  };

  // the trace ---------------------------------------------------------------
  const paintTrace = () => {
    const h2 = trace(traceEl);
    if (state === "loading") return void h2?.setState("running");
    if (state === "error")
      return void h2?.setState("error", {
        message: failure || "The firewall did not answer. Nothing was changed.",
      });
    if (!data?.matrix || !names().length) return void h2?.setState("empty");
    if (!S.from || !S.to || S.from === S.to) {
      trp.read.textContent =
        S.from && S.from === S.to
          ? "Pick two different stacks."
          : "Pick a source and a target.";
      return void h2?.setState("empty");
    }
    const hops = traceHops(data, S.from, S.to, S.port);
    traceEl.setAttribute("data-kp-trace-stop", S.stop);
    setTrace(traceEl, hops);
    h2?.setState("ready");
    setTrace(traceEl, hops);
    trp.read.textContent = traceSummary(hops, S.stop);
    markTrace();
  };
  /** The rule a chip names ("kyu in #3"). @param {string} text */
  const ruleOfChip = (text) => {
    const m = /^(\S+) (in|out) #(\d+)$/.exec(text);
    if (!m || !data?.matrix) return null;
    return (
      data.matrix.rules.find(
        (r) => r.stack === m[1] && r.dir === m[2] && String(r.n) === m[3],
      ) ?? null
    );
  };
  const fillTraceControls = () => {
    const ns = names();
    for (const [box, key] of /** @type {const} */ ([
      [fromBox, "from"],
      [toBox, "to"],
    ])) {
      const list = /** @type {HTMLElement} */ (
        box.querySelector(".kp-combobox__list")
      );
      list.replaceChildren(
        ...ns.map((n) =>
          h(
            "li",
            {
              class: "kp-combobox__option",
              role: "option",
              "data-kp-option": "",
              "data-value": n,
            },
            n,
          ),
        ),
      );
      combobox(box)?.refresh?.();
    }
    const ports = data?.matrix ? allPorts(data.matrix) : [];
    portSel.replaceChildren(
      h("option", { value: "any" }, "Any port the rules allow"),
      ...ports.map((p) => h("option", { value: p }, p)),
    );
    portSel.value = S.port;
    putTrace();
  };
  let putting = false;
  const putTrace = () => {
    putting = true;
    try {
      combobox(fromBox)?.set([S.from]);
      combobox(toBox)?.set([S.to]);
      portSel.value = [...portSel.options].some((o) => o.value === S.port)
        ? S.port
        : "any";
    } finally {
      putting = false;
    }
  };

  // the rules table ---------------------------------------------------------
  const ruleKey = (/** @type {RuleRow} */ r) => `${r.stack}/${r.dir}/${r.n}`;
  /** @param {string} key */
  const ruleDetail = (key) => {
    const m = data?.matrix;
    const r = m?.rules.find((x) => ruleKey(x) === key);
    if (!m || !r) return null;
    const pairs = m.cells.filter((c) => ruleHits(r, c.from, c.to));
    const box = h(
      "div",
      { class: "fw-detail" },
      h(
        "p",
        null,
        h("b", null, r.action === "ACCEPT" ? "Lets through" : "Drops"),
        `: ${r.note || "no reason given"}.`,
        r.disabled
          ? " Switched off: Proxmox skips it."
          : r.enabled
            ? ""
            : " Not in force: declared in the file, not applied yet.",
      ),
      h(
        "p",
        null,
        pairs.length
          ? `Decides ${pairs.length.toLocaleString("en-GB")} ${pairs.length === 1 ? "pair" : "pairs"}: ${pairs.map(cellWords).join(" ")}`
          : `Names ${r.peer}, which is not a stack, so it decides no pair of the matrix.`,
      ),
      h("p", { class: "fw-hint" }, ruleOrderText(r, m.rules)),
      h(
        "div",
        { class: "kp-row-actions" },
        drivable(
          h(
            "a",
            {
              class: "kp-button kp-button--sm",
              href: fwHref(r.stack),
              title:
                "Edit, move or switch off this rule on the stack's Settings",
            },
            "Edit this rule",
          ),
          EDIT_RULE,
          key,
        ),
        r.n > 1
          ? drivable(
              h(
                "a",
                {
                  class: "kp-button kp-button--sm kp-button--ghost",
                  href: `${fwHref(r.stack)}&rule=${r.n}&do=up`,
                  title:
                    "Move this rule one place up: the stack's firewall opens with the move staged; you review and write it there",
                },
                "Move up",
              ),
              RULE_UP,
              key,
            )
          : null,
        drivable(
          h(
            "a",
            {
              class: "kp-button kp-button--sm kp-button--ghost",
              href: `${fwHref(r.stack)}&rule=${r.n}&do=${r.disabled ? "enable" : "disable"}`,
              title: r.disabled
                ? "Switch this rule back on: the stack's firewall opens with it staged; you review and write it there"
                : "Switch this rule off (it stays in the file, Proxmox skips it): the stack's firewall opens with it staged; you review and write it there",
            },
            r.disabled ? "Enable" : "Disable",
          ),
          RULE_OFF,
          key,
        ),
      ),
    );
    return box;
  };
  /** The rules table is drawn once per read. @type {RuleRow[] | null} */
  let ruDrawn = null;
  const paintRules = () => {
    const m = data?.matrix;
    if (!m) return;
    if (ruDrawn !== m.rules) {
      ruDrawn = m.rules;
      const head = (/** @type {"in" | "out"} */ dir) =>
        `<tr data-kp-row-group="${dir}" data-kp-group-always><th scope="rowgroup" colspan="9"><b>${dir === "in" ? "Inbound" : "Outbound"}</b> <span class="kp-badge" data-kp-group-count></span> <span class="fw-hint">${dir === "in" ? "what may reach the stack" : "what the stack may open"}</span></th></tr>`;
      ruTable.innerHTML = "";
      const shell = h(
        "div",
        {
          class: "kp-datatable",
          id: "fw-rule-table",
          "data-kp-datatable": "",
          "data-kp-cards": "",
          "data-kp-expandable": "",
          "data-kp-expand-single": "",
          "data-kp-column-menu": "",
          "data-kp-sort-multi": "click",
          "data-kp-page-size": "100",
          "data-kp-page-sizes": "none",
          "data-kp-max-height": "38rem",
          "data-kp-remember": "firewall-rules",
          "data-kp-busy-overlay": "",
        },
        h(
          "div",
          { class: "kp-datatable__bar" },
          drivable(
            h("input", {
              class: "kp-datatable__search",
              type: "search",
              "data-kp-datatable-search": "",
              placeholder: "Search by address, port, stack or why",
              "aria-label": "Search the rules",
            }),
            RULE_SEARCH,
          ),
        ),
        // The failed plate is the package's own: fail(reason) draws it with its Retry.
        h(
          "div",
          { class: "kp-table-wrap" },
          h(
            "table",
            { class: "kp-table" },
            h(
              "caption",
              { class: "kp-sr-only" },
              "Firewall rules, grouped inbound and outbound",
            ),
            h(
              "thead",
              null,
              h(
                "tr",
                null,
                h("th", { scope: "col", "data-kp-sort": "number" }, "#"),
                h(
                  "th",
                  {
                    scope: "col",
                    "data-kp-sort": "text",
                    "data-kp-filter": "choice",
                  },
                  "Stack",
                ),
                h(
                  "th",
                  {
                    scope: "col",
                    "data-kp-sort": "text",
                    "data-kp-filter": "choice",
                  },
                  "Action",
                ),
                h("th", { scope: "col", "data-kp-sort": "text" }, "Peer"),
                h("th", { scope: "col", "data-kp-sort": "text" }, "Ports"),
                h("th", { scope: "col", "data-kp-sort": "text" }, "Why"),
                h("th", { scope: "col", "data-kp-sort": "text" }, "In force"),
              ),
            ),
            h("tbody"),
          ),
        ),
        h(
          "div",
          { class: "kp-empty", "data-kp-datatable-empty": "", hidden: "" },
          h(
            "div",
            { "data-kp-datatable-empty-none": "" },
            h("p", { class: "kp-empty__title" }, "No rules yet"),
            h(
              "p",
              null,
              "Declare a firewall in a stack file to see its rules here.",
            ),
          ),
          h(
            "div",
            { "data-kp-datatable-empty-nomatch": "", hidden: "" },
            h("p", { class: "kp-empty__title" }, "No rule matches"),
            drivable(
              h(
                "button",
                {
                  type: "button",
                  class: "kp-button kp-button--secondary",
                  "data-kp-datatable-clear": "",
                },
                "Clear the search and filters",
              ),
              CLEAR_RULES,
            ),
          ),
        ),
        h(
          "div",
          { class: "kp-datatable__bar" },
          h("p", {
            class: "kp-datatable__status",
            "data-kp-datatable-status": "",
            role: "status",
            "aria-live": "polite",
          }),
          h("div", {
            class: "kp-datatable__pager",
            "data-kp-datatable-pager": "",
          }),
        ),
      );
      ruTable.append(shell);
      const tbody = /** @type {HTMLElement} */ (shell.querySelector("tbody"));
      for (const dir of /** @type {const} */ (["in", "out"])) {
        const g = h("tbody");
        g.innerHTML = head(dir);
        tbody.append(...g.children);
        for (const r of m.rules.filter((x) => x.dir === dir)) {
          const key = ruleKey(r);
          tbody.append(
            h(
              "tr",
              { "data-kp-row-key": key, "data-kp-group-of": dir },
              h("td", { "data-label": "#", class: "kp-numeric" }, `#${r.n}`),
              h(
                "td",
                { "data-label": "Stack", "data-kp-filter-value": r.stack },
                drivable(
                  h(
                    "button",
                    {
                      type: "button",
                      class: "fw-stackbtn",
                      title: `Show only ${r.stack}'s rules`,
                      onclick: (/** @type {Event} */ e) => {
                        e.stopPropagation();
                        onlyStack(r.stack);
                      },
                    },
                    h("b", null, r.stack),
                  ),
                  RULE_STACK,
                  r.stack,
                ),
              ),
              h(
                "td",
                { "data-label": "Action", "data-kp-filter-value": r.action },
                h(
                  "span",
                  {
                    class: `kp-badge kp-badge--${r.action === "ACCEPT" ? "success" : "destructive"}`,
                  },
                  r.action,
                ),
                r.disabled ? h("span", { class: "kp-badge" }, "off") : null,
              ),
              h(
                "td",
                { "data-label": "Peer" },
                h("code", { class: "kp-mono" }, r.peer),
                r.peer_stacks.length ? ` (${r.peer_stacks.join(", ")})` : "",
              ),
              h(
                "td",
                { "data-label": "Ports", class: "kp-numeric" },
                portsText(r),
              ),
              h("td", { "data-label": "Why" }, r.note || "—"),
              h(
                "td",
                { "data-label": "In force" },
                r.disabled ? "no (off)" : r.enabled ? "yes" : "no",
              ),
            ),
          );
        }
      }
      attachDataTables(ruTable, {
        detail: (row) => ruleDetail(row.dataset.kpRowKey ?? ""),
        decorate: (part, info) => {
          if (info.kind === "row-toggle" && info.value)
            drivable(part, RULE, info.value);
        },
      });
      const dt = ruDt();
      if (dt) {
        dt.element.addEventListener(VIEW_EVENT, paintRuleCounts);
        cleanups.push(bindTableUrl(dt, "firewall-rules"));
        if (S.stack) dt.filter(2, [S.stack]);
      }
    }
    paintRuleCounts();
  };
  /** @param {string} stack */
  const onlyStack = (stack) => {
    const dt = ruDt();
    if (!dt) return;
    const f = dt.view().filters[2];
    const same = Array.isArray(f) && f.length === 1 && f[0] === stack;
    dt.filter(2, same ? null : [stack]);
    S.stack = same ? "" : stack;
    keepUrl();
  };
  const paintRuleCounts = () => {
    const m = data?.matrix;
    if (!m) return;
    const acc = m.rules.filter((r) => r.action === "ACCEPT").length;
    const set = (/** @type {HTMLElement} */ b, /** @type {number} */ n) => {
      b.setAttribute("data-kp-count", String(n));
      toggleOf(b)?.setCount(n);
    };
    for (const b of /** @type {HTMLElement[]} */ ([
      ...toggles.querySelectorAll("button"),
    ])) {
      const v = b.getAttribute("data-kp-toggle-value");
      set(
        b,
        v === "ACCEPT"
          ? acc
          : v === "DROP"
            ? m.rules.length - acc
            : m.rules.length,
      );
    }
    const shown = ruDt()?.view().shown ?? m.rules.length;
    rup.read.textContent = `${m.rules.length.toLocaleString("en-GB")} rules: ${acc.toLocaleString("en-GB")} ACCEPT, ${(m.rules.length - acc).toLocaleString("en-GB")} DROP${shown !== m.rules.length ? `; ${shown.toLocaleString("en-GB")} shown` : ""}.`;
    const st = S.stack || ([...S.lit][0] ?? "");
    editThese.hidden = !st;
    editThese.title = st
      ? `Edit ${st}'s rules on its Settings`
      : "Edit the shown stack's rules on its Settings";
  };
  toggles.addEventListener(TOGGLE_CHANGED_EVENT, (e) => {
    const keys = /** @type {CustomEvent} */ (e).detail.pressedKeys ?? [];
    ruDt()?.filter(3, keys.length ? keys : null);
  });

  // the guard ---------------------------------------------------------------
  const paintGuard = () => {
    const ns = names();
    if (!data || !ns.length) {
      setCardState(
        guardCard.el,
        state === "loading" ? "loading" : state === "error" ? "error" : "empty",
        {
          words: "Reading the stacks…",
          title: "No stack to edit",
          text: "No stack has a firewall to edit yet.",
        },
      );
      return;
    }
    setCardState(guardCard.el, "ready");
    if (!S.guard || !ns.includes(S.guard))
      S.guard = S.stack && ns.includes(S.stack) ? S.stack : (ns[0] ?? "");
    chips.replaceChildren(
      ...ns.map((n) => {
        const s = data?.stacks.find((x) => x.stack === n);
        const st = s ? fwState(s) : null;
        const b = h(
          "button",
          {
            class: "kp-button",
            type: "button",
            "data-kp-toggle": "",
            "data-kp-look": "chip",
            "data-kp-toggle-value": n,
            "aria-pressed": String(n === S.guard),
            "data-kp-hovercard": s
              ? `${n} · CT ${s.vmid} · ${(s.ip ?? "").split("/")[0]}. ${st?.word}. Inbound ${s.policy_in ?? "—"}, outbound ${s.policy_out ?? "—"}, ${s.rules} rules. ${s.live_matches_repo === false || s.live_enforced === false ? "Live does not match the files." : "Live matches the files."}${s.management_open ? ` Management open: ${s.management_open}.` : ""}`
              : n,
          },
          n,
        );
        return drivable(b, GUARD_STACK, n);
      }),
    );
    attachToggles(chips);
    attachHoverCards(chips);
    guardCurrent();
  };
  const guardCurrent = () => {
    const s = data?.stacks.find((x) => x.stack === S.guard);
    if (!s) return;
    const gate = ackGate(ack);
    if (s.management_open) {
      ack.setAttribute("data-kp-ack-key", `mgmt-${s.stack}`);
      warn.replaceChildren(
        h(
          "strong",
          null,
          `${s.stack} has its management open: ${s.management_open}.`,
        ),
        " A rule change you make here reaches every container on the LAN.",
      );
      gate?.setState("ready");
    } else gate?.setState("empty");
    editGo.disabled = false;
    editGo.title = `Open ${s.stack}'s rules on its Settings`;
    attachDisabledReasons(root);
    editGo.dataset.stack = s.stack;
  };
  chips.addEventListener(TOGGLE_CHANGED_EVENT, (e) => {
    const keys = /** @type {CustomEvent} */ (e).detail.pressedKeys ?? [];
    if (keys[0]) S.guard = keys[0];
    ackGate(ack)?.set(false);
    guardCurrent();
  });
  editGo.addEventListener("click", () => {
    if (editGo.disabled) return;
    go(fwHref(editGo.dataset.stack ?? S.guard));
  });

  // events ------------------------------------------------------------------
  const paintAll = () => {
    keepUrl();
    paintHeader();
    paintNeeds();
    paintTotals();
    paintStacks();
    setSegmentedValues(modeSeg, [S.mode]);
    paintMx();
    paintTrace();
    paintRules();
    paintGuard();
    const keys = totalKeys();
    totals.read.textContent = !data
      ? ""
      : keys.length
        ? `Showing ${keys.length.toLocaleString("en-GB")} of ${names().length.toLocaleString("en-GB")} stacks: ${keys.join(", ")}.`
        : `Showing all ${names().length.toLocaleString("en-GB")} stacks.`;
    setAgo(mxAgo, readAt);
    setAgo(ago, readAt);
  };
  /** @type {number | null} */
  let readAt = null;

  modeSeg.addEventListener(SEGMENTED_EVENT, (e) => {
    const v = /** @type {CustomEvent} */ (e).detail.value;
    S.mode = v === "files" || v === "differences" ? v : "live";
    keepUrl();
    paintMx();
  });
  mxEl.addEventListener(MATRIX_PICK_EVENT, (e) => {
    const d = /** @type {CustomEvent} */ (e).detail;
    S.cell = `${d.rowKey}>${d.colKey}`;
    S.from = d.rowKey;
    S.to = d.colKey;
    S.port = "any";
    keepUrl();
    putTrace();
    paintTrace();
    const p = pinnedPair();
    if (p) mxp.read.textContent = cellReading(p);
  });
  mxEl.addEventListener(MATRIX_LIGHT_EVENT, (e) => {
    const keys = /** @type {CustomEvent} */ (e).detail.keys ?? [];
    S.lit = new Set(
      keys.filter((/** @type {string} */ k) => !totalKeys().includes(k)),
    );
    keepUrl();
    lit.textContent = S.lit.size
      ? `Lit up: ${[...S.lit].join(", ")}.`
      : "Press stack names to light them up; each press turns one on or off.";
    showAll.hidden = S.lit.size === 0;
    paintStacks();
  });
  fromBox.addEventListener(CHANGE_EVENT, (e) => {
    const v = /** @type {CustomEvent} */ (e).detail.value;
    if (putting || !v || v === S.from) return;
    S.from = v;
    S.port = "any";
    putTrace();
    paintTrace();
  });
  toBox.addEventListener(CHANGE_EVENT, (e) => {
    const v = /** @type {CustomEvent} */ (e).detail.value;
    if (putting || !v || v === S.to) return;
    S.to = v;
    S.port = "any";
    putTrace();
    paintTrace();
  });
  portSel.addEventListener("change", () => {
    S.port = portSel.value;
    paintTrace();
  });
  traceEl.addEventListener(TRACE_SWAP_EVENT, () => {
    [S.from, S.to] = [S.to, S.from];
    S.port = "any";
    putTrace();
    paintTrace();
    if (S.from && S.to) setMatrixPick(mxEl, { row: S.from, column: S.to });
  });
  traceEl.addEventListener(TRACE_RULE_EVENT, (e) => {
    const r = ruleOfChip(
      String(/** @type {CustomEvent} */ (e).detail.rule ?? ""),
    );
    if (!r) return;
    const dt = ruDt();
    dt?.filter(2, null);
    dt?.expand([ruleKey(r)]);
    rup.el.scrollIntoView({ block: "start", behavior: "smooth" });
    rup.read.textContent = `${r.stack} ${r.dir === "in" ? "inbound" : "outbound"} rule ${r.n} is open below.`;
  });
  traceEl.addEventListener(TRACE_RETRY_EVENT, () => retry());
  for (const el of [mxCard.el, traceCard.el, guardCard.el])
    el.addEventListener(CARD_RETRY_EVENT, () => retry());
  stTable.addEventListener("kp-datatable-retry", () => retry());

  cleanups.push(
    registerShortcuts("page:firewall", [
      {
        keys: "Escape",
        what: "Clear the lit stacks and totals",
        group: GROUP_FIREWALL,
        when: () => S.lit.size > 0 || S.totals.size > 0,
        reason: "Nothing is lit",
        run: unlessDialog(() => {
          if (!S.lit.size && !S.totals.size) return false;
          S.lit.clear();
          S.totals.clear();
          applyTotals();
          paintAll();
          return undefined;
        }),
      },
    ]),
  );

  // loading -----------------------------------------------------------------
  const setLoading = () => {
    state = "loading";
    paintTotals();
    const dt = stDt();
    dt?.busy({ text: "Reading the firewalls…", overlay: true });
    dt?.state("loading");
    setMatrix(mxEl, {
      rows: [],
      columns: [],
      state: "loading",
      skeleton: 11,
      label: "Reading the matrix",
    });
    trace(traceEl)?.setState("running");
    setCardState(guardCard.el, "loading", { words: "Reading the stacks…" });
    setCardState(mxCard.el, "ready");
    setCardState(traceCard.el, "ready");
  };
  const setFailed = (/** @type {string} */ why) => {
    state = "error";
    failure = why;
    const dt = stDt();
    dt?.busy(null);
    dt?.fail(why);
    setMatrix(mxEl, {
      rows: [],
      columns: [],
      state: "error",
      message: "The firewall rules could not be read.",
    });
    trace(traceEl)?.setState("error", {
      message: "The firewall did not answer. Nothing was changed.",
    });
    setCardState(guardCard.el, "error", {
      title: "The stacks could not be read",
      text: "The firewall did not answer. Nothing was changed.",
    });
    for (const t of kpis.tiles.values())
      t.set({ value: "—", ctx: "not read", loading: false });
    const ruleHost = ruTable.querySelector(".kp-datatable");
    if (ruleHost) {
      const dtr = dataTable(ruleHost);
      dtr?.fail(why);
    }
    needs.el.hidden = true;
  };
  const load = async () => {
    setLoading();
    const r = await fetchJson(
      "/data/firewall",
      "the fleet's firewall",
      abort.signal,
    );
    if (abort.signal.aborted) return;
    if (!r.ok) {
      fail.set(
        failSummary([{ what: "the fleet's firewall", error: r.error }]),
        retry,
      );
      setFailed(r.error.why);
      return;
    }
    fail.set(null, retry);
    data = /** @type {FirewallRead} */ (r.body);
    data.stacks = data.stacks ?? [];
    readAt = Date.now() / 1000;
    state = data.stacks.length ? "ready" : "empty";
    stDt()?.busy(null);
    stDt()?.state("ready");
    fillTraceControls();
    if (!S.from || !names().includes(S.from)) {
      const seed = data.matrix ? defaultPair() : null;
      if (seed) [S.from, S.to] = seed;
    }
    if (!S.cell && S.from && S.to) S.cell = `${S.from}>${S.to}`;
    putTrace();
    paintAll();
  };
  /** A pair worth tracing before anyone asks: the pinned one, else the one with the most ports. */
  const defaultPair = () => {
    const m = data?.matrix;
    if (!m) return null;
    let best = null;
    for (const c of m.cells)
      if (
        c.state === "some" &&
        (!best || c.allowed.length > best.allowed.length)
      )
        best = c;
    best = best ?? m.cells[0] ?? null;
    return best ? /** @type {[string, string]} */ ([best.from, best.to]) : null;
  };
  const retry = () => void load().catch(() => {});
  const off = listen("repo", retry);
  cleanups.push(off);
  retry();
  return () => {
    abort.abort();
    for (const c of cleanups) c();
  };
}
