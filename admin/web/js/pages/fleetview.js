// The Map (redesign-371-map, release 3.71.0; the Fleet view renamed —
// Kenny approved the demo 2026-10-03: ~/.local/share/homelab/redesign-3.71/
// fleetview.html + fleetview.js, implemented exactly). How the stacks
// connect, what they use and what is out of date, all derived from the
// stack files and the fleet's metrics, nothing typed by hand.
//
// Top to bottom: the header (freshness, "Show measured traffic"), four
// KPI tiles, the ONE topology (fix-215, invariant 24: the Firewall page
// links here) as a graph with a side panel or as a list, Capacity and
// Disk growth side by side, and Stale images with an Update per row
// (fix-231/232, invariant 34). Each block reads its own route and fails
// on its own, so a Prometheus outage only empties the two blocks that
// need it.
//
// Data: /data/topology, /data/fleet-traffic (only once the switch is on),
// /data/capacity, /data/disk-growth, /data/stale-images, the fleet store.

import { fetchJson, h, slowRead } from "../dom.js";
import { declare, declareField, drivable } from "../drivable.js";
import { formatDateTime } from "../format.js";
import {
  EDGE_KINDS,
  capacityRows,
  firewallOf,
  hubOf,
  hues,
  isStack,
  mapFigures,
  neighbours,
  sortGrowth,
  trafficByStack,
} from "../mapview.js";
import {
  capacityCard,
  diskCard,
  matrixView,
  sidePanel,
  staleCard,
} from "../mapviews.js";
import { openPinUpdate } from "../pinupdate.js";
import { stackHref } from "../router.js";
import { current, subscribe } from "../store.js";
import { updateHref } from "../updateflow.js";
import {
  dataTable,
  ensureStyle,
  kpiStrip,
  dot,
  pageHeader,
  section,
  segSwitch,
  tableSlot,
} from "../ui.js";
import { attachSwitches } from "/static/kp/js/forms.js";
import { attachCardStates, setCardState } from "/static/kp/js/card-state.js";
import { mapGraph } from "./mapgraph.js";

// fix-239: Live view reaches every stale image's Update (`homelab ui click
// pin-update <stack>/<app>/<service>`) and every other control here.
const PIN_UPDATE = declare({
  id: "pin-update",
  page: "fleetview",
  // redesign-integrate-8: since redesign-flows-1 it opens the one Update
  // flow (an address), not a dialog; Live view waited for a dialog.
  opens: "view",
  row: "<stack>/<app>/<service>",
  what: "a stale image's Update: open the Update flow with that app picked (back up, move the pinned image, commit, deploy, verify)",
});
const VIEW = declare({
  id: "map-view",
  page: "fleetview",
  opens: "view",
  row: "graph|list|matrix",
  what: "show the topology as a graph, as a list or as a matrix of who reaches whom",
});
const KIND = declare({
  id: "map-edge-kind",
  page: "fleetview",
  opens: "view",
  row: "declared|planned|open|named|route",
  what: "show or hide one kind of connection on the topology",
});
// review M5: every page field Live view may set is declared (drivable.js
// `declareField`); the client and the dashboard refuse any other.
const FLEET_TRAFFIC = declareField({
  id: "fleetview-traffic",
  page: "fleetview",
  what: "show the measured traffic on the map",
});
const TRAFFIC = declare({
  id: "map-traffic",
  page: "fleetview",
  opens: "view",
  what: "size each stack's ring by its measured network traffic, or stop",
});
const TILE = declare({
  id: "map-kpi",
  page: "fleetview",
  opens: "view",
  row: "<tile>",
  what: "a KPI tile: scroll to the block it sums up",
});
const OPEN_STACK = declare({
  id: "map-open-stack",
  page: "fleetview",
  opens: "view",
  row: "<stack>",
  what: "open a stack's own page from the Map (side panel, list or capacity)",
});
const FW_RULES = declare({
  id: "map-firewall-rules",
  page: "fleetview",
  opens: "view",
  row: "<stack>",
  what: "open a stack's firewall rules from the Map's side panel",
  shows: "while a node is selected",
  reach: [{ do: "click", control: "map-node", row: "*" }],
});

/**
 * @typedef {import("../topology.js").Topology} Topology
 * @typedef {import("../topology.js").TopoNode} TopoNode
 */

/**
 * fix-231: one stale row's Update — what it moves to, a MAJOR jump marked
 * on the button itself (the dialog then asks for the release notes first).
 * @param {import("../mapview.js").StaleGroup} g
 */
function updateButton(g) {
  const stack = g.stacks[0];
  const btn = h(
    "button",
    {
      type: "button",
      class: `kp-button kp-button--sm${g.major ? "" : " kp-button--primary"}`,
      "data-pin-update": "",
      "data-from": g.pinned,
      "data-to": g.latest,
      ...(g.major ? { "data-major": "" } : {}),
      title: `Back up ${stack}, move ${g.key} from ${g.pinned} to ${g.latest}, commit and deploy, in the Update flow${g.major ? "; a major version: the flow asks you to read the release notes first" : ""}`,
    },
    `Update to ${g.latest}${g.major ? "…" : ""}`,
  );
  drivable(btn, PIN_UPDATE, `${stack}/${g.key}`);
  btn.addEventListener("click", () => {
    void openPinUpdate({
      stack,
      container: g.container,
      key: /** @type {string} */ (g.key),
      pinned: g.pinned,
      latest: g.latest,
      upstream: g.upstream,
    });
  });
  return btn;
}

/**
 * A node's identity square in its topology colour (initials, or ↗ for an
 * address outside the fleet).
 * @param {TopoNode} n
 * @param {Map<string, number>} hue
 */
function mark(n, hue) {
  const m = h(
    "span",
    {
      class: `mp-mark${isStack(n) ? "" : " mp-mark--ext"}`,
      "aria-hidden": "true",
    },
    isStack(n) ? n.stack.slice(0, 2).toUpperCase() : "↗",
  );
  if (isStack(n))
    m.style.setProperty("--stack-hue", String(hue.get(n.stack) ?? 0));
  return m;
}

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  ensureStyle("/css/pages/metrics.css");
  ensureStyle("/css/pages/map.css");
  root.classList.add("mk-page", "mp-page");
  const params = new URLSearchParams(location.search);
  const abort = new AbortController();
  /** @type {(() => void)[]} */
  const stops = [];

  // ── header ────────────────────────────────────────────────────────────
  const input = /** @type {HTMLInputElement} */ (
    h("input", {
      class: "kp-switch__input",
      type: "checkbox",
      role: "switch",
      id: FLEET_TRAFFIC,
    })
  );
  input.checked = params.get("traffic") === "1";
  drivable(input, TRAFFIC);
  const trafficSwitch = h(
    "label",
    {
      class: "kp-switch mp-switch",
      title:
        "Size each stack's ring by its measured network traffic (bytes/s from Prometheus, received + transmitted): per stack, not per connection",
    },
    input,
    h("span", { class: "kp-switch__state", "aria-hidden": "true" }),
    h("span", null, "Show measured traffic"),
  );
  const head = pageHeader({
    title: "Map",
    desc: "How the stacks connect, what they use and what is out of date, all derived from the stack files and the fleet's metrics, nothing typed by hand.",
    live: "read",
    actions: [trafficSwitch],
  });

  // ── KPI tiles ─────────────────────────────────────────────────────────
  const strip = kpiStrip(
    [
      { key: "stacks", label: "Stacks in the graph", target: "topo" },
      { key: "connections", label: "Connections", target: "topo" },
      { key: "firewall", label: "Firewall", target: "topo" },
      { key: "stale", label: "Stale images", target: "stale" },
    ],
    { loading: true, label: "The fleet at a glance", drive: { id: TILE } },
  );

  // ── Topology ──────────────────────────────────────────────────────────
  const viewSeg = segSwitch({
    label: "View",
    items: [
      { value: "graph", label: "Graph", hint: "The topology as a picture" },
      {
        value: "list",
        label: "List",
        hint: "The topology as a table: one row per stack",
      },
      {
        value: "matrix",
        label: "Matrix",
        hint: "Who reaches whom, stack by stack, as a grid",
      },
    ],
    value:
      params.get("view") === "list"
        ? "list"
        : params.get("view") === "matrix"
          ? "matrix"
          : "graph",
    onChange: (v) => showView(v),
    drive: { id: VIEW },
  });
  const topo = section({
    id: "topo",
    title: "Topology",
    desc: "Which container talks to which, from the stack files and the firewall state the host enforces now.",
    tools: [viewSeg.el],
  });
  topo.el.setAttribute("aria-label", "Topology");
  // The topology sits in a kp card that carries the four data states: its
  // loading is the package's busy card over the whole footprint.
  const topoData = h("div", { class: "mv-topo" });
  const topoStage = h(
    "div",
    { class: "kp-card mv-stage mv-stage--topo", "data-kp-state": "loading" },
    h(
      "div",
      { class: "kp-card__stage" },
      h("div", { class: "kp-card__data" }, topoData),
    ),
  );
  // The side panel is a card of its own beside the topology's card, never
  // inside its data: a card inside another card's data is hidden with it
  // (and loses its own loading look) when the topology fails.
  const topoRow = h("div", { class: "mp-topo" }, topoStage);
  topo.body.replaceChildren(topoRow);
  const detachStages = attachCardStates(topo.el);
  setCardState(topoStage, "loading", { words: "Reading the topology" });

  // ── Capacity, Disk growth and Stale images (mapviews.js) ──────────────
  const retryAll = () => void readSides();
  const cap = capacityCard({
    stackHref,
    navigate: (href) => {
      history.pushState(null, "", href);
      dispatchEvent(new PopStateEvent("popstate"));
    },
    retry: retryAll,
  });
  const disk = diskCard({ retry: retryAll });
  const stale = staleCard({
    navigate: (href) => {
      history.pushState(null, "", href);
      dispatchEvent(new PopStateEvent("popstate"));
    },
    updateHref: (stacks, pick) =>
      `${updateHref(null)}&only=${encodeURIComponent(stacks.join(","))}&pick=${encodeURIComponent(pick.join(","))}`,
    retry: retryAll,
    rowAction: (g) => (g.key ? updateButton(g) : null),
  });
  stops.push(() => {
    cap.stop();
    disk.stop();
    stale.stop();
    detachStages();
  });

  root.replaceChildren(
    head.el,
    strip.el,
    topo.el,
    h("div", { class: "mp-grid" }, cap.el, disk.el),
    stale.el,
  );
  stops.push(attachSwitches(root));

  /** @type {Topology | null} */
  let topology = null;
  /** @type {any} */
  let staleBody = null;
  /** @type {ReturnType<typeof mapGraph> | null} */
  let graph = null;
  /** @type {Map<string, number> | null} */
  let traffic = null;
  /** @param {string} n */
  const sideLinks = (n) => [
    drivable(
      h(
        "a",
        {
          class: "kp-button kp-button--sm",
          href: stackHref(n),
          title: `Open ${n}'s own page`,
        },
        "Open stack",
      ),
      OPEN_STACK,
      `side:${n}`,
    ),
    drivable(
      h(
        "a",
        {
          class: "kp-button kp-button--sm",
          href: `${stackHref(n, "settings")}?section=firewall`,
          title: `${n}'s firewall rules, in its settings`,
        },
        "Firewall rules",
      ),
      FW_RULES,
      n,
    ),
  ];
  const panel = sidePanel({
    stackHref,
    hues,
    traffic: () => (input.checked ? traffic : null),
    links: sideLinks,
  });
  const matrix = matrixView();
  const side = panel.el;
  const listBox = h("div", { class: "mp-list", hidden: "" });
  const matrixBox = h("div", { class: "mp-matrix", hidden: "" }, matrix.el);
  const topoGrid = h("div", { class: "mp-graphcell" });
  topoRow.append(side);
  /** The side panel has nothing to say when the topology could not be read or is empty. */
  let sideOff = false;
  const listTable = tableSlot();
  stops.push(listTable.stop, panel.stop);

  const paintTiles = () => {
    if (!topology) return;
    const f = mapFigures(topology, staleBody);
    const T = strip.tiles;
    T.get("stacks")?.set({
      value: String(f.stacks),
      ctx: f.outside
        ? `+ ${f.outside} ${f.outside === 1 ? "address" : "addresses"} outside the fleet`
        : "no address outside the fleet",
    });
    // The demo's colours: what the firewall enforces in green, what is
    // open in amber.
    T.get("connections")?.set({
      value: String(f.connections),
      ctxParts: [
        { text: `${f.enforced} enforced`, tone: "ok" },
        ...(f.planned ? [{ text: `${f.planned} not on yet` }] : []),
        { text: `${f.open} open`, tone: f.open ? "warn" : "" },
      ],
    });
    const off = [
      ...f.firewallNone.map((s) => `${s} none`),
      ...f.firewallDiffers.map((s) => `${s} differs`),
    ];
    T.get("firewall")?.set({
      value: String(f.firewallOn),
      unit: `of ${f.stacks} on`,
      ctx: off.length ? off.join(" · ") : "every stack's firewall is on",
      tone: off.length ? "warn" : "",
    });
    T.get("stale")?.set(
      f.stale == null
        ? { value: "…", ctx: "the fleet check is reading" }
        : {
            value: String(f.stale),
            ctx: f.stale
              ? `${f.major} major ${f.major === 1 ? "jump" : "jumps"} · ${f.updatable} you can update`
              : "every pin is current",
            tone: f.stale ? "warn" : "",
          },
    );
  };

  /** @param {string} v */
  function showView(v) {
    const list = v === "list";
    const grid = v === "matrix";
    topoGrid.hidden = list || grid;
    side.hidden = list || grid || sideOff;
    listBox.hidden = !list;
    matrixBox.hidden = !grid;
    const url = new URL(location.href);
    if (v === "graph") url.searchParams.delete("view");
    else url.searchParams.set("view", v);
    history.replaceState(history.state, "", url);
    if (v === "graph") graph?.draw();
  }

  /** @param {string[]} sel @param {string | null} hover */
  const paintSide = (sel, hover) => panel.pick(sel, hover);

  /** @param {string[]} sel @param {string | null} hover */
  const onGraph = (sel, hover) => {
    const url = new URL(location.href);
    if (sel.length) url.searchParams.set("select", sel.join(","));
    else url.searchParams.delete("select");
    if (url.search !== location.search)
      history.replaceState(history.state, "", url);
    paintSide(sel, hover);
  };

  const paintTopology = () => {
    if (!topology) return;
    const t = topology;
    if (!t.nodes.length) {
      sideOff = true;
      side.hidden = true;
      setCardState(topoStage, "empty", {
        title: "No stacks to draw yet",
        text: "The topology is read from the working copy's stack files; once a stack has a working copy, it appears here.",
      });
      return;
    }
    graph?.stop();
    const want = (params.get("select") ?? "")
      .split(",")
      .filter((s) => t.nodes.some((n) => n.stack === s));
    graph = mapGraph(t, {
      traffic: input.checked ? traffic : null,
      selected: want,
      onChange: onGraph,
    });
    const kinds = EDGE_KINDS.filter((k) =>
      t.edges.some((e) => e.kind === k.kind),
    );
    const legend = h("div", {
      class: "mp-kinds",
      role: "group",
      "aria-label": "Connection kinds: click to hide or show",
    });
    for (const k of kinds) {
      const b = h(
        "button",
        {
          type: "button",
          class: "mp-kind",
          "data-kind": k.kind,
          "aria-pressed": "true",
          title: `${k.hint}; click to hide or show these lines`,
        },
        h("i", {
          class: `mp-kind__line mp-kind__line--${k.kind}`,
          "aria-hidden": "true",
        }),
        k.label,
      );
      drivable(b, KIND, k.kind);
      b.addEventListener("click", () => {
        const hide = b.getAttribute("aria-pressed") === "true";
        b.setAttribute("aria-pressed", String(!hide));
        graph?.hideKind(k.kind, hide);
      });
      legend.append(b);
    }
    graph.onKindsReset(() => {
      for (const b of legend.querySelectorAll("button"))
        b.setAttribute("aria-pressed", "true");
    });
    topoGrid.replaceChildren(graph.el);
    sideOff = false;
    panel.set(t, "ready");
    matrix.set(t, "ready");
    // The list view: one row per stack.
    const hue = hues(t);
    const tbl = dataTable({
      cls: "mk-table",
      remember: "map-topology-list",
      caption: "Topology as a list",
      columns: [
        { label: "Stack", sort: "text" },
        { label: "Address", sort: "text" },
        { label: "Firewall", sort: "text" },
        { label: "Depends on", sort: "text" },
        { label: "Depended on by", sort: "text" },
      ],
    });
    tbl.tbody.replaceChildren(
      ...t.nodes.filter(isStack).map((n) => {
        const nb = neighbours(t, n.stack);
        const fw = firewallOf(n);
        return h(
          "tr",
          { "data-kp-row-key": n.stack },
          h(
            "td",
            null,
            h(
              "span",
              { class: "mp-name" },
              mark(n, hue),
              drivable(
                h("a", { href: stackHref(n.stack) }, n.stack),
                OPEN_STACK,
                `list:${n.stack}`,
              ),
            ),
          ),
          h("td", { class: "mono" }, n.ip),
          h("td", null, h("span", { class: "mp-pill" }, dot(fw.tone), fw.text)),
          h("td", null, nb.dependsOn.join(", ") || "—"),
          h("td", null, nb.dependedOnBy.join(", ") || "—"),
        );
      }),
    );
    listBox.replaceChildren(tbl.wrap);
    topoData.replaceChildren(
      h("div", { class: "mp-legends" }, legend),
      topoGrid,
      listBox,
      matrixBox,
    );
    setCardState(topoStage, "ready");
    listTable.attach(listBox);
    showView(viewSeg.value());
    graph.draw();
    onGraph(want, null);
  };

  const ensureTraffic = async () => {
    if (traffic) return;
    const r = await fetchJson(
      "/data/fleet-traffic",
      "the measured traffic",
      abort.signal,
    );
    if (abort.signal.aborted || !r.ok) return;
    traffic = trafficByStack(r.body.panels ?? []);
    graph?.setTraffic(input.checked ? traffic : null);
  };
  input.addEventListener("change", () => {
    const url = new URL(location.href);
    if (input.checked) url.searchParams.set("traffic", "1");
    else url.searchParams.delete("traffic");
    history.replaceState(history.state, "", url);
    if (input.checked) void ensureTraffic().catch(() => {});
    else graph?.setTraffic(null);
  });

  void (async () => {
    const r = await fetchJson("/data/topology", "the topology", abort.signal);
    if (abort.signal.aborted) return;
    if (!r.ok) {
      sideOff = true;
      side.hidden = true;
      setCardState(topoStage, "error", {
        title: "Could not read the topology",
        text: `${r.error.what}: ${r.error.why}`,
        retry: false,
      });
      panel.set(null, "failed", r.error.why);
      matrix.set(null, "failed", r.error.why);
      return;
    }
    topology = r.body.topology;
    head.live?.set(r.body.head?.at ?? Math.floor(Date.now() / 1000));
    paintTopology();
    paintTiles();
    if (input.checked) void ensureTraffic();
  })().catch(() => {});
  // A stack coming online or going down repaints the side panel.
  stops.push(
    subscribe(() => {
      if (graph) panel.redraw();
    }),
  );

  // ── Capacity, Disk growth, Stale images ───────────────────────────────
  /** @param {any} body `/data/stale-images` */
  const paintStale = (body) => {
    staleBody = body;
    paintTiles();
    stale.set({
      images: body.images ?? [],
      at: body.measured_at ?? null,
      status: "ready",
    });
  };
  async function readSides() {
    cap.set(null, "loading");
    disk.set({ status: "loading" });
    stale.set({ status: "loading" });
    await Promise.allSettled([
      (async () => {
        const r = await fetchJson(
          "/data/capacity",
          "the capacity",
          abort.signal,
        );
        if (abort.signal.aborted) return;
        if (!r.ok) {
          cap.set([], "failed", `${r.error.what}: ${r.error.why}`);
          return;
        }
        cap.set(capacityRows(r.body.panels ?? []), "ready");
      })(),
      (async () => {
        const [g, c] = await Promise.all([
          fetchJson("/data/disk-growth", "disk growth", abort.signal),
          fetchJson("/data/charts?range=30d", "the disk history", abort.signal),
        ]);
        if (abort.signal.aborted) return;
        if (!g.ok) {
          disk.set({
            status: "failed",
            why: `${g.error.what}: ${g.error.why}`,
          });
          return;
        }
        /** @type {import("../charts.js").Series[]} */
        let history = [];
        if (c.ok) {
          const panel = (c.body?.panels ?? []).find((/** @type {any} */ x) =>
            /^disk used per filesystem/i.test(x?.panel?.title ?? ""),
          );
          history = (panel?.series ?? []).filter(
            (/** @type {any} */ x) => x?.points?.length,
          );
        }
        disk.set({
          rows: sortGrowth(g.body.rows ?? []),
          history,
          status: "ready",
        });
      })(),
      (async () => {
        const r = await slowRead(
          "/data/stale-images",
          "stale images",
          abort.signal,
          (last) => paintStale(last),
        );
        if (abort.signal.aborted) return;
        if (!r.ok) {
          stale.set({ status: "failed", error: r.error });
          return;
        }
        paintStale(r.body);
      })(),
    ]);
  }
  void readSides().catch(() => {});

  return () => {
    abort.abort();
    graph?.stop();
    for (const s of stops) s();
    root.classList.remove("mk-page", "mp-page");
  };
}
