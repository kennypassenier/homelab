/** The inspectors this module looks after. */
export declare const REQUESTS = "[data-kp-requests]";
/** The header names masked unless the page says otherwise. */
export declare const SECRET_HEADERS: readonly string[];
/** What a masked value is drawn as. */
export declare const MASK = "********";
/** How many requests show before "Show older". */
export declare const REQUESTS_MAX = 20;
/** Fired on the element after a copy: `{ what: 'curl' | 'json' | 'body', masked }`. */
export declare const REQUESTS_COPY_EVENT = "kp-requests-copy";
/** Fired when the empty list's Send test is pressed. */
export declare const REQUESTS_TEST_EVENT = "kp-requests-test";
/** Fired when the failed list's Retry is pressed. */
export declare const REQUESTS_RETRY_EVENT = "kp-requests-retry";
/** How long "Copied" stays [ms]. */
export declare const REQUESTS_COPIED_MS = 1500;
export type CapturedRequest = {
  /**
   * the moment: ISO text or milliseconds
   */
  at?: string | number;
  method: string;
  path: string;
  status?: number;
  headers?: Record<string, string> | [string, string][];
  body?: string;
  /**
   * only the start of the body was kept
   */
  truncated?: boolean;
  /**
   * the size of the whole body
   */
  bodyBytes?: number;
  binary?: boolean;
  /**
   * `test` marks a request the page sent itself
   */
  source?: string;
  /**
   * the full address, for curl (otherwise `origin` + path)
   */
  url?: string;
};
export type InspectorOptions = {
  secretHeaders?: readonly string[];
  secretsInCopy?: "masked" | "masked-only" | "real";
  open?: "failed" | "first" | "none";
  max?: number;
  copyAs?: readonly ("curl" | "json" | "body")[];
  /**
   * put before a path that is not a full address
   */
  origin?: string;
};
export type Memory = {
  list: CapturedRequest[];
  filter: string;
  shown: number;
  /**
   * keys the reader opened
   */
  opened: Set<string>;
  /**
   * keys the reader closed
   */
  closed: Set<string>;
  options: InspectorOptions;
  /**
   * what the page passed to attachRequestInspectors()
   */
  given: InspectorOptions;
};
/** @param {number | undefined} status @returns {'success' | 'info' | 'warning' | 'error' | 'none'} */
export declare function toneOf(
  status: number | undefined,
): "success" | "info" | "warning" | "error" | "none";
/**
 * A request as a curl command.
 * @param {CapturedRequest} request
 * @param {{ mask?: boolean, secretHeaders?: readonly string[], origin?: string }} [options] `mask` draws the secret headers as ********
 * @returns {string}
 */
export declare function toCurl(
  request: CapturedRequest,
  {
    mask,
    secretHeaders,
    origin,
  }?: {
    mask?: boolean;
    secretHeaders?: readonly string[];
    origin?: string;
  },
): string;
/**
 * A request as JSON, with the secret headers masked unless asked.
 * @param {CapturedRequest} request
 * @param {{ mask?: boolean, secretHeaders?: readonly string[] }} [options]
 */
export declare function toJson(
  request: CapturedRequest,
  {
    mask,
    secretHeaders,
  }?: {
    mask?: boolean;
    secretHeaders?: readonly string[];
  },
): string;
/**
 * Give every `[data-kp-requests]` under `root` its card, filter and list.
 * @param {ParentNode} [root]
 * @param {InspectorOptions} [options]
 * @returns {() => void} detach
 */
export declare function attachRequestInspectors(
  root?: ParentNode,
  options?: InspectorOptions,
): () => void;
/**
 * Show a list of requests, newest first. An empty list is the empty state.
 * @param {HTMLElement} el
 * @param {readonly CapturedRequest[]} requests
 */
export declare function setRequests(
  el: HTMLElement,
  requests: readonly CapturedRequest[],
): void;
/**
 * Put the list in one of the card's four states.
 * @param {HTMLElement} el
 * @param {'loading' | 'empty' | 'error' | 'ready'} state
 * @param {{ message?: string }} [options] the sentence of a failed list
 */
export declare function setRequestsState(
  el: HTMLElement,
  state: "loading" | "empty" | "error" | "ready",
  options?: {
    message?: string;
  },
): void;
