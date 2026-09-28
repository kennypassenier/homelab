// The kp-themes module chassis-rs serves at /static/kp/js/forms.js (kp-themes 7.2.0),
// copied from kp-themes js/forms.d.ts: chassis serves the module, not its types.
/**
 * Fired when a form passes validation. A contract value [TH26]: the
 * detail carries the FormData and, since KT6, a `done()` that ends the
 * busy state. A consumer may call it, or return nothing and dispatch
 * `DONE_EVENT` on the form themselves — same effect.
 */
export declare const VALID_EVENT = "kp-form-valid";
/** Fired when a submit failed validation: `{ fields, names }`. */
export declare const INVALID_EVENT = "kp-form-invalid";
/** Fired on a field when its validity was checked: `{ valid, message }`. */
export declare const FIELD_EVENT = "kp-field-validity";
/**
 * Ends the busy state the submit button took on `VALID_EVENT` [KT6].
 * Dispatched on the form by `detail.done()`, or by the consumer directly.
 * Nothing dispatches it on its own: a consumer who navigates away on
 * submit must not get back a button that double-sends.
 */
export declare const DONE_EVENT = "kp-form-done";
/**
 * The name a summary line uses for a field: its label, its
 * `aria-label`, or as a last resort its name attribute.
 *
 * @param {HTMLElement} field
 */
export declare function nameOf(field: HTMLElement): string;
/**
 * Show a message on a field — the browser's, or one the server found.
 *
 * @param {HTMLElement} field
 * @param {string} message
 * @param {{ wrapper?: string, invalidClass?: string }} [options]
 */
export declare function showError(
  field: HTMLElement,
  message: string,
  {
    wrapper,
    invalidClass,
  }?: {
    wrapper?: string;
    invalidClass?: string;
  },
): void;
/**
 * Clear what showError put there.
 *
 * @param {HTMLElement} field
 * @param {{ wrapper?: string, invalidClass?: string }} [options]
 */
export declare function clearError(
  field: HTMLElement,
  {
    wrapper,
    invalidClass,
  }?: {
    wrapper?: string;
    invalidClass?: string;
  },
): void;
export type FormHandle = {
  element: HTMLFormElement;
  /**
   * check every field, show what is wrong, and return whether the form is valid
   */
  validate: () => boolean;
  /**
   * the fields currently marked invalid
   */
  invalid: () => HTMLElement[];
  /**
   * show messages by field name — what a server sends back
   */
  errors: (errors: Record<string, string>) => void;
  /**
   * clear every message
   */
  clear: () => void;
  /**
   * end the busy state
   */
  done: () => void;
};
/** The handle for an attached form, for code that did not call attach. @param {Element} element */
export declare function form(element: Element): FormHandle | null;
/**
 * Attach every form under `root`.
 *
 * @param {ParentNode} root
 * @param {{ validateOn?: 'blur' | 'input' | 'submit', revalidateOn?: 'input' | 'blur' | 'none', focusSummary?: boolean, focusFirstInvalid?: boolean, wrapper?: string, invalidClass?: string, summaryHeading?: string, summaryHeadingClass?: string }} [options]
 *   Defaults, each also settable per form as `data-kp-validate-on`, `data-kp-revalidate-on`, `data-kp-focus-summary`.
 * @returns {(() => void) & { handles: FormHandle[] }} detach
 */
export declare function attachForms(
  root?: ParentNode,
  {
    validateOn,
    revalidateOn,
    focusSummary,
    focusFirstInvalid,
    wrapper,
    invalidClass,
    summaryHeading,
    summaryHeadingClass,
  }?: {
    validateOn?: "blur" | "input" | "submit";
    revalidateOn?: "input" | "blur" | "none";
    focusSummary?: boolean;
    focusFirstInvalid?: boolean;
    wrapper?: string;
    invalidClass?: string;
    summaryHeading?: string;
    summaryHeadingClass?: string;
  },
): (() => void) & {
  handles: FormHandle[];
};
/**
 * Give every switch under `root` its two words [gap-11, KT5].
 *
 * The switch itself needs no script — it is a checkbox, and Space flips
 * it natively. What a server cannot write is the dictionary: the On and
 * Off beside the track come from `js/strings.js`, so a consumer's
 * `setStrings()` (or `strings` here) reaches them like every other word.
 * A state element that already has content is left alone — a server that
 * wrote its own words has decided. The stylesheet shows the word that
 * applies; nothing here listens to the checkbox.
 *
 *   <label class="kp-switch">
 *     <input class="kp-switch__input" type="checkbox" role="switch" />
 *     <span class="kp-switch__state" aria-hidden="true"></span>
 *     <span>Night alarms</span>
 *   </label>
 *
 * @param {ParentNode} [root]
 * @param {{ strings?: Record<string, unknown> }} [options]
 * @returns {() => void} detach: removes the words this call put in
 */
export declare function attachSwitches(
  root?: ParentNode,
  {
    strings,
  }?: {
    strings?: Record<string, unknown>;
  },
): () => void;
