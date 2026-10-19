/** The lists this module looks after. */
export declare const CHECKLIST = "[data-kp-checklist]";
/** Fired on a tick: `{ id, checked, items }`. */
export declare const CHECKLIST_EVENT = "kp-checklist-change";
/** Fired on Clear checked: `{ removed, items }`. */
export declare const CHECKLIST_CLEAR_EVENT = "kp-checklist-clear";
/** Fired on Undo: `{ restored, items }`. */
export declare const CHECKLIST_UNDO_EVENT = "kp-checklist-undo";
/** Fired on the list (not bubbling) after every draw, so a part that adds to the rows can draw its cells [kpadopt6]. */
export declare const CHECKLIST_RENDER_EVENT = "kp-checklist-render";
/** How long Undo is offered after a clear, unless the list says otherwise [ms]. */
export declare const CHECKLIST_UNDO_MS = 6000;
export type ChecklistItem = {
  id: string;
  label: string;
  /**
   * the amount or a note, after the label ("3 pieces")
   */
  detail?: string;
  /**
   * a shop or a section; items without one share a group without a heading
   */
  group?: string;
  checked?: boolean;
  /**
   * who brings it (js/claim.js, `data-kp-checklist-claim`) [kpadopt6]
   */
  claim?: import("./claim.js").ClaimInfo;
};
export type Undo = {
  removed: ChecklistItem[];
  at: Map<string, number>;
  timer: ReturnType<typeof setTimeout>;
};
/** @param {Element} list @returns {ChecklistItem[]} */
export declare function getChecklistItems(list: Element): ChecklistItem[];
/**
 * Give a list its items, changing the rows in place (by id).
 * @param {Element} list
 * @param {ChecklistItem[]} items
 */
export declare function setChecklistItems(
  list: Element,
  items: ChecklistItem[],
): void;
/**
 * Tick or untick one item as the page (no event).
 * @param {Element} list @param {string} id @param {boolean} checked
 */
export declare function setChecklistChecked(
  list: Element,
  id: string,
  checked: boolean,
): void;
/**
 * The list's state: `loading` lays the busy card over the body (and keeps it
 * at its height), `error` says `message` where the rows were, `ready` is
 * neither.
 * @param {Element} list @param {'loading' | 'error' | 'ready'} state @param {string} [message]
 */
export declare function setChecklistState(
  list: Element,
  state: "loading" | "error" | "ready",
  message?: string,
): void;
/**
 * Wire every checklist under `root`: the ticks, Check all, Clear checked and
 * its Undo. Items come from setChecklistItems().
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachChecklists(root?: ParentNode): () => void;
