/** A countdown. */
/** A countdown: the length, or only the moment it counts down to (the documented `-until` markup). */
export declare const COUNTDOWN =
  "[data-kp-countdown], [data-kp-countdown-until]";
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
export type UntilZone = string | number | null;
/**
 * The IANA zone a timer is given in: `attribute` (`data-kp-countdown-timezone`, `data-kp-timer-disc-timezone`) on it or an
 * ancestor; null (the device) when none or unknown.
 * @param {Element} el @param {string} attribute
 */
export declare function timerZone(
  el: Element,
  attribute: string,
): string | null;
/** The fixed offset, in minutes, a date and time string carries ("Z", "+02:00"); null when it carries none. @param {string} text */
export declare function untilOffset(text: string): number | null;
/**
 * The zone an `until` value is given in: the offset it carries, else the timer's zone, else the device.
 * @param {string | number | null | undefined} value @param {string | null} [timeZone]
 * @returns {UntilZone}
 */
export declare function untilZone(
  value: string | number | null | undefined,
  timeZone?: string | null,
): UntilZone;
/**
 * The moment an `until` option names, in epoch ms: "14:30" or "14:30:15" is the next such time on the clock of `timeZone`
 * (the device's when none; today when it is still ahead, else tomorrow), a date and time is that moment (read in `timeZone`
 * when it carries no offset of its own), a number of 10 digits or more is epoch ms.
 * @param {string | number | null | undefined} value
 * @param {number} nowMs
 * @param {string | null} [timeZone] an IANA zone name
 * @returns {number | null} null when the value names no moment
 */
export declare function parseUntil(
  value: string | number | null | undefined,
  nowMs: number,
  timeZone?: string | null,
): number | null;
/**
 * A moment as the words after "until": "14:30" when it is today, else "20/10/2026 09:00", read on the clock of `zone`
 * (the zone the moment was given in; the device's when null).
 * @param {number} ms @param {number} nowMs @param {UntilZone} [zone]
 */
export declare function untilLabel(
  ms: number,
  nowMs: number,
  zone?: UntilZone,
): string;
/**
 * The digits of a time left, rounded up to the second: "25:00", "0:08", "1:02:03".
 * @param {number} ms the time left
 * @param {boolean} hours whether the timer shows hours (it holds an hour or more)
 * @param {boolean} [days] whether 24 hours or more is written with a days unit ("6 d 22 h"); off, it reads "167:59:59"
 */
export declare function formatCountdown(
  ms: number,
  hours: boolean,
  days?: boolean,
): string;
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
 * @param {{ now?: () => number, wall?: () => number }} [options] `now` is the clock's (performance.now), `wall` the device's
 *   date and time (Date.now), which an `until` option is counted against
 * @returns {() => void} detach
 */
export declare function attachCountdowns(
  root?: ParentNode,
  {
    now,
    wall,
  }?: {
    now?: () => number;
    wall?: () => number;
  },
): () => void;
