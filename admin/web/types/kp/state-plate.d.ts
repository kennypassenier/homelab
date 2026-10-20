// The kp-themes module chassis-rs serves at /static/kp/js/state-plate.js (kp-themes 11.1.1),
// copied from kp-themes js/state-plate.d.ts: chassis serves the module, not its types.
export type StatePlateOptions = {
  kind: "empty" | "error";
  /**
   * what is the case, in one line
   */
  title: string;
  /**
   * one sentence more: why, or what fills it
   */
  text?: string;
  /**
   * the action's words (an error always has one when `onAction` is given)
   */
  actionLabel?: string;
  /**
   * what the action does
   */
  onAction?: (event: MouseEvent) => void;
  /**
   * a data attribute written on the action (so a container can listen for it by delegation)
   */
  actionAttribute?: string;
  /**
   * a picture over the title: a node, or a glyph (aria-hidden either way)
   */
  art?: Node | string | null;
  /**
   * an element that already stands where the plate goes (a part a theme dresses by its own class):
   * it becomes the plate, keeping its classes, instead of a new element being built [r6r8-1019]
   */
  into?: HTMLElement;
};
/**
 * @typedef {object} StatePlateOptions
 * @property {'empty' | 'error'} kind
 * @property {string} title what is the case, in one line
 * @property {string} [text] one sentence more: why, or what fills it
 * @property {string} [actionLabel] the action's words (an error always has one when `onAction` is given)
 * @property {(event: MouseEvent) => void} [onAction] what the action does
 * @property {string} [actionAttribute] a data attribute written on the action (so a container can listen for it by delegation)
 * @property {Node | string | null} [art] a picture over the title: a node, or a glyph (aria-hidden either way)
 * @property {HTMLElement} [into] an element that already stands where the plate goes (a part a theme dresses by its own class):
 *   it becomes the plate, keeping its classes, instead of a new element being built [r6r8-1019]
 */
/**
 * Build an empty or failed plate.
 * @param {Document} doc
 * @param {StatePlateOptions} options
 * @returns {HTMLElement}
 */
export declare function statePlate(
  doc: Document,
  {
    kind,
    title,
    text,
    actionLabel,
    onAction,
    actionAttribute,
    art,
    into,
  }: StatePlateOptions,
): HTMLElement;
export type HostPlateOptions = {
  kind: "empty" | "error";
  /**
   * what is the case, in one line
   */
  title: string;
  /**
   * one sentence more; an error without one says the package's `stateFailedText`
   */
  text?: string;
  /**
   * a picture over the title
   */
  art?: Node | string | null;
  /**
   * the event an error's Retry fires on `host` (bubbling): `kp-<component>-retry`; without it no Retry
   */
  retryEvent?: string;
  /**
   * the Retry's words (default `stateRetry`)
   */
  retryLabel?: string;
  /**
   * an element that becomes the plate (see StatePlateOptions)
   */
  into?: HTMLElement;
};
/**
 * @typedef {object} HostPlateOptions
 * @property {'empty' | 'error'} kind
 * @property {string} title what is the case, in one line
 * @property {string} [text] one sentence more; an error without one says the package's `stateFailedText`
 * @property {Node | string | null} [art] a picture over the title
 * @property {string} [retryEvent] the event an error's Retry fires on `host` (bubbling): `kp-<component>-retry`; without it no Retry
 * @property {string} [retryLabel] the Retry's words (default `stateRetry`)
 * @property {HTMLElement} [into] an element that becomes the plate (see StatePlateOptions)
 */
/**
 * The plate of a data component's host: an empty or failed area drawn the one way, with its Retry as an event on the host, so a
 * page listens to `kp-<component>-retry` as it does on every other component. The caller puts it where the data was
 * (`host.replaceChildren(plate)`, or into the state box it keeps) and lets js/footprint.js hold the loaded height.
 * @param {HTMLElement} host
 * @param {HostPlateOptions} options
 * @returns {HTMLElement}
 */
export declare function hostStatePlate(
  host: HTMLElement,
  { kind, title, text, art, retryEvent, retryLabel, into }: HostPlateOptions,
): HTMLElement;
