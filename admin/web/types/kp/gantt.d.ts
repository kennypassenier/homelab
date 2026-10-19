// The kp-themes module chassis-rs serves at /static/kp/js/gantt.js (kp-themes 11.0.0),
// copied from kp-themes js/gantt.d.ts: chassis serves the module, not its types.
/** A run timeline: `section.kp-gantt[data-kp-gantt]`. */
export declare const GANTT = "[data-kp-gantt]";
/** Fired on the timeline, bubbling, when a bar was opened: `detail` `{ id, run, source: 'pointer' | 'keyboard' }`. */
export declare const GANTT_OPEN_EVENT = "kp-gantt-open";
/** Fired on the timeline, bubbling, when a range was selected or lifted: `detail` `{ from, to, source }` (epoch ms; null when lifted). */
export declare const GANTT_SELECT_EVENT = "kp-gantt-select";
/** Fired on the timeline, bubbling, when the window moved by a person: `detail` `{ from, to, span, live }` (epoch ms, span in s). */
export declare const GANTT_WINDOW_EVENT = "kp-gantt-window";
/** How often a live window moves, in ms. */
export declare const GANTT_TICK_MS = 1000;
/** The steps an axis chooses from, in seconds. */
export declare const GANTT_STEPS: readonly number[];
export type GanttRunState = "queued" | "running" | "ok" | "failed";
export type GanttRun = {
  /**
   * what the page calls the run; reported by `kp-gantt-open`
   */
  id: string;
  /**
   * the lane's id
   */
  lane: string;
  /**
   * what the run is, one line
   */
  label: string;
  /**
   * epoch ms or an ISO string; for a queued run, the moment it was queued
   */
  start: number | string;
  /**
   * when it ended; ignored for a queued or running run
   */
  end?: number | string | null;
  state: GanttRunState;
  /**
   * more words for the hover card
   */
  detail?: string;
  /**
   * a finished run's colour, `--chart-1` … `--chart-5`, in place of the success colour [kpadopt3]
   */
  series?: 1 | 2 | 3 | 4 | 5;
};
export type GanttLane = {
  id: string;
  label: string;
  href?: string;
  sub?: string;
};
export type GanttPoint = {
  /**
   * what the page calls the moment; reported by `kp-gantt-open`
   */
  id: string;
  /**
   * the lane's id
   */
  lane: string;
  /**
   * epoch ms or an ISO string; a date alone (`2026-10-02`) reads as that day
   */
  at: number | string;
  /**
   * the kind's key, named in the legend by setGanttKinds
   */
  kind: string;
  /**
   * what happened, one line
   */
  label: string;
  /**
   * more words for the hover card
   */
  detail?: string;
  /**
   * still to come: a hollow mark
   */
  planned?: boolean;
};
export type GanttShape = "dot" | "diamond" | "square" | "triangle" | "ring";
export type GanttKind = {
  kind: string;
  label: string;
  shape?: GanttShape;
  series?: 1 | 2 | 3 | 4 | 5;
  tone?: "success" | "warning" | "destructive" | "info" | "muted";
};
export type GanttZone = {
  id?: string;
  lane?: string;
  from: number | string;
  to: number | string;
  label: string;
  tone?: "warning" | "info" | "muted";
};
export type GanttLegendEntry = {
  series: 1 | 2 | 3 | 4 | 5;
  label: string;
};
export type GanttState = "loading" | "ready" | "empty" | "error";
export type Strings = import("./strings.js").Strings;
export type GanttDecorateInfo = {
  kind: "bar" | "live" | "zoom" | "clear";
  host: HTMLElement;
  key?: string;
  id?: string;
};
export type GanttOptions = {
  /**
   * the IANA zone of the axis labels; else `data-kp-time-zone`, else `Europe/Brussels`
   */
  timeZone?: string;
  /**
   * the clock, `Date.now` by default
   */
  now?: () => number;
  /**
   * any of the dictionary's words, for these timelines only
   */
  strings?: Partial<Strings>;
  decorate?: (part: HTMLElement, info: GanttDecorateInfo) => void;
};
export type GanttBar = {
  run: GanttRun;
  s: number;
  e: number | null;
  bar: HTMLButtonElement;
  row: number;
};
export type GanttLaneModel = {
  lane: GanttLane;
  label: HTMLElement;
  track: HTMLElement;
  bars: GanttBar[];
  points: HTMLButtonElement[];
  rows: number;
};
export type GanttModel = {
  el: HTMLElement;
  zone: string;
  now: () => number;
  strings: Strings;
  decorate?: GanttOptions["decorate"];
  base: number;
  span: number;
  live: boolean;
  to: number;
  runs: GanttRun[];
  lanesGiven: GanttLane[];
  lanes: GanttLaneModel[];
  bars: Map<string, HTMLButtonElement>;
  states: Map<string, GanttRunState>;
  state: GanttState;
  active: string | null;
  selected: {
    from: number;
    to: number;
  } | null;
  body: HTMLElement;
  scroll: HTMLElement;
  axis: HTMLElement;
  plot: HTMLElement;
  lines: HTMLElement;
  nowLine: HTMLElement;
  brushBox: HTMLElement;
  selectBox: HTMLElement;
  stateLine: HTMLElement;
  announcer: HTMLElement;
  ranges: HTMLElement;
  liveButton: HTMLButtonElement;
  selection: HTMLElement;
  selectionText: HTMLElement;
  ticks: Map<
    number,
    {
      label: HTMLElement;
      line: HTMLElement;
    }
  >;
  legend: HTMLElement;
  days: boolean;
  dayStop: string | null;
  points: GanttPoint[];
  pointEls: Map<string, HTMLButtonElement>;
  kinds: GanttKind[];
  zones: GanttZone[];
  fit: boolean;
  line: boolean;
  ghosts: HTMLElement[];
  held: number | null;
  busy: import("./busy-card.js").BusyCard | null;
  leaving: Promise<void> | null;
  drawn: boolean;
  timer: number;
  observer: ResizeObserver | null;
  drag: {
    a: number;
    b: number;
  } | null;
  dragMoved: boolean;
  tickKey: string;
};
/**
 * The seconds a range word names: `90m`, `6h`, `24h`, `7d`. NaN when it names nothing.
 * @param {string} word
 * @returns {number}
 */
export declare function rangeSeconds(word: string): number;
/**
 * The step an axis of `span` seconds and `room` px uses: the smallest of
 * GANTT_STEPS that leaves a label at least `gap` px of its own.
 * @param {number} span
 * @param {number} room
 * @param {number} [gap]
 * @returns {number}
 */
export declare function axisStep(
  span: number,
  room: number,
  gap?: number,
): number;
/**
 * Give every run a row of its lane: the row it had when it is still free,
 * else the first free one. A run that is not ended (`e` null) is in the way
 * until the end of time, so a growing run never lands on a later one.
 *
 * @param {{ id: string, s: number, e: number | null }[]} runs in any order, seconds
 * @param {Map<string, number>} [before] the rows the runs had
 * @param {number} [pad] the seconds two runs keep between them
 * @returns {{ rows: Map<string, number>, count: number }}
 */
export declare function packRows(
  runs: {
    id: string;
    s: number;
    e: number | null;
  }[],
  before?: Map<string, number>,
  pad?: number,
): {
  rows: Map<string, number>;
  count: number;
};
/**
 * Attach every run timeline under `root`.
 * @param {ParentNode} [root]
 * @param {GanttOptions} [options]
 * @returns {() => void} detach
 */
export declare function attachGantts(
  root?: ParentNode,
  options?: GanttOptions,
): () => void;
/**
 * The runs and the lanes. Repaints in place, by id: a bar that is still there
 * keeps its node, its focus and its row, so a live update never moves a
 * reader. A timeline that was loading is ready from here on. `lanes` may be
 * left out: a run's `lane` is then its own label, in the order first seen.
 * @param {Element} el
 * @param {GanttRun[]} runs
 * @param {GanttLane[]} [lanes]
 */
export declare function setGanttRuns(
  el: Element,
  runs: GanttRun[],
  lanes?: GanttLane[],
): void;
/**
 * The timeline as a whole: `loading` (ghost lanes at their final size),
 * `ready`, `empty` or `error` (`words` over the plot).
 * @param {Element} el
 * @param {GanttState} state
 * @param {string} [words]
 */
export declare function setGanttState(
  el: Element,
  state: GanttState,
  words?: string,
): void;
/**
 * Set the window; no event, the page asked. `span` in seconds; `live: false`
 * with `to` (epoch ms) holds the window there.
 * @param {Element} el
 * @param {{ span?: number, live?: boolean, to?: number }} window
 */
export declare function setGanttWindow(
  el: Element,
  window: {
    span?: number;
    live?: boolean;
    to?: number;
  },
): void;
/**
 * Show a selected range (epoch ms) or none (null); no event, the page asked.
 * @param {Element} el
 * @param {{ from: number, to: number } | null} selection
 */
export declare function setGanttSelection(
  el: Element,
  selection: {
    from: number;
    to: number;
  } | null,
): void;
/**
 * Name the series of the runs in the legend, after the states [kpadopt3]: a swatch in the series' colour and its words per
 * entry, in the order given; none takes them away again. The states stay as they are.
 * @param {Element} el
 * @param {GanttLegendEntry[]} entries
 */
export declare function setGanttLegend(
  el: Element,
  entries: GanttLegendEntry[],
): void;
/**
 * The moments on the timeline [kpadopt4]: each point a mark in its lane, in the colour and shape of its kind. Repaints in
 * place, by id, as setGanttRuns does; a timeline that was loading is ready from here on. `lanes` may be left out, as there.
 * @param {Element} el
 * @param {GanttPoint[]} points
 * @param {GanttLane[]} [lanes]
 */
export declare function setGanttPoints(
  el: Element,
  points: GanttPoint[],
  lanes?: GanttLane[],
): void;
/**
 * The kinds of the points, named in the legend in the order given [kpadopt4]: `shape` (`dot` by default, `diamond`,
 * `square`, `triangle`, `ring`) and a colour, `series` 1 to 5 (`--chart-N`) or `tone`. A timeline with points and no runs
 * leaves the run states out of its legend.
 * @param {Element} el
 * @param {GanttKind[]} kinds
 */
export declare function setGanttKinds(el: Element, kinds: GanttKind[]): void;
/**
 * The stretches under the marks [kpadopt4]: with `lane`, a thin band in that lane (waiting since the last contact); without,
 * a band behind every lane (the days a follow-up is due), named in the legend. `tone` `warning` (default), `info`, `muted`.
 * @param {Element} el
 * @param {GanttZone[]} zones
 */
export declare function setGanttZones(el: Element, zones: GanttZone[]): void;
