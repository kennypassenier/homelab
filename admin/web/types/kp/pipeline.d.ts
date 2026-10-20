/** A board: `section.kp-pipeline[data-kp-pipeline]`. */
export declare const PIPELINE = "[data-kp-pipeline]";
/** Fired on the board, bubbling, when a card is opened: `{ id, card, source: 'pointer' | 'keyboard' }`. */
export declare const PIPELINE_OPEN_EVENT = "kp-pipeline-open";
/** Fired on the board, bubbling, when Try again is pressed. */
export declare const PIPELINE_RETRY_EVENT = "kp-pipeline-retry";
/** Fired on the board, bubbling, when a lane shows more or fewer: `{ lane, open }`. */
export declare const PIPELINE_MORE_EVENT = "kp-pipeline-more";
export type PipelineLane = {
  key: string;
  label: string;
  tone?: 1 | 2 | 3 | 4 | 5;
  closed?: boolean;
  danger?: boolean;
};
export type PipelineCard = {
  id: string;
  /**
   * the lane's key
   */
  lane: string;
  title: string;
  /**
   * the line under the title (a company)
   */
  sub?: string;
  /**
   * a short last line (a date)
   */
  meta?: string;
  /**
   * a short sign, two letters, in the lane's colour
   */
  mark?: string;
  /**
   * a link; without it the card is a button
   */
  href?: string;
  /**
   * filtered out, kept in its place
   */
  dimmed?: boolean;
};
export type PipelineState = "loading" | "ready" | "empty" | "error";
export type Strings = import("./strings.js").Strings;
export type PipelineOptions = {
  strings?: Partial<Strings>;
};
export type PipelineModel = {
  el: HTMLElement;
  strings: Strings;
  lanes: PipelineLane[];
  cards: PipelineCard[];
  state: PipelineState;
  words: string;
  open: Set<string>;
  selected: string | null;
  active: string | null;
  board: HTMLElement;
  closed: HTMLElement;
  plate: HTMLElement;
  plateKey: string;
  live: HTMLElement;
  nodes: Map<string, HTMLElement>;
  busy: import("./busy-card.js").BusyCard | null;
  leaving: Promise<void> | null;
  drawn: boolean;
};
/**
 * Attach every board under `root`.
 * @param {ParentNode} [root]
 * @param {PipelineOptions} [options]
 * @returns {() => void} detach
 */
export declare function attachPipelines(
  root?: ParentNode,
  options?: PipelineOptions,
): () => void;
/**
 * The lanes and the cards. Cards are kept by id, so a refresh keeps a focused card and moves nothing that stayed. A board
 * that was loading is ready from here on. `lanes` may be left out after the first call.
 * @param {Element} el
 * @param {{ lanes?: PipelineLane[], cards: PipelineCard[] }} data
 */
export declare function setPipeline(
  el: Element,
  {
    lanes,
    cards,
  }: {
    lanes?: PipelineLane[];
    cards: PipelineCard[];
  },
): void;
/**
 * The board as a whole: `loading` (the lanes kept, the table's loading card over them), `ready`, `empty` (the state plate
 * over the whole board, `words` its heading) or `error` (the plate with Try again, `words` the reason under its heading).
 * @param {Element} el
 * @param {PipelineState} state
 * @param {string} [words]
 */
export declare function setPipelineState(
  el: Element,
  state: PipelineState,
  words?: string,
): void;
/**
 * Mark the card that is open (`aria-current`), or none.
 * @param {Element} el
 * @param {string | null} id
 */
export declare function setPipelineSelected(
  el: Element,
  id: string | null,
): void;
