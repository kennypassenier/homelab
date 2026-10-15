// The kp-themes module chassis-rs serves at /static/kp/js/listbox.js (kp-themes 10.1.1),
// copied from kp-themes js/listbox.d.ts: chassis serves the module, not its types.
/** The attribute an option carries. A contract value: consumers write it [TH26]. */
export declare const OPTION_SELECTOR = "[data-kp-option]";
/** Dispatched on the list, bubbling, when the highlight moves: `{ index, option }`. */
export declare const HIGHLIGHT_EVENT = "kp-listbox-highlight";
/** Dispatched on the list, bubbling, when an option is chosen: `{ index, option }`. */
export declare const CHOOSE_EVENT = "kp-listbox-choose";
export type ListboxOptions = {
  /**
   * the text input keeping DOM focus
   */
  input: HTMLElement;
  /**
   * the element with role="listbox"
   */
  list: HTMLElement;
  /**
   * Enter or click on an option
   */
  onChoose?: (index: number, option: HTMLElement) => void;
  /**
   * Escape, or focus leaving
   */
  onDismiss?: () => void;
  onHighlight?: (index: number, option: HTMLElement | null) => void;
  /**
   * whether Down on the last option returns to the first. Default true.
   */
  loop?: boolean;
  /**
   * Default OPTION_SELECTOR.
   */
  optionSelector?: string;
  /**
   * Default `[data-kp-disabled], [aria-disabled="true"]`.
   */
  disabledSelector?: string;
  /**
   * the mouse moves the highlight. Default true.
   */
  hoverHighlights?: boolean;
  /**
   * Default `{ block: 'nearest' }`; false for none.
   */
  scrollIntoView?: boolean | ScrollIntoViewOptions;
  /**
   * Default `is-active`.
   */
  activeClass?: string;
  /**
   * For generated option ids. Default: the list's id, else `kp-listbox`.
   */
  idPrefix?: string;
  /**
   * letters jump to the next option starting with them. Default false.
   */
  typeahead?: boolean;
  /**
   * Default 500.
   */
  typeaheadMs?: number;
  /**
   * Default true.
   */
  dismissOnEscape?: boolean;
  /**
   * dispatch HIGHLIGHT_EVENT and CHOOSE_EVENT on the list. Default true.
   */
  events?: boolean;
};
/**
 * @typedef {object} ListboxOptions
 * @property {HTMLElement} input the text input keeping DOM focus
 * @property {HTMLElement} list the element with role="listbox"
 * @property {(index: number, option: HTMLElement) => void} [onChoose] Enter or click on an option
 * @property {() => void} [onDismiss] Escape, or focus leaving
 * @property {(index: number, option: HTMLElement | null) => void} [onHighlight]
 * @property {boolean} [loop] whether Down on the last option returns to the first. Default true.
 * @property {string} [optionSelector] Default OPTION_SELECTOR.
 * @property {string} [disabledSelector] Default `[data-kp-disabled], [aria-disabled="true"]`.
 * @property {boolean} [hoverHighlights] the mouse moves the highlight. Default true.
 * @property {boolean | ScrollIntoViewOptions} [scrollIntoView] Default `{ block: 'nearest' }`; false for none.
 * @property {string} [activeClass] Default `is-active`.
 * @property {string} [idPrefix] For generated option ids. Default: the list's id, else `kp-listbox`.
 * @property {boolean} [typeahead] letters jump to the next option starting with them. Default false.
 * @property {number} [typeaheadMs] Default 500.
 * @property {boolean} [dismissOnEscape] Default true.
 * @property {boolean} [events] dispatch HIGHLIGHT_EVENT and CHOOSE_EVENT on the list. Default true.
 */
/**
 * Wire virtual focus between an input and a list of options.
 *
 * @param {ListboxOptions} options
 */
export declare function createListbox({
  input,
  list,
  onChoose,
  onDismiss,
  onHighlight,
  loop,
  optionSelector,
  disabledSelector,
  hoverHighlights,
  scrollIntoView,
  activeClass,
  idPrefix,
  typeahead,
  typeaheadMs,
  dismissOnEscape,
  events,
}: ListboxOptions): {
  /** Re-read the options and put the highlight back at the top. */
  refresh(): void;
  /** @param {number} at */
  highlight(at: number): void;
  clear: () => void;
  /** Choose the highlighted option, as Enter would. */
  choose: () => boolean;
  /** @returns {number} the highlighted index, or -1 */
  readonly index: number;
  /** @returns {HTMLElement[]} */
  readonly options: HTMLElement[];
  destroy(): void;
};
/**
 * Does `text` match `query` as a subsequence, the way a command palette
 * matches? "thm" finds "Theme".
 *
 * Deliberately not a fuzzy-score library: this returns whether it matches
 * and the caller keeps its own order. A ranking function that nobody can
 * explain is how a palette starts putting the wrong thing first.
 *
 * @param {string} text
 * @param {string} query
 * @returns {boolean}
 */
export declare function subsequence(text: string, query: string): boolean;
