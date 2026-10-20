export declare const WHEEL = "[data-kp-wheel]";
export declare const WHEEL_SPIN_EVENT = "kp-wheel-spin";
export declare const WHEEL_RESULT_EVENT = "kp-wheel-result";
export type WheelItem = {
  id: string;
  label: string;
  /**
   * 1 by default; larger is likelier and a larger segment
   */
  weight?: number;
  /**
   * a short line about the choice, shown in the answer card and the segment's tooltip
   */
  tag?: string;
  /**
   * stays on the wheel, marked, and is never drawn while another choice is not resting
   */
  resting?: boolean;
  /**
   * a shorter name for the wheel's face, used when the whole label does not fit a thin segment
   */
  short?: string;
};
export type WheelState = {
  items: WheelItem[];
  /**
   * the weights drawn now (they glide to the items' weights)
   */
  shown: Map<string, number>;
  /**
   * the ids drawn, in order (a leaving one stays until it has shrunk)
   */
  order: string[];
  /**
   * the disc's turn, degrees
   */
  angle: number;
  spinning: boolean;
  /**
   * the id the wheel chose last
   */
  last: string | null;
  /**
   * the frame of a glide
   */
  glide: number;
  busy: import("./busy-card.js").BusyCard | null;
};
/**
 * Draw an outcome by the weights. `avoid` (an id) is left out while there is
 * another choice. Pure but for `random`.
 * @param {readonly WheelItem[]} items
 * @param {{ random?: () => number, avoid?: string | null }} [options]
 * @returns {number} the index drawn, -1 when there is none
 */
export declare function pickWeighted(
  items: readonly WheelItem[],
  {
    random,
    avoid,
  }?: {
    random?: () => number;
    avoid?: string | null;
  },
): number;
/**
 * Each segment's start and end, in degrees clockwise from the top, by weight.
 * @param {readonly number[]} weights
 * @returns {[number, number][]}
 */
export declare function wheelAngles(
  weights: readonly number[],
): [number, number][];
/**
 * Give the wheel its choices. A new set clears the outcome (a filter is a new
 * question); one choice is the outcome at once.
 * @param {Element} root @param {WheelItem[]} items
 */
export declare function setWheelItems(root: Element, items: WheelItem[]): void;
/**
 * The wheel's state: `loading` lays the busy card over it, `error` says the
 * message where the outcome goes, `ready` is neither.
 * @param {Element} root @param {'loading' | 'error' | 'ready'} kind @param {string} [message]
 */
export declare function setWheelState(
  root: Element,
  kind: "loading" | "error" | "ready",
  message?: string,
): void;
/** The outcome the wheel stands on, or null. @param {Element} root @returns {WheelItem | null} */
export declare function getWheelResult(root: Element): WheelItem | null;
/**
 * Spin: draw the outcome by the weights, turn the wheel to it, say it.
 * Resolves with the outcome (null when it could not spin).
 * @param {Element} root
 * @param {{ random?: () => number }} [options]
 * @returns {Promise<WheelItem | null>}
 */
export declare function spinWheel(
  root: Element,
  {
    random,
  }?: {
    random?: () => number;
  },
): Promise<WheelItem | null>;
/**
 * Wire every wheel under `scope`: its parts and its Spin button.
 * @param {ParentNode} [scope]
 * @returns {() => void} detach
 */
export declare function attachWheels(scope?: ParentNode): () => void;
