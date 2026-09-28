// Sorting on what a cell means, not on what it reads (ui-tables): a time
// shown as "28 Sep 2026, 14:03" and a duration shown as "2 h 5 min" do not
// sort as text. Each page remembers the number behind every such text under
// a sort kind ("time", "duration"); the table's compare looks it up and
// falls back to its own comparison for everything else.

/** @typedef {(a: string, b: string, kind: string, locale: string) => number} Compare */

/**
 * @returns {{
 *   note: (kind: string, text: string, value: number) => string,
 *   compare: (fallback: Compare) => Compare,
 * }}
 */
export function sortKeys() {
  /** @type {Map<string, Map<string, number>>} */
  const kinds = new Map();
  return {
    /** Remember `value` behind `text` and hand the text back for the cell. */
    note(kind, text, value) {
      let m = kinds.get(kind);
      if (!m) {
        m = new Map();
        kinds.set(kind, m);
      }
      m.set(text, value);
      return text;
    },
    compare(fallback) {
      return (a, b, kind, locale) => {
        const m = kinds.get(kind);
        const x = m?.get(a);
        const y = m?.get(b);
        if (x !== undefined && y !== undefined) return x - y;
        // A dash (no value) sorts before every known one.
        if (x !== undefined) return 1;
        if (y !== undefined) return -1;
        return fallback(a, b, m ? "text" : kind, locale);
      };
    },
  };
}
