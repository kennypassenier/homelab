// The kp-themes module chassis-rs serves at /static/kp/js/snooze.js (kp-themes 11.1.1),
// copied from kp-themes js/snooze.d.ts: chassis serves the module, not its types.
/** A snooze picker. */
export declare const SNOOZE = "[data-kp-snooze]";
/** Fired on the picker when a choice is made: `{ until, choice }`. */
export declare const SNOOZE_EVENT = "kp-snooze";
/** Fired on the picker when End snooze is pressed. */
export declare const SNOOZE_END_EVENT = "kp-snooze-end";
/** The choices a picker offers when it does not say. */
export declare const SNOOZE_CHOICES: string[];
export type SnoozeChoice = {
  /**
   * `1h`, `tonight`, `last`, `pick` …
   */
  key: string;
  /**
   * the choice in words
   */
  label: string;
  /**
   * the moment it resolves to (ms); null for `pick`
   */
  at: number | null;
  /**
   * the moment in words, "Sun 18/10/2026 09:00 · in 18 h 55 min"
   */
  hint: string;
  /**
   * the moment alone, "Sun 18/10/2026 09:00" (a chip has room for no more)
   */
  moment: string;
};
export type SnoozeRules = {
  /**
   * ms
   */
  now: number;
  choices?: string[];
  /**
   * HH:MM
   */
  morning?: string;
  /**
   * HH:MM
   */
  evening?: string;
  /**
   * a length: 7d
   */
  max?: string;
  zone?: string;
  /**
   * the last custom moment, ms
   */
  last?: number | null;
  locale?: string;
};
/**
 * @typedef {object} SnoozeChoice
 * @property {string} key `1h`, `tonight`, `last`, `pick` …
 * @property {string} label the choice in words
 * @property {number | null} at the moment it resolves to (ms); null for `pick`
 * @property {string} hint the moment in words, "Sun 18/10/2026 09:00 · in 18 h 55 min"
 * @property {string} moment the moment alone, "Sun 18/10/2026 09:00" (a chip has room for no more)
 *
 * @typedef {object} SnoozeRules
 * @property {number} now ms
 * @property {string[]} [choices]
 * @property {string} [morning] HH:MM
 * @property {string} [evening] HH:MM
 * @property {string} [max] a length: 7d
 * @property {string} [zone]
 * @property {number | null} [last] the last custom moment, ms
 * @property {string} [locale]
 */
/**
 * The wall-clock moment `days` after today at a time of day in a zone. Pure.
 * @param {number} now @param {number} days @param {{ hour: number, minute: number }} at @param {string} zone
 * @returns {number | null}
 */
export declare function dayAt(
  now: number,
  days: number,
  at: {
    hour: number;
    minute: number;
  },
  zone: string,
): number | null;
/**
 * What a picker offers at a moment: each choice with the exact time it resolves to. A choice that has passed or lies
 * past `max` is left out; `pick` carries no moment. Pure, so a test or a server can ask the same.
 * @param {SnoozeRules} rules
 * @returns {SnoozeChoice[]}
 */
export declare function snoozeChoices({
  now,
  choices,
  morning,
  evening,
  max,
  zone,
  last,
  locale,
}: SnoozeRules): SnoozeChoice[];
export type SnoozeHandle = {
  element: HTMLElement;
  /**
   * the moment snoozed until (ms), or null
   */
  until: () => number | null;
  /**
   * no event
   */
  setUntil: (until: number | string | Date | null) => void;
  setBusy: (busy: boolean) => void;
  /**
   * what is offered now
   */
  choices: () => SnoozeChoice[];
  /**
   * show the date and time panel
   */
  openPanel: () => void;
  /**
   * hide it, the focus left where it is
   */
  closePanel: () => void;
};
/** The handle of an attached picker. @param {Element} element */
export declare function snooze(element: Element): SnoozeHandle | null;
/**
 * Show a picker as snoozed until a moment, or not snoozed (null), without the event.
 * @param {Element} element @param {number | string | Date | null} until
 */
export declare function setSnoozedUntil(
  element: Element,
  until: number | string | Date | null,
): void;
/**
 * Show a picker at work (the page is saving the reminder): its button shows the spinner and says no to a press.
 * @param {Element} element @param {boolean} busy
 */
export declare function setSnoozeBusy(element: Element, busy: boolean): void;
/**
 * Open a picker's date and time panel, as the "Pick a date and time…" choice does, or close it.
 * @param {Element} element @param {boolean} [open]
 */
export declare function openSnoozePanel(element: Element, open?: boolean): void;
/**
 * Wire every snooze picker under `root`.
 * @param {ParentNode} [root]
 * @param {{ now?: () => number }} [options]
 * @returns {() => void} detach
 */
export declare function attachSnoozes(
  root?: ParentNode,
  {
    now: nowOption,
  }?: {
    now?: () => number;
  },
): () => void;
