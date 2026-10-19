// The kp-themes module chassis-rs serves at /static/kp/js/usual-picker.js (kp-themes 11.0.0),
// copied from kp-themes js/usual-picker.d.ts: chassis serves the module, not its types.
/** A picker with the usual on top. */
export declare const USUAL = "[data-kp-usual]";
/** Fired on the picker when something is picked: `{ value, from }`. */
export declare const USUAL_PICK_EVENT = "kp-usual-pick";
/** Fired on the picker, cancelable, before a typed name is added: `{ label, nearTo }`. */
export declare const USUAL_ADD_EVENT = "kp-usual-add";
/** Fired on the picker when an item's "Not today" is set or cleared: `{ value, on }`. */
export declare const USUAL_NOT_TODAY_EVENT = "kp-usual-not-today";
/** Fired on the picker when Retry is pressed on the plate of a list that could not be read. */
export declare const USUAL_RETRY_EVENT = "kp-usual-retry";
/** The states a picker can be in. */
export declare const USUAL_STATES: readonly [
  "ready",
  "loading",
  "empty",
  "error",
];
/** More items than this fold behind "Show all N" unless `data-kp-all` says otherwise. */
export declare const USUAL_FOLD_ABOVE = 12;
export type UsualItem = {
  value: string;
  label: string;
  el: HTMLElement;
};
export type UsualModel = {
  box: HTMLElement;
  input: HTMLInputElement;
  list: HTMLElement;
  status: HTMLElement | null;
  handle: import("./combobox.js").ComboboxHandle;
  items: UsualItem[];
  /**
   * the band set from a script
   */
  given:
    | {
        value: string;
        uses?: number;
      }[]
    | null;
  expanded: boolean;
  nextFrom: string | null;
  notToday: Set<string>;
  panel: HTMLElement;
  busy: import("./busy-card.js").BusyCard | null;
  hintEl: HTMLElement;
  /**
   * the line under the field after an add, with its Undo
   */
  noteEl: HTMLElement;
  /**
   * the usual choices' own check (Not today), when the picker has one
   */
  check: HTMLElement | null;
  /**
   * what the check was drawn for (its look and the band), so a sync redraws only on a change
   */
  checkKey: string;
  /**
   * detaches the three-way chips of the check
   */
  checkOff: () => void;
  /**
   * a change made by the check itself: no pick event
   */
  silent: boolean;
};
/** A name as the guard compares it: case, accents and spacing ignored. Pure. @param {string} text */
export declare function normaliseName(text: string): string;
/**
 * How alike two names are, 0 to 1: one minus the edit distance over the longer length, after `normaliseName`. Pure.
 * @param {string} a @param {string} b
 */
export declare function similarity(a: string, b: string): number;
/**
 * The names closest to what was typed, nearest first: at least `threshold` alike, at most `max`, never an identical one. Pure.
 * @template {{ label: string }} T
 * @param {string} typed @param {readonly T[]} names @param {number} [threshold] @param {number} [max]
 * @returns {T[]}
 */
export declare function nearNames<
  T extends {
    label: string;
  },
>(typed: string, names: readonly T[], threshold?: number, max?: number): T[];
/**
 * Wire every picker with `data-kp-usual` under `root`. The combobox it stands on is attached here (a box with the usual is
 * skipped by `attachComboboxes` until this has made it ready).
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachUsualPickers(root?: ParentNode): () => void;
export type UsualHandle = {
  element: HTMLElement;
  /**
   * hand over the band (`{ value, uses }` with the source `counts`)
   */
  setUsual: (
    values: readonly (
      | string
      | {
          value: string;
          uses?: number;
        }
    )[],
  ) => void;
  /**
   * the band as drawn
   */
  usual: () => string[];
  /**
   * the values marked "Not today"
   */
  notToday: () => string[];
  setState: (state: "ready" | "loading" | "empty" | "error") => void;
  setHint: (text: string) => void;
  /**
   * read the options again after the page changed them
   */
  refresh: () => void;
};
/**
 * @typedef {object} UsualHandle
 * @property {HTMLElement} element
 * @property {(values: readonly (string | { value: string, uses?: number })[]) => void} setUsual hand over the band (`{ value, uses }` with the source `counts`)
 * @property {() => string[]} usual the band as drawn
 * @property {() => string[]} notToday the values marked "Not today"
 * @property {(state: 'ready' | 'loading' | 'empty' | 'error') => void} setState
 * @property {(text: string) => void} setHint
 * @property {() => void} refresh read the options again after the page changed them
 */
/**
 * The handle of an attached picker.
 * @param {Element} element
 * @returns {UsualHandle | null}
 */
export declare function usualPicker(element: Element): UsualHandle | null;
/**
 * Hand a picker its band: values (in order), or `{ value, uses }` which `data-kp-usual-source="counts"` ranks by uses.
 * @param {Element} element
 * @param {readonly (string | { value: string, uses?: number })[]} values
 */
export declare function setUsual(
  element: Element,
  values: readonly (
    | string
    | {
        value: string;
        uses?: number;
      }
  )[],
): void;
/**
 * The state of a picker: `loading` (the busy card in the place of the list, the input waiting), `empty`, `error` (Retry fires
 * `kp-usual-retry`; the last good list stays), `ready`.
 * @param {Element} element
 * @param {'ready' | 'loading' | 'empty' | 'error'} state
 */
export declare function setUsualState(
  element: Element,
  state: "ready" | "loading" | "empty" | "error",
): void;
/**
 * The values marked "Not today".
 * @param {Element} element
 * @returns {string[]}
 */
export declare function usualNotToday(element: Element): string[];
