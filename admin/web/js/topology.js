// The fleet topology's shared types and one pure helper. The drawing lives in
// pages/mapgraph.js (kp-themes' graph); the two renderers that used to be here
// (topologyEl, topologyFigure) had no caller left.
//
// fix-207 (Kenny, Dutch): "gebruik voor elke stack een andere kleur" - every
// stack gets its own hue, evenly spaced around the wheel in sorted order, so
// the colour a stack gets never depends on which other stacks exist.

/**
 * fix-207: one hue per stack, evenly spaced around the wheel in sorted
 * order — the same order the layout already uses — so the colour a stack
 * gets is deterministic and never collides with a neighbour's.
 * @param {{stack: string}[]} nodes
 * @returns {Map<string, number>}
 */
export function stackHues(nodes) {
  const sorted = [...new Set(nodes.map((n) => n.stack))].sort((a, b) =>
    a.localeCompare(b),
  );
  const hues = new Map();
  sorted.forEach((stack, i) => {
    hues.set(stack, Math.round((360 * i) / Math.max(sorted.length, 1)));
  });
  return hues;
}

/**
 * fix-203: `kind` tells nodes apart generically — `docker` (a compose
 * stack), `native` (declares at least one `natives` unit), `gateway`
 * (whichever vmid some OTHER stack's own `gateway_route.gateway_vmid`
 * names) and `external` (an address a stack's own files name that belongs
 * to no fleet stack at all — a house device, Home Assistant, pve — labelled
 * by its bare IP since nothing here is allowed to know its name).
 * @typedef {{stack: string, vmid: number, ip: string,
 *   kind: "docker"|"native"|"gateway"|"external",
 *   live_enforced?: boolean, live_matches_repo?: boolean}} TopoNode
 * @typedef {{from: string, to: string,
 *   kind: "declared"|"planned"|"open"|"named"|"route", detail: string[]}} TopoEdge
 * @typedef {{nodes: TopoNode[], edges: TopoEdge[]}} Topology
 */
