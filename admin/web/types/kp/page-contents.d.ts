/** The lists this module looks after. */
export declare const CONTENTS = "[data-kp-contents]";
/** Fired on the list, bubbling, when the current entry changes: `{ id, label, previous }`. */
export declare const CONTENTS_EVENT = "kp-contents-change";
export type ContentsItem = {
  /**
   * the section's `id`
   */
  id: string;
  /**
   * the entry's words
   */
  label: string;
  /**
   * 0 (default) for a section, 1 under it, and so on
   */
  level?: number;
  /**
   * a small number or word after the label
   */
  count?: string | number;
};
export type ContentsHandle = {
  /**
   * the id of the current entry
   */
  current: () => string | null;
  /**
   * read the headings again now
   */
  refresh: () => void;
  /**
   * entries the page gives; null goes back to the headings
   */
  setItems: (items: readonly ContentsItem[] | null) => void;
};
/** The id a heading's words make: lower case, letters and digits joined by hyphens. @param {string} text */
export declare function contentsSlug(text: string): string;
/**
 * Which entry is current: the first section (in page order) whose top has passed the offset line and whose bottom has not; with none,
 * the last one passed; and the last entry at the foot of the page. Pure.
 * @param {readonly { top: number, bottom: number }[]} boxes the sections' boxes, in page order, relative to the window
 * @param {number} offset the line, px from the top of the window
 * @param {boolean} atEnd the page is scrolled to its foot
 * @returns {number} the index, -1 for none
 */
export declare function currentSection(
  boxes: readonly {
    top: number;
    bottom: number;
  }[],
  offset: number,
  atEnd: boolean,
): number;
/**
 * The handle of an attached list.
 * @param {Element} nav
 * @returns {ContentsHandle | null}
 */
export declare function contents(nav: Element): ContentsHandle | null;
/**
 * Give a list its entries (the page's own sections), or `null` to go back to reading the headings.
 * @param {Element} nav an attached `[data-kp-contents]`
 * @param {readonly ContentsItem[] | null} items
 */
export declare function setContents(
  nav: Element,
  items: readonly ContentsItem[] | null,
): void;
/**
 * Build a contents list for every `[data-kp-contents]` under `root` and follow the reader.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachContents(root?: ParentNode): () => void;
