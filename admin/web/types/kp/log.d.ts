/** The custom property a source's colour is written to. */
export declare const SOURCE_PROPERTY = "--kp-source-colour";
/**
 * FNV-1a over the name's bytes, as kp-tui hashes it: the same offset basis
 * and the same prime, so `media` is the third chart colour in a terminal and
 * the third here. A `DefaultHasher` (or any seeded one) would not be stable
 * between two runs, let alone between two languages.
 *
 * @param {string} name
 * @returns {number} 1 to 5, the chart colour the name takes
 */
export declare function sourceIndex(name: string): number;
/**
 * The value to give a colour property for `name`: the register's own chart
 * colour, by reference, so a theme switch moves it without this running
 * again.
 * @param {string} name
 */
export declare const sourceColour: (name: string) => string;
/**
 * Give every `[data-kp-source]` under `root` its name's colour.
 *
 * The attribute's value is the name where it has one, so a shortened label on
 * screen ("prom" for "prometheus") can still hash the full name; where it is
 * empty the element's own text is the name.
 *
 * @param {ParentNode} [root]
 * @returns {Element[]} the elements coloured
 */
export declare function attachLogs(root?: ParentNode): Element[];
