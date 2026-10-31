// The Map's side views on kp-themes 11 (feat-kp11demo-2, approved; the side
// panel, the matrix, capacity, disk growth and stale images were redone
// after the first review: every part loads, empties and fails with the
// theme's own states, and nothing is taller than its content). The network
// graph itself is kp-themes' own (gap 102);
// js/pages/mapgraph.js draws it, this module draws what stands around it.
//
// Pure helpers first (tested without a browser), the DOM builders after.

import { neighbours } from "./mapview.js";

/**
 * @typedef {import("./topology.js").Topology} Topology
 * @typedef {import("/static/kp/dist/kp-themes.min.js").DepItem} DepItem
 */

/**
 * What a stack uses, and who uses it, the Map's way: a stack uses the
 * stacks it connects to; a gateway route points from the gateway at its
 * backend, so the backend is used by the gateway.
 * @param {Topology} topo
 * @param {string} id
 * @returns {{uses: {other: string, kind: string}[], usedBy: {other: string, kind: string}[]}}
 */
export function usesOf(topo, id) {
  const real = topo.edges.filter((e) => e.kind !== "named");
  return {
    uses: [
      ...real
        .filter((e) => e.from === id && e.kind !== "route")
        .map((e) => ({ other: e.to, kind: e.kind })),
      ...real
        .filter((e) => e.to === id && e.kind === "route")
        .map((e) => ({ other: e.from, kind: e.kind })),
    ],
    usedBy: [
      ...real
        .filter((e) => e.to === id && e.kind !== "route")
        .map((e) => ({ other: e.from, kind: e.kind })),
      ...real
        .filter((e) => e.from === id && e.kind === "route")
        .map((e) => ({ other: e.to, kind: e.kind })),
    ],
  };
}

/**
 * The stacks that stop working with this one.
 * @param {Topology} topo
 * @param {string} id
 */
export const blastOf = (topo, id) => [
  ...new Set(usesOf(topo, id).usedBy.map((x) => x.other)),
];

/**
 * The words of a connection kind, for the deplist grouped by kind.
 * @type {Record<string, string>}
 */
export const KIND_WORDS = {
  declared: "declared, firewall enforced",
  planned: "declared, firewall not on yet",
  open: "target has no firewall (open)",
  named: "address named in a stack file",
  route: "gateway route to a backend",
};

/** The matrix tone of each connection kind. @type {Record<string, "ok" | "warn" | "bad" | "muted" | "level">} */
export const KIND_TONE = {
  declared: "ok",
  planned: "warn",
  open: "bad",
  named: "muted",
  route: "level",
};

/**
 * The matrix cell of one pair: the connection between them, or none.
 * @param {Topology} topo
 * @param {string} row
 * @param {string} col
 * @param {boolean} notEnforced only the open and named connections show
 */
export function matrixCell(topo, row, col, notEnforced) {
  if (row === col) return undefined;
  const e = topo.edges.find((x) => x.from === row && x.to === col);
  if (!e || (notEnforced && e.kind !== "open" && e.kind !== "named"))
    return {
      tone: /** @type {const} */ ("none"),
      label: `${row} does not reach ${col}`,
    };
  return {
    tone: KIND_TONE[e.kind] ?? "muted",
    text: e.kind,
    label: `${row} reaches ${col}: ${KIND_WORDS[e.kind] ?? e.kind}`,
  };
}

/**
 * How many stacks a stack uses and is used by, for the matrix's line.
 * @param {Topology} topo
 * @param {string} id
 */
export function countsOf(topo, id) {
  const n = neighbours(topo, id);
  return { uses: n.dependsOn.length, usedBy: n.dependedOnBy.length };
}

/**
 * The fleet's capacity rows as bar cells: memory as "9.8 of 12 GB" when the
 * fleet says what the stack may use, a percent otherwise.
 * @param {{stack: string, cpu: number | null, mem: number | null, disk: number | null}} r
 * @param {import("./fleet.js").Stack | undefined} s
 * @param {"cpu" | "mem" | "disk"} measure
 * @param {(mb: number | null | undefined) => string} gb
 */
export function capacityText(r, s, measure, gb) {
  const v = r[measure];
  if (v == null) return "not read";
  if (measure === "mem" && s?.ram_used_mb != null && s.ram_max_mb)
    return `${gb(s.ram_used_mb)} of ${gb(s.ram_max_mb)} GB`;
  return `${Math.round(v)} %`;
}

/**
 * The line of a filesystem's projection to full: from now at its pace to
 * 100 %, or a note when it is not growing.
 * @param {{pct_now: number, pct_per_day_robust: number, days_to_full: number | null}} fit
 * @param {number} nowS unix seconds
 * @returns {{points: [number, number][], fullAt: number | null}}
 */
export function projection(fit, nowS) {
  if (fit.days_to_full == null || fit.days_to_full > 365)
    return { points: [], fullAt: null };
  const at = nowS + fit.days_to_full * 86400;
  return {
    points: [
      [nowS, Math.round(fit.pct_now * 10) / 10],
      [at, 100],
    ],
    fullAt: at,
  };
}

/** The matrix view this browser remembers, grid only: the lines view was dropped (see the report). */
export const MATRIX_VIEW = "grid";
