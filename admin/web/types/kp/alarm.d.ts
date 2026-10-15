// The kp-themes module chassis-rs serves at /static/kp/js/alarm.js (kp-themes 10.1.1),
// copied from kp-themes js/alarm.d.ts: chassis serves the module, not its types.
/** Dispatched when an alarm closes, bubbling, on its trigger (or the document): `{ reason, id }`. */
export declare const ALARM_CLOSE_EVENT = "kp-alarm-close";
/** Dispatched on the alarm, bubbling, once it is open: `{ id, mode }`. */
export declare const ALARM_OPEN_EVENT = "kp-alarm-open";
/** How long an auto alarm stays when the caller names no `seconds`. An operational knob. */
export declare const ALARM_SECONDS = 8;
export type AlarmOptions = {
  /**
   * The huge word(s). Also the alarm's accessible name.
   */
  title: string;
  /**
   * The line under the headline: what happened and what follows.
   */
  detail?: string;
  /**
   * The small line above the headline ("Security protocol 7 · lockout").
   */
  code?: string;
  /**
   * 'ack' (default): only its button closes it. 'auto': closes after `seconds`.
   */
  mode?: "ack" | "auto";
  /**
   * Auto only. Default 8, at least 1.
   */
  seconds?: number;
  /**
   * Ack only: Escape closes it as well. An auto alarm always accepts Escape.
   */
  escape?: boolean;
  /**
   * The button's label. Default: the dictionary's `alarmAction`.
   */
  action?: string;
  /**
   * Per-alarm words, over the dictionary.
   */
  strings?: Partial<import("./strings.js").Strings>;
  /**
   * Where focus returns and the close event is dispatched. Default: what had focus.
   */
  trigger?: HTMLElement | null;
};
export type AlarmReason = "ack" | "timeout" | "escape";
/** Two noise glyphs for one letter cell, from the decipher set js/effects.js uses. */
export declare const noiseGlyph: () => string;
/**
 * The headline as letter cells for the stylesheet's decode. Each cell carries
 * its place (`--i`) and two noise glyphs drawn in its pseudo-elements, so the
 * letter itself never moves and the layout never changes; a cell changes
 * three times, ever. Words stay whole (`.kp-alarm__word`), so a line breaks
 * between words and never inside one. Spaces count as a place, so the decode
 * keeps its rhythm across them.
 *
 * @param {string} text
 * @returns {{ word: string, chars: { ch: string, i: number }[] }[]} the words, and each run of spaces as `word: ' '` with no cells
 */
export declare function glyphCells(text: string): {
  word: string;
  chars: {
    ch: string;
    i: number;
  }[];
}[];
/**
 * Build an alarm's markup, closed and not yet in the document. Both
 * channels write this tree (components/alarm.jsx renders the same), so a
 * consumer's register rule and a test reach the same elements in each.
 *
 * @param {AlarmOptions & { id?: string }} options
 * @returns {{ dialog: HTMLDialogElement, ack: HTMLButtonElement, keep: HTMLButtonElement, when: HTMLElement, left: HTMLElement, strings: import('./strings.js').Strings, action: string, seconds: number }}
 */
export declare function buildAlarm({
  title,
  detail,
  code,
  mode,
  seconds,
  escape,
  action,
  strings,
  id,
}?: AlarmOptions & {
  id?: string;
}): {
  dialog: HTMLDialogElement;
  ack: HTMLButtonElement;
  keep: HTMLButtonElement;
  when: HTMLElement;
  left: HTMLElement;
  strings: import("./strings.js").Strings;
  action: string;
  seconds: number;
};
/**
 * Raise a full-screen alarm and wait for it to close.
 *
 *   const reason = await showAlarm({ title: 'Access denied', detail: '…', mode: 'ack' });
 *
 * @param {AlarmOptions} options
 * @returns {Promise<AlarmReason>}
 */
export declare function showAlarm(options: AlarmOptions): Promise<AlarmReason>;
/**
 * Write the countdown's two shares and its words. Shared with the React
 * channel, so both bars read the same two custom properties.
 *
 * @param {HTMLElement} dialog
 * @param {HTMLElement | null} left the words under the bar
 * @param {number} share the continuous share of the time left, 1 to 0
 * @param {number} stepped the share in whole seconds, for reduced motion
 * @param {string} words
 */
export declare function writeCountdown(
  dialog: HTMLElement,
  left: HTMLElement | null,
  share: number,
  stepped: number,
  words: string,
): void;
/**
 * Wire `[data-kp-alarm]` triggers: a press raises the alarm the trigger
 * describes, and the close event is dispatched on the trigger with the
 * reason. The value of `data-kp-alarm` is the headline.
 *
 *   <button type="button" class="kp-button kp-button--destructive"
 *           data-kp-alarm="Access denied"
 *           data-kp-alarm-code="Security protocol 7 · lockout"
 *           data-kp-alarm-detail="Three failed attempts. This console is locked."
 *           data-kp-alarm-mode="auto" data-kp-alarm-seconds="6">Test the alarm</button>
 *
 * `data-kp-alarm-escape` (present) lets Escape close an acknowledged alarm;
 * `data-kp-alarm-action` names its button.
 *
 * @param {ParentNode} [root]
 * @param {{ strings?: Partial<import('./strings.js').Strings> }} [options]
 * @returns {() => void} detach
 */
export declare function attachAlarms(
  root?: ParentNode,
  {
    strings,
  }?: {
    strings?: Partial<import("./strings.js").Strings>;
  },
): () => void;
