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
};
/**
 * @typedef {object} StatePlateOptions
 * @property {'empty' | 'error'} kind
 * @property {string} title what is the case, in one line
 * @property {string} [text] one sentence more: why, or what fills it
 * @property {string} [actionLabel] the action's words (an error always has one when `onAction` is given)
 * @property {(event: MouseEvent) => void} [onAction] what the action does
 * @property {string} [actionAttribute] a data attribute written on the action (so a container can listen for it by delegation)
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
  }: StatePlateOptions,
): HTMLElement;
