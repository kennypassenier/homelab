// refresh-one-minute-1: background refreshes are quiet. A self-updating
// part reads again on a timer (REFRESH_MS, one minute), and a read that
// brings nothing new changes nothing on screen: the loading animation
// belongs to a page's first load or to an element that newly appears, never
// to a refresh. Pure helpers, no DOM.

/** The default period of every self-updating part, in ms (one minute). */
export const REFRESH_MS = 60_000;

/** The same period in seconds, for kp's `data-kp-refresh`. */
export const REFRESH_S = REFRESH_MS / 1000;

/**
 * A stable text of a value: object keys sorted, so two reads with the same
 * content give the same text whatever order the server wrote the keys in.
 * @param {unknown} value
 * @param {readonly string[]} [ignore] top-level keys left out (a read time)
 * @returns {string}
 */
export function contentKey(value, ignore = []) {
  /** @param {unknown} v @returns {unknown} */
  const norm = (v) => {
    if (Array.isArray(v)) return v.map(norm);
    if (v && typeof v === "object") {
      return Object.fromEntries(
        Object.entries(/** @type {Record<string, unknown>} */ (v))
          .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
          .map(([k, x]) => [k, norm(x)]),
      );
    }
    return v;
  };
  const top =
    value && typeof value === "object" && !Array.isArray(value)
      ? Object.fromEntries(
          Object.entries(/** @type {Record<string, unknown>} */ (value)).filter(
            ([k]) => !ignore.includes(k),
          ),
        )
      : value;
  return JSON.stringify(norm(top)) ?? "undefined";
}

/**
 * Remembers the last content a part drew. `isNew(v)` is true on the first
 * call and whenever the content moved; false for a refresh that brought
 * nothing new, so the caller skips the redraw (and any loading state).
 * @param {readonly string[]} [ignore] top-level keys that do not count
 * @returns {{isNew: (v: unknown) => boolean, forget: () => void,
 *   seen: () => boolean}}
 */
export function changeGate(ignore = []) {
  /** @type {string | null} */
  let last = null;
  return {
    isNew(v) {
      const key = contentKey(v, ignore);
      if (key === last) return false;
      last = key;
      return true;
    },
    forget() {
      last = null;
    },
    seen: () => last !== null,
  };
}
