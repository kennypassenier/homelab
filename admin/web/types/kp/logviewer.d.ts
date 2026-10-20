// The kp-themes module chassis-rs serves at /static/kp/js/logviewer.js (kp-themes 11.1.1),
// copied from kp-themes js/logviewer.d.ts: chassis serves the module, not its types.
/** The viewers this module looks after. */
export declare const LOGVIEWER = "[data-kp-logviewer]";
/** Fired when the buffer, the filter or the find changes: `{ total, shown, dropped, matches, current }`. */
export declare const LOGVIEWER_EVENT = "kp-logviewer-change";
/** Fired by the Retry of a viewer whose source failed. */
export declare const LOGVIEWER_RETRY_EVENT = "kp-logviewer-retry";
/** The default cap of the ring buffer. */
export declare const LOGVIEWER_MAX = 2000;
/** The levels, lowest first. */
export declare const LOG_LEVELS: readonly string[];
export type LogLine = {
  /**
   * what makes a line the same line when it arrives twice
   */
  id?: string | number;
  /**
   * when it was written
   */
  ts: number | string | Date;
  /**
   * the stack or unit that wrote it
   */
  source?: string;
  /**
   * debug, info, notice, warning, error or critical (or a common alias)
   */
  level: string;
  /**
   * the text
   */
  message: string;
};
export type LogViewerSpec = {
  /**
   * replaces the buffer
   */
  lines?: LogLine[];
  state?: "ready" | "loading" | "error";
  max?: number;
  wrap?: boolean;
  time?: "time" | "datetime" | "off";
  level?: "debug" | "info" | "warning" | "error";
  search?: string;
  searchOnly?: boolean;
};
export type Kept = {
  id: string;
  ts: number;
  source: string;
  level: string;
  message: string;
};
export type ViewerState = {
  lines: Kept[];
  nodes: Map<string, HTMLElement>;
  dropped: number;
  serial: number;
  sig: string;
  matches: HTMLElement[];
  at: number;
  card: import("./busy-card.js").BusyCard | null;
  off: (() => void)[];
  said: string;
  tools: string;
  highlighted: string;
};
/** The canonical level of a word. @param {string} word */
export declare const logLevel: (word: string) => string;
/**
 * Add lines to the buffer. A line whose `id` is already kept is skipped, so a poll that returns overlapping windows can be
 * handed over whole. The oldest lines past the cap are dropped and counted.
 *
 * @param {Element} el
 * @param {LogLine[]} lines
 * @returns {number} how many lines were new
 */
export declare function appendLogLines(el: Element, lines: LogLine[]): number;
/**
 * The lines the buffer holds now, oldest first.
 * @param {Element} el
 * @returns {LogLine[]}
 */
export declare function logViewerLines(el: Element): LogLine[];
/**
 * The text of the lines shown (or of every kept line), one per row, as Copy and Download hand it over.
 * @param {Element} el
 * @param {{ all?: boolean }} [options]
 */
export declare function logViewerText(
  el: Element,
  options?: {
    all?: boolean;
  },
): string;
/**
 * Empty the buffer. The dropped count starts again from zero.
 * @param {Element} el
 */
export declare function clearLogViewer(el: Element): void;
/**
 * Find a text in the log, as the search box does.
 * @param {Element} el
 * @param {string} text  empty clears the find
 * @returns {number} how many matches
 */
export declare function searchLogViewer(el: Element, text: string): number;
/**
 * Go to the next or the previous match; the log scrolls to it (and stops following the tail).
 * @param {Element} el
 * @param {1 | -1} [delta]
 * @returns {number} the match now current, counted from 1, or 0 when there is none
 */
export declare function stepLogMatch(el: Element, delta?: 1 | -1): number;
/**
 * Set the viewer's options and state, or replace its lines.
 * @param {Element} el
 * @param {LogViewerSpec} spec
 */
export declare function setLogViewer(el: Element, spec: LogViewerSpec): void;
/**
 * Give every `[data-kp-logviewer]` under `root` its bar, its buffer and its states.
 *
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachLogViewers(root?: ParentNode): () => void;
