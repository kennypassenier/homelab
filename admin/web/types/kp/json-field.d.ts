/** The fields this module looks after. */
export declare const JSON_FIELD = "textarea[data-kp-json]";
/** Fired on the textarea, bubbling, when its state changed: `{ state, line, column, message, keys }`, state `empty` | `valid` | `invalid` | `text`. */
export declare const JSON_STATE_EVENT = "kp-json-state";
/**
 * Fired on the textarea, bubbling, for a text that parses: `{ value, text, schema, refuse(why) }`. A handler that calls `refuse` makes the field invalid with that sentence.
 */
export declare const JSON_VALIDATE_EVENT = "kp-json-validate";
/** The debounce of the check while typing [ms]. */
export declare const JSON_CHECK_MS = 300;
/** The most lines the field grows to. */
export declare const JSON_MAX_ROWS = 20;
/** How long "Copied" stays on the button [ms]. */
export declare const JSON_COPIED_MS = 1500;
export type JsonFieldOptions = {
  mode?: "strict" | "allow-text";
  check?: "input" | "blur";
  format?: "button" | "on-blur" | "off";
  indent?: "2" | "4" | "tab";
  gutter?: boolean;
  wrap?: boolean;
};
export type JsonFieldHandle = {
  state: () => "empty" | "valid" | "invalid" | "text";
  check: () => void;
  /**
   * lay it out; false when it is not JSON
   */
  format: () => boolean;
  minify: () => boolean;
  /**
   * the parsed value, or `undefined`
   */
  value: () => unknown;
};
/**
 * What is wrong with `text`, and where: the engines word it differently.
 * @param {string} text
 * @returns {{ ok: true, value: unknown } | { ok: false, why: string, line: number | null, column: number | null }}
 */
export declare function explainJson(text: string):
  | {
      ok: true;
      value: unknown;
    }
  | {
      ok: false;
      why: string;
      line: number | null;
      column: number | null;
    };
/**
 * The handle of a field that has been attached.
 * @param {Element} area
 * @returns {JsonFieldHandle | null}
 */
export declare function jsonField(area: Element): JsonFieldHandle | null;
/**
 * Give every `textarea[data-kp-json]` under `root` its checking, gutter and tools.
 * @param {ParentNode} [root]
 * @param {JsonFieldOptions} [options]
 * @returns {() => void} detach
 */
export declare function attachJsonFields(
  root?: ParentNode,
  options?: JsonFieldOptions,
): () => void;
