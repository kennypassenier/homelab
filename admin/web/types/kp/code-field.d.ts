/** The fields this module looks after. */
export declare const CODE_FIELD = "textarea[data-kp-code-field]";
/** Fired on the textarea, bubbling, when it is time to check the text: `{ value, language, error(line, column, message) }`. */
export declare const CODE_FIELD_CHECK_EVENT = "kp-code-field-check";
/** Fired on the textarea, bubbling, when the field's state changed: `{ state, line, column, message }`, state `ready` or `invalid`. */
export declare const CODE_FIELD_STATE_EVENT = "kp-code-field-state";
/** Fired on the textarea, bubbling, when the caret moved to another line or column: `{ line, column }`. */
export declare const CODE_FIELD_CARET_EVENT = "kp-code-field-caret";
/** The debounce of the check while typing [ms]. */
export declare const CODE_FIELD_CHECK_MS = 300;
/** The most lines the field grows to. */
export declare const CODE_FIELD_MAX_ROWS = 20;
export type CodeFieldHandle = {
  /**
   * the caret's line and column, counted from 1
   */
  position: () => {
    line: number;
    column: number;
  };
  /**
   * mark the problem the page found; `null` takes it back
   */
  setError: (
    line: number | null,
    column?: number | null,
    message?: string,
  ) => void;
  /**
   * ask the page to check the text now
   */
  check: () => void;
  state: () => {
    state: "ready" | "invalid";
    line: number | null;
    column: number | null;
    message: string;
  };
};
/**
 * The line and column (from 1) of an offset in a text.
 * @param {string} text @param {number} offset
 * @returns {{ line: number, column: number }}
 */
export declare function caretPosition(
  text: string,
  offset: number,
): {
  line: number;
  column: number;
};
/**
 * The handle of a field that has been attached.
 * @param {Element} area
 * @returns {CodeFieldHandle | null}
 */
export declare function codeField(area: Element): CodeFieldHandle | null;
/**
 * Mark the problem the page found in a code field: its line in the gutter, "Line 3, column 12: why" under the field, the form held back.
 * `setCodeFieldError(area, null)` takes it back.
 * @param {Element} area an attached `textarea[data-kp-code-field]`
 * @param {number | null} line from 1; null: no problem
 * @param {number | null} [column] from 1
 * @param {string} [message]
 */
export declare function setCodeFieldError(
  area: Element,
  line: number | null,
  column?: number | null,
  message?: string,
): void;
/**
 * Give every `textarea[data-kp-code-field]` under `root` its gutter, readout and error line.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachCodeFields(root?: ParentNode): () => void;
