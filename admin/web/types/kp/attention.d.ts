// The kp-themes module chassis-rs serves at /static/kp/js/attention.js (kp-themes 10.0.0),
// copied from kp-themes js/attention.d.ts: chassis serves the module, not its types.
/** The band, and the severities it orders by, worst first. */
export declare const ATTENTION = ".kp-attention";
export declare const SEVERITIES: readonly ["critical", "warning", "info"];
/**
 * Put a band's items in severity order in the DOM (a stable sort: two
 * problems of one severity keep the order the page gave them).
 * @param {Element} band
 */
export declare function sortAttention(band: Element): void;
export type AttentionItem = {
  /**
   * its stable id (`data-kp-key`): the same key is the same element on every refresh
   */
  key: string;
  severity: "critical" | "warning" | "info";
  /**
   * the problem, in one line
   */
  title: string;
  /**
   * one sentence more
   */
  text?: string;
  /**
   * the fix (a button or a link). Left out, a kept item keeps the action it has; `null` removes it. A new node equal in markup to the one shown (`isEqualNode`) keeps the one shown, so its focus and listeners survive a refresh.
   */
  action?: Node | null;
  /**
   * the word a screen reader hears before the title, in place of the dictionary's
   */
  srSeverity?: string;
};
/**
 * Set a band's problems, by key [port spec F]. An item whose key is on the
 * band already stays the same element and only what changed in it is
 * rewritten (an alert is not inserted again, so it is not announced again,
 * and focus on its fix stays); a new key makes a new item (`role="alert"`
 * for a critical one, `role="status"` otherwise); a key no longer given
 * leaves the theme's way (`leave()` from js/motion.js, at once under
 * reduced motion). The band ends worst first, in the given order within a
 * severity, and only items out of place are moved.
 *
 * @param {Element} band a `.kp-attention`
 * @param {AttentionItem[]} items
 */
export declare function setAttention(
  band: Element,
  items: AttentionItem[],
): void;
/**
 * Keep every attention band under `root` worst first, now and whenever an
 * item arrives or changes severity. A band added under `root` later is
 * sorted as it arrives and watched from then on; a band that leaves the
 * page is let go (its watcher disconnected). The band hides itself in CSS
 * when it holds nothing.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachAttention(root?: ParentNode): () => void;
