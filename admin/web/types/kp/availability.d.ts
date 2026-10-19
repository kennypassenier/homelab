/** The grids this module looks after. */
export declare const AVAILABILITY = "[data-kp-availability]";
/** Fired when a person's answers change: `{ person, keys, answer, previous, answers, settle }`. */
export declare const AVAILABILITY_EVENT = "kp-availability-change";
/** Fired by "Choose these dates": `{ keys }`. */
export declare const AVAILABILITY_CHOOSE_EVENT = "kp-availability-choose";
/** Fired when the organiser switches the columns between periods and days: `{ unit }`. */
export declare const AVAILABILITY_UNIT_EVENT = "kp-availability-unit";
/** Fired by the Retry of a grid that could not be read. */
export declare const AVAILABILITY_RETRY_EVENT = "kp-availability-retry";
/** The answers a cell can hold, in the order a press cycles them. */
export declare const AVAILABILITY_ANSWERS: readonly string[];
/** How long a refused cell keeps its mark [ms]. */
export declare const AVAILABILITY_REFUSED_MS = 3000;
/** How long Undo is offered after a stroke [ms]. */
export declare const AVAILABILITY_UNDO_MS = 6000;
export type AvailabilityAnswer = "yes" | "maybe" | "no";
export type AvailabilityColumn = {
  key: string;
  label: string;
  sub?: string;
};
export type AvailabilityPerson = {
  id: string;
  name: string;
  mine?: boolean;
};
export type AvailabilityAnswers = Record<
  string,
  Record<string, AvailabilityAnswer | undefined>
>;
export type AvailabilitySet = {
  columns: AvailabilityColumn[];
  answers: AvailabilityAnswers;
};
export type AvailabilitySpec = {
  /**
   * what a column is (default `range`)
   */
  unit?: "range" | "day";
  columns?: AvailabilityColumn[];
  people?: AvailabilityPerson[];
  answers?: AvailabilityAnswers;
  /**
   * both kinds of column, with their answers
   */
  sets?: {
    range?: AvailabilitySet;
    day?: AvailabilitySet;
  };
  /**
   * show the organiser's switch between the two sets (the attribute data-kp-availability-unit-switch on or off overrides it at run time)
   */
  canSwitchUnit?: boolean;
  state?: "ready" | "loading" | "empty" | "error";
  /**
   * the failed or empty plate's sentence
   */
  message?: string;
  /**
   * the empty plate's action words (fires `kp-availability-retry` with `{ action: 'add' }`)
   */
  emptyAction?: string;
};
export type AvState = {
  spec: AvailabilitySpec;
  set: AvailabilitySet;
  unit: "range" | "day";
  refused: Map<string, number>;
  saving: Set<string>;
  off: (() => void)[];
  menu: HTMLElement | null;
};
/**
 * Day columns in rule 52: `Sat 21/6`, with the long date under it.
 * @param {string} start an ISO date `2026-06-19`
 * @param {number} count
 * @param {string} [zone]
 * @returns {AvailabilityColumn[]}
 */
export declare function availabilityDays(
  start: string,
  count: number,
  zone?: string,
): AvailabilityColumn[];
/**
 * A proposed period as one column: `21 to 23 Jun`, with its length under it.
 * @param {string} start an ISO date @param {string} end an ISO date (the last day, inclusive)
 * @returns {AvailabilityColumn}
 */
export declare function availabilityRange(
  start: string,
  end: string,
): AvailabilityColumn;
/** The best column or columns, as keys. @param {HTMLElement} root @returns {string[]} */
export declare function availabilityBest(root: HTMLElement): string[];
/**
 * Give the grid its people, columns and answers.
 * @param {HTMLElement} root @param {AvailabilitySpec} spec
 */
export declare function setAvailability(
  root: HTMLElement,
  spec: AvailabilitySpec,
): void;
/** The answers as they stand. @param {HTMLElement} root @returns {AvailabilityAnswers} */
export declare function availabilityAnswers(
  root: HTMLElement,
): AvailabilityAnswers;
/**
 * Change one cell as the page (no event): a push update from another person.
 * @param {HTMLElement} root @param {string} person @param {string} column @param {AvailabilityAnswer | null} answer
 */
export declare function setAvailabilityAnswer(
  root: HTMLElement,
  person: string,
  column: string,
  answer: AvailabilityAnswer | null,
): void;
/**
 * Wire every availability grid under `root`: presses, painting, keys, the switch, Choose.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachAvailability(root?: ParentNode): () => void;
