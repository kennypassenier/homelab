/** The longest note drawn, in characters. */
export declare const MARKDOWN_LIMIT = 40000;
export type MarkdownOptions = {
  /**
   * the level a `#` heading is written at, 1 to 6 (default 4); deeper headings follow, to 6
   */
  headings?: number;
  /**
   * the most characters read (default 40 000); more is cut at a line with a sentence saying so
   */
  limit?: number;
};
/**
 * @typedef {object} MarkdownOptions
 * @property {number} [headings] the level a `#` heading is written at, 1 to 6 (default 4); deeper headings follow, to 6
 * @property {number} [limit] the most characters read (default 40 000); more is cut at a line with a sentence saying so
 */
/**
 * Whether an address may be a link: http, https, mailto, or no scheme at all.
 * @param {string} href
 * @returns {boolean}
 */
export declare function safeHref(href: string): boolean;
/**
 * Build the DOM of a markdown text. Nothing is parsed as HTML.
 * @param {Document} doc
 * @param {string} text
 * @param {MarkdownOptions} [options]
 * @returns {DocumentFragment}
 */
export declare function markdownFragment(
  doc: Document,
  text: string,
  { headings, limit }?: MarkdownOptions,
): DocumentFragment;
