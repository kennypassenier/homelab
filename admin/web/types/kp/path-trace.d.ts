/** The traces this module draws. */
export declare const TRACE = "[data-kp-trace]";
/** Fired on the trace when the deciding rule is pressed: `{ rule, hop }`. */
export declare const TRACE_RULE_EVENT = "kp-trace-rule";
/** Fired on the trace when Swap is pressed or S is typed. */
export declare const TRACE_SWAP_EVENT = "kp-trace-swap";
/** Fired on the trace when Retry is pressed. */
export declare const TRACE_RETRY_EVENT = "kp-trace-retry";
export type TraceHop = {
  /**
   * the thing the packet reaches (a device, a network, a rule, a target)
   */
  title: string;
  /**
   * one line under it
   */
  note?: string;
  verdict: "allowed" | "refused" | "unknown";
  /**
   * the rule that decided it
   */
  rule?: string;
  /**
   * "tcp 8096"
   */
  ports?: string;
};
/**
 * @typedef {object} TraceHop
 * @property {string} title the thing the packet reaches (a device, a network, a rule, a target)
 * @property {string} [note] one line under it
 * @property {'allowed' | 'refused' | 'unknown'} verdict
 * @property {string} [rule] the rule that decided it
 * @property {string} [ports] "tcp 8096"
 */
/**
 * The verdict of each hop once the first refusal stops the packet: `done`, `failed`, `skipped` or `pending`.
 * @param {readonly TraceHop[]} hops
 * @param {'first-refusal' | 'all'} [stop]
 * @returns {{ hop: TraceHop, state: 'done' | 'failed' | 'skipped' | 'pending' }[]}
 */
export declare function traceStates(
  hops: readonly TraceHop[],
  stop?: "first-refusal" | "all",
): {
  hop: TraceHop;
  state: "done" | "failed" | "skipped" | "pending";
}[];
/**
 * The outcome in one sentence.
 * @param {readonly TraceHop[]} hops
 * @param {'first-refusal' | 'all'} [stop]
 */
export declare function traceSummary(
  hops: readonly TraceHop[],
  stop?: "first-refusal" | "all",
): string;
/**
 * The trace as text for a bug report: "tv > Firewall refused (rule IoT to media) > Jellyfin not reached".
 * @param {readonly TraceHop[]} hops
 * @param {'first-refusal' | 'all'} [stop]
 */
export declare function traceText(
  hops: readonly TraceHop[],
  stop?: "first-refusal" | "all",
): string;
export type TraceHandle = {
  element: HTMLElement;
  /**
   * draw the hops (clears any state)
   */
  set: (hops: readonly TraceHop[]) => void;
  setState: (
    state: "ready" | "running" | "empty" | "error",
    options?: {
      message?: string;
    },
  ) => void;
  /**
   * the trace as text
   */
  text: () => string;
};
/**
 * The handle of a trace.
 * @param {Element} el
 * @returns {TraceHandle | null}
 */
export declare function trace(el: Element): TraceHandle | null;
/**
 * Give every trace under `root` its parts; fill one with `setTrace` or the handle.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachTraces(root?: ParentNode): () => void;
/**
 * Draw the hops of a trace (attaching it first when it was not yet).
 * @param {HTMLElement} el
 * @param {readonly TraceHop[]} hops
 */
export declare function setTrace(
  el: HTMLElement,
  hops: readonly TraceHop[],
): void;
