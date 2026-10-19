/** A connection banner. */
export declare const CONNECTION = "[data-kp-connection]";
/** Fired when the shown state changed. */
export declare const CONNECTION_STATE_EVENT = "kp-connection-state";
/** Fired when a try should be made. */
export declare const CONNECTION_RETRY_EVENT = "kp-connection-retry";
/** The states. */
export declare const CONNECTION_STATES: readonly [
  "online",
  "offline",
  "reconnecting",
  "back",
  "refused",
];
export type ConnectionState =
  "online" | "offline" | "reconnecting" | "back" | "refused";
export type ConnectionDetail = {
  /**
   * changes waiting to be sent
   */
  pending?: number;
  /**
   * a sentence from the app, for `refused`
   */
  why?: string;
  /**
   * the number of the try under way
   */
  attempt?: number;
};
export type ConnectionHandle = {
  element: HTMLElement;
  state: () => ConnectionState;
  set: (state: ConnectionState, detail?: ConnectionDetail) => void;
};
/** The handle of an attached banner. @param {Element} element */
export declare const connectionOf: (
  element: Element,
) => ConnectionHandle | null;
/**
 * Report the connection from the app.
 * @param {Element} element @param {ConnectionState} state @param {ConnectionDetail} [detail]
 */
export declare function setConnection(
  element: Element,
  state: ConnectionState,
  detail?: ConnectionDetail,
): void;
/**
 * Wire every connection banner under `root`.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachConnection(root?: ParentNode): () => void;
