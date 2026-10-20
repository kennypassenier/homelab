// The kp-themes module chassis-rs serves at /static/kp/js/code.js (kp-themes 11.1.1),
// copied from kp-themes js/code.d.ts: chassis serves the module, not its types.
/** A code block js/auto.js fills from its markup. */
export declare const CODE = "[data-kp-code]";
/** How long "Copied" stays, in ms. */
export declare const CODE_COPIED_MS = 1500;
export type CodeSpec = {
  text?: string;
  title?: string;
  /**
   * shown in the head; 'JSON' (any case) offers Pretty and Raw when the text parses
   */
  language?: string;
  /**
   * shown in the head, and in the binary sentence
   */
  size?: string;
  /**
   * line numbers
   */
  lines?: boolean;
  /**
   * wrapped from the start (the reader can toggle it)
   */
  wrap?: boolean;
  /**
   * fold past this many lines (0: never)
   */
  maxLines?: number;
  /**
   * the JSON layout to start with (default 'pretty')
   */
  layout?: "pretty" | "raw";
  /**
   * the Copy button (default true)
   */
  copy?: boolean;
  truncated?: {
    shown: string;
    total: string;
  };
  state?: "ready" | "loading" | "empty" | "binary" | "error";
  /**
   * the sentence of a failed or empty block
   */
  message?: string;
  loadingWords?: string;
  /**
   * a table cell: wrapped, three lines, a muted line for empty and binary [kpadopt5]
   */
  cell?: boolean;
  /**
   * where the rest of a folded cell opens (default inline) [kpadopt5]
   */
  open?: "inline" | "dialog";
  /**
   * a link under a binary cell's line [kpadopt5]
   */
  downloadUrl?: string;
};
export type CodeMemory = {
  spec: CodeSpec;
  wrap: boolean;
  layout: "pretty" | "raw";
  folded: boolean;
  /**
   * the text as shown, which Copy copies
   */
  shown: string;
  lines: number;
  card: import("./busy-card.js").BusyCard | null;
  drawn: boolean;
  pending: CodeSpec | null;
  head: HTMLElement | null;
  revert: number;
  cell: boolean;
};
/**
 * The JSON of `text` laid out with two-space indents, or null when it does
 * not parse.
 * @param {string} text
 */
export declare function prettyJson(text: string): string | null;
/**
 * Draw (or redraw) a code block. The reader's Wrap, layout and fold are kept
 * across calls unless the spec names them again.
 * @param {HTMLElement} el the root, a `.kp-code`
 * @param {CodeSpec} given
 */
export declare function setCode(el: HTMLElement, given: CodeSpec): void;
/**
 * Fill every `[data-kp-code]` under `root` from its markup: the text of its
 * `code` (or `pre`), and `data-kp-code-title`, `-language`, `-size`,
 * `-lines`, `-wrap`, `-max`, `-layout`, `-state`.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachCodes(root?: ParentNode): () => void;
