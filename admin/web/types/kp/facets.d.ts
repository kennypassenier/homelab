/** The groups this module looks after. */
export declare const FACETS = "[data-kp-facets]";
/** Fired on the group, bubbling, when the choice changed: `{ group, values, source: 'toggle' | 'only' | 'all' | 'set', value }`. */
export declare const FACET_EVENT = "kp-facet-change";
export type FacetItem = {
  /**
   * what the row stands for
   */
  value: string;
  label: string;
  /**
   * shown at the end of the row; null leaves its place empty
   */
  count?: number | string | null;
  /**
   * a CSS colour for the small square before the name
   */
  swatch?: string;
  /**
   * a seed (a name) for an identity pattern drawn over the swatch [kpadopt3]; '' takes it away
   */
  pattern?: string;
  /**
   * a new row is on when absent; a row that is already there keeps its state when absent
   */
  checked?: boolean;
  /**
   * cannot be turned on or off, and has no "only"
   */
  disabled?: boolean;
};
/**
 * The cells of an identity pattern: FNV-1a over the seed, then a 5 by 5 grid mirrored about its middle column, a cell
 * filled where its bit is set. Pure and the same everywhere, so a name has one pattern on every page [kpadopt3].
 * @param {string} seed
 * @returns {[number, number][]} filled cells as [column, row], 0 to 4
 */
export declare function identityCells(seed: string): [number, number][];
/**
 * The values that are on.
 * @param {Element} group
 * @returns {string[]}
 */
export declare function getFacetValues(group: Element): string[];
/**
 * Turn exactly these values on (every other off); `null` turns all on. The
 * event is not fired: the page that calls this knows.
 *
 * @param {Element} group
 * @param {string[] | null} values
 */
export declare function setFacetValues(
  group: Element,
  values: string[] | null,
): void;
/**
 * Write the counts. `counts` maps a value to its count; a value not in it
 * keeps its count. `all` is the "Everything" row's (the sum of the numbers
 * when absent). The group stops being busy.
 *
 * @param {Element} group
 * @param {Record<string, number | string | null>} counts
 * @param {{ all?: number | string | null }} [options]
 */
export declare function setFacetCounts(
  group: Element,
  counts: Record<string, number | string | null>,
  {
    all,
  }?: {
    all?: number | string | null;
  },
): void;
/**
 * Make the rows these items, in this order, changing what is there in place.
 *
 * @param {Element} group
 * @param {FacetItem[]} items
 */
export declare function setFacetItems(group: Element, items: FacetItem[]): void;
/**
 * Wire every facet group under `root`: the checkboxes, the "only" buttons and
 * the "Everything" row. Rows written later by setFacetItems() are covered
 * because the listeners are on the group.
 *
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachFacets(root?: ParentNode): () => void;
