/** The lists this module looks after. */
export declare const CLAIM = "[data-kp-checklist-claim]";
/** Fired on the list, bubbling and cancelable, when somebody takes an item, lets go or is assigned. */
export declare const CLAIM_EVENT = "kp-claim-change";
/** Fired when the filter chip changes: `{ filter }`. */
export declare const CLAIM_FILTER_EVENT = "kp-claim-filter";
/** Fired when the Retry of a failed list is pressed. */
export declare const CLAIM_RETRY_EVENT = "kp-claim-retry";
/** The filters of the chip group. */
export declare const CLAIM_FILTERS: readonly string[];
/** How long a refused row keeps its mark [ms]. */
export declare const CLAIM_REFUSED_MS = 3000;
/** How long Undo is offered after a release [ms]. */
export declare const CLAIM_UNDO_MS = 6000;
export type ClaimPerson = {
  id: string;
  name: string;
};
export type ClaimInfo = {
  /**
   * who brings it
   */
  by?: ClaimPerson | ClaimPerson[] | null;
  /**
   * the reason nobody can change it ("Already packed in the van")
   */
  locked?: string;
};
export type ClaimDetail = {
  id: string;
  action: "join" | "leave" | "assign" | "unassign" | "restore" | "take-all";
  /**
   * who the change is about
   */
  person: ClaimPerson | null;
  /**
   * who bring it after the change
   */
  by: ClaimPerson[];
  previousBy: ClaimPerson[];
  /**
   * hold the row on its spinner until the server has answered
   */
  settle: (answer: Promise<unknown>) => void;
};
export type ClaimState = {
  viewer: ClaimPerson | null;
  people: ClaimPerson[];
  busy: Set<string>;
  refused: Map<string, number>;
  filter: string;
  off: (() => void)[];
  said: string;
};
/** @param {ClaimPerson | ClaimPerson[] | null | undefined} by @returns {ClaimPerson[]} */
export declare function claimPeople(
  by: ClaimPerson | ClaimPerson[] | null | undefined,
): ClaimPerson[];
/**
 * Say who is looking. Their button is the live one, their name the held face.
 * @param {Element} list @param {ClaimPerson | null} viewer
 */
export declare function setClaimViewer(
  list: Element,
  viewer: ClaimPerson | null,
): void;
/**
 * Give the assign menu its people (the viewer is added first when missing).
 * @param {Element} list @param {ClaimPerson[]} people
 */
export declare function setClaimPeople(
  list: Element,
  people: ClaimPerson[],
): void;
/** The filter now shown. @param {Element} list */
export declare function claimFilter(list: Element): string;
/** @param {Element} list @param {string} filter */
export declare function setClaimFilter(list: Element, filter: string): void;
/**
 * A push update: the item's claim as the server says it now, without an event.
 * @param {Element} list @param {string} id @param {ClaimInfo} claim
 */
export declare function setClaim(
  list: Element,
  id: string,
  claim: ClaimInfo,
): void;
/**
 * Wire every claim checklist under `root`: the buttons, the menu, the chips, the keys. Items come from the checklist's
 * setChecklistItems() with a `claim` on each; the viewer from setClaimViewer().
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachClaims(root?: ParentNode): () => void;
