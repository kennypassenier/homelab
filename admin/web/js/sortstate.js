// A table's sort as data (DESIGN_LANGUAGE §12; Kenny's rule: tables sort
// on several keys and remember their sort per table): the pure half, so
// the node tests read it without a DOM. ui.js re-exports it with the
// header block (`sortHead`) and `sortableTable` built on it.

/**
 * One key of a sort: the column's key and its direction (1 ascending,
 * -1 descending). A table's sort is a list of them, the first one first.
 * @typedef {{key: string, dir: 1 | -1}} SortKey
 */

/**
 * A click on a sort header (Kenny's rule, redesign-final Low: multi-sort
 * without Shift): a plain click adds the column as a further key,
 * ascending; clicking it again reverses it, a third click takes it out.
 * The first column clicked sorts first. "Reset sort" (`[]`) clears them.
 * @template {string} K
 * @param {{key: K, dir: 1 | -1}[]} sort
 * @param {K} key
 * @returns {{key: K, dir: 1 | -1}[]}
 */
export function nextSort(sort, key) {
  const i = sort.findIndex((s) => s.key === key);
  if (i < 0) return [...sort, { key, dir: 1 }];
  if (sort[i].dir < 0) return sort.filter((_, j) => j !== i);
  return sort.map((s, j) => (j === i ? { key, dir: -1 } : s));
}

/**
 * Compares two values a column sorts by: numbers as numbers, text without
 * case and with numbers inside it in their order ("ct 9" before "ct 10");
 * empty (null, undefined, "") last whatever the direction.
 * @param {unknown} a
 * @param {unknown} b
 * @returns {number}
 */
export function compareValues(a, b) {
  const none = (/** @type {unknown} */ v) => v == null || v === "";
  if (none(a) || none(b)) return none(a) === none(b) ? 0 : none(a) ? 1 : -1;
  if (typeof a === "number" && typeof b === "number") return a - b;
  return String(a).localeCompare(String(b), undefined, {
    numeric: true,
    sensitivity: "base",
  });
}

/**
 * `rows` sorted by every key of `sort` in turn, stable (rows that tie on
 * every key keep their order); empty values last in both directions.
 * Returns a new array.
 * @template R
 * @template {string} K
 * @param {R[]} rows
 * @param {{key: K, dir: 1 | -1}[]} sort
 * @param {(row: R, key: K) => unknown} get the value a row sorts by
 * @returns {R[]}
 */
export function applySort(rows, sort, get) {
  if (!sort.length) return [...rows];
  return rows
    .map((row, i) => ({ row, i }))
    .sort((x, y) => {
      for (const { key, dir } of sort) {
        const a = get(x.row, key);
        const b = get(y.row, key);
        const empty = (/** @type {unknown} */ v) => v == null || v === "";
        const c = compareValues(a, b);
        if (c !== 0) return empty(a) || empty(b) ? c : c * dir;
      }
      return x.i - y.i;
    })
    .map((x) => x.row);
}

/** The storage key a table's remembered sort lives under. */
export const sortStoreKey = (/** @type {string} */ name) => `nx-sort:${name}`;

/**
 * The sort a table was left with (per table name, in this browser), or
 * `fallback` when none was kept or the stored one cannot be read.
 * @param {string} name
 * @param {SortKey[]} [fallback]
 * @returns {SortKey[]}
 */
export function rememberedSort(name, fallback = []) {
  try {
    const raw = globalThis.localStorage?.getItem(sortStoreKey(name));
    if (!raw) return fallback;
    const v = JSON.parse(raw);
    if (
      Array.isArray(v) &&
      v.every(
        (s) => s && typeof s.key === "string" && (s.dir === 1 || s.dir === -1),
      )
    )
      return v.map((s) => ({ key: s.key, dir: s.dir }));
  } catch {
    // storage blocked or the value is not ours: fall back
  }
  return fallback;
}

/**
 * Keep a table's sort for the next visit (an empty sort forgets it).
 * @param {string} name
 * @param {SortKey[]} sort
 */
export function keepSort(name, sort) {
  try {
    if (sort.length)
      globalThis.localStorage?.setItem(
        sortStoreKey(name),
        JSON.stringify(sort),
      );
    else globalThis.localStorage?.removeItem(sortStoreKey(name));
  } catch {
    // storage blocked: the sort lasts for this visit only
  }
}
