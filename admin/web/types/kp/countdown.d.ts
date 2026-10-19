/** A countdown. */
export declare const COUNTDOWN = "[data-kp-countdown]";
/** Fired on the timer when it starts, pauses, resets or gains time: `{ state, remaining, total }` (ms). */
export declare const COUNTDOWN_EVENT = "kp-countdown-change";
/** Fired on the timer when its time is up: `{ total }` (ms). */
export declare const COUNTDOWN_END_EVENT = "kp-countdown-end";
/** How much "a minute more" adds, in seconds, when the timer does not say (`data-kp-countdown-step`). */
export declare const COUNTDOWN_STEP = 60;
/**
 * A length in seconds from "90", "25m", "1h30m", "45s", "25:00" or "1:30:00".
 * @param {string | number | null | undefined} value
 * @returns {number} seconds, 0 when unreadable
 */
export declare function parseDuration(
  value: string | number | null | undefined,
): number;
/**
 * The digits of a time left, rounded up to the second: "25:00", "0:08", "1:02:03".
 * @param {number} ms the time left
 * @param {boolean} hours whether the timer shows hours (it holds an hour or more)
 */
export declare function formatCountdown(ms: number, hours: boolean): string;
export type CountdownState = "idle" | "running" | "paused" | "ended";
export type CountdownClock = {
  state: () => CountdownState;
  /**
   * ms left
   */
  remaining: () => number;
  /**
   * ms the timer was set to, with every minute added
   */
  total: () => number;
  /**
   * from idle or paused; nothing when running or ended
   */
  start: () => void;
  pause: () => void;
  /**
   * back to idle and full, at `seconds` when given
   */
  reset: (seconds?: number) => void;
  /**
   * more time; an ended timer starts again with it
   */
  add: (seconds: number) => void;
  /**
   * stop every timer it holds (detach)
   */
  stop: () => void;
};
/**
 * @typedef {'idle' | 'running' | 'paused' | 'ended'} CountdownState
 * @typedef {object} CountdownClock
 * @property {() => CountdownState} state
 * @property {() => number} remaining  ms left
 * @property {() => number} total      ms the timer was set to, with every minute added
 * @property {() => void} start        from idle or paused; nothing when running or ended
 * @property {() => void} pause
 * @property {(seconds?: number) => void} reset  back to idle and full, at `seconds` when given
 * @property {(seconds: number) => void} add     more time; an ended timer starts again with it
 * @property {() => void} stop          stop every timer it holds (detach)
 */
/**
 * A countdown clock: the time left is the end moment minus now, so nothing drifts.
 * @param {{ seconds: number, now?: () => number, onEnd?: () => void, onChange?: (state: CountdownState) => void }} options
 * @returns {CountdownClock}
 */
export declare function countdownClock({
  seconds,
  now,
  onEnd,
  onChange,
}: {
  seconds: number;
  now?: () => number;
  onEnd?: () => void;
  onChange?: (state: CountdownState) => void;
}): CountdownClock;
/** The clock of an attached countdown, to drive it from a script. @param {Element} el */
export declare const countdownOf: (el: Element) => CountdownClock | null;
/**
 * Build one control of a timer: a package button whose words keep their width.
 * @param {Document} doc @param {string} className @param {string[]} words @param {string} [title]
 */
export declare function timerButton(
  doc: Document,
  className: string,
  words: string[],
  title?: string,
): {
  button: HTMLButtonElement;
  label: HTMLSpanElement;
};
/**
 * Wire every countdown under `root`.
 * @param {ParentNode} [root]
 * @param {{ now?: () => number }} [options]
 * @returns {() => void} detach
 */
export declare function attachCountdowns(
  root?: ParentNode,
  {
    now,
  }?: {
    now?: () => number;
  },
): () => void;
