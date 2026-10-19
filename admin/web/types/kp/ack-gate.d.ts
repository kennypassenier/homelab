// The kp-themes module chassis-rs serves at /static/kp/js/ack-gate.js (kp-themes 11.0.0),
// copied from kp-themes js/ack-gate.d.ts: chassis serves the module, not its types.
/** The gates this module drives. */
export declare const ACK_GATE = "[data-kp-ack]";
/** Fired on the gate when its tick changes: `{ key, checked, remaining, at }`. */
export declare const ACK_EVENT = "kp-ack-change";
/** Fired on the gate when its Retry is pressed. */
export declare const ACK_RETRY_EVENT = "kp-ack-retry";
export type AckHandle = {
  element: HTMLElement;
  checked: () => boolean;
  set: (checked: boolean) => void;
  setState: (state: "ready" | "loading" | "error" | "empty") => void;
  setLocked: (locked: boolean) => void;
  /**
   * how many gates of this gate's controls are still unticked
   */
  remaining: () => number;
};
/**
 * The handle of an attached gate.
 * @param {Element} el
 * @returns {AckHandle | null}
 */
export declare function ackGate(el: Element): AckHandle | null;
/**
 * Give every acknowledge gate under `root` its tick and guard its controls.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachAckGates(root?: ParentNode): () => void;
