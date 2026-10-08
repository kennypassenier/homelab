// The kp-themes module chassis-rs serves at /static/kp/js/update.js (kp-themes 10.0.0),
// copied from kp-themes js/update.d.ts: chassis serves the module, not its types.
/** The attribute written on an element while its update plays. */
export declare const UPDATING_ATTRIBUTE = "data-kp-updating";
/** The custom property a register names its update idea in. */
export declare const UPDATE_PROPERTY = "--kp-update";
/** The inline properties update() sets while it plays, and takes off after. */
export declare const UPDATE_STYLE: readonly string[];
/**
 * The idea a register's `--kp-update` names, or '' for none.
 * @param {string | null | undefined} value the computed custom property
 * @returns {string}
 */
export declare function updateIdea(value: string | null | undefined): string;
/**
 * How long an update plays and on which curve, from the theme's motion: the
 * box's resize time or the dialog's close time, whichever is longer, times
 * 1.25; the entrance's curve without its overshoot. 0 without motion.
 * @param {{ size: number, close: number, ease: string }} motion what themeMotion() returns
 * @returns {{ duration: number, ease: string }}
 */
export declare function updateTimingOf({
  size,
  close,
  ease,
}: {
  size: number;
  close: number;
  ease: string;
}): {
  duration: number;
  ease: string;
};
/**
 * The update timing of the theme at `scope`.
 * @param {Element} [scope]
 * @returns {{ duration: number, ease: string }}
 */
export declare function updateTiming(scope?: Element): {
  duration: number;
  ease: string;
};
/**
 * Whether an update with this idea plays anything: whenever the register
 * names one. Its timing may be 0 (reduced motion): a register that names an
 * idea there plays a still version on its own time.
 * @param {string} idea
 */
export declare const updatePlays: (idea: string) => boolean;
/**
 * Put the mark on: the theme's timing as inline custom properties, an
 * inline host as an inline-block (a transform and an overlay need a box),
 * the attribute last. No reflow of its own: a mark that was on came off
 * before the batch read the host's style, so setting it again starts the
 * keyframes again.
 * @param {HTMLElement} host
 * @param {string} idea
 * @param {{ duration: number, ease: string }} timing
 * @param {{ inline?: boolean }} [options] whether the host is laid out inline now
 * @returns {Record<string, string>} the inline values update() replaced, to put back
 */
export declare function markUpdating(
  host: HTMLElement,
  idea: string,
  timing: {
    duration: number;
    ease: string;
  },
  {
    inline,
  }?: {
    inline?: boolean;
  },
): Record<string, string>;
/**
 * Take the mark off and put back what markUpdating() replaced.
 * @param {HTMLElement} host
 * @param {Record<string, string>} before
 */
export declare function unmarkUpdating(
  host: HTMLElement,
  before: Record<string, string>,
): void;
export type SparkWrite = {
  svg: SVGSVGElement;
  parts: string;
  from: {
    line: string;
    area: string;
  };
  to: {
    line: string;
    area: string;
  };
};
export type UpdateJob = {
  el: Element;
  host: HTMLElement;
  spark: SparkWrite | null;
  resolve: (took: number) => void;
};
/**
 * Show `next` in `el` the theme's way. The value is written at the call and
 * stays readable throughout; any width the new value itself needs is taken
 * then, with the value, so nothing moves while the idea plays and nothing
 * moves when it ends. The mark goes on in the batch of this task (see
 * flush()), before the next frame.
 * @param {Element} el a text element, a `.kp-state-word`, or a spark's svg
 * @param {string | number | readonly number[]} next
 * @returns {Promise<number>} how long it played, in ms; 0 when nothing played
 */
export declare function update(
  el: Element,
  next: string | number | readonly number[],
): Promise<number>;
