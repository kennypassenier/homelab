// The kp-themes module chassis-rs serves at /static/kp/js/idmark.js (kp-themes 11.1.1),
// copied from kp-themes js/idmark.d.ts: chassis serves the module, not its types.
/** A mark the module fills from its name. */
export declare const IDMARK = "[data-kp-idmark]";
/** How many colours the theme's chart palette has. */
export declare const IDMARK_SLOTS = 5;
/** The corner glyphs, in the order the hash picks them. */
export declare const IDMARK_GLYPHS: readonly string[];
/**
 * The letters of a name: the initials of its first two words, or its first
 * two letters when it is one word. Letters and digits only.
 * @param {string} name @param {number} [count]
 */
export declare function idmarkLetters(name: string, count?: number): string;
/**
 * Everything a mark is, from its name. Pure: the same name gives the same
 * mark in node and in every browser.
 * @param {string} name
 * @returns {{ name: string, empty: boolean, slot: number, hue: number, letters: string, glyph: string, cells: [number, number][] }}
 *   `slot` 1 to 5 (the chart colour), `hue` 0 to 359 (the free hue)
 */
export declare function idmarkFor(name: string): {
  name: string;
  empty: boolean;
  slot: number;
  hue: number;
  letters: string;
  glyph: string;
  cells: [number, number][];
};
/**
 * Palette slots for names read together: each name keeps its own slot where
 * no other name took it; a collision moves the later name to the nearest
 * slot still free, and only when all five are taken do names share.
 * The order of `names` decides who keeps a contested slot.
 * @param {readonly string[]} names
 * @returns {Map<string, number>} name to slot, 1 to 5
 */
export declare function idmarkColours(
  names: readonly string[],
): Map<string, number>;
/**
 * Fill a mark from a name: its colour, letters, pattern and glyph. Call it
 * again with a new name; an empty name gives the quiet plate.
 * @param {HTMLElement} el a `.kp-idmark`
 * @param {string} name
 * @param {{ slot?: number, hue?: 'palette' | 'free', title?: boolean }} [options]
 *   `slot` a palette slot from idmarkColours(); `hue` defaults to the element's
 *   `data-kp-idmark-hue` ('free') else 'palette'; `title` (default true) puts
 *   the name in the element's title for a pointer
 */
export declare function setIdmark(
  el: HTMLElement,
  name: string,
  {
    slot,
    hue,
    title,
  }?: {
    slot?: number;
    hue?: "palette" | "free";
    title?: boolean;
  },
): void;
/** The statuses a mark can show. */
export declare const IDMARK_STATUSES: readonly string[];
export type IdmarkPerson = {
  name: string;
  /**
   * the photo
   */
  src?: string;
  status?: "online" | "away" | "busy" | "none";
};
/**
 * @typedef {object} IdmarkPerson
 * @property {string} name
 * @property {string} [src] the photo
 * @property {'online' | 'away' | 'busy' | 'none'} [status]
 */
/**
 * Draw a group of marks with an exact "+N" for the people left out. The "+N" is
 * a button that opens the list of their names (`list: false` leaves it a plain
 * count). Counts are the real number, never "9+".
 * @param {HTMLElement} el the group, a `.kp-idmark-group`
 * @param {readonly IdmarkPerson[]} people
 * @param {{ max?: number, size?: 'xs' | 'sm' | 'md' | 'lg' | 'xl', round?: boolean, list?: boolean }} [options]
 *   `max` marks shown (default 4); `size` the marks' size (default md); `round` round marks (default true)
 */
export declare function setIdmarkGroup(
  el: HTMLElement,
  people: readonly IdmarkPerson[],
  {
    max,
    size,
    round,
    list,
  }?: {
    max?: number;
    size?: "xs" | "sm" | "md" | "lg" | "xl";
    round?: boolean;
    list?: boolean;
  },
): void;
/**
 * Fill every `[data-kp-idmark]` under `root`, and each one added or renamed
 * later. Marks inside one `[data-kp-idmark-set]` share out the palette with
 * idmarkColours(), in document order.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachIdmarks(root?: ParentNode): () => void;
