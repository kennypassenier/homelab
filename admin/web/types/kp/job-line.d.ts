/** The lines this module looks after. */
export declare const JOBLINE = "[data-kp-jobline]";
/** Fired on the line, bubbling, when its text changed: `{ text }`. */
export declare const JOBLINE_EVENT = "kp-jobline";
export type JobReport = {
  /**
   * the step now at work, 1-based
   */
  step?: number;
  /**
   * the steps in all
   */
  steps?: number;
  /**
   * bytes moved so far
   */
  done?: number;
  /**
   * bytes in all
   */
  total?: number;
  /**
   * the time left, in seconds
   */
  etaSeconds?: number;
  /**
   * bytes per second; the time left is worked out from it when `etaSeconds` is not given
   */
  rate?: number;
};
/**
 * The sentence for a report, parts left out when unknown.
 * @param {JobReport} report
 * @param {{ base?: 1000 | 1024 }} [options]
 * @returns {string}
 */
export declare function jobLineText(
  report: JobReport,
  {
    base,
  }?: {
    base?: 1000 | 1024;
  },
): string;
/** The step position of a list: the running (or current) step, else the first one still waiting, else the last; 0 while nothing has started. @param {Element} list @returns {{ step: number, steps: number }} */
export declare function stepPosition(list: Element): {
  step: number;
  steps: number;
};
/**
 * Report to a job line: only what is given changes; `null` for a key takes it back out.
 * @param {HTMLElement} line a `[data-kp-jobline]`
 * @param {JobReport} report
 */
export declare function setJobLine(line: HTMLElement, report: JobReport): void;
/**
 * Build a job line.
 * @param {{ label?: string, steplist?: string }} [options] `label`: its accessible name; `steplist`: a selector for the list it follows
 * @returns {HTMLParagraphElement}
 */
export declare function buildJobLine({
  label,
  steplist,
}?: {
  label?: string;
  steplist?: string;
}): HTMLParagraphElement;
/**
 * Prepare every job line under `root` and follow the step lists they name.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachJobLines(root?: ParentNode): () => void;
