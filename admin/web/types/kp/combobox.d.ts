// The kp-themes module chassis-rs serves at /static/kp/js/combobox.js (kp-themes 11.0.0),
// copied from kp-themes js/combobox.d.ts: chassis serves the module, not its types.
/**
 * The event a consumer listens for. A contract value [TH26]: the detail
 * carries `{ value, label, values, action }` — the value just chosen or
 * removed, its label, every value held (one for a combobox, the whole
 * set for a tag input), and which of 'add' | 'remove' | 'set' it was.
 */
export declare const CHANGE_EVENT = "kp-combobox-change";
/** Fired when the list opens or closes: `{ open }`. */
export declare const OPEN_EVENT = "kp-combobox-open";
export type Matcher = (optionText: string, query: string) => boolean;
/** @typedef {(optionText: string, query: string) => boolean} Matcher */
/** @type {Record<string, Matcher>} */
export declare const MATCHERS: Record<string, Matcher>;
export type ComboboxHandle = {
  element: HTMLElement;
  values: () => string[];
  /**
   * replace the held values (a combobox takes the first)
   */
  set: (values: readonly string[]) => void;
  open: () => void;
  close: () => void;
  /**
   * type on the consumer's behalf
   */
  query: (text: string) => void;
  /**
   * re-read the options after the consumer changed them
   */
  refresh: () => void;
  /**
   * let a layer on top of the combobox (js/usual-picker.js) decide which options show, what the status says and what a choice does [kpadopt6]
   */
  extend: (hook: ComboboxHook | null) => void;
};
export type ComboboxHook = {
  shown?: (option: HTMLElement, query: string, matched: boolean) => boolean;
  after?: (query: string, visible: number) => void;
  status?: (query: string, visible: number) => string | null;
  choose?: (option: HTMLElement) => boolean;
};
/** The handle for an attached combobox. @param {Element} element */
export declare function combobox(element: Element): ComboboxHandle | null;
/**
 * Attach every combobox and tag input under `root`.
 *
 * @param {ParentNode} root
 * @param {{ match?: keyof typeof MATCHERS | Matcher, loop?: boolean, openOnFocus?: boolean, closeOnBlur?: boolean, backspaceRemoves?: boolean, stayOpen?: boolean, maxTags?: number, allowDuplicates?: boolean, debounceMs?: number, emptyRow?: boolean, creatable?: boolean, renderTag?: (value: string, label: string) => HTMLElement, removeGlyph?: string }} [options]
 *   Defaults; per box as data-attributes: `data-kp-match`, `data-kp-loop`, `data-kp-open-on-focus`, `data-kp-close-on-blur`, `data-kp-backspace-removes`, `data-kp-stay-open`, `data-kp-max-tags`, `data-kp-duplicates`, `data-kp-debounce`, `data-kp-empty-row`, `data-kp-creatable`.
 *   A tag input adds from typed text with Enter or a comma [scope-60]: text that names an option (its label, any case) takes that option; other text becomes a tag of its own only with `creatable` (default false, as the React channel's prop).
 *   `backspaceRemoves` (default false since Kenny's second nostromo pass, 2026-09-13): Backspace in an empty field removes the last tag; `data-kp-backspace-removes` opts one box in.
 *   `emptyRow` (default true): a query that matches nothing keeps the list open with a "no results" row — the server's own `[data-kp-combobox-empty]` element inside the list if it wrote one, else one built from the dictionary; `false` closes the list instead, as before 6.1 [gap-11].
 * @returns {(() => void) & { handles: ComboboxHandle[] }} detach
 */
export declare function attachComboboxes(
  root?: ParentNode,
  {
    match,
    loop,
    openOnFocus,
    closeOnBlur,
    backspaceRemoves,
    stayOpen,
    maxTags,
    allowDuplicates,
    debounceMs,
    emptyRow,
    creatable,
    renderTag,
    removeGlyph,
  }?: {
    match?: keyof typeof MATCHERS | Matcher;
    loop?: boolean;
    openOnFocus?: boolean;
    closeOnBlur?: boolean;
    backspaceRemoves?: boolean;
    stayOpen?: boolean;
    maxTags?: number;
    allowDuplicates?: boolean;
    debounceMs?: number;
    emptyRow?: boolean;
    creatable?: boolean;
    renderTag?: (value: string, label: string) => HTMLElement;
    removeGlyph?: string;
  },
): (() => void) & {
  handles: ComboboxHandle[];
};
/**
 * Whether a select gets the drawn list: a single select that is the
 * package's field (`.kp-field__input`) or asks for it (`data-kp-select`),
 * unless it opts out with `data-kp-select="native"`.
 *
 * @param {Element} element
 * @returns {element is HTMLSelectElement}
 */
export declare function drawsSelect(
  element: Element,
): element is HTMLSelectElement;
export type SelectHandle = {
  element: HTMLSelectElement;
  /**
   * the drawn listbox
   */
  list: HTMLElement;
  open: () => void;
  close: () => void;
  /**
   * re-read the options and the value
   */
  refresh: () => void;
};
/** The handle for an attached drawn select. @param {Element} element */
export declare function drawnSelect(element: Element): SelectHandle | null;
/**
 * Lay a drawn list over one select. Returns its detach.
 *
 * @param {HTMLSelectElement} select
 * @param {{ loop?: boolean, typeaheadMs?: number }} [options]
 * @returns {(() => void) & { handle: SelectHandle }}
 */
export declare function attachSelect(
  select: HTMLSelectElement,
  {
    loop,
    typeaheadMs,
  }?: {
    loop?: boolean;
    typeaheadMs?: number;
  },
): (() => void) & {
  handle: SelectHandle;
};
/**
 * Lay a drawn list over every select under `root` that `drawsSelect` accepts:
 * each single `select.kp-field__input` and each `select[data-kp-select]`, but
 * never one marked `data-kp-select="native"` and never a multiple select
 * [scope-54; the default since Kenny's form of 2026-09-13].
 *
 * @param {ParentNode} [root]
 * @param {{ loop?: boolean, typeaheadMs?: number }} [options] Defaults; per select `data-kp-loop`.
 * @returns {(() => void) & { handles: SelectHandle[] }} detach
 */
export declare function attachSelects(
  root?: ParentNode,
  options?: {
    loop?: boolean;
    typeaheadMs?: number;
  },
): (() => void) & {
  handles: SelectHandle[];
};
