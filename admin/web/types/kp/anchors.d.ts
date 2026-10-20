/**
 * Draw a container's marks again, after an option changed (the position of the mark, the levels).
 * @param {Element} box
 */
export declare function refreshAnchors(box: Element): void;
/** A container whose headings or rows get anchors. */
export declare const ANCHORS = "[data-kp-anchors]";
/** One element that says its own id. */
export declare const ANCHOR = "[data-kp-anchor]";
/** Fired on the mark when a link was copied: `{ id, url, text, markdown, ok }`. */
export declare const ANCHOR_COPY_EVENT = "kp-anchor-copy";
/** Fired (cancelable) on the container before a link lands in a place that may be folded or on another page: `{ id, target }`. */
export declare const ANCHOR_REVEAL_EVENT = "kp-anchor-reveal";
/** Fired on the container when a hash has no place to land: `{ id }`. */
export declare const ANCHOR_MISSING_EVENT = "kp-anchor-missing";
/** How long a place a link landed on stays marked, in ms. */
export declare const ANCHOR_HIT_MS = 2000;
/** The longest a hash that has no row yet is waited for, in ms. */
export declare const ANCHOR_WAIT_MS = 10000;
/** How long a container must have been still, with nothing in it drawing, before a hash without a place is given up, in ms. */
export declare const ANCHOR_QUIET_MS = 1000;
/** How long "Copied" stays on the mark, in ms. */
export declare const ANCHOR_COPIED_MS = 1500;
/**
 * A slug of some text: lower case, accents folded, anything else a dash. Pure.
 * @param {string} text
 */
export declare function anchorSlug(text: string): string;
/**
 * An id that no element of the document has yet: the slug, then `-2`, `-3`. Pure over the document.
 * @param {Document} doc @param {string} slug @param {string} [fallback]
 */
export declare function uniqueAnchorId(
  doc: Document,
  slug: string,
  fallback?: string,
): string;
export type AnchorOptions = {
  anchors?: "headings" | "rows" | "both" | "off";
  levels?: number[];
  show?: "hover" | "always" | "touch";
  copy?: "url" | "hash";
  target?: "highlight" | "none";
  open?: boolean;
  copiedMs?: number;
  /**
   * the longest wait for a place not drawn yet, in ms
   */
  wait?: number;
};
/**
 * Wire every container under `root`.
 * @param {ParentNode} [root]
 * @param {AnchorOptions} [options] overrides the markup
 * @returns {() => void} detach
 */
export declare function attachAnchors(
  root?: ParentNode,
  options?: AnchorOptions,
): () => void;
