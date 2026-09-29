// The kp-themes module chassis-rs serves at /static/kp/js/wizard.js (kp-themes 8.0.0),
// copied from kp-themes js/wizard.d.ts: chassis serves the module, not its types.
/** Fired when the step changed: `{ step, of, previous, direction }`. A contract value [TH26]. */
export declare const STEP_EVENT = "kp-wizard-step";
/** Fired before a step change, cancelable: `{ from, to, direction }`. preventDefault() holds the wizard where it is. */
export declare const BEFORE_STEP_EVENT = "kp-wizard-before-step";
/** Fired when Next is pressed on the last step: `{ of }`. */
export declare const FINISH_EVENT = "kp-wizard-finish";
export type WizardHandle = {
  element: HTMLElement;
  step: () => number;
  /**
   * resolves to whether the wizard moved
   */
  goTo: (index: number) => Promise<boolean>;
  next: () => Promise<boolean>;
  back: () => Promise<boolean>;
};
/** The handle for an attached wizard. @param {Element} element */
export declare function wizard(element: Element): WizardHandle | null;
/**
 * @param {ParentNode} root
 * @param {{ validate?: boolean, focusStep?: boolean, navigableLabels?: boolean, beforeStep?: (from: number, to: number) => boolean | Promise<boolean> }} [options]
 *   Defaults; each also per wizard: `data-kp-validate="false"`, `data-kp-focus-step="false"`, `data-kp-navigable`.
 * @returns {(() => void) & { handles: WizardHandle[] }} detach
 */
export declare function attachWizards(
  root?: ParentNode,
  {
    validate,
    focusStep,
    navigableLabels,
    beforeStep,
  }?: {
    validate?: boolean;
    focusStep?: boolean;
    navigableLabels?: boolean;
    beforeStep?: (from: number, to: number) => boolean | Promise<boolean>;
  },
): (() => void) & {
  handles: WizardHandle[];
};
