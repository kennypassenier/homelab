// The kp-themes module chassis-rs serves at /static/kp/js/chart.js (kp-themes
// 10.0.0); copied from kp-themes js/chart.d.ts.
/** A chart: `data-kp-chart` (a plot) or `data-kp-chart="spark"`. */
export declare const CHART = "[data-kp-chart]";
/** Charts that share one crosshair, one zoom and one range. */
export declare const CHART_GROUP = "[data-kp-chart-group]";
/** Fired on a chart, bubbling, when its sources on change: `detail.on`, indexes (none = all shown). */
export declare const CHART_SELECT_EVENT = "kp-chart-select";
/** Fired on a group, bubbling, when its zoom changes: `detail` `{ from, to }` or null. */
export declare const CHART_ZOOM_EVENT = "kp-chart-zoom";
/** Fired on a group, bubbling, when a range button is pressed, before the charts redraw: `detail` `{ range, span }`. */
export declare const CHART_RANGE_EVENT = "kp-chart-range";
/** The zone a chart prints its times in and aligns its time axis to, unless attachCharts() is given another (rule 52). */
export declare const CHART_TIME_ZONE = "Europe/Brussels";
/** A key figure's 24-hour trend: the spark variant without its head line. */
export declare const TREND_CHART =
  '[data-kp-chart="spark"][data-kp-spark-head="none"]';
export type ChartPoint = [number, number];
export type ChartSeries = {
  label: string;
  /**
   * oldest first
   */
  points?: ChartPoint[];
  /**
   * with `step` and `values`: evenly spaced points, compact in JSON
   */
  start?: number;
  /**
   * ms
   */
  step?: number;
  values?: number[];
  /**
   * the consumer's own colour (`var(--…)`) instead of the next `--chart-n`
   */
  colour?: string;
  /**
   * shown in the legend while no time is under the crosshair
   */
  total?: number;
};
export type ChartEvent = {
  /**
   * ms since the epoch
   */
  at: number;
  label: string;
  tone?: "critical" | "warning" | "info";
  /**
   * offered once the event is pinned
   */
  href?: string;
};
export type ChartUnitKind =
  "percent" | "bytes" | "bytes/s" | "rate" | "celsius" | "count" | "flag";
export type ChartValueWhere =
  "axis" | "tip" | "delta" | "legend" | "readout" | "spark";
export type ChartValueFormat = (
  v: number,
  where: ChartValueWhere,
  chart: ChartData,
) => string | null | undefined;
export type ChartPartKind =
  "source" | "show-all" | "zoom-reset" | "release" | "plot" | "range";
export type ChartDecorateInfo = {
  /**
   * which control: a legend `source`, `show-all`, a zoom chip's `zoom-reset`, a pinned
   * tooltip's `release` (✕), a chart's `plot`, a group's `range` button
   */
  kind: ChartPartKind;
  /**
   * the chart's element (for `range` the group's, for `zoom-reset` the chip)
   */
  host: Element;
  /**
   * the chart's `key` (or `data-kp-key`), or the group element's `data-kp-key`
   */
  key?: string;
  /**
   * `source`: the source's place in `series`
   */
  index?: number;
  /**
   * `source`: the source's label
   */
  label?: string;
  /**
   * `range`: the range, as `data-kp-chart-range` names it
   */
  value?: string;
};
export type ChartDecorate = (
  part: HTMLElement,
  info: ChartDecorateInfo,
) => void;
export type ChartData = {
  /**
   * the chart's name; else the element's `aria-label`
   */
  label?: string;
  /**
   * the chart's stable name for the page (written as `data-kp-key`, passed to `decorate`)
   */
  key?: string;
  /**
   * printed after a value ("bar", "m³/h"); not on the axis
   */
  unit?: string;
  /**
   * the value's kind, printed and scaled the dashboard's way, its unit on
   * the axis too; `unit` and `digits` then do nothing
   */
  unitKind?: ChartUnitKind;
  /**
   * fraction digits of a value; else by its size
   */
  digits?: number;
  /**
   * why there are no readings: shown in the plot, at its height, instead of the lines
   */
  error?: string;
  /**
   * while every source has a single reading, say so under the plot (`chartOnePoint`)
   */
  onePointNote?: boolean;
  series: ChartSeries[];
  events?: ChartEvent[];
  /**
   * the window, when the group's range should not set it
   */
  from?: number;
  to?: number;
  /**
   * the top of the axis; else a round number above the highest value
   */
  yMax?: number;
  /**
   * a dashed line (an alarm level)
   */
  threshold?: number;
  /**
   * sources on top of each other, as areas
   */
  stacked?: boolean;
  /**
   * px; else `--kp-chart-height` on the chart (px or rem), else 168
   */
  height?: number;
};
export type ChartWindow = {
  from: number;
  to: number;
};
export type ChartTimeStyle = "clock" | "tick" | "full" | "range";
export type ChartTimeContext = {
  /**
   * `tick`: the distance between two ticks, ms (a day or more: the ticks are dates)
   */
  stride?: number;
  /**
   * `range`: the zoom's start, ms
   */
  from?: number;
  /**
   * `range`: the zoom's end, ms
   */
  to?: number;
};
export type ChartTimeFormat = (
  ms: number,
  style: ChartTimeStyle,
  ctx: ChartTimeContext,
) => string;
export type ChartOptions = {
  /**
   * any of the dictionary's `chart…` words, for these charts only
   */
  strings?: Partial<Strings>;
  /**
   * the numbers; else the nearest `lang` above each group
   */
  locale?: string;
  /**
   * the IANA zone every printed time is in and the time axis is aligned to;
   * `Europe/Brussels` by default
   */
  timeZone?: string;
  /**
   * prints every time itself; `numericTime(timeZone)` by default
   */
  time?: ChartTimeFormat;
  /**
   * prints values itself, `(v, where, chart) => string`; anything but a
   * string falls back to formatChartValue()
   */
  format?: ChartValueFormat;
  /**
   * called with every control a chart builds, each time it builds it (a
   * rebuilt legend included), so the page can mark it
   */
  decorate?: ChartDecorate;
  /**
   * the moment a trend's axis counts from (`now`, `today`, `yesterday`);
   * `Date.now()` by default
   */
  now?: () => number;
};
export type ChartStrings = ReturnType<typeof chartWords>;
export type Strings = import("./strings.js").Strings;
/**
 * @typedef {[number, number]} ChartPoint time (ms since the epoch), value
 * @typedef {object} ChartSeries
 * @property {string} label
 * @property {ChartPoint[]} [points] oldest first
 * @property {number} [start] with `step` and `values`: evenly spaced points, compact in JSON
 * @property {number} [step] ms
 * @property {number[]} [values]
 * @property {string} [colour] the consumer's own colour (`var(--…)`) instead of the next `--chart-n`
 * @property {number} [total] shown in the legend while no time is under the crosshair
 * @typedef {object} ChartEvent
 * @property {number} at ms since the epoch
 * @property {string} label
 * @property {'critical' | 'warning' | 'info'} [tone]
 * @property {string} [href] offered once the event is pinned
 * @typedef {'percent' | 'bytes' | 'bytes/s' | 'rate' | 'celsius' | 'count' | 'flag'} ChartUnitKind a value's kind:
 *   `bytes` and `bytes/s` (`rate` is the same) in binary steps, B to PiB; `percent` with an axis of 10, 25, 50 or
 *   100 (above 100 only when a value is); `celsius` whole degrees; `count` and `flag` exact numbers, abbreviated
 *   (`12.3k`) on the axis only
 * @typedef {'axis' | 'tip' | 'delta' | 'legend' | 'readout' | 'spark'} ChartValueWhere where a value is printed:
 *   `axis` a y-axis label, `tip` the tooltip's value, `delta` the tooltip's change over the hour before (its size),
 *   `legend` a legend button's value, `readout` what a screen reader hears, `spark` the spark variant's value
 * @typedef {(v: number, where: ChartValueWhere, chart: ChartData) => string | null | undefined} ChartValueFormat
 *   prints a value itself; anything but a string leaves it to the chart
 * @typedef {'source' | 'show-all' | 'zoom-reset' | 'release' | 'plot' | 'range'} ChartPartKind
 * @typedef {object} ChartDecorateInfo
 * @property {ChartPartKind} kind which control: a legend `source`, `show-all`, a zoom chip's `zoom-reset`, a pinned
 *   tooltip's `release` (✕), a chart's `plot`, a group's `range` button
 * @property {Element} host the chart's element (for `range` the group's, for `zoom-reset` the chip)
 * @property {string} [key] the chart's `key` (or `data-kp-key`), or the group element's `data-kp-key`
 * @property {number} [index] `source`: the source's place in `series`
 * @property {string} [label] `source`: the source's label
 * @property {string} [value] `range`: the range, as `data-kp-chart-range` names it
 * @typedef {(part: HTMLElement, info: ChartDecorateInfo) => void} ChartDecorate
 * @typedef {object} ChartData
 * @property {string} [label] the chart's name; else the element's `aria-label`
 * @property {string} [key] the chart's stable name for the page (written as `data-kp-key`, passed to `decorate`)
 * @property {string} [unit] printed after a value ("bar", "m³/h"); not on the axis
 * @property {ChartUnitKind} [unitKind] the value's kind, printed and scaled the dashboard's way, its unit on
 *   the axis too; `unit` and `digits` then do nothing
 * @property {number} [digits] fraction digits of a value; else by its size
 * @property {string} [error] why there are no readings: shown in the plot, at its height, instead of the lines
 * @property {boolean} [onePointNote] while every source has a single reading, say so under the plot (`chartOnePoint`)
 * @property {ChartSeries[]} series
 * @property {ChartEvent[]} [events]
 * @property {number} [from] the window, when the group's range should not set it
 * @property {number} [to]
 * @property {number} [yMax] the top of the axis; else a round number above the highest value
 * @property {number} [threshold] a dashed line (an alarm level)
 * @property {boolean} [stacked] sources on top of each other, as areas
 * @property {number} [height] px; else `--kp-chart-height` on the chart (px or rem), else 168
 * @typedef {{ from: number, to: number }} ChartWindow
 * @typedef {'clock' | 'tick' | 'full' | 'range'} ChartTimeStyle where a time is printed: `clock` the crosshair's
 *   and an event line's time, `tick` a time-axis label, `full` the tooltip's head, a marker's title and the readout,
 *   `range` the zoom chip
 * @typedef {object} ChartTimeContext
 * @property {number} [stride] `tick`: the distance between two ticks, ms (a day or more: the ticks are dates)
 * @property {number} [from] `range`: the zoom's start, ms
 * @property {number} [to] `range`: the zoom's end, ms
 * @typedef {(ms: number, style: ChartTimeStyle, ctx: ChartTimeContext) => string} ChartTimeFormat
 * @typedef {object} ChartOptions
 * @property {Partial<Strings>} [strings] any of the dictionary's `chart…` words, for these charts only
 * @property {string} [locale] the numbers; else the nearest `lang` above each group
 * @property {string} [timeZone] the IANA zone every printed time is in and the time axis is aligned to;
 *   `Europe/Brussels` by default
 * @property {ChartTimeFormat} [time] prints every time itself; `numericTime(timeZone)` by default
 * @property {ChartValueFormat} [format] prints values itself, `(v, where, chart) => string`; anything but a
 *   string falls back to formatChartValue()
 * @property {ChartDecorate} [decorate] called with every control a chart builds, each time it builds it (a
 *   rebuilt legend included), so the page can mark it
 * @property {() => number} [now] the moment a trend's axis counts from (`now`, `today`, `yesterday`);
 *   `Date.now()` by default
 * @typedef {ReturnType<typeof chartWords>} ChartStrings
 * @typedef {import('./strings.js').Strings} Strings
 */
/**
 * The words a chart says, from the dictionary (`chart…` in js/strings.js),
 * with the caller's own on top: `attachCharts(root, { strings })` takes any
 * of the dictionary's keys.
 * @param {Partial<Strings>} [overrides]
 */
export declare function chartWords(overrides?: Partial<Strings>): {
  plot: (label: string) => string;
  spark: (label: string) => string;
  sources: (label: string) => string;
  source: (label: string) => string;
  showAll: string;
  showAllTitle: string;
  hintPointer: string;
  hintTouch: string;
  pinned: string;
  release: string;
  change: string;
  open: string;
  mark: (label: string, time: string) => string;
  zoomed: (from: string, to: string) => string;
  zoomedSpan: (span: string) => string;
  zoomedByEnds: boolean;
  reset: string;
  resetTitle: string;
  up: string;
  down: string;
  same: string;
  now: string;
  empty: string;
  onePoint: string;
  loading: string;
  pinnedOutside: string;
  today: string;
  yesterday: string;
  trendKeys: string;
};
/**
 * The dash of the `i`-th source's line: none for the first five (each its
 * own `--chart-n`), one of three dashes when the colours come round again.
 * @param {number} i
 * @returns {0 | 1 | 2 | 3} 0 = solid
 */
export declare const dashOf: (i: number) => 0 | 1 | 2 | 3;
/**
 * "1h", "24h", "7d", "15m" in ms; anything else null.
 * @param {string | null | undefined} text
 * @returns {number | null}
 */
export declare function parseSpan(
  text: string | null | undefined,
): number | null;
/**
 * The top of the y axis: a round number just above the highest value. For
 * `percent` 10, 25, 50 or 100 (above 100 the round number, so nothing is
 * cut off); for `bytes` and `bytes/s` round in the binary unit the value is
 * printed in (2 GiB, not 1.86).
 * @param {number} v
 * @param {ChartUnitKind} [kind]
 * @returns {number}
 */
export declare function niceMax(v: number, kind?: ChartUnitKind): number;
/**
 * A value as a chart prints it, without a page's own `format`. With a
 * `unitKind`: `percent` one decimal under 10 (`7.5%`, `42%`, `0%`); `bytes`
 * in binary steps, a decimal under 100 of a unit (`512 B`, `1.5 GiB`,
 * `120 MiB`); `bytes/s` (or `rate`) the same per second (`3.2 MiB/s`);
 * `celsius` whole degrees (`54 °C`); `count` and `flag` exact and grouped
 * (`12,345`), on the axis only abbreviated (`12.3k`). The axis carries the
 * unit for a unit kind. Without one: `digits` (else by size) and `unit`
 * after it, not on the axis.
 * @param {number} v
 * @param {ChartValueWhere} [where]
 * @param {Pick<ChartData, 'unit' | 'unitKind' | 'digits'>} [data]
 * @param {string} [locale] the numbers' grouping and decimal sign; `en-GB` by default
 */
export declare function formatChartValue(
  v: number,
  where?: ChartValueWhere,
  data?: Pick<ChartData, "unit" | "unitKind" | "digits">,
  locale?: string,
): string;
/**
 * The index of the point nearest to `t` (the points oldest first).
 * @param {readonly ChartPoint[]} points
 * @param {number} t
 */
export declare function indexAt(
  points: readonly ChartPoint[],
  t: number,
): number;
/**
 * A source's change over the hour before point `i`: its value there minus
 * the value an hour of points earlier (or the first point).
 * @param {readonly ChartPoint[]} points
 * @param {number} i
 */
export declare function hourChange(
  points: readonly ChartPoint[],
  i: number,
): number;
/**
 * The legend's selection after a click on source `i` of `n`: on or off,
 * several allowed; all on is the same as none (everything shown).
 * @param {ReadonlySet<number>} sel
 * @param {number} i
 * @param {number} n
 * @returns {Set<number>}
 */
export declare function toggleSource(
  sel: ReadonlySet<number>,
  i: number,
  n: number,
): Set<number>;
/**
 * The legend's selection after chartSelect(el, index, on): `null` shows
 * every source again; without `on` source `index` toggles, as a click does;
 * with `on` it is put on or off. Null when nothing changes (an index outside
 * `0..n-1`, or a source already as asked).
 * @param {ReadonlySet<number>} sel
 * @param {number | null} index
 * @param {number} n the number of sources
 * @param {boolean} [on]
 * @returns {Set<number> | null}
 */
export declare function nextSelection(
  sel: ReadonlySet<number>,
  index: number | null,
  n: number,
  on?: boolean,
): Set<number> | null;
/**
 * A series' points, whichever way it was given.
 * @param {ChartSeries} series
 * @returns {ChartPoint[]}
 */
export declare function pointsOf(series: ChartSeries): ChartPoint[];
/**
 * The wall-clock date and time of `ms` in `timeZone`, to the minute. An
 * unknown zone throws a RangeError (from Intl).
 * @param {number} ms
 * @param {string} [timeZone]
 * @returns {{ year: number, month: number, day: number, hour: number, minute: number }}
 */
export declare function wallTime(
  ms: number,
  timeZone?: string,
): {
  year: number;
  month: number;
  day: number;
  hour: number;
  minute: number;
};
/**
 * The moment a wall-clock time in `timeZone` names: null when the clock
 * skips it (the hour lost when summer time starts); the earlier of the two
 * when the clock runs it twice (the hour gained when it ends).
 * @param {number} year
 * @param {number} month 1-12
 * @param {number} day
 * @param {number} hour
 * @param {number} minute
 * @param {string} [timeZone]
 * @returns {number | null}
 */
export declare function zonedTime(
  year: number,
  month: number,
  day: number,
  hour: number,
  minute: number,
  timeZone?: string,
): number | null;
/**
 * The built-in way a chart prints a time (rule 52): `clock` `14:05`, `full`
 * `04/10/2026 14:05`, `tick` `14:00` below a day's stride and `04/10/2026`
 * from a day on, `range` `14:00–16:30` within one day of `timeZone` and
 * `03/10/2026 22:00 – 04/10/2026 02:00` across days. 24-hour, day before
 * month, no weekday or month name, in whatever language the page is.
 * @param {string} [timeZone] an IANA zone, `Europe/Brussels` by default
 * @returns {ChartTimeFormat}
 */
export declare function numericTime(timeZone?: string): ChartTimeFormat;
/**
 * The words under a key figure's trend (`data-kp-spark-axis="relative"`):
 * where it starts, as the first point's clock and its day (`14:40
 * yesterday`, `07:00 today`, or `08:10 02/10/2026` when older), and where it
 * ends: `now` while the last point is at most two steps old, else that
 * point's clock (`14:00`, with its day when it is not today's). Fewer than
 * two points: a no-break space each, so the row keeps its height. Days are
 * the wall clock's in `timeZone` (rule 52).
 * @param {readonly ChartPoint[]} points oldest first
 * @param {{ now: number, step?: number, timeZone?: string, time?: ChartTimeFormat,
 *   words?: { now: string, today: string, yesterday: string } }} context `step` is the points' spacing (10 min by
 *   default); `time` prints the clock and an older date (`numericTime(timeZone)` by default); `words` are the
 *   dictionary's `chartNow`, `chartToday` and `chartYesterday` unless given
 * @returns {[string, string]}
 */
export declare function trendAxis(
  points: readonly ChartPoint[],
  {
    now,
    step,
    timeZone,
    time,
    words,
  }: {
    now: number;
    step?: number;
    timeZone?: string;
    time?: ChartTimeFormat;
    words?: {
      now: string;
      today: string;
      yesterday: string;
    };
  },
): [string, string];
/**
 * The distance between two time ticks over a window of `span`: the finest
 * the span allows (15 min up to 2 h, 30 min up to 4 h, 1 h up to 8 h, 3 h up
 * to 16 h, 6 h up to 1.2 days, a day beyond), coarser until no more than
 * `most` strides fit.
 * @param {number} span ms
 * @param {number} most
 */
export declare function tickStride(span: number, most: number): number;
/**
 * The time ticks of a window: round wall-clock times in `timeZone` (six-hour
 * ticks on 00:00, 06:00, 12:00 and 18:00, day ticks at midnight), worked out
 * one by one, so a change of the clock inside the window neither moves them
 * off the round hour nor prints an hour twice. Weeks start on a Monday.
 * @param {number} from ms
 * @param {number} to ms
 * @param {number} most the most strides that fit (the plot's width over 70 px)
 * @param {string} [timeZone]
 * @returns {{ stride: number, ticks: number[] }}
 */
export declare function timeTicks(
  from: number,
  to: number,
  most: number,
  timeZone?: string,
): {
  stride: number;
  ticks: number[];
};
export type ChartState = {
  host: HTMLElement;
  spark: boolean;
  group: ChartGroup;
  draw: () => void;
  cursorAt: (t: number | null, own: boolean) => void;
  setData: (data: ChartData) => void;
  select: (i: number | null, on?: boolean) => boolean;
  lastTime: () => number | null;
  destroy: () => void;
  /**
   * set by attachCharts(): its resize watch off
   */
  unobserve: () => void;
};
export type ChartContext = {
  strings: ChartStrings;
  time: ChartTimeFormat;
  timeZone: string;
  format: ChartValueFormat | undefined;
  decorate: ChartDecorate | undefined;
  now: () => number;
};
/**
 * @typedef {object} ChartState
 * @property {HTMLElement} host
 * @property {boolean} spark
 * @property {ChartGroup} group
 * @property {() => void} draw
 * @property {(t: number | null, own: boolean) => void} cursorAt
 * @property {(data: ChartData) => void} setData
 * @property {(i: number | null, on?: boolean) => boolean} select
 * @property {() => number | null} lastTime
 * @property {() => void} destroy
 * @property {() => void} unobserve set by attachCharts(): its resize watch off
 *
 * @typedef {object} ChartContext what one attachCharts() call gives its charts
 * @property {ChartStrings} strings
 * @property {ChartTimeFormat} time
 * @property {string} timeZone
 * @property {ChartValueFormat | undefined} format
 * @property {ChartDecorate | undefined} decorate
 * @property {() => number} now
 */
/**
 * Give a chart its sources and events (the page's own data). Before the chart
 * is attached the data waits for it; a `<script type="application/json"
 * data-kp-chart-data>` child is the markup way to do the same. Data ends a
 * `data-kp-chart-loading` the element carries. Called again (a live
 * update), the chart keeps its crosshair, pin, zoom, selection and focus;
 * only new sources reset the selection (with a `kp-chart-select` of `[]`).
 * @param {Element} el the `[data-kp-chart]` element
 * @param {ChartData} data
 */
export declare function setChartData(el: Element, data: ChartData): void;
export type TrendData = {
  /**
   * `[[ms, value]…]`, oldest first
   */
  points: ChartPoint[];
  /**
   * the points' spacing, ms (the axis says `now` while the last is at most two steps old)
   */
  step?: number;
  unit?: string;
  unitKind?: ChartUnitKind;
  digits?: number;
};
/**
 * @typedef {object} TrendData a key figure's trend, the short way
 * @property {ChartPoint[]} points `[[ms, value]…]`, oldest first
 * @property {number} [step] the points' spacing, ms (the axis says `now` while the last is at most two steps old)
 * @property {string} [unit]
 * @property {ChartUnitKind} [unitKind]
 * @property {number} [digits]
 */
/**
 * Give a key figure's trend its points: setChartData() with one source, the
 * short way. `null` is loading: the trend stays empty at its final height
 * and the tile around it is `aria-busy` until data comes. An empty
 * `points` is a tile with no trend.
 * @param {Element} figure the `[data-kp-chart="spark"]` element
 * @param {TrendData | null} data
 */
export declare function setTrendData(
  figure: Element,
  data: TrendData | null,
): void;
/**
 * Press legend source `index` of a chart, the way a click does: on or off
 * (a toggle without `on`, else on when `on` is true and off when false), or
 * with `null` every source shown again. Fires `kp-chart-select` when the
 * selection changes. For a page that drives the chart from elsewhere (a
 * table whose row presses its source).
 * @param {Element} el the `[data-kp-chart]` element
 * @param {number | null} index
 * @param {boolean} [on]
 * @returns {boolean} whether the selection changed
 */
export declare function chartSelect(
  el: Element,
  index: number | null,
  on?: boolean,
): boolean;
/**
 * Zoom a group to `zoom` (ms), or with `null` show its whole range again,
 * the way a drag and Reset do; fires `kp-chart-zoom`. `el` is a chart, a
 * group element (or anything inside one), or the document (its charts
 * outside any group).
 * @param {Element | Document} el
 * @param {ChartWindow | null} zoom
 * @returns {boolean} whether a group was found
 */
export declare function chartZoom(
  el: Element | Document,
  zoom: ChartWindow | null,
): boolean;
/**
 * Take one chart away: its drawn parts, its place in its group and its
 * resize watch. The rest of its group draws on; a group left with no chart
 * lets go of its range buttons and zoom chips. The page's data stays, so
 * attaching again draws it.
 * @param {Element} el the `[data-kp-chart]` element
 * @returns {boolean} whether a chart was attached there
 */
export declare function detachChart(el: Element): boolean;
/** Charts that share one crosshair, one zoom and one range, across attachCharts() calls. */
declare class ChartGroup {
  el: HTMLElement | null;
  doc: Document;
  ctx: ChartContext;
  /** @type {Set<ChartState>} */
  charts: Set<ChartState>;
  /** @type {ChartWindow | null} */
  zoom: ChartWindow | null;
  /** The chart the crosshair was last moved on: the only one, unpinned, that shows its tooltip. @type {ChartState | null} */
  owner: ChartState | null;
  span: number | null;
  /** @type {(() => void)[]} */
  stops: (() => void)[];
  /** Every chip given to this group, and its words. @type {Map<HTMLElement, HTMLElement>} */
  chips: Map<HTMLElement, HTMLElement>;
  /**
   * @param {HTMLElement | null} el the `[data-kp-chart-group]`, or none (the document's loose charts)
   * @param {Document} doc
   * @param {ChartContext} ctx the first call's words and times, for the chips
   */
  constructor(el: HTMLElement | null, doc: Document, ctx: ChartContext);
  /** Take every chip on the page that names this group and is not taken yet. */
  bindChips(): void;
  /** @param {HTMLElement} chip */
  bindChip(chip: HTMLElement): void;
  wireRanges(): void;
  /** The newest point of any plot in the group. */
  end(): number | null;
  /** @param {ChartWindow | null} zoom */
  setZoom(zoom: ChartWindow | null): void;
  /**
   * The chip's words for a zoom: its span in the `range` style (dates
   * once it spans more than one day).
   * @param {ChartWindow} zoom
   */
  zoomText(zoom: ChartWindow): string;
  /** @param {number | null} t @param {ChartState | null} owner */
  cursor(t: number | null, owner: ChartState | null): void;
  /** No chart left: let go of the page. */
  stop(): void;
}
/**
 * Draw every `[data-kp-chart]` under `root` and wire it: the charts of one
 * `[data-kp-chart-group]` (or, outside any group, all the others on the
 * page) share their crosshair, zoom and range, also with charts an earlier
 * or a later call attached. A chart's data comes from its JSON child or from
 * setChartData(), before or after this runs.
 * Every time is printed as dd/mm/yyyy HH:mm (or a part of it) on a 24-hour
 * clock in `timeZone`, Europe/Brussels by default, whatever the page's
 * language (rule 52); `time` prints them another way.
 * @param {ParentNode} [root]
 * @param {ChartOptions} [options] `strings`: any of the dictionary's `chart…` words for these charts only;
 *   `locale`: the numbers, else the nearest `lang` above each group; `timeZone`: the zone of every printed time
 *   and of the time axis; `time`: prints every time itself (`(ms, style, ctx) => string`); `format`: prints
 *   values itself (`(v, where, chart) => string`); `decorate`: marks every control a chart builds
 * @returns {() => void} detach: these charts taken away (detachChart() each)
 */
export declare function attachCharts(
  root?: ParentNode,
  options?: ChartOptions,
): () => void;
/**
 * Draw only the key figures' trends under `root` (`TREND_CHART`), with the
 * options attachCharts() takes; for a page that wires its trends apart from
 * its other charts. attachCharts() draws them too.
 * @param {ParentNode} [root]
 * @param {ChartOptions} [options]
 * @returns {() => void} detach
 */
export declare function attachTrendCharts(
  root?: ParentNode,
  options?: ChartOptions,
): () => void;
export {};
