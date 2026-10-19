/** The custom property carrying the stylesheet's version [AR25]. */
export declare const VERSION_PROPERTY = "--kp-themes-version";
/** The custom property carrying the stylesheet's theme names [AR25]. */
export declare const NAMES_PROPERTY = "--kp-themes-names";
export type Side = {
  /**
   * the version this half was generated from, or null when it does not say
   */
  version: string | null;
  /**
   * the theme names this half knows
   */
  themes: string[];
  /**
   * the hook values js/effects.js reported as unknown on this page [AR44]
   */
  unknownEffects?: string[];
};
export type Report = {
  status:
    | "match"
    | "stylesheet-behind"
    | "script-behind"
    | "themes-differ"
    | "no-version";
  stylesheet: Side;
  script: Side;
  /**
   * themes the stylesheet has and the JavaScript does not
   */
  onlyInStylesheet: string[];
  /**
   * themes the JavaScript has and the stylesheet does not
   */
  onlyInScript: string[];
  /**
   * one sentence, from the dictionary, saying which half is behind
   */
  verdict: string;
};
/**
 * Compare two dotted version numbers.
 *
 * @param {string} a
 * @param {string} b
 * @returns {number | null} negative when a is older, 0 when equal, positive when a is newer; null when either is not a version
 */
export declare function compareVersions(a: string, b: string): number | null;
/**
 * What the stylesheet on this page says about itself.
 *
 * A stylesheet older than 3.2.0 declares neither property, and that is
 * information rather than an error: it is older than any JavaScript that
 * can ask the question.
 *
 * @param {{ root?: Element }} [options]
 * @returns {Side}
 */
export declare function stylesheetSide({ root }?: { root?: Element }): Side;
/**
 * What the JavaScript on this page says about itself.
 *
 * @returns {Side}
 */
export declare function scriptSide(): Side;
/**
 * Lay the two halves beside each other and name the one that is behind.
 *
 * Pure: same arguments, same report, no document and no clock. The test
 * that measures this feeds it a pair it knows to be mismatched, because a
 * verdict that has only ever seen a matching pair has never been read.
 *
 * @param {Side} stylesheet
 * @param {Side} script
 * @param {import('./strings.js').Strings} [strings]
 * @returns {Report}
 */
export declare function diagnose(
  stylesheet: Side,
  script: Side,
  strings?: import("./strings.js").Strings,
): Report;
/**
 * The same judgement, over the page this runs on.
 *
 * @param {{ root?: Element, strings?: import('./strings.js').Strings }} [options]
 * @returns {Report}
 */
export declare function diagnostics({
  root,
  strings,
}?: {
  root?: Element;
  strings?: import("./strings.js").Strings;
}): Report;
/**
 * Render the report into an element, as a table and one sentence.
 *
 * Every side is overridable, which is how the test feeds it a pair it
 * knows to be wrong [KT6]: nothing here reaches for a value it was not
 * given or could not read from the page it was pointed at.
 *
 * @param {Element} target
 * @param {{ root?: Element, stylesheet?: Side, script?: Side, strings?: import('./strings.js').Strings }} [options]
 * @returns {Report} the report it just drew
 */
export declare function renderDiagnostics(
  target: Element,
  {
    root,
    stylesheet,
    script,
    strings,
  }?: {
    root?: Element;
    stylesheet?: Side;
    script?: Side;
    strings?: import("./strings.js").Strings;
  },
): Report;
