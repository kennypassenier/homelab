// The kp-themes module chassis-rs serves at /static/kp/js/segmented.js (kp-themes 11.0.0),
// copied from kp-themes js/segmented.d.ts: chassis serves the module, not its types.
/** A segmented control or a chip group. */
export declare const SEGMENTED = "[data-kp-segmented]";
/** Fired on the group when the choice changes by a person; `{ value, values, item }`. */
export declare const SEGMENTED_EVENT = "kp-segmented-change";
/** One button of a group. */
export declare const SEGMENTED_ITEM = ".kp-segmented__item";
/** Set on the group while the plate under the chosen segment is placed. */
export declare const THUMB_ATTRIBUTE = "data-kp-segmented-thumb";
/** Set one frame after the plate was first placed, so only later moves glide. */
export declare const GLIDE_ATTRIBUTE = "data-kp-segmented-glide";
/** A three-way chip's state: `include` or `exclude`; absent is either way [kpadopt4]. */
export declare const TRI_ATTRIBUTE = "data-kp-segmented-tri";
/** The order a press walks a three-way chip through. */
export declare const TRI_STATES: readonly ["any", "include", "exclude"];
/** Set while the segments are wider than the group, which then scrolls. */
export declare const OVERFLOW_ATTRIBUTE = "data-kp-segmented-overflow";
export type SegmentedItem = {
  value: string;
  label: string;
  /**
   * Shown beside the label; null or absent shows none.
   */
  count?: number | null;
  /**
   * The item's `title`.
   */
  hint?: string;
  disabled?: boolean;
  /**
   * This is the "All" item (Clear all in a three-way group).
   */
  all?: boolean;
  /**
   * A three-way chip's state; a chip still there keeps its own when absent.
   */
  state?: "any" | "include" | "exclude";
};
export type TriValues = {
  /**
   * the values that must be in
   */
  include: string[];
  /**
   * the values that must not be in
   */
  exclude: string[];
};
/** Set on a scrolling track to name the sides that have more segments out of sight: `start`, `end`, or both. */
export declare const MORE_ATTRIBUTE = "data-kp-segmented-more";
/** @param {Element} group @returns {HTMLElement[]} */
export declare function itemsOf(group: Element): HTMLElement[];
/** @param {Element} item */
export declare function valueOf(item: Element): string;
/**
 * The values that are on. Single: the chosen one (empty when none is chosen).
 * Multi: every value on except the "All" item; empty means All.
 * @param {Element} group
 * @returns {string[]}
 */
export declare function getSegmentedValues(group: Element): string[];
/**
 * The values that must be in and the values that must not be in, in the
 * order of the chips (a three-way group) [kpadopt4].
 * @param {Element} group
 * @returns {TriValues}
 */
export declare function getSegmentedTri(group: Element): TriValues;
/**
 * Set a three-way group's chips without firing the event; a value in
 * neither list is let go.
 * @param {Element} group
 * @param {{ include?: Iterable<string>, exclude?: Iterable<string> }} values
 */
export declare function setSegmentedTri(
  group: Element,
  {
    include,
    exclude,
  }: {
    include?: Iterable<string>;
    exclude?: Iterable<string>;
  },
): void;
/**
 * Place the plate under the chosen segment (single, segmented look) and note
 * whether the segments overflow the group.
 * @param {Element} group
 */
export declare function layout(group: Element): void;
/**
 * Set the values on without firing the event: for a page that moves the
 * group because something else moved (the address changed).
 * @param {Element} group
 * @param {Iterable<string>} values  Empty is All (multi) or nothing chosen (single).
 */
export declare function setSegmentedValues(
  group: Element,
  values: Iterable<string>,
): void;
/**
 * Turn every value off (back to All) and fire the event if anything was on.
 * @param {Element} group
 */
export declare function resetSegmented(group: Element): void;
/**
 * Write the numbers beside the values. `null` empties a count; a value not
 * named keeps its number. The count element is made when the item has none.
 * @param {Element} group
 * @param {Record<string, number | null | undefined>} counts
 */
export declare function setSegmentedCounts(
  group: Element,
  counts: Record<string, number | null | undefined>,
): void;
/**
 * Replace the items of a group (the values can change under it). The values
 * still present stay on; the focus stays on its value.
 * @param {Element} group
 * @param {readonly SegmentedItem[]} items
 */
export declare function setSegmentedItems(
  group: Element,
  items: readonly SegmentedItem[],
): void;
/**
 * Wire every `[data-kp-segmented]` under `root`: its roles and states, its
 * roving tab stop, its clicks and its arrow keys, and the plate under the
 * chosen segment. A group rendered later is picked up on its first use, or
 * by calling this again on its subtree.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachSegmented(root?: ParentNode): () => void;
