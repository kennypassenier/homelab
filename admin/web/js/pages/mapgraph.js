// redesign-371-map (3.71.0): the Map's ONE topology (fix-215, invariant
// 24), drawn as the approved demo draws it (fleetview.html/.js): the hub in
// the middle, every other stack on a ring round it, the addresses outside
// the fleet dashed, one colour per stack (invariant 16), the connection
// kinds told apart by line style.
//
//   hover / focus a node                       isolate its connections
//   click / Enter                              turn the node on or off in
//                                              the selection (several may
//                                              be on, no modifier keys)
//   Show all, Esc                              clear the selection and
//                                              every hidden kind
//
// A hover only toggles classes, never rebuilds a node (invariant 56: a
// click right after the pointer moved must land on the node under it);
// only a resize, the traffic switch and the data itself redraw.

import { h } from "../dom.js";
import { declare, drivable } from "../drivable.js";
import { bends, hues, isStack, layout, radius } from "../mapview.js";

const NS = "http://www.w3.org/2000/svg";

const NODE = declare({
  id: "map-node",
  page: "fleetview",
  opens: "view",
  row: "<stack>",
  what: "turn a stack (or an outside address) on or off in the Map's selection, showing its connections and details",
});
const SHOW_ALL = declare({
  id: "map-show-all",
  page: "fleetview",
  opens: "view",
  what: "clear the Map's selection and show every connection kind again",
  shows: "while a node is selected",
  reach: [{ do: "click", control: "map-node", row: "*" }],
});

/**
 * @param {string} tag
 * @param {Record<string, string | number>} [attrs]
 * @returns {SVGElement}
 */
function svg(tag, attrs = {}) {
  const e = /** @type {SVGElement} */ (document.createElementNS(NS, tag));
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, String(v));
  return e;
}

/**
 * @typedef {import("../topology.js").Topology} Topology
 */

/**
 * @param {Topology} topo
 * @param {{traffic: Map<string, number> | null, selected?: Iterable<string>,
 *   onChange: (selected: string[], hover: string | null) => void}} opts
 */
export function mapGraph(topo, opts) {
  const hue = hues(topo);
  /** @type {(() => void) | null} */
  let kindsChanged = null;
  let drawnWidth = -1;
  /** @type {Set<string>} */
  const pinned = new Set(opts.selected ?? []);
  /** @type {string | null} */
  let hover = null;
  /** @type {Set<string>} */
  const hiddenKinds = new Set();
  let traffic = opts.traffic;

  const root = /** @type {SVGSVGElement} */ (
    svg("svg", {
      class: "mp-svg",
      role: "group",
      "aria-label":
        "Topology: Tab to a stack, Enter to add it to the selection or take it out",
    })
  );
  const reset = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--sm mp-graph__reset",
      hidden: "",
      title: "Clear the selection and show every connection kind again (Esc)",
    },
    "Show all",
  );
  drivable(reset, SHOW_ALL);
  const box = h(
    "div",
    { class: "mp-graph", id: "map-graph" },
    reset,
    root,
    h(
      "p",
      { class: "mp-graph__hint" },
      h("span", null, "Hover a stack to see only its connections"),
      h(
        "span",
        null,
        "Click a stack to add it to the selection, click again to remove it",
      ),
      h("span", null, "Esc shows all"),
    ),
  );

  /** @param {string} id */
  const toggle = (id) => {
    if (pinned.has(id)) pinned.delete(id);
    else pinned.add(id);
    restyle();
    opts.onChange([...pinned], hover);
  };
  /** @param {string | null} id */
  const setHover = (id) => {
    hover = id;
    restyle();
    opts.onChange([...pinned], hover);
  };

  function draw() {
    drawnWidth = box.clientWidth;
    const W = Math.max(280, box.clientWidth || 800);
    const H = W < 600 ? 480 : Math.max(420, Math.min(560, Math.round(W * 0.6)));
    root.setAttribute("viewBox", `0 0 ${W} ${H}`);
    root.setAttribute("height", String(H));
    root.replaceChildren();
    const pos = layout(topo, W, H);
    const bend = bends(topo.edges);
    const eg = svg("g", { class: "mp-edges" });
    topo.edges.forEach((e, i) => {
      const a = pos.get(e.from);
      const b = pos.get(e.to);
      if (!a || !b) return;
      const len = Math.hypot(b[0] - a[0], b[1] - a[1]) || 1;
      const mx = (a[0] + b[0]) / 2 - ((b[1] - a[1]) / len) * bend[i];
      const my = (a[1] + b[1]) / 2 + ((b[0] - a[0]) / len) * bend[i];
      const p = svg("path", {
        d: `M${a[0].toFixed(1)},${a[1].toFixed(1)} Q${mx.toFixed(1)},${my.toFixed(1)} ${b[0].toFixed(1)},${b[1].toFixed(1)}`,
        class: `mp-edge mp-edge--${e.kind}`,
        "data-from": e.from,
        "data-to": e.to,
        "data-kind": e.kind,
      });
      const t = svg("title");
      t.textContent = `${e.from} → ${e.to}: ${e.detail.join(", ") || e.kind}`;
      p.append(t);
      eg.append(p);
    });
    root.append(eg);
    for (const n of topo.nodes) {
      const at = pos.get(n.stack);
      if (!at) continue;
      const [x, y] = at;
      const r = radius(n, traffic);
      const ext = !isStack(n);
      const g = svg("g", {
        class: `mp-node${ext ? " mp-node--ext" : ""}`,
        "data-stack": n.stack,
        tabindex: 0,
        role: "button",
        "aria-pressed": String(pinned.has(n.stack)),
        "aria-label": ext
          ? `${n.stack}, an address outside the fleet`
          : `${n.stack}, vmid ${n.vmid}, ${n.ip}`,
      });
      if (!ext)
        g.style.setProperty("--stack-hue", String(hue.get(n.stack) ?? 0));
      if (traffic && !ext)
        g.setAttribute(
          "data-traffic",
          String(Math.round(traffic.get(n.stack) ?? 0)),
        );
      g.append(
        svg("circle", {
          class: `mp-ring${traffic && !ext ? " mp-ring--traffic" : ""}`,
          cx: x,
          cy: y,
          r,
        }),
      );
      if (!ext)
        g.append(
          svg("circle", {
            class: "mp-core",
            cx: x,
            cy: y,
            r: Math.max(4, r - 6),
          }),
        );
      if (n.live_enforced != null && n.live_matches_repo === false)
        g.append(svg("circle", { class: "mp-fwring", cx: x, cy: y, r: r + 5 }));
      const t = svg("text", { x, y: y + r + 16, "text-anchor": "middle" });
      t.textContent = n.stack;
      g.append(t);
      drivable(/** @type {any} */ (g), NODE, n.stack);
      g.addEventListener("pointerenter", () => setHover(n.stack));
      g.addEventListener("pointerleave", () => setHover(null));
      g.addEventListener("focus", () => setHover(n.stack));
      g.addEventListener("blur", () => setHover(null));
      g.addEventListener("click", () => toggle(n.stack));
      g.addEventListener("keydown", (e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          toggle(n.stack);
        }
      });
      root.append(g);
    }
    restyle();
  }

  /** Selection, hover and hidden kinds change classes only, never the DOM. */
  function restyle() {
    const focus = new Set(pinned);
    if (hover) focus.add(hover);
    box.toggleAttribute("data-focus", focus.size > 0);
    const on = new Set(focus);
    for (const p of root.querySelectorAll(".mp-edge")) {
      const a = p.getAttribute("data-from") ?? "";
      const b = p.getAttribute("data-to") ?? "";
      const hidden = hiddenKinds.has(p.getAttribute("data-kind") ?? "");
      const lit = !hidden && (focus.has(a) || focus.has(b));
      p.classList.toggle("is-hidden", hidden);
      p.classList.toggle("is-on", lit);
      p.classList.toggle("is-dim", focus.size > 0 && !lit && !hidden);
      if (lit) {
        on.add(a);
        on.add(b);
      }
    }
    for (const g of root.querySelectorAll(".mp-node")) {
      const id = g.getAttribute("data-stack") ?? "";
      g.classList.toggle("is-on", on.has(id));
      g.classList.toggle("is-dim", focus.size > 0 && !on.has(id));
      g.classList.toggle("is-pinned", pinned.has(id));
      g.setAttribute("aria-pressed", String(pinned.has(id)));
    }
    reset.hidden = pinned.size === 0 && hiddenKinds.size === 0;
  }

  const clear = () => {
    pinned.clear();
    hiddenKinds.clear();
    restyle();
    opts.onChange([], hover);
    return true;
  };
  reset.addEventListener("click", () => {
    clear();
    kindsChanged?.();
  });
  const onKey = (/** @type {KeyboardEvent} */ e) => {
    if (e.key !== "Escape") return;
    if (
      /** @type {Element | null} */ (e.target)?.closest?.(
        "dialog, input, textarea, select",
      )
    )
      return;
    if (pinned.size === 0 && hiddenKinds.size === 0) return;
    clear();
    kindsChanged?.();
  };
  document.addEventListener("keydown", onKey);
  const ro =
    typeof ResizeObserver === "function"
      ? // Only a new width redraws: the box grows taller when the side
        // panel beside it does (a hover fills it), and that must never
        // rebuild a node (invariant 56).
        new ResizeObserver(() => {
          if (box.clientWidth !== drawnWidth) draw();
        })
      : null;
  ro?.observe(box);
  return {
    el: box,
    draw,
    /** @param {Map<string, number> | null} t */
    setTraffic: (t) => {
      traffic = t;
      draw();
    },
    /** @param {string} kind @param {boolean} hide */
    hideKind: (kind, hide) => {
      if (hide) hiddenKinds.add(kind);
      else hiddenKinds.delete(kind);
      restyle();
    },
    hidden: () => new Set(hiddenKinds),
    /** @param {() => void} f */
    onKindsReset: (f) => {
      kindsChanged = f;
    },
    selected: () => [...pinned],
    stop: () => {
      ro?.disconnect();
      document.removeEventListener("keydown", onKey);
    },
  };
}
