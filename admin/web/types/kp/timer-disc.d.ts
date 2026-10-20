// The kp-themes module chassis-rs serves at /static/kp/js/timer-disc.js (kp-themes 11.1.1),
// copied from kp-themes js/timer-disc.d.ts: chassis serves the module, not its types.
/** A visual timer. */
export declare const TIMER_DISC =
  "[data-kp-timer-disc], [data-kp-timer-disc-until]";
/** The custom property the disc is drawn by: 1 full, 0 gone. */
export declare const TIMER_DISC_LEFT = "--kp-timer-disc-left";
/**
 * The share of the disc left: the time left over the face's scale.
 * @param {number} remainingMs @param {number} scaleMs the face's whole turn
 */
export declare function discShare(remainingMs: number, scaleMs: number): number;
/** The clock of an attached visual timer, to drive it from a script. @param {Element} el */
export declare const timerDiscOf: (
  el: Element,
) => import("./countdown.js").CountdownClock | null;
/**
 * Wire every visual timer under `root`.
 * @param {ParentNode} [root]
 * @param {{ now?: () => number, wall?: () => number }} [options] `now` is the clock's (performance.now), `wall` the device's
 *   date and time (Date.now), which an `until` option is counted against
 * @returns {() => void} detach
 */
export declare function attachTimerDiscs(
  root?: ParentNode,
  {
    now,
    wall,
  }?: {
    now?: () => number;
    wall?: () => number;
  },
): () => void;
