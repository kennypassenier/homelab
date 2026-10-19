/** A button this module drives. */
export declare const TOGGLE = "[data-kp-toggle]";
/** Fired on the button before its state changes, cancelable: `{ button, pressed, previous, group, pressedKeys, wait(promise) }`. */
export declare const TOGGLE_EVENT = "kp-toggle";
/** Fired on the button after its state changed, or was rolled back: `{ button, pressed, group, pressedKeys, source }`. */
export declare const TOGGLE_CHANGED_EVENT = "kp-toggle-changed";
/** The looks a toggle may have. */
export declare const TOGGLE_LOOKS: readonly ["button", "chip", "icon"];
/** How a group lets its members be on together. */
export declare const TOGGLE_MODES: readonly [
  "multi",
  "single",
  "single-or-none",
];
/** How long a refusal's word stays on the button, in ms (the flash button's value). */
export declare const TOGGLE_REFUSED_MS = 3000;
export type PressedState = "true" | "false" | "mixed";
export type ToggleOptions = {
  /**
   * how long a refusal's word stays (default 3000)
   */
  refusedMs?: number;
};
export type ToggleHandle = {
  element: HTMLElement;
  state: () => PressedState;
  /**
   * set the state without the event; a group keeps its rules
   */
  set: (
    state: boolean | "mixed",
    options?: {
      silent?: boolean;
    },
  ) => void;
  /**
   * the exact number beside the label, a skeleton, or none
   */
  setCount: (count: number | "loading" | null) => void;
  /**
   * the line under the label while it is on
   */
  setHint: (text: string) => void;
  /**
   * an async answer: it went through
   */
  done: () => void;
  /**
   * an async answer: it was refused; the state goes back and the word shows
   */
  refuse: (message?: string) => void;
};
/**
 * The handle of an attached toggle.
 * @param {Element} el
 * @returns {ToggleHandle | null}
 */
export declare function toggle(el: Element): ToggleHandle | null;
/** The key a member has in `pressedKeys`. @param {HTMLElement} el */
export declare const toggleKey: (el: HTMLElement) => string;
/**
 * The group a toggle belongs to: its members, its mode and its container.
 * @param {HTMLElement} button
 * @returns {{ name: string, members: HTMLElement[], mode: 'multi' | 'single' | 'single-or-none', container: HTMLElement | null, all: HTMLElement | null } | null}
 */
export declare function toggleGroup(button: HTMLElement): {
  name: string;
  members: HTMLElement[];
  mode: "multi" | "single" | "single-or-none";
  container: HTMLElement | null;
  all: HTMLElement | null;
} | null;
/**
 * The keys of every member that is on, the "All" member left out.
 * @param {HTMLElement} anyMember
 * @returns {string[]}
 */
export declare function togglePressedKeys(anyMember: HTMLElement): string[];
/**
 * Give every toggle button under `root` its press, its group and its parts.
 *
 * @param {ParentNode} [root]
 * @param {ToggleOptions} [options]
 * @returns {() => void} detach
 */
export declare function attachToggles(
  root?: ParentNode,
  options?: ToggleOptions,
): () => void;
