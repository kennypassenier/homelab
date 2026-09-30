// The start page (replace-homepage, Kenny 2026-09-30): one tile per service,
// grouped, each opening its app, with the line its stack file's reading
// prints. Everything on a tile is declared in the stack that owns the route
// (`tiles:` in lxc-compose.yml); nothing here knows an app.

import { h, fetchJson } from "../dom.js";

/**
 * @typedef {{stack: string, host: string, url: string, name: string,
 *   group: string, order: number, description?: string, lines: string[],
 *   error?: string}} Tile
 */

/**
 * Tiles in page order, cut into their groups.
 * @param {Tile[]} tiles already sorted by the host
 * @returns {{group: string, tiles: Tile[]}[]}
 */
export function grouped(tiles) {
  /** @type {{group: string, tiles: Tile[]}[]} */
  const out = [];
  for (const t of tiles) {
    const last = out[out.length - 1];
    if (last && last.group === t.group) last.tiles.push(t);
    else out.push({ group: t.group, tiles: [t] });
  }
  return out;
}

/** @param {Tile} t */
function tileEl(t) {
  /** @type {(string | Node)[]} */
  const parts = [h("span", { class: "start-tile__name" }, t.name)];
  if (t.description)
    parts.push(h("span", { class: "start-tile__desc" }, t.description));
  for (const l of t.lines)
    parts.push(h("span", { class: "start-tile__line" }, l));
  if (t.error)
    parts.push(
      h(
        "span",
        { class: "start-tile__error", title: t.error },
        "reading failed",
      ),
    );
  return h(
    "a",
    { class: "start-tile", href: t.url, rel: "noopener" },
    ...parts,
  );
}

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const status = h(
    "p",
    { class: "measured", role: "status" },
    "Reading the tiles…",
  );
  const body = h("div", { class: "start" });
  root.replaceChildren(h("h1", null, "Start"), status, body);
  const abort = new AbortController();
  (async () => {
    const r = await fetchJson("/data/tiles", "the start page", abort.signal);
    if (!r.ok) {
      status.textContent = `${r.error.what}: ${r.error.why}`;
      return;
    }
    /** @type {Tile[]} */
    const tiles = r.body.report?.tiles ?? [];
    status.textContent = tiles.length
      ? ""
      : "No stack declares a tile yet: add `tiles:` to a stack file and deploy it.";
    body.replaceChildren(
      ...grouped(tiles).map((g) =>
        h(
          "section",
          { class: "start-group" },
          h("h2", null, g.group),
          h("div", { class: "start-grid" }, ...g.tiles.map(tileEl)),
        ),
      ),
    );
  })().catch(() => {});
  return () => abort.abort();
}
