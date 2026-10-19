export type Hsl = {
  h: number;
  s: number;
  l: number;
};
export type ColourDetail = {
  value: string;
  h: number;
  s: number;
  l: number;
  ratio: number | null;
  ok: boolean | null;
  against: string;
};
/** @typedef {{ h: number, s: number, l: number }} Hsl */
/** @typedef {{ value: string, h: number, s: number, l: number, ratio: number | null, ok: boolean | null, against: string }} ColourDetail */
/** Fired when the colour changes. A contract value [TH26]. Detail: ColourDetail. */
export declare const COLOR_EVENT = "kp-color-change";
export type ColorPickerHandle = {
  element: HTMLElement;
  get: () => ColourDetail;
  /**
   * an object, or an hsl() string
   */
  set: (colour: Hsl | string) => void;
  /**
   * re-run the contrast measurement (after a theme or target change)
   */
  measure: () => void;
};
/** The handle for an attached picker. @param {Element} element */
export declare function colorPicker(element: Element): ColorPickerHandle | null;
/**
 * @param {ParentNode} root
 * @param {{ followTheme?: boolean }} [options] re-measure on theme change (default true; per picker `data-kp-follow-theme="false"`)
 * @returns {(() => void) & { handles: ColorPickerHandle[] }} detach
 */
export declare function attachColorPickers(
  root?: ParentNode,
  {
    followTheme,
  }?: {
    followTheme?: boolean;
  },
): (() => void) & {
  handles: ColorPickerHandle[];
};
