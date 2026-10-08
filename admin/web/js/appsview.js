// redesign-stacks (3.71.0, Kenny approved the Apps demo 2026-10-03:
// redesign-3.71/apps.html): the pure half of the Apps page — each tile's
// watch state in words, the launch search (filter, highlight, the first
// match that Enter opens), the board's groups with Starred on top, and the
// one-line verdict. No DOM, so `node --test` drives it.

/**
 * @typedef {import("./pages/home.js").Tile} Tile
 * @typedef {{key: string, state: "up" | "flaky" | "down" | "deploying",
 *   down?: boolean, failing_since?: number | null, why?: string | null,
 *   checked_at?: number | null}} Watch
 * @typedef {{state: string, tone: "ok" | "warn" | "bad" | "info" | "",
 *   word: string, via: "tile" | "stack" | null,
 *   checkedAt: number | null}} TileState
 */

/**
 * What the minute watch says about a tile: its own probe when it has one,
 * otherwise its stack's container (a tile with no probe is not asked
 * itself; its stack running is the best the dashboard knows).
 * @param {Map<string, Watch>} watch
 * @param {Tile} t
 * @returns {TileState}
 */
export function tileState(watch, t) {
  const own = watch.get(`tile:${t.host}`);
  const w = own ?? watch.get(`stack:${t.stack}`);
  const via = own ? "tile" : w ? "stack" : null;
  const checkedAt = w?.checked_at ?? null;
  if (!w)
    return {
      state: "unwatched",
      tone: "",
      word: "not watched",
      via,
      checkedAt,
    };
  if (w.state === "deploying")
    return {
      state: "deploying",
      tone: "info",
      word: "deploying",
      via,
      checkedAt,
    };
  if (w.state === "down" || w.down)
    return { state: "down", tone: "bad", word: "down", via, checkedAt };
  if (w.state === "flaky" || w.failing_since != null)
    return {
      state: "flaky",
      tone: "warn",
      word: "failing, not yet down",
      via,
      checkedAt,
    };
  return { state: "up", tone: "ok", word: "answers", via, checkedAt };
}

/**
 * @param {Tile} t
 * @returns {string}
 */
const hay = (t) =>
  [t.name, t.host, t.description ?? "", t.stack].join(" ").toLowerCase();

/**
 * The tiles a search keeps, in page order.
 * @param {Tile[]} tiles
 * @param {string} q
 */
export function filterTiles(tiles, q) {
  const s = q.trim().toLowerCase();
  return s ? tiles.filter((t) => hay(t).includes(s)) : tiles;
}

/**
 * The board: Starred first (only while nothing is searched), then each
 * group in page order without its starred tiles; while searching, every
 * group shows all its matches and runs the full width.
 * @param {Tile[]} tiles already sorted by the host
 * @param {Set<string>} starred tile hosts
 * @param {string} q
 * @returns {{group: string, tiles: Tile[], wide: boolean}[]}
 */
export function board(tiles, starred, q) {
  const searching = q.trim() !== "";
  const shown = filterTiles(tiles, q);
  /** @type {{group: string, tiles: Tile[], wide: boolean}[]} */
  const out = [];
  const stars = shown.filter((t) => starred.has(t.host));
  if (stars.length && !searching)
    out.push({ group: "Starred", tiles: stars, wide: true });
  for (const g of [...new Set(shown.map((t) => t.group))]) {
    const ts = shown.filter(
      (t) => t.group === g && (searching || !starred.has(t.host)),
    );
    if (ts.length) out.push({ group: g, tiles: ts, wide: searching });
  }
  return out;
}

/**
 * The text cut around the search's first match, for a `<mark>`.
 * @param {string} text
 * @param {string} q
 * @returns {{text: string, hit: boolean}[]}
 */
export function markParts(text, q) {
  const s = q.trim().toLowerCase();
  const i = s ? text.toLowerCase().indexOf(s) : -1;
  if (i < 0) return [{ text, hit: false }];
  return [
    { text: text.slice(0, i), hit: false },
    { text: text.slice(i, i + s.length), hit: true },
    { text: text.slice(i + s.length), hit: false },
  ].filter((p) => p.text !== "");
}

/**
 * The tile's colour: one of the five chart hues, the same for a name
 * everywhere.
 * @param {string} name
 */
export function hueOf(name) {
  return ([...name].reduce((a, c) => a + c.charCodeAt(0), 0) % 5) + 1;
}

/**
 * The two letters on a tile's icon.
 * @param {string} name
 */
export const initials = (name) => name.trim().slice(0, 2) || "?";

/**
 * The page's one line: every tile answering, or how many need a person.
 * @param {Tile[]} tiles
 * @param {Map<string, Watch>} watch
 * @returns {{tone: "ok" | "bad" | "warn" | "", text: string,
 *   checkedAt: number | null}}
 */
export function appsVerdict(tiles, watch) {
  const states = tiles.map((t) => tileState(watch, t));
  const down = states.filter((s) => s.state === "down").length;
  const flaky = states.filter((s) => s.state === "flaky").length;
  const watched = states.filter((s) => s.state !== "unwatched");
  const checkedAt = watched.reduce(
    (m, s) => (s.checkedAt != null && s.checkedAt > m ? s.checkedAt : m),
    0,
  );
  const at = checkedAt || null;
  if (down)
    return {
      tone: "bad",
      text: `${down} ${down === 1 ? "app is" : "apps are"} down${flaky ? ` · ${flaky} failing` : ""}`,
      checkedAt: at,
    };
  if (flaky)
    return {
      tone: "warn",
      text: `${flaky} ${flaky === 1 ? "app is" : "apps are"} failing, not yet down`,
      checkedAt: at,
    };
  if (!watched.length)
    return {
      tone: "",
      text: `${tiles.length} apps · none watched yet`,
      checkedAt: null,
    };
  return {
    tone: "ok",
    text:
      watched.length === tiles.length
        ? `All ${tiles.length} apps answer`
        : `${watched.length} of ${tiles.length} apps answer · ${tiles.length - watched.length} not watched`,
    checkedAt: at,
  };
}

/**
 * The starred tiles, read from what this browser kept (a list of hosts);
 * anything else reads as none.
 * @param {string | null} raw
 * @returns {Set<string>}
 */
export function readStars(raw) {
  try {
    const v = JSON.parse(raw ?? "[]");
    return new Set(
      Array.isArray(v) ? v.filter((x) => typeof x === "string") : [],
    );
  } catch {
    return new Set();
  }
}

// fix-371-1 (Kenny approved the packed-columns demo 2026-10-04:
// redesign-3.71/demos-3711/apps-packed.html): the board has as many
// columns as its width allows, and each group goes into the column that
// is shortest by tile count, so short groups stack under each other and
// no group leaves a hole a whole group could fill. Starred stays full
// width above the columns.

/** A column's minimum width on a wide board (1000 px and up), in rem. */
export const PACK_WIDE_REM = 22;
/** A column's minimum width on a narrow board (700 px and 390 px), in rem. */
export const PACK_NARROW_REM = 18;
/** The board width, in rem, from which a column needs PACK_WIDE_REM. */
export const PACK_BREAK_REM = 53;

/**
 * A column's minimum width for a board this wide, in rem.
 * @param {number} boardPx
 * @param {number} remPx
 */
export function packMinRem(boardPx, remPx) {
  return boardPx >= PACK_BREAK_REM * remPx ? PACK_WIDE_REM : PACK_NARROW_REM;
}

/**
 * How many columns a board this wide holds.
 * @param {number} boardPx
 * @param {number} gapPx
 * @param {number} remPx
 */
export function packColumns(boardPx, gapPx, remPx) {
  const min = packMinRem(boardPx, remPx) * remPx;
  return Math.max(1, Math.floor((boardPx + gapPx) / (min + gapPx)));
}

/**
 * Each group into the column that is shortest by tile count at that
 * moment, in page order; a folded group counts as its title alone. A
 * column must be shorter by more than half a tile to win, so equal
 * columns fill left to right.
 * @param {{tiles: number, folded?: boolean}[]} groups
 * @param {number} cols
 * @returns {number[][]} per column, the indexes of its groups in order
 */
export function packGroups(groups, cols) {
  const n = Math.max(1, cols);
  const used = Array.from({ length: n }, () => 0);
  /** @type {number[][]} */
  const out = Array.from({ length: n }, () => []);
  groups.forEach((g, i) => {
    let c = 0;
    for (let k = 1; k < n; k++) if (used[k] < used[c] - 0.5) c = k;
    out[c].push(i);
    used[c] += (g.folded ? 0 : g.tiles) + 0.6;
  });
  return out;
}
