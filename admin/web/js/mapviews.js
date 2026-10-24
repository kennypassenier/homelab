// The Map's side views, DOM half (see mapparts.js for the pure half and the
// reasons): the side panel beside the graph, the Matrix view, Capacity,
// Disk growth and Stale images, each a kp component with its own loading,
// empty and failed states.

import { h, selectCell, tableBlock, labelCells, stateWord } from "./dom.js";
import { declare, drivable } from "./drivable.js";
import { gb } from "./fleet.js";
import { formatDay, gbText } from "./format.js";
import { timeChart } from "./charts.js";
import {
  KIND_TONE,
  KIND_WORDS,
  blastOf,
  capacityText,
  countsOf,
  matrixCell,
  projection,
  usesOf,
} from "./mapparts.js";
import { firewallOf, isStack, staleGroups } from "./mapview.js";
import { current } from "./store.js";
import { section, segSwitch } from "./ui.js";
import { setBarCell } from "/static/kp/js/barcell.js";
import { setBulletsQuiet } from "./inplace.js";
import { attachCardStates, setCardState } from "/static/kp/js/card-state.js";
import { attachDataTables } from "/static/kp/js/datatable.js";
import { setDeplist } from "/static/kp/js/deplist.js";
import {
  MATRIX_PICK_EVENT,
  MATRIX_RETRY_EVENT,
  setHeatLegend,
  setMatrix,
} from "/static/kp/js/heat.js";
import { attachIdmarks } from "/static/kp/js/idmark.js";
import { bindSelection } from "./selectbind.js";
import {
  setSelectionBusy,
  setSelectionResult,
} from "/static/kp/js/selection-bar.js";
import { setStackbar } from "/static/kp/js/stackbar.js";
import {
  attachVersionDeltas,
  releaseUrl,
  versionLevel,
} from "/static/kp/js/version-delta.js";

/** @typedef {import("./topology.js").Topology} Topology */

/**
 * An image's upstream as the page shows it: a repository on a known forge is
 * a link to that repository, anything else stays words.
 * @param {string | null | undefined} upstream
 * @returns {Node | string}
 */
const upstreamLink = (upstream) => {
  if (!upstream) return "\u2014";
  if (
    !/^(?:github\.com|gitlab\.com|codeberg\.org)\/[\w.-]+\/[\w.-]+$/.test(
      upstream,
    )
  )
    return upstream;
  return h(
    "a",
    {
      href: `https://${upstream}`,
      target: "_blank",
      rel: "noopener noreferrer",
      title: `${upstream}, in a new tab`,
    },
    upstream,
  );
};

const MODE = declare({
  id: "map-panel-mode",
  page: "fleetview",
  opens: "view",
  row: "normal|blast",
  what: "show what the picked stack uses and who uses it, or what would stop if it stopped",
});
const ORDER = declare({
  id: "map-deplist-order",
  page: "fleetview",
  opens: "view",
  row: "numbers|kind",
  what: "list the connections in start order, numbered, or grouped by kind of connection",
});
const MATRIX_SHOW = declare({
  id: "map-matrix-show",
  page: "fleetview",
  opens: "view",
  row: "all|open",
  what: "show every connection in the matrix, or only those the firewall does not enforce (open and named)",
  shows: "in the Matrix view",
  reach: [{ do: "click", control: "map-view", row: "matrix" }],
});
const CAP_LAYOUT = declare({
  id: "map-capacity-layout",
  page: "fleetview",
  opens: "view",
  row: "table|bullets",
  what: "show capacity as a table with three bar columns or as ranked bars for one measure",
});
const CAP_MEASURE = declare({
  id: "map-capacity-measure",
  page: "fleetview",
  opens: "view",
  row: "mem|cpu|disk",
  what: "rank the stacks by memory, CPU or disk",
  shows: "in the ranked bars layout",
  reach: [{ do: "click", control: "map-capacity-layout", row: "bullets" }],
});
const CAP_OPEN = declare({
  id: "map-capacity-open",
  page: "fleetview",
  opens: "view",
  row: "<stack>",
  what: "open a ranked stack's hub",
  shows: "in the ranked bars layout",
  reach: [{ do: "click", control: "map-capacity-layout", row: "bullets" }],
});
const DISK_FS = declare({
  id: "map-disk-filesystem",
  page: "fleetview",
  opens: "view",
  what: "choose which filesystem the disk growth chart shows",
});
const STALE_TICK = declare({
  id: "map-stale-tick",
  page: "fleetview",
  opens: "view",
  row: "<stack>/<container>",
  selects: true,
  what: "tick one stale image to send it to the Update flow with the others",
});
const STALE_UNTICK = declare({
  id: "map-stale-untick",
  page: "fleetview",
  opens: "view",
  what: "untick every ticked stale image",
  shows: "while a stale image is ticked",
  reach: [{ do: "click", control: "map-stale-tick", row: "*" }],
});
const STALE_ALL = declare({
  id: "map-stale-tick-all",
  page: "fleetview",
  opens: "view",
  what: "tick every stale image the Update flow can move",
});
const STALE_GO = declare({
  id: "map-stale-update-selected",
  page: "fleetview",
  opens: "view",
  what: "open the Update flow with the ticked images chosen",
  shows: "while a stale image is ticked",
  reach: [{ do: "click", control: "map-stale-tick", row: "*" }],
});

/** @param {string} key @param {string} fallback */
const remembered = (key, fallback) => {
  try {
    return localStorage.getItem(key) ?? fallback;
  } catch {
    return fallback;
  }
};
/** @param {string} key @param {string} value */
const remember = (key, value) => {
  try {
    localStorage.setItem(key, value);
  } catch {
    // kept for this visit only
  }
};

/** @param {number} n @param {string} one */
const plural = (n, one) => `${n} ${n === 1 ? one : `${one}s`}`;

/** A kp card that carries the four data states around `inner`. @param {HTMLElement} inner @param {string} cls */
function stageOf(inner, cls = "") {
  return h(
    "div",
    { class: `kp-card mv-stage ${cls}`.trim(), "data-kp-state": "loading" },
    h(
      "div",
      { class: "kp-card__stage" },
      h("div", { class: "kp-card__data" }, inner),
    ),
  );
}

// ════════════════════════════════════════════════════════════════════════
// The side panel
// ════════════════════════════════════════════════════════════════════════

/**
 * The panel beside the graph: for each picked stack its state, what it
 * uses in start order, who uses it, and in the other mode what would stop
 * with it.
 * @param {{stackHref: (n: string, tab?: string) => string,
 *   hues: (t: Topology) => Map<string, number>,
 *   traffic: () => Map<string, number> | null,
 *   links: (n: string) => Node[]}} deps
 */
export function sidePanel(deps) {
  /** @type {Topology | null} */
  let topo = null;
  /** @type {string[]} */
  let ids = [];
  /** @type {"normal" | "blast"} */
  let mode = "normal";
  let order =
    remembered("homelab.map.deplist", "numbers") === "kind"
      ? "kind"
      : "numbers";
  /** @type {"loading" | "failed" | "ready"} */
  let status = "loading";
  let why = "";

  const modes = segSwitch({
    label: "Mode",
    value: mode,
    items: [
      {
        value: "normal",
        label: "Uses and used by",
        hint: "What the stack connects to and who connects to it",
      },
      {
        value: "blast",
        label: "If this stack stopped",
        hint: "Which stacks would lose their connection with it",
      },
    ],
    onChange: (v) => {
      mode = v === "blast" ? "blast" : "normal";
      draw();
    },
    mark: (b, v) => void drivable(b, MODE, v),
  });
  const orders = segSwitch({
    label: "Order",
    value: order,
    items: [
      {
        value: "numbers",
        label: "In start order, numbered",
        hint: "The order things must start in, numbered",
      },
      {
        value: "kind",
        label: "Grouped by kind",
        hint: "Each connection with the kind of connection it is",
      },
    ],
    onChange: (v) => {
      order = v === "kind" ? "kind" : "numbers";
      remember("homelab.map.deplist", order);
      draw();
    },
    mark: (b, v) => void drivable(b, ORDER, v),
  });
  const body = h("div", { class: "mv-panel__body", "aria-live": "polite" });
  const ctl = h(
    "div",
    { class: "mv-ctl" },
    h("b", { class: "mv-ctl__label" }, "Mode"),
    modes.el,
    h("b", { class: "mv-ctl__label" }, "Order"),
    orders.el,
  );
  const stage = stageOf(body, "mv-panelcard");
  const el = h(
    "aside",
    { class: "mp-side mv-panel", "aria-label": "The picked stack" },
    ctl,
    stage,
  );
  const detachCards = attachCardStates(el);
  const detachMarks = attachIdmarks(el);
  setCardState(stage, "loading", { words: "Reading the topology" });

  /** @param {string} name */
  const mark = (name) =>
    h("span", {
      class: "kp-idmark kp-idmark--sm",
      "data-kp-idmark": name,
      "aria-hidden": "true",
    });

  /** @param {string} id */
  const block = (id) => {
    const t = /** @type {Topology} */ (topo);
    const n = t.nodes.find((x) => x.stack === id);
    if (!n) return null;
    if (!isStack(n))
      return h(
        "div",
        { class: "mv-block" },
        h("div", { class: "mv-block__head" }, mark(id), h("h3", null, id)),
        h("p", { class: "mv-line" }, "An address outside the fleet."),
        h(
          "p",
          { class: "mv-line" },
          `Reached by ${
            usesOf(t, id)
              .usedBy.map((x) => x.other)
              .join(", ") || "no stack"
          }.`,
        ),
      );
    const st = current().fleet?.stacks.find((x) => x.name === id);
    const running = st ? st.online !== false : null;
    const stateOf = (/** @type {string} */ other) => {
      const o = current().fleet?.stacks.find((x) => x.name === other);
      /** @type {"running" | "stopped" | "unknown"} */
      const out = !o ? "unknown" : o.online === false ? "stopped" : "running";
      return out;
    };
    const { uses, usedBy } = usesOf(t, id);
    const fw = firewallOf(n);
    const traffic = deps.traffic();
    const blast = blastOf(t, id);
    const list = (
      /** @type {string} */ title,
      /** @type {DepItemInput[]} */ items,
    ) => {
      const box = h("div", {
        class: "kp-deplist mv-deplist",
        "data-kp-deplist": "",
        ...(order === "kind" ? { "data-kp-dep-order": "none" } : {}),
      });
      setDeplist(
        box,
        items.map((x) => ({
          from: x.from,
          to: x.to,
          state: stateOf(x.other),
          ...(order === "kind" ? { label: KIND_WORDS[x.kind] ?? x.kind } : {}),
        })),
      );
      return [h("h4", null, title), box];
    };
    return h(
      "div",
      { class: "mv-block" },
      h(
        "div",
        { class: "mv-block__head" },
        mark(id),
        h("h3", null, id),
        running == null
          ? null
          : stateWord(running ? "running" : "stopped", ["running", "stopped"]),
      ),
      h(
        "p",
        { class: "mv-line" },
        `CT ${n.vmid} · ${n.ip} · firewall ${fw.text}${traffic ? ` · ${(traffic.get(id) ?? 0).toFixed(0)} bytes/s` : ""}`,
      ),
      ...(mode === "blast"
        ? [
            h(
              "div",
              { class: "kp-alert kp-alert--warning", role: "status" },
              h(
                "div",
                { class: "kp-alert__body" },
                h(
                  "strong",
                  null,
                  `Stopping ${id} ${blast.length ? `cuts ${plural(blast.length, "stack")} off` : "cuts nothing off"}. `,
                ),
                blast.length
                  ? blast.join(", ")
                  : "No other stack depends on it.",
              ),
            ),
          ]
        : []),
      ...list(
        `${id} uses`,
        uses.map((x) => ({
          from: id,
          to: x.other,
          other: x.other,
          kind: x.kind,
        })),
      ),
      ...list(
        "Used by",
        usedBy.map((x) => ({
          from: x.other,
          to: id,
          other: x.other,
          kind: x.kind,
        })),
      ),
      h("div", { class: "mv-block__acts" }, ...deps.links(id)),
    );
  };
  /** @typedef {{from: string, to: string, other: string, kind: string}} DepItemInput */

  const draw = () => {
    modes.set(mode);
    orders.set(order);
    if (status === "loading") {
      setCardState(stage, "loading", { words: "Reading the topology" });
      return;
    }
    if (status === "failed") {
      setCardState(stage, "error", {
        title: "Could not read the topology",
        text: why,
        retry: false,
      });
      return;
    }
    if (!topo || !topo.nodes.length) {
      setCardState(stage, "empty", {
        title: "No connection known",
        text: "Declare what a stack talks to in its firewall rules and it shows here.",
      });
      return;
    }
    if (!ids.length) {
      setCardState(stage, "empty", {
        title: "Nothing picked",
        text: "Hover a stack in the graph, or press one to pick it; several can be picked to read side by side.",
      });
      return;
    }
    setCardState(stage, "ready");
    body.replaceChildren(...ids.map(block).filter((x) => x != null));
  };
  draw();
  return {
    el,
    /** @param {Topology | null} t @param {"loading" | "failed" | "ready"} s @param {string} [w] */
    set: (t, s, w = "") => {
      topo = t;
      status = s;
      why = w;
      draw();
    },
    /** @param {string[]} sel @param {string | null} hover */
    pick: (sel, hover) => {
      const next = sel.length ? sel : hover ? [hover] : [];
      // The same stacks again (the graph re-laid out, the pointer moved
      // inside one node) change nothing the panel shows; `redraw` is the
      // way to repaint after the stacks' own state moved.
      if (next.length === ids.length && next.every((x, i) => x === ids[i]))
        return;
      ids = next;
      draw();
    },
    redraw: draw,
    stop: () => {
      detachCards();
      detachMarks();
    },
  };
}

// ════════════════════════════════════════════════════════════════════════
// The matrix
// ════════════════════════════════════════════════════════════════════════

/**
 * The Matrix view: who reaches whom, stack by stack.
 * @param {{retry: () => void}} deps the failed plate's Retry reads the topology again
 */
export function matrixView(deps) {
  /** @type {Topology | null} */
  let topo = null;
  /** @type {"loading" | "failed" | "ready"} */
  let status = "loading";
  let why = "";
  /** @type {"all" | "open"} */
  let show =
    /** @type {any} */ (remembered("homelab.map.matrix", "all")) === "open"
      ? "open"
      : "all";
  const el = h("div", {
    class: "kp-matrix mv-matrix",
    "data-kp-matrix": "",
    "data-kp-matrix-owned": "",
    "data-kp-matrix-select": "follow",
  });
  const shows = segSwitch({
    label: "Show",
    value: show,
    items: [
      {
        value: "all",
        label: "Every connection",
        hint: "Every connection the stack files and the firewall know",
      },
      {
        value: "open",
        label: "Not enforced",
        hint: "Only the open and named connections, the ones the firewall does not cover",
      },
    ],
    onChange: (v) => {
      show = v === "open" ? "open" : "all";
      remember("homelab.map.matrix", show);
      draw();
    },
    mark: (b, v) => void drivable(b, MATRIX_SHOW, v),
  });
  const line = h("p", { class: "mv-line", role: "status" });
  const box = h(
    "div",
    { class: "mv-matrixbox" },
    h(
      "div",
      { class: "mv-ctl" },
      h("b", { class: "mv-ctl__label" }, "Show"),
      shows.el,
    ),
    el,
    line,
  );
  // kp's heat legend under the matrix: one swatch per kind of connection.
  setHeatLegend(
    el,
    Object.entries(KIND_WORDS).map(([k, words]) => ({
      tone: /** @type {any} */ (KIND_TONE[k]),
      label: `${k}: ${words}`,
    })),
    { label: "Kinds of connection" },
  );
  const draw = () => {
    shows.set(show);
    const stacks = (topo?.nodes ?? []).filter(isStack);
    const nodes = stacks.map((n) => ({
      key: n.stack,
      label: n.stack,
      sub: `CT ${n.vmid}`,
    }));
    setMatrix(el, {
      corner: "from (row) to (column)",
      label: "Which stack reaches which",
      view: "grid",
      rows: nodes,
      columns: nodes,
      state:
        status === "ready"
          ? nodes.length
            ? "ready"
            : "empty"
          : status === "failed"
            ? "error"
            : "loading",
      message: status === "failed" ? why : "No connection is known yet.",
      skeleton: 8,
      cell: (r, c) =>
        topo ? matrixCell(topo, r, c, show === "open") : undefined,
      countLabel: (n) => `${n} ${n === 1 ? "stack" : "stacks"}`,
      sourcesLabel: "Stacks that reach out",
      usersLabel: "Stacks reached",
    });
  };
  el.addEventListener(MATRIX_RETRY_EVENT, () => deps.retry());
  el.addEventListener("kp-matrix-light", (e) => {
    const d = /** @type {CustomEvent} */ (e).detail;
    if (d.key && topo) {
      const c = countsOf(topo, d.key);
      line.textContent = `${d.key}: uses ${plural(c.uses, "stack")}, used by ${plural(c.usedBy, "stack")}.`;
    } else line.textContent = "";
  });
  el.addEventListener(MATRIX_PICK_EVENT, (e) => {
    const d = /** @type {CustomEvent} */ (e).detail;
    const edge = topo?.edges.find(
      (x) => x.from === d.rowKey && x.to === d.colKey,
    );
    line.textContent = `${d.rowKey} reaches ${d.colKey}: ${edge ? (KIND_WORDS[edge.kind] ?? edge.kind) : "no connection known"}.`;
  });
  draw();
  return {
    el: box,
    /** @param {Topology | null} t @param {"loading" | "failed" | "ready"} s @param {string} [w] */
    set: (t, s, w = "") => {
      topo = t;
      status = s;
      why = w;
      draw();
    },
  };
}

// ════════════════════════════════════════════════════════════════════════
// Capacity
// ════════════════════════════════════════════════════════════════════════

/**
 * Capacity: a table with three bar columns, or ranked bars for one
 * measure.
 * @param {{stackHref: (n: string) => string, navigate: (href: string) => void, retry: () => void}} deps
 */
export function capacityCard(deps) {
  /** @type {{stack: string, cpu: number | null, mem: number | null, disk: number | null}[]} */
  let rows = [];
  /** @type {"loading" | "failed" | "ready"} */
  let status = "loading";
  let why = "";
  let layout =
    remembered("homelab.map.capacity", "table") === "bullets"
      ? "bullets"
      : "table";
  /** @type {"mem" | "cpu" | "disk"} */
  let measure = /** @type {any} */ (
    remembered("homelab.map.capacity.measure", "mem")
  );
  if (!["mem", "cpu", "disk"].includes(measure)) measure = "mem";

  const block = tableBlock({
    remember: "map-capacity",
    caption: "What each stack uses of its allowance",
    captionHidden: true,
    search: null,
    nothing: "No stack has metrics yet.",
    columns: [
      { label: "Stack", sort: "text" },
      { label: "Memory", sort: "number" },
      { label: "CPU", sort: "number" },
      { label: "Disk", sort: "number" },
    ],
  });
  block.wrap.setAttribute("data-kp-sort-multi", "");
  const bullets = h("div", {
    class: "kp-bullets mv-bullets",
    "data-kp-bullets": "",
    hidden: "",
  });
  const layoutSeg = segSwitch({
    label: "Layout",
    value: layout,
    items: [
      {
        value: "table",
        label: "Table with three bar columns",
        hint: "Memory, CPU and disk side by side; sort by any column",
      },
      {
        value: "bullets",
        label: "Ranked bars for one measure",
        hint: "The stacks ranked, worst first, for memory, CPU or disk",
      },
    ],
    onChange: (v) => {
      layout = v === "bullets" ? "bullets" : "table";
      remember("homelab.map.capacity", layout);
      draw();
    },
    mark: (b, v) => void drivable(b, CAP_LAYOUT, v),
  });
  const measureSeg = segSwitch({
    label: "Measure",
    value: measure,
    items: [
      {
        value: "mem",
        label: "Memory",
        hint: "Rank by the share of each stack's memory in use",
      },
      {
        value: "cpu",
        label: "CPU",
        hint: "Rank by the share of its cores in use",
      },
      {
        value: "disk",
        label: "Disk",
        hint: "Rank by the share of its volume that is full",
      },
    ],
    onChange: (v) => {
      measure = /** @type {any} */ (v);
      remember("homelab.map.capacity.measure", v);
      draw();
    },
    mark: (b, v) => void drivable(b, CAP_MEASURE, v),
  });
  const measureRow = h(
    "div",
    { class: "mv-ctl" },
    h("b", { class: "mv-ctl__label" }, "Measure"),
    measureSeg.el,
  );
  const sec = section({
    title: "Capacity",
    desc: "Memory, CPU and disk of every stack against its own allowance, as a table that sorts on any measure or as ranked bars.",
    id: "capacity",
    body: [
      h(
        "div",
        { class: "mv-ctl" },
        h("b", { class: "mv-ctl__label" }, "Layout"),
        layoutSeg.el,
      ),
      measureRow,
      block.wrap,
      bullets,
    ],
    foot: ["Prometheus · now", "amber from 75 % · red from 90 %"],
  });
  sec.el.classList.add("mp-half", "mv-card");
  sec.el.setAttribute("aria-label", "Capacity");
  const detachTables = attachDataTables(sec.el);
  const detachMarks = attachIdmarks(sec.el);
  block.wrap.addEventListener("kp-datatable-retry", () => deps.retry());
  bullets.addEventListener("kp-bullets-retry", () => deps.retry());
  bullets.addEventListener("kp-bullets-pick", (e) =>
    deps.navigate(deps.stackHref(/** @type {CustomEvent} */ (e).detail.key)),
  );
  new MutationObserver(() => {
    for (const b of bullets.querySelectorAll(
      ".kp-bullets__link[data-kp-bullets-key]",
    ))
      if (b instanceof HTMLElement && !b.dataset.drive)
        drivable(b, CAP_OPEN, b.dataset.kpBulletsKey ?? "");
  }).observe(bullets, { childList: true, subtree: true });

  const draw = () => {
    layoutSeg.set(layout);
    measureSeg.set(measure);
    block.wrap.hidden = layout !== "table";
    bullets.hidden = layout !== "bullets";
    measureRow.hidden = layout !== "bullets";
    const fleet = current().fleet?.stacks ?? [];
    if (layout === "table") {
      if (status === "loading")
        return block.loading({ words: "Reading the capacity" });
      if (status === "failed")
        return block.failed({
          what: "the capacity",
          why,
          fix: "Check that Prometheus answers, then try again",
        });
      block.tbody.replaceChildren(
        ...rows.map((r) => {
          const s = fleet.find((x) => x.name === r.stack);
          /** @param {"mem" | "cpu" | "disk"} m */
          const cell = (m) => {
            const c = h("span", { class: "kp-bar-cell" });
            const v = r[m];
            setBarCell(c, {
              value: v == null ? null : v / 100,
              text: capacityText(r, s, m, gb),
              sortValue: v ?? -1,
              warnAt: 0.75,
              badAt: 0.9,
            });
            return h("td", null, c);
          };
          return h(
            "tr",
            { "data-kp-row-key": r.stack },
            h(
              "td",
              { "data-kp-sort-value": r.stack },
              h(
                "span",
                { class: "mv-name" },
                h("span", {
                  class: "kp-idmark kp-idmark--sm",
                  "data-kp-idmark": r.stack,
                  "aria-hidden": "true",
                }),
                h(
                  "a",
                  { href: deps.stackHref(r.stack) },
                  h("b", null, r.stack),
                ),
              ),
            ),
            cell("mem"),
            cell("cpu"),
            cell("disk"),
          );
        }),
      );
      labelCells(block.tbody, () => ["Stack", "Memory", "CPU", "Disk"]);
      block.ready();
      return;
    }
    const state =
      status === "loading"
        ? "loading"
        : status === "failed"
          ? "error"
          : rows.length
            ? "ready"
            : "empty";
    setBulletsQuiet(bullets, {
      threshold: 75,
      bad: 90,
      max: 100,
      sort: "worst",
      state,
      message: why,
      emptyTitle: "No stack has metrics yet",
      emptyText:
        "Each stack's node-exporter reports to Prometheus; a stack appears here at its first reading.",
      loadingRows: 6,
      loadingWords: "Reading the capacity",
      rows: rows.map((r) => ({
        key: r.stack,
        label: r.stack,
        lead: r.stack,
        value:
          r[measure] == null
            ? null
            : Math.round(/** @type {number} */ (r[measure])),
        figure:
          r[measure] == null
            ? "—"
            : `${Math.round(/** @type {number} */ (r[measure]))} %`,
        pick: true,
      })),
    });
  };
  draw();
  return {
    el: sec.el,
    /** @param {typeof rows | null} r @param {"loading" | "failed" | "ready"} s @param {string} [w] */
    set: (r, s, w = "") => {
      if (r) rows = r;
      status = s;
      why = w;
      draw();
    },
    redraw: draw,
    stop: () => {
      detachTables();
      detachMarks();
    },
  };
}

// ════════════════════════════════════════════════════════════════════════
// Disk growth
// ════════════════════════════════════════════════════════════════════════

/**
 * Disk growth: the filesystem's last days, a line from now to 100 % at the
 * same pace and the day it fills; above it who fills the root disk.
 * @param {{retry: () => void}} deps
 */
export function diskCard(deps) {
  /** @type {any[]} */
  let rows = [];
  /** @type {import("./charts.js").Series[]} the filesystems' last 30 days */
  let histories = [];
  /** @type {"loading" | "failed" | "ready"} */
  let status = "loading";
  let why = "";
  let pick = 0;

  const who = h("figure", {
    class: "kp-stackbar kp-stackbar--lg",
    "data-kp-stackbar": "",
  });
  const line = h("p", { class: "mv-line", role: "status" });
  const chartHost = h("figure", {
    class: "kp-chart mv-chart",
    "data-kp-chart": "",
    "aria-label": "Disk used, last 30 days and projection",
  });
  const chartStage = stageOf(chartHost, "mv-stage--chart");
  const select = /** @type {HTMLSelectElement} */ (
    h("select", { class: "kp-field__input", "aria-label": "Filesystem" })
  );
  drivable(select, DISK_FS);
  const sec = section({
    title: "Disk growth",
    desc: "Charts a filesystem's last days, continues the line to 100 % at the same pace and says on which day it fills; above it a bar shows who holds the root disk.",
    id: "growth",
    body: [
      who,
      h(
        "div",
        { class: "mv-ctl" },
        h("b", { class: "mv-ctl__label" }, "Filesystem"),
        h("div", { class: "mv-ctl__control" }, select),
      ),
      line,
      chartStage,
    ],
    foot: ["Prometheus · 7-day robust trend", "warns before a disk is full"],
  });
  sec.el.classList.add("mp-half", "mv-card");
  sec.el.setAttribute("aria-label", "Disk growth");
  const detachCards = attachCardStates(sec.el);

  const diskParts = () => {
    const f = current().fleet;
    const d = f?.host.disk_detail;
    if (!f || !d) return null;
    const total = Math.round(d.root_lv_size_gb);
    const dirs = d.top_dirs.map(([path, size]) => ({
      key: path,
      label: path,
      value: Math.round(size * 10) / 10,
    }));
    const named = dirs.reduce((t, p) => t + p.value, 0);
    const other = Math.max(
      0,
      Math.round(((f.host.disk_pct / 100) * d.root_lv_size_gb - named) * 10) /
        10,
    );
    return {
      total,
      parts: [
        ...dirs,
        ...(other > 0
          ? [{ key: "other", label: "Everything else", value: other }]
          : []),
      ],
    };
  };

  const draw = () => {
    const row = rows[pick];
    const isRoot =
      row?.subject === "/" ||
      row?.subject === "" ||
      (row?.scope === "host" && row?.subject === "/");
    const dp = diskParts();
    setStackbar(who, {
      title: dp
        ? `Who fills the root disk, ${gbText(dp.total)}`
        : "Who fills the root disk",
      total: dp?.total,
      format: (v) => gbText(v),
      restLabel: "Free space",
      marks: [{ at: 0.8, label: "amber at 80 %" }],
      parts: isRoot ? (dp?.parts ?? []) : [],
      label: "Who fills the root disk",
      state:
        status === "loading"
          ? "loading"
          : status === "failed"
            ? "error"
            : !isRoot
              ? "empty"
              : dp
                ? "ready"
                : "empty",
      message:
        status === "failed"
          ? why
          : !isRoot
            ? "The split is known for the host root only."
            : "The host reports no filesystem detail yet.",
    });
    select.disabled = status !== "ready" || !rows.length;
    select.replaceChildren(
      ...rows.map((r, i) =>
        h(
          "option",
          { value: String(i), ...(i === pick ? { selected: "" } : {}) },
          `${r.scope === "host" ? "Host" : "Stack"} ${r.subject || "/"}`,
        ),
      ),
    );
    if (status === "loading") {
      line.textContent = "";
      setCardState(chartStage, "loading", { words: "Reading the disk trends" });
      return;
    }
    if (status === "failed") {
      line.textContent = "";
      setCardState(chartStage, "error", {
        title: "Could not read the disk growth",
        text: why,
      });
      return;
    }
    if (!row) {
      line.textContent = "";
      setCardState(chartStage, "empty", {
        title: "No filesystem has enough history to predict yet",
        text: "A trend needs 7 days of disk readings; until then the Capacity card shows how full each disk is now.",
      });
      return;
    }
    const nowS = Math.floor(Date.now() / 1000);
    const days = row.fit.days_to_full;
    const proj = projection(row.fit, nowS);
    line.textContent =
      days == null
        ? `${row.subject || "/"}: ${Math.round(row.fit.pct_now * 10) / 10} % used, not growing.`
        : `${row.subject || "/"}: ${Math.round(row.fit.pct_now * 10) / 10} % used, full in ${days > 365 ? "more than a year" : `${days.toFixed(1)} days`}${proj.fullAt ? ` (${formatDay(proj.fullAt)})` : ""} at the pace of the last days.`;
    setCardState(chartStage, "ready");
    const mine = histories.find((x) => x.label === (row.subject || "/"));
    const hist = mine ? [mine] : [];
    const to = proj.fullAt
      ? Math.min(proj.fullAt, nowS + 45 * 86400) + 2 * 86400
      : nowS + 2 * 86400;
    timeChart(chartHost, {
      label: `Disk used: ${row.subject || "/"}`,
      key: "disk-growth",
      unit: "percent",
      yMax: 100,
      from: nowS - 30 * 86400,
      to,
      series: [
        ...hist,
        ...(proj.points.length
          ? [
              {
                label: "Projected",
                colour: "var(--destructive)",
                points: proj.points,
              },
            ]
          : hist.length
            ? []
            : [
                {
                  label: row.subject || "/",
                  points: /** @type {[number, number][]} */ ([
                    [nowS, Math.round(row.fit.pct_now * 10) / 10],
                  ]),
                },
              ]),
      ],
      annotations:
        proj.fullAt && proj.fullAt <= to
          ? [
              {
                at: proj.fullAt,
                label: `Full in ${days > 365 ? "more than a year" : `${Math.round(days)} days`} (${formatDay(proj.fullAt)})`,
                tone: /** @type {const} */ ("bad"),
              },
            ]
          : [],
    });
  };
  select.addEventListener("change", () => {
    pick = Number(select.value) || 0;
    draw();
  });
  who.addEventListener("kp-stackbar-retry", () => deps.retry());
  draw();
  return {
    el: sec.el,
    /**
     * @param {{rows?: any[], history?: import("./charts.js").Series[], status: "loading" | "failed" | "ready", why?: string}} m
     */
    set: (m) => {
      if (m.rows) rows = m.rows;
      if ("history" in m) histories = m.history ?? [];
      status = m.status;
      why = m.why ?? "";
      draw();
    },
    stop: detachCards,
  };
}

// ════════════════════════════════════════════════════════════════════════
// Stale images
// ════════════════════════════════════════════════════════════════════════

/**
 * Stale images: a kp datatable with a selection bar that sends the ticked
 * images to the Update flow; a row opens to show what uses the image.
 * @param {{navigate: (href: string) => void, updateHref: (stacks: string[], pick: string[]) => string,
 *   retry: () => void, rowAction: (g: import("./mapview.js").StaleGroup) => HTMLElement | null}} deps
 */
export function staleCard(deps) {
  /** @type {import("./mapview.js").StaleGroup[]} */
  let groups = [];
  /** @type {"loading" | "failed" | "ready"} */
  let status = "loading";
  let why = "";
  /** @type {import("./doctor.js").RouteError | null} */
  let error = null;
  /** @type {Set<string>} */
  let ticked = new Set();
  const when = h("span", { class: "mk-muted mk-sm" });

  const block = tableBlock({
    remember: "stale-images",
    caption: "Images behind their upstream; tick the ones to update",
    captionHidden: true,
    search: null,
    nothing: "Every pinned image is current.",
    select: {
      label: "Select every stale image the Update flow can move",
      actions: [],
    },
    columns: [
      { label: "App", sort: "text" },
      { label: "Image", sort: "text" },
      { label: "Jump", sort: "number" },
      { label: "Waiting", sort: "number", cls: "kp-text-end" },
      { label: "Action", sort: null, cls: "mk-n" },
    ],
  });
  block.wrap.querySelector("[data-kp-datatable-actions]")?.remove();
  block.wrap.setAttribute("data-kp-expandable", "");
  block.wrap.setAttribute("data-kp-expand-single", "");
  block.wrap.id = "mv-stale";
  const all = block.wrap.querySelector("[data-kp-select-all]");
  if (all instanceof HTMLElement) drivable(all, STALE_ALL);
  const bar = h(
    "div",
    {
      class: "kp-selection mv-bar",
      "data-kp-selection": "",
      "data-kp-selection-for": "mv-stale",
      "data-kp-placement": "sticky-top",
      id: "mv-stale-bar",
      "aria-label": "Images to update",
    },
    h(
      "div",
      { class: "kp-selection__actions" },
      drivable(
        h(
          "button",
          {
            type: "button",
            class: "kp-button kp-button--primary",
            "data-kp-selection-action": "update",
          },
          "Update",
        ),
        STALE_GO,
      ),
    ),
  );
  const sec = section({
    title: "Stale images",
    desc: "Lists every image that is behind its upstream, the riskiest jump first; open a row to see which stacks use the image, and send the ticked ones to the Update flow (a major version is taken on its own).",
    id: "stale",
    tools: [when],
    body: [bar, block.wrap],
  });
  sec.el.classList.add("mv-card");
  const detachTables = attachDataTables(sec.el);
  const detachMarks = attachIdmarks(sec.el);

  const idOf = (/** @type {import("./mapview.js").StaleGroup} */ g) =>
    `${g.stacks[0]}/${g.container || g.image}`;
  const sel = bindSelection(
    sec.el,
    bar,
    () => ({
      update: (items) => {
        const majors = items.filter(
          (i) => groups.find((g) => idOf(g) === i.key)?.major,
        );
        return majors.length && items.length > 1
          ? `A major update needs its own read: update ${majors[0].label} on its own`
          : "";
      },
    }),
    undefined,
    (c) => drivable(c, STALE_UNTICK),
  );
  bar.addEventListener("kp-selection-change", (e) => {
    ticked = new Set(/** @type {CustomEvent} */ (e).detail.keys);
  });
  bar.addEventListener("kp-selection-action", (e) => {
    const { keys } = /** @type {CustomEvent} */ (e).detail;
    const chosen = groups.filter((g) => keys.includes(idOf(g)));
    const stacks = [...new Set(chosen.flatMap((g) => g.stacks))];
    setSelectionBusy(bar, null);
    setSelectionResult(bar, { done: chosen.length, refused: [] });
    deps.navigate(deps.updateHref(stacks, keys));
  });
  block.wrap.addEventListener("kp-datatable-retry", () => deps.retry());

  const draw = () => {
    if (status === "loading")
      return block.loading({ words: "Reading the stale images" });
    if (status === "failed")
      return error
        ? block.failed(error)
        : block.failed({ what: "the stale images", why, fix: "" });
    block.tbody.replaceChildren(
      ...groups.flatMap((g) => {
        const id = idOf(g);
        const movable = Boolean(g.key);
        const tick = selectCell(id, `Select ${g.image}`);
        const box = /** @type {HTMLInputElement} */ (
          tick.querySelector("input")
        );
        drivable(box, STALE_TICK, id);
        if (!movable) {
          box.disabled = true;
          box.title =
            "This pin lives in the homelab binary: it moves with a homelab release";
        }
        const level = versionLevel(g.pinned, g.latest, {});
        const url = releaseUrl(g.upstream, g.latest);
        const released = g.released
          ? Date.parse(`${g.released}T12:00:00Z`)
          : NaN;
        const waited = Number.isFinite(released)
          ? Math.max(0, Math.floor((Date.now() - released) / 86400000))
          : null;
        const users = stacksUsing(g);
        return [
          h(
            "tr",
            { "data-kp-row-key": id, "data-kp-row-label": g.image },
            tick,
            h(
              "td",
              { "data-kp-sort-value": g.image },
              h(
                "span",
                { class: "mv-name" },
                h("span", {
                  class: "kp-idmark kp-idmark--sm",
                  "data-kp-idmark": g.image,
                  "aria-hidden": "true",
                }),
                h("b", null, g.image),
                h("span", { class: "kp-timestamp" }, g.stacks.join(", ")),
              ),
            ),
            h(
              "td",
              { class: "kp-mono", "data-kp-sort-value": g.upstream },
              upstreamLink(g.upstream),
            ),
            h(
              "td",
              {
                "data-kp-sort-value": String(
                  { major: 3, minor: 2, patch: 1 }[
                    /** @type {"major"} */ (level)
                  ] ?? 0,
                ),
              },
              h("span", {
                class: "kp-badge kp-badge--version",
                "data-kp-version-from": g.pinned,
                "data-kp-version-to": g.latest,
                "data-kp-version-app": g.image,
                ...(url ? { "data-kp-version-href": url } : {}),
              }),
            ),
            h(
              "td",
              {
                class: "kp-text-end",
                "data-kp-sort-value": String(waited ?? -1),
              },
              waited == null
                ? "—"
                : `${waited} ${waited === 1 ? "day" : "days"}`,
            ),
            h(
              "td",
              { class: "mk-n row-actions" },
              deps.rowAction(g) ??
                h(
                  "span",
                  {
                    class: "mk-muted mk-sm",
                    title: "This pin lives in the homelab binary",
                  },
                  "with a homelab release",
                ),
            ),
          ),
          h(
            "tr",
            { "data-kp-row-detail": "" },
            h(
              "td",
              { colspan: "6" },
              h(
                "div",
                { class: "mv-detail" },
                h("h4", null, "Used by"),
                users.length
                  ? (() => {
                      const box = h("div", {
                        class: "kp-deplist",
                        "data-kp-deplist": "",
                      });
                      setDeplist(
                        box,
                        users.map((u) => ({
                          from: u,
                          to: g.image,
                          state: /** @type {const} */ ("running"),
                        })),
                      );
                      return box;
                    })()
                  : h(
                      "p",
                      { class: "mv-line" },
                      "Nothing depends on this image.",
                    ),
                h(
                  "p",
                  { class: "mv-line" },
                  ...(url
                    ? [
                        "Release notes: ",
                        h(
                          "a",
                          {
                            href: url,
                            target: "_blank",
                            rel: "noopener noreferrer",
                            title: `The ${g.latest} release notes, in a new tab`,
                          },
                          "open them in a new tab",
                        ),
                      ]
                    : ["No release address is known for this image."]),
                ),
              ),
            ),
          ),
        ];
      }),
    );
    labelCells(block.tbody, () => [
      "",
      "App",
      "Image",
      "Jump",
      "Waiting",
      "Action",
    ]);
    attachVersionDeltas(block.tbody);
    block.ready();
    for (const b of /** @type {NodeListOf<HTMLInputElement>} */ (
      block.tbody.querySelectorAll("input[data-kp-select-row]")
    )) {
      const want = ticked.has(b.value) && !b.disabled;
      if (b.checked !== want) {
        b.checked = want;
        b.dispatchEvent(new Event("change", { bubbles: true }));
      }
    }
  };
  /** The stacks whose files hold this image, one name each. @param {import("./mapview.js").StaleGroup} g */
  const stacksUsing = (g) => g.stacks;
  draw();
  return {
    el: sec.el,
    /**
     * @param {{images?: any[], at?: number | null, status: "loading" | "failed" | "ready", error?: import("./doctor.js").RouteError | null}} m
     */
    set: (m) => {
      if (m.images) groups = staleGroups(m.images);
      status = m.status;
      error = m.error ?? null;
      why = m.error?.why ?? "";
      when.textContent = m.at
        ? `from the fleet check · ${formatDay(m.at)}`
        : "";
      draw();
    },
    stop: () => {
      detachTables();
      sel.stop();
      detachMarks();
    },
  };
}
