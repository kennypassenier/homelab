/** A bullet list the page fills with setBullets(). */
export declare const BULLETS = "[data-kp-bullets]";
/** Fired on the root when a `pick` row is pressed: `{ key }`. */
export declare const BULLETS_PICK_EVENT = "kp-bullets-pick";
/** Fired on the root when the Retry of a failed list is pressed. */
export declare const BULLETS_RETRY_EVENT = "kp-bullets-retry";
export type BulletRow = {
  key: string;
  label: string;
  detail?: string;
  /**
   * null: nothing to measure
   */
  value: number | null;
  /**
   * the figure as written, in place of format(value)
   */
  figure?: string;
  /**
   * a name (an identity mark is made) or a node
   */
  lead?: string | Node;
  /**
   * the row is a link
   */
  href?: string;
  /**
   * the row is a button
   */
  pick?: boolean;
};
export type BulletsSpec = {
  rows?: BulletRow[];
  threshold: number;
  /**
   * where the bar ends (default twice the threshold)
   */
  max?: number;
  /**
   * at or past this the row is destructive (default none)
   */
  bad?: number;
  format?: (value: number) => string;
  /**
   * default 'worst'
   */
  sort?: "worst" | "none";
  state?: "ready" | "loading" | "empty" | "error";
  /**
   * the sentence of a failed list
   */
  message?: string;
  /**
   * false leaves Retry out of a failed list (a read the page cannot repeat)
   */
  retry?: boolean;
  emptyTitle?: string;
  emptyText?: string;
  /**
   * how many waiting rows (default: the rows shown last, else 3)
   */
  loadingRows?: number;
  /**
   * what the busy card says
   */
  loadingWords?: string;
};
/**
 * The tone of a value: warning at or past the threshold, destructive at or
 * past `bad`.
 * @param {number | null} value @param {number} threshold @param {number} [bad]
 * @returns {'' | 'warning' | 'destructive'}
 */
export declare function bulletTone(
  value: number | null,
  threshold: number,
  bad?: number,
): "" | "warning" | "destructive";
/**
 * Draw (or redraw) a bullet list.
 * @param {HTMLElement} el the root, a `.kp-bullets`
 * @param {BulletsSpec} spec
 */
export declare function setBullets(el: HTMLElement, spec: BulletsSpec): void;
/**
 * Fill every `[data-kp-bullets]` under `root` that carries its spec as JSON
 * in the attribute (a page that draws with setBullets() needs nothing).
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachBullets(root?: ParentNode): () => void;
