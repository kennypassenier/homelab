/** A slider. */
export declare const SLIDER = "[data-kp-slider]";
/** Fired on the slider on every move by a person: `{ values, value, thumb, text }`. */
export declare const SLIDER_INPUT_EVENT = "kp-slider-input";
/** Fired on the slider when a value is committed (on release, or at once with `data-kp-commit="input"`). */
export declare const SLIDER_CHANGE_EVENT = "kp-slider-change";
/** The states a slider can be in. */
export declare const SLIDER_STATES: readonly ["ready", "loading"];
/**
 * A value snapped to the step from the minimum, clamped, and without floating-point dust. Pure.
 * @param {number} value @param {number} min @param {number} max @param {number} step
 */
export declare function snapSlider(
  value: number,
  min: number,
  max: number,
  step: number,
): number;
/**
 * Where a value stands along the track, 0 to 100. Pure.
 * @param {number} value @param {number} min @param {number} max
 */
export declare function sliderPercent(
  value: number,
  min: number,
  max: number,
): number;
/**
 * A number typed in the box: the unit and spaces dropped, a comma read as the decimal; NaN when it is not a number. Pure.
 * @param {string} text @param {string} [unit]
 */
export declare function parseSliderText(text: string, unit?: string): number;
export type SliderHandle = {
  element: HTMLElement;
  /**
   * one value, or two for a range
   */
  values: () => number[];
  /**
   * replace the values (no event)
   */
  set: (values: readonly number[]) => void;
  setState: (state: "ready" | "loading") => void;
  /**
   * the value as the box shows it
   */
  text: () => string;
};
/** The handle of an attached slider. @param {Element} element */
export declare function slider(element: Element): SliderHandle | null;
/**
 * The values of a slider.
 * @param {Element} element
 */
export declare function sliderValues(element: Element): number[];
/**
 * Set the values of a slider from a script, without the event.
 * @param {Element} element @param {readonly number[]} values
 */
export declare function setSliderValues(
  element: Element,
  values: readonly number[],
): void;
/**
 * Put a slider in a state: `loading` shows its skeleton and switches it off.
 * @param {Element} element @param {'ready' | 'loading'} state
 */
export declare function setSliderState(
  element: Element,
  state: "ready" | "loading",
): void;
/**
 * Wire every slider under `root`.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachSliders(root?: ParentNode): () => void;
