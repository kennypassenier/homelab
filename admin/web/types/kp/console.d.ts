/** The consoles this module looks after. */
export declare const CONSOLE = "[data-kp-console]";
/** Fired before a command goes to the app, cancelable: `{ command, context }`. */
export declare const CONSOLE_RUN_EVENT = "kp-console-run";
/** Fired when a block is made or changes: `{ block }`. */
export declare const CONSOLE_BLOCK_EVENT = "kp-console-block";
/** Fired by the Retry of a console that could not reach its host. */
export declare const CONSOLE_RETRY_EVENT = "kp-console-retry";
/** The states a block can be in. */
export declare const CONSOLE_STATES: readonly string[];
/** The default number of recalled lines. */
export declare const CONSOLE_HISTORY = 50;
/** The most output bytes kept per block. */
export declare const CONSOLE_MAX_BYTES = 262144;
export type ConsoleChange = {
  state?:
    | "sending"
    | "queued"
    | "running"
    | "ok"
    | "failed"
    | "refused"
    | "cancelled"
    | "lost";
  /**
   * the exit code
   */
  code?: number;
  /**
   * replaces the output
   */
  output?: string;
  /**
   * adds to the output
   */
  append?: string;
  /**
   * a refusal's what
   */
  error?: string;
  /**
   * a refusal's what now
   */
  remedy?: string;
};
export type ConsoleJob = {
  update: (handler: (change: ConsoleChange) => void) => void;
  cancel?: () => void;
};
export type ConsoleRun = (
  command: string,
  context: string,
) => ConsoleJob | Promise<ConsoleJob>;
export type ConsoleBlock = {
  id: string;
  command: string;
  context: string;
  state: ConsoleChange["state"];
  code?: number;
  output: string;
  error?: string;
  remedy?: string;
  startedAt: number;
  endedAt?: number;
  collapsed: boolean;
  truncated?: boolean;
  total?: number;
};
export type ConsoleState = {
  run: ConsoleRun | null;
  complete: ((prefix: string) => string[] | Promise<string[]>) | null;
  blocks: ConsoleBlock[];
  jobs: Map<string, ConsoleJob>;
  history: string[];
  at: number;
  draft: string;
  spec: {
    state?: string;
    message?: string;
  };
  context: {
    options: {
      value: string;
      label: string;
    }[];
    value: string;
    state: string;
    label: string;
  };
  card: import("./busy-card.js").BusyCard | null;
  tick: number;
  pending: string | null;
  pendingTimer: number;
  off: (() => void)[];
  showAll: boolean;
  said: string;
};
/**
 * Give a console its `run`.
 * @param {HTMLElement} root @param {ConsoleRun} run
 */
export declare function setConsoleRun(root: HTMLElement, run: ConsoleRun): void;
/**
 * Give the Tab key its completions.
 * @param {HTMLElement} root @param {(prefix: string) => string[] | Promise<string[]>} complete
 */
export declare function setConsoleComplete(
  root: HTMLElement,
  complete: (prefix: string) => string[] | Promise<string[]>,
): void;
/**
 * Fill the context select (the container to run in).
 * @param {HTMLElement} root @param {{ options?: { value: string, label: string }[], value?: string, state?: 'loading' | 'ready', label?: string }} context
 */
export declare function setConsoleContext(
  root: HTMLElement,
  context: {
    options?: {
      value: string;
      label: string;
    }[];
    value?: string;
    state?: "loading" | "ready";
    label?: string;
  },
): void;
/**
 * The console's state: `loading`, `error` (the host could not be reached) or `ready`.
 * @param {HTMLElement} root @param {{ state?: 'ready' | 'loading' | 'error', message?: string, hint?: string }} spec
 */
export declare function setConsole(
  root: HTMLElement,
  spec: {
    state?: "ready" | "loading" | "error";
    message?: string;
    hint?: string;
  },
): void;
/** The blocks as they stand. @param {HTMLElement} root @returns {ConsoleBlock[]} */
export declare function consoleBlocks(root: HTMLElement): ConsoleBlock[];
/** Run a line as if it was typed (a "Run again" or a page's own button). @param {HTMLElement} root @param {string} command */
export declare function runConsoleCommand(
  root: HTMLElement,
  command: string,
): Promise<boolean>;
/** Clear the screen; the blocks come back for a while (Undo). @param {HTMLElement} root @param {{ undo?: boolean }} [options] */
export declare function clearConsole(
  root: HTMLElement,
  {
    undo,
  }?: {
    undo?: boolean;
  },
): void;
/**
 * Wire every console under `root`.
 * @param {ParentNode} [root]
 * @param {{ run?: ConsoleRun, complete?: (prefix: string) => string[] | Promise<string[]> }} [options]
 * @returns {() => void} detach
 */
export declare function attachConsoles(
  root?: ParentNode,
  options?: {
    run?: ConsoleRun;
    complete?: (prefix: string) => string[] | Promise<string[]>;
  },
): () => void;
