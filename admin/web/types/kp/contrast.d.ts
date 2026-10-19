export type Hsl = {
  h: number;
  s: number;
  l: number;
};
export type Rgb = [number, number, number];
/** @typedef {{h: number, s: number, l: number}} Hsl */
/** @typedef {[number, number, number]} Rgb sRGB in 0..1 */
/**
 * @param {string} text e.g. "hsl(40, 25%, 97%)"
 * @returns {Hsl}
 */
export declare function parseHsl(text: string): Hsl;
/** @param {Hsl} c @returns {string} */
export declare function formatHsl({ h, s, l }: Hsl): string;
/** @param {Hsl} c @returns {Rgb} */
export declare function hslToRgb({ h, s, l }: Hsl): Rgb;
/** @param {Rgb} rgb @returns {Hsl} */
export declare function rgbToHsl([r, g, b]: Rgb): Hsl;
/**
 * WCAG relative luminance. Note the 0.03928 threshold: it is the value the
 * specification names, and it differs slightly from the sRGB standard's
 * 0.04045. Pinned deliberately — this measures against WCAG.
 *
 * @param {Rgb} rgb
 */
export declare function luminance([r, g, b]: Rgb): number;
/**
 * WCAG contrast ratio between two colours.
 *
 * @param {Rgb} a @param {Rgb} b
 */
export declare function contrast(a: Rgb, b: Rgb): number;
/** Convenience: parse an authored value straight to sRGB. @param {string} text */
export declare const hsl: (text: string) => Rgb;
/**
 * Read a token off an element and return it as sRGB.
 *
 * The tokens are authored as `hsl(...)`, so this parses that. A theme that
 * ever emits another notation would need this to change, which is why the
 * gate that keeps every colour in the token layer exists.
 *
 * @param {string} token e.g. "--background"
 * @param {Element} [from]
 * @returns {Rgb | null} null when the token is absent or not an hsl() value
 */
export declare function tokenColour(token: string, from?: Element): Rgb | null;
/**
 * Does this pair reach the level it needs?
 *
 * The three numbers are WCAG's, not this project's: 4.5 for body text
 * (SC 1.4.3), 3.0 for large text and for non-text boundaries (SC 1.4.11).
 *
 * @param {number} ratio
 * @param {'text' | 'large' | 'non-text'} kind
 */
export declare function meets(
  ratio: number,
  kind?: "text" | "large" | "non-text",
): boolean;
