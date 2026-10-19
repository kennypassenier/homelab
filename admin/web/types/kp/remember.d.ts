/** The attribute an author writes to ask for a memory. A contract value [TH26]. */
export declare const REMEMBER_ATTRIBUTE = "data-kp-remember";
/** Fired on the element that was refused a memory because its name was taken: `{ name, component, other }`. */
export declare const REMEMBER_CLASH_EVENT = "kp-remember-clash";
/** The first segment of every key this package writes. */
export declare const REMEMBER_PREFIX = "kp-remember";
export type Remembered =
  | "sidenav"
  | "disclosure"
  | "tree"
  | "split"
  | "datatable"
  | "tour"
  | "refresh"
  | "notices"
  | "claim"
  | "availability"
  | "time"
  | "console"
  | "focus-steps"
  | "picker";
export type Memory = {
  /**
   * the name the element gave itself
   */
  name: string;
  /**
   * the composed storage key, so a consumer can read or clear it themselves [KT6]
   */
  key: (slot: string) => string;
  read: <T>(slot: string, fallback: T) => T;
  /**
   * whether it will survive a reload
   */
  write: (slot: string, value: unknown) => boolean;
  forget: (slot: string) => void;
};
export type RememberConfig = {
  prefix?: string;
  storage?: Storage | null;
};
/**
 * Set the defaults once: the first key segment, and where the values go.
 * `storage: null` turns the whole mechanism off without touching a
 * component — the way out KT6 asks for.
 *
 * @param {RememberConfig} next
 */
export declare function configureRemember(next: RememberConfig): void;
/**
 * The name an element gave itself, or null when it asked for nothing.
 *
 * @param {Element} element
 * @param {Remembered} component
 * @returns {string | null}
 */
export declare function rememberedName(
  element: Element,
  component: Remembered,
): string | null;
/**
 * Drop every claim. For a test, and for a consumer who tears a page down
 * and builds it again inside one document [KT6].
 */
export declare function forgetClaims(): void;
/**
 * The memory for one element, or null when it asked for none — and null
 * too when another element of the same component already answers to that
 * name, because two of them sharing one key is the fault the name prevents.
 *
 * @param {Element} element
 * @param {Remembered} component
 * @param {{ storage?: Storage | null }} [options] where the values go, for this element alone
 * @returns {Memory | null}
 */
export declare function memoryFor(
  element: Element,
  component: Remembered,
  {
    storage: given,
  }?: {
    storage?: Storage | null;
  },
): Memory | null;
/**
 * The groups of one side navigation, each with the id its state is kept
 * under: its own `data-kp-remember`, else the words in its toggle, else its
 * place in the panel. Exported so the module writes the same ids the
 * painter reads.
 *
 * @param {Element} panel
 * @returns {{ group: Element, toggle: Element | null, id: string }[]}
 */
export declare function sidenavGroups(panel: Element): {
  group: Element;
  toggle: Element | null;
  id: string;
}[];
/**
 * A tree item's id, the same rule js/structure.js uses for its handle.
 *
 * @param {Element} item
 * @returns {string}
 */
export declare function treeItemId(item: Element): string;
/**
 * A column's id: the field it names, else the words in its header. An
 * index would have been shorter and wrong — the data table inserts columns
 * of controls of its own, so the third column in the markup is not the
 * third column the module counts.
 *
 * @param {Element} header
 * @returns {string}
 */
export declare function columnId(header: Element): string;
/**
 * Put what was remembered back on the element, as markup, and hand the
 * caller the memory to go on writing to. Null when the element asked for
 * no memory or its name was taken.
 *
 * @param {Element} element
 * @param {Remembered} component
 * @param {{ storage?: Storage | null }} [options]
 * @returns {Memory | null}
 */
export declare function paintRemembered(
  element: Element,
  component: Remembered,
  options?: {
    storage?: Storage | null;
  },
): Memory | null;
/**
 * Paint every remembering element under `root`, before anything attaches
 * and before the first frame. Idempotent: each module paints its own
 * element again when it attaches, so a component rendered later is
 * restored too.
 *
 * @param {ParentNode} [root]
 */
export declare function restoreRemembered(root?: ParentNode): void;
/** The attribute that holds a disclosure's memory still: while it is on the element or an ancestor, the memory is neither painted nor written. */
export declare const REMEMBER_HOLD_ATTRIBUTE = "data-kp-remember-hold";
/**
 * Wire the disclosures that remember — a `<details data-kp-remember>`, the
 * accordion's own element. Every other component writes its state from its
 * own module; a `<details>` has no module, so this is it.
 *
 * A disclosure added under `root` later is wired as it arrives, and its
 * memory painted in the same step, before the browser paints it, so a page
 * that rebuilds its groups on every refresh shows each one as it was left,
 * with no frame of its markup default (port spec J3). One that leaves the
 * page is let go.
 *
 * `data-kp-remember-hold` on the element or an ancestor holds the memory
 * still: the page sets `open` itself (every group open while a search runs)
 * and nothing is painted over it or written from it. When the hold goes,
 * the stored state is painted back.
 *
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachRemembered(root?: ParentNode): () => void;
/**
 * Forget the stored state of every element of `component` whose name starts
 * with `prefix` and is not in `names`: a page whose groups come and go
 * (named `apps-<group>`) prunes the groups that no longer exist, so its
 * memory does not grow without end.
 *
 * @param {Remembered} component
 * @param {string} prefix
 * @param {Iterable<string>} names the names still in use
 * @param {{ storage?: Storage | null }} [options]
 * @returns {number} how many stored values were removed
 */
export declare function forgetRememberedExcept(
  component: Remembered,
  prefix: string,
  names: Iterable<string>,
  {
    storage: given,
  }?: {
    storage?: Storage | null;
  },
): number;
