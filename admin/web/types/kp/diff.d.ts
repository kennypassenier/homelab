/** The diffs this module looks after. */
export declare const DIFF = "[data-kp-diffview]";
/** Fired when a file is opened or closed: `{ path, open }`. */
export declare const DIFF_TOGGLE_EVENT = "kp-diff-toggle";
/** Fired when a fold of unchanged lines opens or closes: `{ path, lines, open }`. */
export declare const DIFF_EXPAND_EVENT = "kp-diff-expand";
/** Fired when a path was copied: `{ path }`. */
export declare const DIFF_COPY_EVENT = "kp-diff-copy";
/** Fired by the Retry of a failed diff. */
export declare const DIFF_RETRY_EVENT = "kp-diff-retry";
/** Unchanged lines kept on each side of a change. */
export declare const DIFF_CONTEXT = 3;
/** The least number of unchanged lines worth a fold: hiding one or two costs more than showing them. */
export declare const DIFF_FOLD_MIN = 3;
/** Lines drawn per file before the rest waits behind "Show the other N lines". */
export declare const DIFF_MAX_LINES = 400;
/** A line longer than this is not split into words: the split is for a line, not a minified file. */
export declare const DIFF_WORD_LIMIT = 500;
/** More than this share of a line changed and the whole line is the change: marking every word says nothing. */
export declare const DIFF_WORD_SHARE = 0.6;
/** The statuses of a file. */
export declare const DIFF_STATUSES: readonly string[];
export type DiffLine = {
  kind: "same" | "added" | "removed";
  text: string;
  /**
   * the number shown (the new file's for a kept or added line, the old file's for a removed one)
   */
  no?: number | null;
  /**
   * used when `no` is absent
   */
  oldNo?: number | null;
  /**
   * used when `no` is absent
   */
  newNo?: number | null;
};
export type DiffHunk = {
  /**
   * the whole header, `@@ -12,6 +12,8 @@ function name`; else it is made from the four numbers
   */
  head?: string;
  oldStart?: number;
  oldLen?: number;
  newStart?: number;
  newLen?: number;
  /**
   * the place after the second `@@`
   */
  context?: string;
  lines: DiffLine[];
};
export type DiffFile = {
  path: string;
  /**
   * the name before a rename
   */
  oldPath?: string;
  /**
   * `modified`, or `renamed` when `oldPath` differs
   */
  status?: "added" | "removed" | "modified" | "renamed" | "binary";
  /**
   * the count; counted from the lines when absent
   */
  added?: number;
  removed?: number;
  hunks?: DiffHunk[];
};
export type DiffSpec = {
  files?: DiffFile[];
  /**
   * absent: ready, or empty without files
   */
  state?: "ready" | "loading" | "empty" | "error";
  /**
   * the failed plate's heading
   */
  message?: string;
  /**
   * the names compared, for the empty plate's sentence
   */
  compared?: string[];
};
export type Segment = {
  text: string;
  changed: boolean;
};
export type NormFile = {
  path: string;
  oldPath: string;
  status: string;
  added: number;
  removed: number;
  hunks: {
    range: string;
    context: string;
    lines: DiffLine[];
  }[];
};
export type DiffState = {
  spec: DiffSpec;
  files: NormFile[];
  opened: Map<string, boolean>;
  folds: Set<string>;
  all: Set<string>;
  card: import("./busy-card.js").BusyCard | null;
  off: (() => void)[];
  mode: string;
  said: string;
};
/**
 * The words that differ between two lines: the longest run of pieces they share is kept, the rest is marked.
 * `changed` is false when there is nothing to mark, or when so much of the line differs that the line itself is the
 * change (more than `DIFF_WORD_SHARE` of the longer one), or when a line is too long to split.
 *
 * @param {string} oldText @param {string} newText
 * @returns {{ old: Segment[], new: Segment[], changed: boolean, share: number }}
 */
export declare function diffWords(
  oldText: string,
  newText: string,
): {
  old: Segment[];
  new: Segment[];
  changed: boolean;
  share: number;
};
export type Part = {
  kind: "lines" | "fold";
  /**
   * the index of its first line in the hunk
   */
  from: number;
  lines: DiffLine[];
};
/**
 * @typedef {object} Part
 * @property {'lines' | 'fold'} kind
 * @property {number} from the index of its first line in the hunk
 * @property {DiffLine[]} lines
 */
/**
 * Cut a hunk's lines into stretches that show and stretches that fold. A run of unchanged lines keeps `keep` lines next to
 * every change and folds the middle when that hides at least `min` lines; a run at the start or the end of the hunk keeps
 * its lines next to the change only. A hunk with no change, and `keep` of 0, fold nothing.
 *
 * @param {readonly DiffLine[]} lines @param {number} [keep] @param {number} [min]
 * @returns {Part[]}
 */
export declare function foldContext(
  lines: readonly DiffLine[],
  keep?: number,
  min?: number,
): Part[];
/**
 * Give a diff its files or its state.
 * @param {HTMLElement} root @param {DiffSpec | DiffFile[]} spec
 */
export declare function setDiff(
  root: HTMLElement,
  spec: DiffSpec | DiffFile[],
): void;
/**
 * Wire every diff under `root`.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachDiffs(root?: ParentNode): () => void;
