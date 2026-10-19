/** A quote card. */
export declare const QUOTE = "[data-kp-quote]";
/** A list of quote cards. */
export declare const QUOTE_LIST = "[data-kp-quote-list]";
/** Fired on the card when a hide is committed (the Undo window passed): `{ text, wait }`. */
export declare const QUOTE_HIDE_EVENT = "kp-quote-hide";
/** Fired on the card when Retry is pressed on a failed card. */
export declare const QUOTE_RETRY_EVENT = "kp-quote-retry";
/** Fired on the card when a long quote is opened or folded: `{ expanded }`. */
export declare const QUOTE_FOLD_EVENT = "kp-quote-fold";
/** The states a quote card can be in. */
export declare const QUOTE_STATES: readonly [
  "ready",
  "loading",
  "empty",
  "error",
];
/** How many lines a quote shows before it folds. */
export declare const QUOTE_LINES = 4;
/** How many quotes a list shows before "Show all". */
export declare const QUOTE_LIST_MAX = 3;
export type QuoteModel = {
  el: HTMLElement;
  text: string;
  expanded: boolean;
  hiding: boolean;
  textEl: HTMLElement | null;
  more: HTMLButtonElement | null;
  live: HTMLElement;
  unkey: () => void;
  resize: ResizeObserver | null;
  copyTimer: number;
};
/**
 * The date line: "Tuesday 14/10/2026", "3 days ago" or both (rule 52). Pure.
 * @param {number} ms the moment
 * @param {'date' | 'relative' | 'both'} style
 * @param {number} [now]
 * @param {string} [locale]
 */
export declare function quoteWhen(
  ms: number,
  style?: "date" | "relative" | "both",
  now?: number,
  locale?: string,
): string;
export type QuoteOptions = {
  /**
   * how many lines before it folds (default 4)
   */
  lines?: number;
  /**
   * show Copy (default false)
   */
  copy?: boolean;
};
/**
 * Wire every quote card and quote list under `root`.
 * @param {ParentNode} [root]
 * @param {QuoteOptions} [options] defaults; each card's own `data-kp-quote-*` wins
 * @returns {() => void} detach
 */
export declare function attachQuotes(
  root?: ParentNode,
  options?: QuoteOptions,
): () => void;
export type QuoteSpec = {
  /**
   * the words, verbatim
   */
  text: string;
  by?: string;
  /**
   * ISO moment
   */
  date?: string;
  dateStyle?: "date" | "relative" | "both";
  why?: string;
  source?: string;
  lang?: string;
  lines?: number | "all";
  hide?: boolean;
  copy?: boolean;
  size?: "md" | "lg";
};
/**
 * @typedef {object} QuoteSpec
 * @property {string} text the words, verbatim
 * @property {string} [by] @property {string} [date] ISO moment @property {'date' | 'relative' | 'both'} [dateStyle]
 * @property {string} [why] @property {string} [source] @property {string} [lang]
 * @property {number | 'all'} [lines] @property {boolean} [hide] @property {boolean} [copy] @property {'md' | 'lg'} [size]
 */
/**
 * Write a quote's words and options from a script. The card is attached if it is not yet.
 * @param {HTMLElement} el a `figure.kp-quote`
 * @param {QuoteSpec} spec
 */
export declare function setQuote(el: HTMLElement, spec: QuoteSpec): void;
/**
 * The state of a quote card: `loading` (the card's busy card over a quote-shaped stage), `empty`, `error` (with Retry,
 * `kp-quote-retry`), `ready`.
 * @param {HTMLElement} el
 * @param {'ready' | 'loading' | 'empty' | 'error'} state
 * @param {{ title?: string, text?: string, words?: string }} [words] the words of the state, instead of the dictionary's
 */
export declare function setQuoteState(
  el: HTMLElement,
  state: "ready" | "loading" | "empty" | "error",
  words?: {
    title?: string;
    text?: string;
    words?: string;
  },
): void;
export type QuoteHandle = {
  element: HTMLElement;
  /**
   * the words as written
   */
  text: () => string;
  set: (spec: QuoteSpec) => void;
  setState: (state: "ready" | "loading" | "empty" | "error") => void;
  /**
   * hide it as a press on Hide does
   */
  hide: () => void;
};
/**
 * @typedef {object} QuoteHandle
 * @property {HTMLElement} element
 * @property {() => string} text the words as written
 * @property {(spec: QuoteSpec) => void} set
 * @property {(state: 'ready' | 'loading' | 'empty' | 'error') => void} setState
 * @property {() => void} hide hide it as a press on Hide does
 */
/**
 * The handle of an attached quote card.
 * @param {Element} element
 * @returns {QuoteHandle | null}
 */
export declare function quote(element: Element): QuoteHandle | null;
