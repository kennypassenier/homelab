import { parseDuration } from "./countdown.js";
/** The lists this module looks after. */
export declare const TIMERS = "[data-kp-timers]";
/** A button anywhere in the page that starts a timer. */
export declare const TIMER_START = "[data-kp-timer-start]";
export declare const TIMER_START_EVENT = "kp-timer-start";
export declare const TIMER_CHANGE_EVENT = "kp-timer-change";
export declare const TIMER_DONE_EVENT = "kp-timer-done";
export declare const TIMER_STOP_EVENT = "kp-timer-stop";
/** How often the shown time is brought up to date [ms]; the time itself comes from the end, not from this. */
export declare const TIMER_TICK_MS = 250;
export type TimerState = {
  id: string;
  name: string;
  /**
   * the length it was started with (plus what was added)
   */
  seconds: number;
  /**
   * seconds left, now
   */
  remaining: number;
  /**
   * epoch ms it ends at while running, else null
   */
  endsAt: number | null;
  state: "running" | "paused" | "done";
  /**
   * the sentence spoken when it ends, if the list speaks
   */
  say?: string;
};
export type Timer = {
  id: string;
  name: string;
  /**
   * ms
   */
  total: number;
  /**
   * epoch ms, while running
   */
  endsAt: number | null;
  /**
   * ms, while paused or done
   */
  left: number;
  state: "running" | "paused" | "done";
  /**
   * the last minute was said
   */
  warned: boolean;
  /**
   * the sentence spoken when it ends ('' = the dictionary's)
   */
  say: string;
  ring: ReturnType<typeof setInterval> | null;
  card: HTMLElement;
};
export { parseDuration };
/** @param {Element} list @returns {TimerState[]} */
export declare function getTimers(list: Element): TimerState[];
/** The sounds a list may choose. @type {readonly ['chime', 'bell', 'buzzer']} */
export declare const TIMER_SOUNDS: readonly ["chime", "bell", "buzzer"];
/**
 * Play one of the sounds, once. Call it from a press (a preview) or let a timer's end call it.
 * @param {string} [sound]  'chime' (default), 'bell' or 'buzzer'; anything else is the chime, 'off' is silence
 * @returns {number} how long it lasts in seconds (0 where nothing played)
 */
export declare function playTimerSound(sound?: string): number;
/**
 * Say a sentence aloud with the browser's voice, in the page's language. Where there is no speech nothing happens.
 * @param {string} text
 * @returns {boolean} whether it was handed to the voice
 */
export declare function speakTimerMessage(text: string): boolean;
/**
 * Ask the browser for permission to show a system notification when a timer
 * ends. Call it from a press. Resolves with the answer.
 * @returns {Promise<NotificationPermission | 'unsupported'>}
 */
export declare function askTimerNotifications(): Promise<
  NotificationPermission | "unsupported"
>;
/**
 * Start a timer. With an `id` that already runs, nothing new starts and that
 * id is returned.
 * @param {Element} list
 * @param {{ name: string, seconds: number, id?: string, say?: string }} options  `say`: what is spoken when it ends
 * @returns {string | null} the timer's id
 */
export declare function startTimer(
  list: Element,
  {
    name,
    seconds,
    id,
    say: message,
  }: {
    name: string;
    seconds: number;
    id?: string;
    say?: string;
  },
): string | null;
/** @param {Element} list @param {string} id */
export declare function pauseTimer(list: Element, id: string): void;
/** @param {Element} list @param {string} id */
export declare function resumeTimer(list: Element, id: string): void;
/**
 * Add time to a timer; a done one starts again for that long.
 * @param {Element} list @param {string} id @param {number} [seconds]
 */
export declare function addTimerTime(
  list: Element,
  id: string,
  seconds?: number,
): void;
/** Stop a done timer's chime and let it rest on its done plate. @param {Element} list @param {string} id */
export declare function stopTimerRing(list: Element, id: string): void;
/**
 * Remove a timer (Stop): its card leaves the theme's way.
 * @param {Element} list @param {string} id
 */
export declare function stopTimer(list: Element, id: string): void;
/**
 * Put saved timers back (after a reload): a running one counts on from its
 * end time, one whose end has passed is shown done (and rings only when it
 * ended within the last minute).
 * @param {Element} list @param {TimerState[]} saved
 */
export declare function restoreTimers(list: Element, saved: TimerState[]): void;
/**
 * Wire every timer list under `root` (its buttons) and every starter in the
 * page. Counting goes on from the end time when the page comes back.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachTimers(root?: ParentNode): () => void;
