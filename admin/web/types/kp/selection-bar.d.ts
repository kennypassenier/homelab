// The kp-themes module chassis-rs serves at /static/kp/js/selection-bar.js (kp-themes 11.0.0),
// copied from kp-themes js/selection-bar.d.ts: chassis serves the module, not its types.
/** The bars this module looks after. */
export declare const SELECTION = "[data-kp-selection]";
/** Fired on the bar when what is selected changed: `{ keys, items, count, matching }`. */
export declare const SELECTION_EVENT = "kp-selection-change";
/** Fired on the bar when the reader unticked all with its button or Esc: `{ keys }` (the ones that were selected). */
export declare const SELECTION_CLEAR_EVENT = "kp-selection-clear";
/** Fired on the bar when the offer for all matching rows is taken or given back: `{ matching, total }`. */
export declare const SELECTION_MATCHING_EVENT = "kp-selection-matching";
/** Fired on the bar, cancelable, when a button marked `data-kp-selection-action` is pressed: `{ action, button, keys, items, matching, undoMs }`. */
export declare const SELECTION_ACTION_EVENT = "kp-selection-action";
/** The names shown before "+N more". */
export declare const SELECTION_NAMES = 6;
/** How long the bar waits for a burst of ticks before it says the count [ms]. */
export declare const SELECTION_SAY_MS = 350;
export type SelectionItem = {
  key: string;
  label: string;
};
export type SelectionOptions = {
  placement?: "sticky-top" | "inline" | "floating-bottom";
  names?: "names" | "count";
  maxNames?: number;
  matching?: number;
};
export type Memory = {
  sources: HTMLElement[];
  items: SelectionItem[];
  /**
   * every key the sources had at the last read
   */
  known: Set<string>;
  /**
   * fed by setSelection()
   */
  manual: boolean;
  matchingTotal: number;
  matching: boolean;
  namesOpen: boolean;
  busy: string;
  rules: Record<string, (items: SelectionItem[]) => string | null>;
  sayTimer: number;
  refresh: () => void;
};
/**
 * Give every selection bar under `root` its parts and its source.
 * @param {ParentNode} [root]
 * @param {SelectionOptions} [options]
 * @returns {() => void} detach
 */
export declare function attachSelectionBars(
  root?: ParentNode,
  options?: SelectionOptions,
): () => void;
/**
 * Feed the bar by hand, for a list that has no boxes to read (a board, a server's selection). The bar then ignores the boxes.
 * @param {HTMLElement} bar
 * @param {readonly (string | SelectionItem)[]} items the keys, or keys with the names to show
 * @param {{ matching?: number }} [options] `matching`: how many rows match in all, for the offer beyond these
 */
export declare function setSelection(
  bar: HTMLElement,
  items: readonly (string | SelectionItem)[],
  options?: {
    matching?: number;
  },
): void;
/**
 * What the bar holds now.
 * @param {HTMLElement} bar
 * @returns {{ keys: string[], items: SelectionItem[], count: number, matching: boolean }}
 */
export declare function getSelection(bar: HTMLElement): {
  keys: string[];
  items: SelectionItem[];
  count: number;
  matching: boolean;
};
/**
 * Say an action is running ("Backing up 3…"), or that it is done (no words). The buttons stop answering, the count gives way to the words in its own room.
 * @param {HTMLElement} bar
 * @param {string | null} [words] `null` or an empty string ends it
 */
export declare function setSelectionBusy(
  bar: HTMLElement,
  words?: string | null,
): void;
/**
 * Say how an action ended when some rows refused it: "2 done, 1 refused: why", with a mark on each refused row.
 * @param {HTMLElement} bar
 * @param {{ done: number, refused: readonly { key: string, reason: string }[] }} result
 */
export declare function setSelectionResult(
  bar: HTMLElement,
  result: {
    done: number;
    refused: readonly {
      key: string;
      reason: string;
    }[];
  },
): void;
/**
 * Turn an action off, with the reason, for a selection it does not fit: the function answers the reason ("2 of 3 can be updated") or an empty string.
 * @param {HTMLElement} bar
 * @param {Record<string, (items: SelectionItem[]) => string | null>} rules by the action's id (`data-kp-selection-action`)
 */
export declare function setSelectionRules(
  bar: HTMLElement,
  rules: Record<string, (items: SelectionItem[]) => string | null>,
): void;
