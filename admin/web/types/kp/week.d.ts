/** A week agenda: `section.kp-week[data-kp-week]`. */
export declare const WEEK = "[data-kp-week]";
/** Fired on the agenda, bubbling, when an item was opened: `detail` `{ id, event, source: 'pointer' | 'keyboard' }`. */
export declare const WEEK_OPEN_EVENT = "kp-week-open";
/** Fired on the agenda, bubbling, when a week button moved the range: `detail` `{ start: 'YYYY-MM-DD', source: 'nav' }`. */
export declare const WEEK_RANGE_EVENT = "kp-week-range";
/** How often the now-line and the past marks move, in ms. */
export declare const WEEK_TICK_MS = 30000;
export type WeekEventState = "next" | "off" | "skipped" | "reference";
export type WeekEvent = {
  /**
   * what the page calls the item; reported by `kp-week-open`
   */
  id: string;
  /**
   * the moment: epoch ms, or an ISO string with its offset
   */
  start: number | string;
  /**
   * what it is, one line (clipped with an ellipsis)
   */
  label: string;
  /**
   * the colour, from `--chart-1`; 1 when absent
   */
  series?: 1 | 2 | 3 | 4 | 5;
  /**
   * absent for a plain item
   */
  state?: WeekEventState;
  /**
   * the hover card's words, and the end of the item's accessible name
   */
  detail?: string;
};
export type WeekState = "loading" | "ready" | "empty" | "error";
export type Strings = import("./strings.js").Strings;
export type WeekDecorateInfo = {
  kind: "pill" | "more" | "prev" | "next" | "today";
  host: HTMLElement;
  key?: string;
  id?: string;
};
export type WeekOptions = {
  /**
   * the IANA zone that decides which day an item is on; else `data-kp-time-zone`, else `Europe/Brussels`
   */
  timeZone?: string;
  /**
   * the weekday names; else `data-kp-locale`, else the nearest `lang`
   */
  locale?: string;
  /**
   * the clock, `Date.now` by default
   */
  now?: () => number;
  /**
   * any of the dictionary's words, for these agendas only
   */
  strings?: Partial<Strings>;
  decorate?: (part: HTMLElement, info: WeekDecorateInfo) => void;
};
export type WeekItem = {
  event: WeekEvent;
  ms: number;
  at: number;
  hhmm: string;
  button: HTMLButtonElement;
  day: string;
};
export type WeekDay = {
  iso: string;
  cell: HTMLElement;
  head: HTMLElement;
  track: HTMLElement;
  items: WeekItem[];
  now: HTMLElement;
  more: HTMLElement | null;
  moreButton: HTMLButtonElement | null;
  hidden: WeekItem[];
};
export type WeekModel = {
  el: HTMLElement;
  zone: string;
  locale: string;
  now: () => number;
  strings: Strings;
  decorate?: WeekOptions["decorate"];
  start: string;
  days: number;
  events: WeekEvent[];
  state: WeekState;
  active: string | null;
  grid: HTMLElement;
  axis: HTMLElement;
  cols: WeekDay[];
  frame: HTMLElement;
  busy: HTMLElement | null;
  leaving: Promise<unknown> | null;
  held: number[] | null;
  stateLine: HTMLElement;
  title: HTMLElement | null;
  today: HTMLButtonElement | null;
  legend: HTMLElement;
  motion: {
    gone: boolean;
    stop: (() => void) | null;
    asked: boolean;
  };
  pills: Map<string, HTMLButtonElement>;
  timer: number;
  observer: ResizeObserver | null;
  mode: "grid" | "list";
  upcoming: {
    n: number;
    on: boolean;
    list: HTMLOListElement | null;
    rows: Map<string, HTMLLIElement>;
    items: WeekItem[];
  };
};
/**
 * Where the pills of one column stand: each at its time of day, pushed down
 * until it clears the one above, and pulled back up where the stack would run
 * out of the bottom. Pure; every number is px of the column's height.
 *
 * @param {number[]} wants each pill's wanted top, as a share of the column (0 to 1), in time order
 * @param {number} height the column's height
 * @param {number} pill one pill's height
 * @param {number} [gap] the space kept between two pills
 * @returns {number[]} each pill's top
 */
export declare function stackPills(
  wants: number[],
  height: number,
  pill: number,
  gap?: number,
): number[];
/**
 * How many pills a column of `height` holds at one pill's height.
 * @param {number} height
 * @param {number} pill
 * @param {number} [gap]
 * @returns {number}
 */
export declare function pillCapacity(
  height: number,
  pill: number,
  gap?: number,
): number;
/**
 * Attach every week agenda under `root`.
 * @param {ParentNode} [root]
 * @param {WeekOptions} [options]
 * @returns {() => void} detach
 */
export declare function attachWeeks(
  root?: ParentNode,
  options?: WeekOptions,
): () => void;
/**
 * The items of the range. Repaints in place, by id: a pill that is still there
 * keeps its node, its focus and its place in the tab order, so a live update
 * never moves a reader. A week that was loading is ready from here on.
 * @param {Element} el
 * @param {WeekEvent[]} events
 */
export declare function setWeekEvents(el: Element, events: WeekEvent[]): void;
/**
 * The agenda as a whole: `loading` (the data table's busy panel over the empty days), `ready`,
 * `empty` or `error` (`words` over the grid; an error says it in the page's
 * words).
 * @param {Element} el
 * @param {WeekState} state
 * @param {string} [words]
 */
export declare function setWeekState(
  el: Element,
  state: WeekState,
  words?: string,
): void;
/**
 * Move the range to start on `iso` (`YYYY-MM-DD`); no event, the page asked.
 * The items stay until the page gives the new ones.
 * @param {Element} el
 * @param {string} iso
 */
export declare function setWeekStart(el: Element, iso: string): void;
/**
 * The legend under the agenda: a swatch and its words per series, in the
 * order given; none hides it.
 * @param {Element} el
 * @param {{ series: 1 | 2 | 3 | 4 | 5, label: string }[]} entries
 */
export declare function setWeekLegend(
  el: Element,
  entries: {
    series: 1 | 2 | 3 | 4 | 5;
    label: string;
  }[],
): void;
