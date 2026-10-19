/** A transfer row. */
export declare const TRANSFER = "[data-kp-transfer]";
/** Fired when a transfer finished: `{ total, ms }`. */
export declare const TRANSFER_DONE_EVENT = "kp-transfer-done";
/** Fired when Retry is pressed on a failed transfer. */
export declare const TRANSFER_RETRY_EVENT = "kp-transfer-retry";
/** Fired when a finished transfer left the list after its linger. */
export declare const TRANSFER_LEAVE_EVENT = "kp-transfer-leave";
export type TransferSample = {
  /**
   * bytes so far
   */
  done?: number;
  /**
   * bytes in all; left out when unknown
   */
  total?: number;
  /**
   * the moment of this report, ms since the epoch (default: now)
   */
  at?: number;
  /**
   * the file's name
   */
  name?: string;
  /**
   * why it failed (the app's own sentence); ends the transfer
   */
  failed?: string;
};
export type TransferState = {
  samples: {
    t: number;
    done: number;
  }[];
  /**
   * the first report
   */
  start: number;
  /**
   * when `done` last grew
   */
  lastMove: number;
  lastDone: number;
  total: number | null;
  phase: "running" | "done" | "failed";
  /**
   * the highest announced step, 0 to 4
   */
  announced: number;
  now: () => number;
  leaveTimer: number;
};
/**
 * Moving-average speed in bytes per second over the last `windowS` seconds of samples; null with fewer than two.
 * @param {{ t: number, done: number }[]} samples oldest first
 * @param {number} windowS
 * @returns {number | null}
 */
export declare function transferRate(
  samples: {
    t: number;
    done: number;
  }[],
  windowS?: number,
): number | null;
/**
 * The time left in human words, coarse so it does not shimmer: exact under 10 s, to 5 s under a minute, to the minute under
 * an hour.
 * @param {number} seconds
 * @returns {string}
 */
export declare function transferEta(seconds: number): string;
/**
 * Report a transfer's progress. The first report starts it; a report whose `done` reaches `total` finishes it.
 * @param {HTMLElement} row a `[data-kp-transfer]`
 * @param {TransferSample} sample
 * @param {{ now?: () => number }} [options] a clock for the stall check (default: the real one)
 */
export declare function setTransfer(
  row: HTMLElement,
  sample: TransferSample,
  {
    now,
  }?: {
    now?: () => number;
  },
): void;
/**
 * Take a transfer back to before its first report (a new run in the same row).
 * @param {HTMLElement} row
 */
export declare function resetTransfer(row: HTMLElement): void;
/**
 * Prepare every transfer row under `root` (its parts and ARIA), waiting for the app's first report.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachTransfers(root?: ParentNode): () => void;
