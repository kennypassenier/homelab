// The Firewall page (redesign-firewall, release 3.71.0; Kenny approved
// the demo 2026-10-03: ~/.local/share/homelab/redesign-3.71/firewall.html,
// implemented exactly). It lives under System. Top to bottom: the header
// ("working copy <commit>", Topology and Edit rules…), four KPI tiles, one
// attention row per stack whose firewall is not in force (worst first,
// each with its fix), then Stacks and "Who may reach whom" side by side —
// a click on a stack (a row, or a name on the matrix) lights it up, each
// click turning one on or off, Esc or Show all resetting — and last the
// Rules card: one stack at a time, inbound beside outbound in the order
// Proxmox reads them; hovering a rule outlines the squares it decides.
//
// feat-firewall-2 still holds: everything is read from the working
// copy's stack files (`/data/firewall`), and what the host says pve
// enforces wins over the declaration (fix-207). Edits go through each
// stack hub's Settings (FLOWS.md: the stack's Firewall tab merged there).
//
// `fwSummary` (fwview.js, re-exported here) is the one-stack summary the
// stack hub's Settings tab draws.

import { agoEl, setAgo } from "../ago.js";
import { fetchJson, tableBlock } from "../dom.js";
import { declare, drivable } from "../drivable.js";
import {
  busiestStack,
  cellLabel,
  cellOf,
  cellWords,
  defaultCell,
  fwAttention,
  fwKpis,
  fwState,
  portsText,
  ruleHits,
  ruleMatches,
  ruleOrderText,
  rulesFor,
} from "../fwview.js";
import { stackHref } from "../router.js";
import { listen } from "../store.js";
import {
  attentionBand,
  emptyState,
  kpiStrip,
  section,
  skeletonTable,
  stackMark,
  toggleChips,
  toolbar,
} from "../ui.js";
import { setParams } from "../urlstate.js";
import {
  chip,
  dot,
  el,
  failBand,
  failNote,
  hideTip,
  highlight,
  pageHeader,
  pageStyles,
  rowKeys,
  seg,
  skel,
  tipOn,
} from "./configkit.js";
import { attachDataTables, dataTable } from "/static/kp/js/datatable.js";

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
  what: "show one stack's rules in the Rules card",
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
  what: "jump from the pinned square to a rule that decides it",
});
const CLEAR_RULES = declare({
  id: "firewall-clear-rule-filter",
  page: "firewall",
  opens: "view",
  what: "clear the Rules card's search and action filter",
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
  what: "go to the chosen stack's firewall on its Settings (the header's Edit rules…)",
});
const EDIT_THESE = declare({
  id: "firewall-edit-these-rules",
  page: "firewall",
  opens: "view",
  what: "go to the Rules card's stack's firewall on its Settings",
});
const FIX = declare({
  id: "firewall-fix",
  page: "firewall",
  opens: "view",
  row: "<stack>",
  what: "go to a stack's firewall on its Settings from its attention row (Declare… or Compare…)",
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
  what: "turn one lit stack off from its chip above the matrix",
});
const ACTION_FILTER = declare({
  id: "firewall-action-filter",
  page: "firewall",
  opens: "view",
  row: "ACCEPT|DROP",
  what: "show only ACCEPT or only DROP rules (again shows every rule)",
});
const RULE_SEARCH = declare({
  id: "firewall-rule-search",
  page: "firewall",
  opens: "view",
  what: "the Rules card's filter box (address, port, stack or why)",
});
const EDIT_RULE = declare({
  id: "firewall-edit-rule",
  page: "firewall",
  opens: "view",
  row: "<stack>/<in|out>/<n>",
  what: "go to an opened rule's stack firewall on its Settings",
});
const RULE_UP = declare({
  id: "firewall-rule-move-up",
  page: "firewall",
  opens: "view",
  row: "<stack>/<in|out>/<n>",
  what: "move an opened rule one place up: its stack's firewall editor opens with the move staged for review",
});
const RULE_OFF = declare({
  id: "firewall-rule-disable",
  page: "firewall",
  opens: "view",
  row: "<stack>/<in|out>/<n>",
  what: "switch an opened rule off (or back on): its stack's firewall editor opens with the change staged for review",
});
const SORT = declare({
  id: "firewall-sort",
  page: "firewall",
  opens: "view",
  row: "stack|state|rules",
  what: "sort the Stacks table by a column (again flips it, a third time clears)",
});

/** The settings address of a stack's firewall. @param {string} s */
const fwHref = (s) => `${stackHref(s, "settings")}?section=firewall`;

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  pageStyles("firewall");
  root.classList.add("cf-page", "fw-page");
  const abort = new AbortController();
  const q0 = new URLSearchParams(location.search);

  /** @type {FirewallRead | null} */
  let data = null;
  const S = {
    /** @type {Set<string>} lit stacks */
    focus: new Set((q0.get("lit") ?? "").split(",").filter(Boolean)),
    /** the stack whose rules show */
    stack: q0.get("stack") ?? "",
    /** the pinned cell, "from>to" */
    cell: q0.get("cell") ?? "",
    /** @type {Set<string>} */
    act: new Set(),
    q: (q0.get("q") ?? "").toLowerCase(),
    /** @type {string | null} the open rule, "stack/dir/n" */
    open: null,
    /** @type {RuleRow | null} */
    hot: null,
  };
  const keepUrl = () => {
    const next = setParams(location.search, {
      lit: [...S.focus].join(","),
      stack: S.stack,
      cell: S.cell,
      q: S.q,
    });
    history.replaceState(history.state, "", `${location.pathname}${next}`);
  };

  // ── header ────────────────────────────────────────────────────────────
  const commit = el("span", { class: "cf-mono" }, skel("10ch"));
  const live = el("span", { class: "cf-live" }, "working copy ", commit);
  const editRules = /** @type {HTMLAnchorElement} */ (
    el(
      "a",
      {
        class: "kp-button kp-button--primary",
        href: "/firewall",
        title: "Edit the selected stack's rules on its Settings",
      },
      "Edit rules…",
    )
  );
  drivable(editRules, EDIT_RULES);
  const head = pageHeader({
    title: "Firewall",
    desc: "What each container's Proxmox firewall lets through, read from the stack files. See who can reach whom, spot what is unprotected, and jump to a stack's rules to change them.",
    meta: [live],
    actions: [
      drivable(
        el(
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
    primary: editRules,
  });

  // ── KPIs and attention ────────────────────────────────────────────────
  const kpis = kpiStrip(
    [
      {
        key: "protected",
        label: "Protected",
        ctx: "stacks with rules in force",
      },
      {
        key: "unprotected",
        label: "Unprotected",
        ctx: "stacks whose file declares no firewall",
      },
      {
        key: "open",
        label: "Open paths",
        ctx: "pairs where every port gets through",
      },
      { key: "rules", label: "Rules", ctx: "" },
    ],
    { loading: true },
  );
  kpis.el.setAttribute("aria-label", "Firewall totals");
  const attention = attentionBand([]);
  const fail = failBand("firewall");

  // ── stacks card ───────────────────────────────────────────────────────
  const stBox = el("div", { class: "cf-table-wrap" }, skeletonTable(5, 5));
  const ago = agoEl("read");
  const foot = el(
    "div",
    { class: "cf-card__foot" },
    el("span", null, skel("40%")),
    el("span", null, ""),
  );
  const stacksCard = section({
    title: "Stacks",
    desc: "Each container's firewall: whether it is in force, its default policies and how many rules it has. Click rows to light stacks up; each click turns one on or off.",
    id: "fw-stacks",
  });
  stacksCard.el.classList.add("cf-card", "cf-span-6");
  stacksCard.body.append(stBox);
  stacksCard.el.append(foot);

  // ── matrix card ───────────────────────────────────────────────────────
  const mxActive = el("div", { class: "cf-active" });
  const mx = el("div", {
    class: "fw-mx",
    role: "grid",
    "aria-label": "Row may reach column",
  });
  const legend = el(
    "div",
    { class: "fw-legend" },
    el("span", null, el("i", { class: "fw-legend--some" }), "some ports"),
    el(
      "span",
      null,
      el("i", { class: "fw-legend--open" }),
      "every port (target unprotected)",
    ),
    el("span", null, el("i", { class: "fw-legend--none" }), "nothing"),
    el(
      "span",
      null,
      el("i", { class: "fw-legend--stopped" }),
      "dropped at the source",
    ),
  );
  const pathBox = el("div", { class: "fw-path", "aria-live": "polite" });
  const mxCard = section({
    title: "Who may reach whom",
    desc: "Read row to column: the ports the row's container may open on the column's, after both firewalls. Hover a square for its story, click to pin it.",
    id: "fw-matrix",
  });
  mxCard.el.classList.add("cf-card", "cf-span-6");
  mxCard.body.classList.add("fw-mx-body");
  mxCard.body.append(mxActive, mx, legend, pathBox);

  // ── rules card ────────────────────────────────────────────────────────
  const stackSeg = seg({
    label: "Stack",
    options: [],
    value: S.stack,
    onChange: (v) => {
      S.stack = v;
      S.open = null;
      keepUrl();
      paintRules();
    },
    mark: (b, v) => void drivable(b, RULE_STACK, v),
  });
  const acts = toggleChips({
    label: "Show",
    chips: [
      {
        value: "ACCEPT",
        label: "ACCEPT",
        hint: "Show only ACCEPT rules; again shows every rule",
      },
      {
        value: "DROP",
        label: "DROP",
        hint: "Show only DROP rules; again shows every rule",
      },
    ],
    onChange: (sel) => {
      S.act = sel;
      markActs();
      paintRules();
    },
  });
  const markActs = () => {
    for (const b of acts.el.querySelectorAll("button[data-value]"))
      drivable(
        /** @type {HTMLElement} */ (b),
        ACTION_FILTER,
        /** @type {HTMLElement} */ (b).dataset.value ?? "",
      );
  };
  markActs();
  const ruleCount = el("span", { class: "cf-count" });
  const tb = toolbar({
    search: {
      placeholder: "Filter by address, port, stack or why",
      label: "Filter rules",
      value: S.q,
      onInput: (v) => {
        S.q = v.trim().toLowerCase();
        keepUrl();
        paintRules();
      },
    },
    groups: [acts.el],
    state: [ruleCount],
  });
  if (tb.search) drivable(tb.search, RULE_SEARCH);
  // The demo's order: the ACCEPT / DROP toggles first, then the search
  // with its "/", the count against the right edge (redesign-config-9).
  {
    const filters = tb.el.querySelector(".nx-tb__filters");
    if (filters) tb.el.prepend(filters);
    tb.el.classList.add("fw-tb");
  }
  const rulesBody = el("div", { class: "fw-rules2" });
  const editThese = /** @type {HTMLAnchorElement} */ (
    el(
      "a",
      {
        class: "kp-button kp-button--sm",
        href: "/firewall",
        title: "Edit these rules on the stack's Settings",
      },
      "Edit these rules",
    )
  );
  drivable(editThese, EDIT_THESE);
  const rulesCard = section({
    title: "Rules",
    desc: "One stack at a time, inbound beside outbound, in the order Proxmox reads them. Hover a rule to see the squares it decides; click it for details.",
    id: "fw-rules",
    tools: [editThese],
  });
  rulesCard.el.classList.add("cf-card");
  rulesCard.body.classList.add("fw-rules-body");
  rulesCard.body.append(stackSeg.el, tb.el, rulesBody);

  root.replaceChildren(
    head.el,
    fail.el,
    kpis.el,
    attention.el,
    el("div", { class: "cf-grid" }, stacksCard.el, mxCard.el),
    rulesCard.el,
  );

  // ── painting ──────────────────────────────────────────────────────────
  const names = () =>
    data?.matrix?.stacks ?? data?.stacks.map((s) => s.stack) ?? [];
  /** @param {string} n */
  const toggleFocus = (n) => {
    if (S.focus.has(n)) S.focus.delete(n);
    else S.focus.add(n);
  };
  const paintAll = () => {
    keepUrl();
    paintHeader();
    paintStacks();
    paintMx();
    paintRules();
  };

  const paintHeader = () => {
    if (!data) return;
    commit.replaceChildren(String(data.head?.commit ?? "").slice(0, 10) || "—");
    live.title = data.head?.subject ?? "";
    const st = S.stack || data.stacks[0]?.stack || "";
    editRules.href = st ? fwHref(st) : "/firewall";
    editRules.title = st
      ? `Edit ${st}'s rules on its Settings`
      : "Edit a stack's rules on its Settings";
    editThese.href = editRules.href;
    const k = fwKpis(data);
    kpis.tiles.get("protected")?.set({
      value: String(k.protected),
      unit: `/ ${k.total}`,
      tone: k.protected < k.total ? "warn" : null,
    });
    kpis.tiles.get("unprotected")?.set({
      value: String(k.unprotected),
      tone: k.unprotected ? "bad" : null,
    });
    kpis.tiles
      .get("open")
      ?.set({ value: String(k.open), tone: k.open ? "warn" : null });
    kpis.tiles.get("rules")?.set({
      value: String(k.rules),
      ctx: `${k.drop} drop · ${k.accept} accept`,
    });
    for (const t of kpis.tiles.values()) delete t.el.dataset.loading;
    attention.set(
      fwAttention(data.stacks).map((a) => ({
        key: a.stack,
        tone: a.tone,
        title: a.title,
        text: a.text,
        action: drivable(
          el(
            "a",
            {
              class: "kp-button kp-button--sm",
              href: fwHref(a.stack),
              title: a.hint,
            },
            a.fix,
          ),
          FIX,
          a.stack,
        ),
      })),
    );
  };

  // The Stacks card is a kp datatable (Kenny's rule: sortable, Shift for
  // a second key, remembered per table; redesign-config-6). Its rows are
  // drawn once per read; lighting a stack up only marks them.
  /** @type {ReturnType<typeof tableBlock> | null} */
  let stTable = null;
  let detachSt = () => {};
  /** @type {StackFw[] | null} */
  let stDrawn = null;
  /** @param {StackFw} s */
  const stackRow = (s) => {
    const st = fwState(s);
    const tr = el(
      "tr",
      {
        class: "fw-row",
        tabindex: "0",
        "data-stack": s.stack,
        "data-kp-row-key": s.stack,
        "aria-selected": String(S.focus.has(s.stack)),
        title:
          "Click to light this stack up in the matrix (click again to turn it off) and show its rules",
        onclick: (/** @type {MouseEvent} */ e) => {
          if (/** @type {Element} */ (e.target).closest("a,button")) return;
          toggleFocus(s.stack);
          S.stack = s.stack;
          S.open = null;
          paintAll();
        },
      },
      el(
        "td",
        { class: "cf-id", "data-label": "Stack" },
        el(
          "span",
          { class: "fw-idcell" },
          stackMark(s.stack, 16),
          el(
            "span",
            { class: "fw-idcell__text" },
            drivable(
              el(
                "a",
                { href: stackHref(s.stack), title: `Open ${s.stack}` },
                s.stack,
              ),
              OPEN_STACK,
              s.stack,
            ),
            el("small", { class: "cf-mono" }, `CT ${s.vmid} · ${s.ip}`),
          ),
        ),
      ),
      el(
        "td",
        { class: "nowrap", "data-label": "State" },
        dot(st.tone, st.word),
      ),
      el(
        "td",
        { class: "kp-col-low", "data-label": "Default" },
        s.policy_in
          ? el(
              "span",
              { class: "fw-policy" },
              el("b", null, "in"),
              el(
                "span",
                { class: `fw-act fw-act--${s.policy_in}` },
                s.policy_in,
              ),
              el("b", null, "out"),
              el(
                "span",
                { class: `fw-act fw-act--${s.policy_out}` },
                s.policy_out ?? "—",
              ),
            )
          : el("span", { class: "cf-muted" }, "—"),
      ),
      el("td", { class: "num", "data-label": "Rules" }, String(s.rules)),
      el(
        "td",
        { class: "cf-row-actions", "data-label": "" },
        drivable(
          el(
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
    );
    drivable(tr, LIGHT, s.stack);
    if (s.management_open)
      tipOn(/** @type {HTMLElement} */ (tr.children[2]), () => [
        el("b", null, "Management network"),
        el("span", null, s.management_open ?? ""),
      ]);
    return tr;
  };
  const markStacks = () => {
    for (const tr of stBox.querySelectorAll("tr.fw-row"))
      tr.setAttribute(
        "aria-selected",
        String(
          S.focus.has(/** @type {HTMLElement} */ (tr).dataset.stack ?? ""),
        ),
      );
  };

  const paintStacks = () => {
    if (!data) return;
    if (stDrawn === data.stacks) return markStacks();
    stDrawn = data.stacks;
    if (!data.stacks.length) {
      detachSt();
      stTable = null;
      stBox.replaceChildren(
        emptyState({
          title: "No stack files",
          text: "The working copy holds no stack files, so there is no firewall to show.",
        }),
      );
      return;
    }
    if (!stTable) {
      stTable = tableBlock({
        remember: "firewall-stacks",
        caption: "Each stack's firewall",
        captionHidden: true,
        search: null,
        busyOverlay: false,
        nothing: "No stack files.",
        columns: [
          { label: "Stack", sort: "text", cls: "fw-col-stack" },
          { label: "State", sort: "text" },
          { label: "Default", sort: null, cls: "fw-col-default kp-col-low" },
          { label: "Rules", sort: "number", cls: "num" },
          { label: "Actions", sort: null, cls: "fw-col-acts" },
        ],
      });
      stTable.wrap.classList.add("fw-stacks-dt");
      // A phone gets kp's card layout (one card per stack, label: value),
      // so no column is squeezed over its neighbour (redesign-config-1).
      stTable.wrap.setAttribute("data-kp-cards", "");
      stTable.tbody.closest("table")?.classList.add("fw-stacks");
      stTable.tbody.replaceChildren(...data.stacks.map(stackRow));
      stBox.replaceChildren(stTable.wrap);
      detachSt = attachDataTables(stBox);
      const ths = stTable.wrap.querySelectorAll("thead th");
      ["stack", "state", "", "rules"].forEach((k, i) => {
        if (k) drivable(/** @type {HTMLElement} */ (ths[i]), SORT, k);
      });
    } else {
      stTable.tbody.replaceChildren(...data.stacks.map(stackRow));
      dataTable(stTable.wrap)?.refresh();
    }
    stTable.ready();
  };

  const paintMx = () => {
    const m = data?.matrix;
    if (!m) {
      if (data) {
        mx.replaceChildren(
          emptyState({
            title: "No matrix yet",
            text: "The host's answer carried no reachability matrix; it needs at least one stack with an address.",
          }),
        );
        pathBox.hidden = true;
      }
      return;
    }
    const ns = names();
    mx.style.setProperty("--n", String(ns.length));
    mx.classList.toggle("has-focus", S.focus.size > 0);
    const pinned =
      (S.cell &&
        cellOf(m, .../** @type {[string, string]} */ (S.cell.split(">")))) ||
      defaultCell(m);
    /** @param {string} n @param {"col" | "row"} kind */
    const nameBtn = (n, kind) => {
      const b = el(
        "button",
        {
          type: "button",
          class: `fw-mx__${kind}`,
          "aria-pressed": String(S.focus.has(n)),
          title: `Light up ${n}'s ${kind === "col" ? "column and row" : "row and column"} (click again to turn it off; click more names to add them)`,
          onclick: () => {
            toggleFocus(n);
            paintAll();
          },
        },
        kind === "row" ? stackMark(n, 12) : null,
        el("span", null, n),
      );
      return drivable(b, kind === "col" ? LIGHT_COL : LIGHT_ROW, n);
    };
    mx.replaceChildren(
      el("span", { class: "cf-hint fw-mx__corner" }, "from ↓ to →"),
      ...ns.map((n) => nameBtn(n, "col")),
      ...ns.flatMap((from, ri) => [
        nameBtn(from, "row"),
        ...ns.map((to, ci) => {
          if (from === to)
            return el("span", {
              class: "fw-mx__cell fw-mx__cell--self is-hot",
              "aria-label": "itself",
            });
          const c = cellOf(m, from, to);
          if (!c)
            return el("span", {
              class: "fw-mx__cell fw-mx__cell--none is-hot",
              "aria-label": `${from} to ${to}: unknown`,
            });
          const hot = !S.focus.size || S.focus.has(from) || S.focus.has(to);
          const isRule = !!S.hot && ruleHits(S.hot, from, to);
          const isPinned = c === pinned;
          const b = el(
            "button",
            {
              type: "button",
              class: `fw-mx__cell fw-mx__cell--${c.state}${c.stopped_at_source.length ? " fw-mx__cell--stopped" : ""}${hot ? " is-hot" : ""}${isRule ? " is-rule" : ""}`,
              "aria-pressed": String(isPinned),
              "aria-label": `${from} to ${to}: ${c.state}`,
              tabindex: isPinned ? "0" : "-1",
              "data-r": String(ri),
              "data-c": String(ci),
              onclick: () => {
                S.cell = `${from}>${to}`;
                keepUrl();
                paintMx();
              },
            },
            cellLabel(c),
          );
          drivable(b, CELL, `${from}>${to}`);
          tipOn(b, () => [
            el("b", null, `${from} → ${to}`),
            el("span", null, cellWords(c)),
            c.stopped_at_source.length
              ? dot(
                  "bad",
                  `${c.stopped_at_source.join(", ")} dropped on the way out of ${from}`,
                )
              : null,
            el(
              "span",
              { class: "cf-hint" },
              `${rulesFor(m, c).length} rule(s) decide this · click to pin`,
            ),
          ]);
          return b;
        }),
      ]),
    );
    // The pinned square's story.
    if (pinned) {
      const rs = rulesFor(m, pinned);
      pathBox.hidden = false;
      pathBox.replaceChildren(
        el(
          "div",
          { class: "fw-path__top" },
          el("strong", null, `${pinned.from} → ${pinned.to}`),
          el(
            "span",
            { class: "cf-hint" },
            "Pinned · click another square to change",
          ),
        ),
        el("span", null, cellWords(pinned)),
        pinned.stopped_at_source.length
          ? dot(
              "bad",
              `${pinned.stopped_at_source.join(", ")} is let in by ${pinned.to} but dropped on the way out of ${pinned.from}`,
            )
          : "",
        rs.length
          ? el(
              "div",
              { class: "cf-row" },
              el("span", { class: "cf-hint" }, "Decided by"),
              rs.map((r) => {
                const key = `${r.stack}/${r.dir}/${r.n}`;
                const b = el(
                  "button",
                  {
                    type: "button",
                    class: "cf-chip cf-chip--btn",
                    title: "Show this rule below",
                    onclick: () => {
                      S.stack = r.stack;
                      S.open = key;
                      S.act = new Set();
                      acts.set([]);
                      markActs();
                      paintAll();
                      rulesCard.el.scrollIntoView({
                        behavior: "smooth",
                        block: "start",
                      });
                    },
                  },
                  `${r.stack} #${r.n} ${r.dir}`,
                );
                return drivable(b, DECIDED_BY, key);
              }),
            )
          : el(
              "span",
              { class: "cf-hint" },
              "Decided by the default policies.",
            ),
      );
    } else pathBox.hidden = true;
    const showAll = drivable(
      el(
        "button",
        {
          type: "button",
          class: "cf-linkbtn",
          title: "Turn every lit stack off (Esc)",
          onclick: () => {
            S.focus.clear();
            paintAll();
          },
        },
        "Show all",
      ),
      SHOW_ALL,
    );
    mxActive.replaceChildren(
      ...(S.focus.size
        ? [
            el("span", null, "Lit up:"),
            ...[...S.focus].map((n) =>
              el(
                "span",
                { class: "cf-tag" },
                n,
                drivable(
                  el(
                    "button",
                    {
                      type: "button",
                      "aria-label": `Turn ${n} off`,
                      title: "Turn this stack off",
                      onclick: () => {
                        S.focus.delete(n);
                        paintAll();
                      },
                    },
                    "×",
                  ),
                  UNLIGHT,
                  n,
                ),
              ),
            ),
            showAll,
          ]
        : [
            el(
              "span",
              null,
              "Click names to light them up (each click turns one on or off) · Esc or Show all resets · arrow keys move between squares.",
            ),
          ]),
    );
  };

  mx.addEventListener("keydown", (e) => {
    const t = /** @type {HTMLElement} */ (e.target);
    if (!t.matches(".fw-mx__cell[data-r]")) return;
    const step = /** @type {Record<string, [number, number]>} */ ({
      ArrowRight: [0, 1],
      ArrowLeft: [0, -1],
      ArrowDown: [1, 0],
      ArrowUp: [-1, 0],
    })[e.key];
    if (!step) return;
    e.preventDefault();
    const ns = names();
    const n = ns.length;
    let r = Number(t.dataset.r);
    let c = Number(t.dataset.c);
    for (let i = 0; i < n; i++) {
      r = (r + step[0] + n) % n;
      c = (c + step[1] + n) % n;
      if (r !== c) break;
    }
    S.cell = `${ns[r]}>${ns[c]}`;
    keepUrl();
    paintMx();
    /** @type {HTMLElement | null} */ (
      mx.querySelector(`.fw-mx__cell[data-r="${r}"][data-c="${c}"]`)
    )?.focus();
  });

  const paintRules = () => {
    const m = data?.matrix;
    if (!m) return;
    const withRules = names().filter((n) => m.rules.some((r) => r.stack === n));
    if (!withRules.length) {
      stackSeg.el.hidden = true;
      tb.el.hidden = true;
      rulesBody.replaceChildren(
        emptyState({
          title: "No stack declares a rule yet",
          text: "Declare a firewall on a stack's Settings; its rules appear here, in the order Proxmox reads them.",
        }),
      );
      ruleCount.textContent = "";
      return;
    }
    stackSeg.el.hidden = false;
    tb.el.hidden = false;
    if (!withRules.includes(S.stack))
      S.stack =
        [...S.focus].find((n) => withRules.includes(n)) ??
        busiestStack(withRules, m.rules);
    stackSeg.setOptions(
      withRules.map((n) => ({
        value: n,
        label: `${n} · ${m.rules.filter((r) => r.stack === n).length}`,
        hint: `Show ${n}'s rules`,
      })),
    );
    stackSeg.set(S.stack);
    const href = fwHref(S.stack);
    editThese.href = href;
    editRules.href = href;
    editRules.title = `Edit ${S.stack}'s rules on its Settings`;
    let shown = 0;
    let total = 0;
    /** @param {"in" | "out"} dir */
    const side = (dir) => {
      const all = m.rules.filter((r) => r.stack === S.stack && r.dir === dir);
      const rs = all.filter((r) => ruleMatches(r, S.act, S.q));
      shown += rs.length;
      total += all.length;
      return el(
        "div",
        { class: "fw-side", "data-dir": dir },
        el(
          "h3",
          null,
          dir === "in" ? "Inbound" : "Outbound",
          chip(
            rs.length === all.length
              ? String(all.length)
              : `${rs.length} of ${all.length}`,
          ),
        ),
        el(
          "div",
          { class: "cf-table-wrap" },
          el(
            "table",
            { class: "kp-table fw-rules" },
            el(
              "thead",
              null,
              el(
                "tr",
                null,
                el("th", { class: "num", scope: "col" }, "#"),
                el("th", { scope: "col" }, "Action"),
                el("th", { scope: "col" }, dir === "in" ? "From" : "To"),
                el("th", { scope: "col" }, "Ports"),
                el("th", { scope: "col", class: "fw-hide-sm" }, "Why"),
              ),
            ),
            el(
              "tbody",
              null,
              rs.length
                ? rs.flatMap((r) => ruleRow(r, m.rules))
                : [
                    el(
                      "tr",
                      null,
                      el(
                        "td",
                        { colspan: "5" },
                        el(
                          "div",
                          { class: "cf-empty" },
                          el(
                            "strong",
                            null,
                            all.length
                              ? "No rule matches"
                              : "No rules this way",
                          ),
                          all.length
                            ? drivable(
                                el(
                                  "button",
                                  {
                                    type: "button",
                                    class: "cf-linkbtn",
                                    onclick: clearRuleFilter,
                                  },
                                  "Clear the filters",
                                ),
                                CLEAR_RULES,
                              )
                            : el(
                                "span",
                                null,
                                "The default policy decides everything in this direction.",
                              ),
                        ),
                      ),
                    ),
                  ],
            ),
          ),
        ),
      );
    };
    rulesBody.replaceChildren(side("in"), side("out"));
    ruleCount.textContent =
      shown === total ? `${total} rules` : `${shown} of ${total} rules`;
  };

  const clearRuleFilter = () => {
    S.q = "";
    if (tb.search) tb.search.value = "";
    S.act = new Set();
    acts.set([]);
    keepUrl();
    paintRules();
  };

  /** @param {RuleRow} r @param {RuleRow[]} every */
  const ruleRow = (r, every) => {
    const key = `${r.stack}/${r.dir}/${r.n}`;
    const open = S.open === key;
    const tr = el(
      "tr",
      {
        class: `fw-rule${r.disabled ? " fw-rule--off" : ""}`,
        tabindex: "0",
        "data-rule": key,
        "aria-expanded": String(open),
        title: "Hover to see it in the matrix · click for details",
        onclick: () => {
          S.open = open ? null : key;
          paintRules();
        },
        onpointerenter: () => {
          S.hot = r;
          paintMx();
        },
        onpointerleave: () => {
          S.hot = null;
          paintMx();
        },
        onfocus: () => {
          S.hot = r;
          paintMx();
        },
      },
      el("td", { class: "num cf-muted" }, String(r.n)),
      el(
        "td",
        null,
        el("span", { class: `fw-act fw-act--${r.action}` }, r.action),
        r.disabled ? chip("off") : null,
      ),
      el(
        "td",
        { class: "fw-peer" },
        el("span", { class: "cf-mono" }, highlight(r.peer, S.q)),
        r.peer_stacks.length
          ? el(
              "span",
              { class: "cf-row" },
              r.peer_stacks.map((p) => chip(highlight(p, S.q))),
            )
          : null,
      ),
      el("td", { class: "cf-mono nowrap" }, highlight(portsText(r), S.q)),
      el("td", { class: "fw-note fw-hide-sm" }, highlight(r.note, S.q) || "—"),
    );
    drivable(tr, RULE, key);
    if (!open) return [tr];
    return [
      tr,
      el(
        "tr",
        { class: "fw-rule-more" },
        el("td"),
        el(
          "td",
          { colspan: "4" },
          el(
            "dl",
            { class: "cf-kv" },
            el("dt", null, "In force"),
            el(
              "dd",
              null,
              r.disabled
                ? "no: switched off, Proxmox skips it"
                : r.enabled
                  ? `yes, live on CT ${r.vmid}`
                  : "declared, not applied yet",
            ),
            el("dt", null, "Order"),
            el("dd", null, ruleOrderText(r, every)),
            el("dt", null, "Why"),
            el("dd", null, r.note || "—"),
            el("dt", null, "Matrix"),
            el(
              "dd",
              null,
              r.peer_stacks.length
                ? `affects ${r.peer_stacks.length} square(s), outlined above while you hover`
                : "matches no managed stack",
            ),
          ),
          el(
            "div",
            { class: "cf-row fw-rule-more__acts" },
            drivable(
              el(
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
                  el(
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
              el(
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
        ),
      ),
    ];
  };

  rowKeys(stBox, "tr.fw-row", (tr) => tr.click());
  rowKeys(rulesCard.el, "tr.fw-rule", (tr) => tr.click());

  /** @param {KeyboardEvent} e */
  const onKey = (e) => {
    if (e.key !== "Escape") return;
    const a = document.activeElement;
    if (a && /^(INPUT|TEXTAREA|SELECT)$/.test(a.tagName)) return;
    if (document.querySelector("dialog[open]")) return;
    hideTip();
    if (S.focus.size) {
      S.focus.clear();
      paintAll();
    }
  };
  document.addEventListener("keydown", onKey);

  // ── loading ───────────────────────────────────────────────────────────
  const mxSkeleton = () => {
    mx.style.setProperty("--n", "5");
    mx.replaceChildren(
      el("span", { class: "cf-hint fw-mx__corner" }, "from ↓ to →"),
      ...Array.from({ length: 5 }, () =>
        el("span", { class: "fw-mx__col" }, skel("80%")),
      ),
      ...Array.from({ length: 5 }, () => [
        el("span", { class: "fw-mx__row" }, skel("80%")),
        ...Array.from({ length: 5 }, () =>
          el("span", { class: "fw-mx__cell fw-mx__cell--sk" }),
        ),
      ]).flat(),
    );
    pathBox.replaceChildren(skel("40%"), skel("70%"));
  };
  mxSkeleton();
  rulesBody.replaceChildren(skeletonTable(6, 5), skeletonTable(6, 5));

  const load = async () => {
    const r = await fetchJson(
      "/data/firewall",
      "the fleet's firewall",
      abort.signal,
    );
    if (!r.ok) {
      // One read, one message (redesign-config-11): the alert under the
      // header says why with one Try again; each card says it is empty.
      fail.set([{ what: "the fleet's firewall", error: r.error }], retry);
      const note = failNote("Not read: see the message at the top.");
      stBox.replaceChildren(note);
      mx.replaceChildren(note.cloneNode(true));
      pathBox.hidden = true;
      rulesBody.replaceChildren(note.cloneNode(true));
      return;
    }
    fail.set([], retry);
    data = /** @type {FirewallRead} */ (r.body);
    data.stacks = data.stacks ?? [];
    foot.replaceChildren(el("span", null, data.head?.subject ?? ""), ago);
    setAgo(ago, Date.now() / 1000);
    paintAll();
  };
  const retry = () => void load().catch(() => {});
  const off = listen("repo", retry);
  retry();
  return () => {
    abort.abort();
    off();
    hideTip();
    document.removeEventListener("keydown", onKey);
    detachSt();
    root.classList.remove("cf-page", "fw-page");
  };
}
