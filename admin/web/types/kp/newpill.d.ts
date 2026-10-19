/** The scrollers this module looks after. */
export declare const FOLLOW = "[data-kp-follow]";
/** Fired on the scroller when following starts or stops, or the count changes: `{ following, paused, unseen }`. */
export declare const FOLLOW_EVENT = "kp-follow-change";
/** Fired on the scroller when the pill is pressed: `{ unseen }`. */
export declare const NEWPILL_JUMP_EVENT = "kp-newpill-jump";
/** px from the end that still count as following. */
export declare const FOLLOW_THRESHOLD = 24;
/** How often a screen reader hears the count [ms]. */
export declare const NEWPILL_SAY_MS = 10000;
export type NewPillOptions = {
  follow?: "tail" | "head";
  threshold?: number;
  placement?: "bottom" | "top";
  /**
   * "line,lines"
   */
  unit?: string;
  sinceLeft?: boolean;
  key?: string;
  space?: boolean;
};
export type NewPillHandle = {
  /**
   * stop following; new rows are counted
   */
  pause: () => void;
  /**
   * jump to the end and follow
   */
  resume: () => void;
  /**
   * same as resume
   */
  jump: () => void;
  /**
   * how many shown rows arrived out of sight
   */
  unseen: () => number;
  following: () => boolean;
};
/**
 * The handle of a scroller.
 * @param {Element} el
 * @returns {NewPillHandle | null}
 */
export declare function newPill(el: Element): NewPillHandle | null;
/**
 * Give every live list under `root` its following and its pill.
 * @param {ParentNode} [root]
 * @param {NewPillOptions} [options]
 * @returns {() => void} detach
 */
export declare function attachNewPills(
  root?: ParentNode,
  options?: NewPillOptions,
): () => void;
