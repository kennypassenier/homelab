/** A share bar the page fills with setStackbar(). */
export declare const STACKBAR = "[data-kp-stackbar]";
/** Fired on the root when a part or its legend row is pressed: `{ key, part }`. */
export declare const STACKBAR_PICK_EVENT = "kp-stackbar-pick";
/** Fired on the root when the Retry of a failed share bar is pressed. */
export declare const STACKBAR_RETRY_EVENT = "kp-stackbar-retry";
export type StackbarPart = {
  /**
   * stable id (default the label)
   */
  key?: string;
  label: string;
  /**
   * in the unit of `total`
   */
  value: number;
  /**
   * the hover card's text, in place of label, value and share
   */
  tip?: string;
  /**
   * a part that needs attention takes the status colour
   */
  tone?: "warning" | "destructive";
};
export type StackbarSpec = {
  parts?: StackbarPart[];
  /**
   * the whole; without it, the sum of the parts
   */
  total?: number;
  /**
   * a line over the bar
   */
  title?: string;
  /**
   * the words at the head's end (default: the sum of the format over the whole)
   */
  summary?: string;
  /**
   * how a value reads (default: the number in the page's language)
   */
  format?: (value: number) => string;
  /**
   * the free row's label (default the dictionary's `stackbarRest`)
   */
  restLabel?: string;
  /**
   * parts under this share are merged into "Other" (default 0, none)
   */
  other?: number;
  /**
   * ticks across the bar at a share of the whole
   */
  marks?: {
    at: number;
    label?: string;
  }[];
  /**
   * default true
   */
  legend?: boolean;
  state?: "ready" | "loading" | "empty" | "error";
  /**
   * the sentence of an empty or failed bar
   */
  message?: string;
  /**
   * false leaves Retry out of a failed bar (a read the page cannot repeat)
   */
  retry?: boolean;
  /**
   * the bar's accessible name (default: every part in order)
   */
  label?: string;
};
/** A share as words: one decimal under ten per cent, whole above. @param {number} share @param {string | undefined} lang */
export declare function shareText(
  share: number,
  lang: string | undefined,
): string;
/**
 * The arithmetic of a share bar, with no DOM: the parts as drawn (merged
 * into Other where asked), each with its share of the bar, the free rest,
 * and whether the parts run past the whole.
 * @param {StackbarPart[]} parts @param {number | undefined} total @param {number} [other]
 */
export declare function stackbarParts(
  parts: StackbarPart[],
  total: number | undefined,
  other?: number,
): {
  parts: {
    key: string;
    label: string;
    value: number;
    tip?: string;
    tone?: "warning" | "destructive";
    merged?: number;
    share: number;
    width: number;
  }[];
  whole: number;
  sum: number;
  rest: number;
  over: number;
};
/**
 * Draw (or redraw) a share bar. Calling it again with new numbers keeps the
 * focus on the part it was on, by key.
 * @param {HTMLElement} el the root, a `.kp-stackbar`
 * @param {StackbarSpec} spec
 */
export declare function setStackbar(el: HTMLElement, spec: StackbarSpec): void;
/**
 * Fill every `[data-kp-stackbar]` under `root` that carries its spec as JSON
 * in `data-kp-stackbar` (a page that draws with setStackbar() needs nothing).
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachStackbars(root?: ParentNode): () => void;
