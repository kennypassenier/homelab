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
  /**
   * the release notes as markdown, drawn safely in a fold of the gate; null takes them away
   */
  setNotes: (markdown: string | null) => void;
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
 * Give a gate its release notes as markdown (the host fetched them), drawn safely in a fold under the alert; null takes them away.
 * @param {Element} gate an attached `[data-kp-ack]`
 * @param {string | null} markdown
 */
export declare function setAckNotes(
  gate: Element,
  markdown: string | null,
): void;
/**
 * Give every acknowledge gate under `root` its tick and guard its controls.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachAckGates(root?: ParentNode): () => void;
