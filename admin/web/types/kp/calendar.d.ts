// The kp-themes module chassis-rs serves at /static/kp/js/calendar.js (kp-themes 10.1.1),
// copied from kp-themes js/calendar.d.ts: chassis serves the module, not its types.
/** A month heatmap: `section.kp-calendar[data-kp-calendar]`. */
export declare const CALENDAR = "[data-kp-calendar]";
/** Fired on the calendar, bubbling, when a day is picked: `detail` `{ date: 'YYYY-MM-DD', source: 'pointer' | 'keyboard' }`. */
export declare const CALENDAR_PICK_EVENT = "kp-calendar-pick";
/** Fired on the calendar, bubbling, when a person moves it to another month: `detail` `{ year, month }` (month 1-12). */
export declare const CALENDAR_MONTH_EVENT = "kp-calendar-month";
export type CalendarTone =
  "ok" | "warn" | "bad" | "muted" | "future" | "before" | "loading" | "none";
export type CalendarDay = {
  tone: CalendarTone;
  count?: string;
  label: string;
};
export type CalendarState = "loading" | "ready" | "empty" | "error";
export type Strings = import("./strings.js").Strings;
export type CalendarDecorateInfo = {
  kind: "day" | "month-prev" | "month-next" | "today";
  host: HTMLElement;
  key?: string;
  index?: number;
  label?: string;
  value?: string;
};
export type CalendarOptions = {
  /**
   * the IANA zone that decides which day is today; else the calendar's `data-kp-time-zone`,
   * else `Europe/Brussels`
   */
  timeZone?: string;
  /**
   * the month and weekday names and the first day of the week; else the calendar's
   * `data-kp-locale`, else the nearest `lang` above it
   */
  locale?: string;
  /**
   * 0 (Sunday) to 6; else the calendar's `data-kp-week-starts-on`, else the locale's
   */
  weekStartsOn?: number;
  /**
   * the clock, `Date.now` by default
   */
  now?: () => number;
  /**
   * any of the dictionary's words, for these calendars only
   */
  strings?: Partial<Strings>;
  /**
   * called with every control a calendar
   * builds, each time it builds it, so the page can mark it
   */
  decorate?: (part: HTMLElement, info: CalendarDecorateInfo) => void;
};
export type CalendarModel = {
  el: HTMLElement;
  year: number;
  month: number;
  selected: string | null;
  focus: string | null;
  days: Record<string, CalendarDay>;
  state: CalendarState;
  timeZone: string;
  firstDay: number;
  now: () => number;
  strings: Strings;
  months: string[];
  decorate?: CalendarOptions["decorate"];
  title: HTMLElement;
  stateLine: HTMLElement;
  grid: HTMLTableElement;
  cells: {
    td: HTMLTableCellElement;
    button: HTMLButtonElement;
    num: HTMLElement;
    count: HTMLElement;
    pad: HTMLElement;
  }[];
  legend: HTMLElement;
  shownMonth: string;
  busyText?: HTMLElement;
};
/**
 * The day a moment falls on in `timeZone`, as `YYYY-MM-DD`: the wall clock of
 * that zone decides, so 22:30 UTC on 4 October is the 5th in Brussels.
 * @param {number} ms
 * @param {string} [timeZone]
 * @returns {string}
 */
export declare function dayKey(ms: number, timeZone?: string): string;
/** A date as a person reads it (rule 52): `2026-10-04` → `04/10/2026`. @param {string} iso */
export declare const formatDayKey: (iso: string) => string;
/**
 * The day `days` after `iso` (before, when negative), on the calendar: no
 * clock is involved, so a change to summer time moves nothing.
 * @param {string} iso `YYYY-MM-DD`
 * @param {number} days
 * @returns {string}
 */
export declare function shiftDay(iso: string, days: number): string;
/**
 * The same day `months` later (earlier, when negative), clamped to that
 * month's last day: 31/01 plus a month is 28/02.
 * @param {string} iso `YYYY-MM-DD`
 * @param {number} months
 * @returns {string}
 */
export declare function shiftMonth(iso: string, months: number): string;
/**
 * The 42 days a month's grid shows: six weeks from the first day of the week
 * on or before the month's first, so every month is six rows tall.
 * @param {number} year
 * @param {number} month 1-12
 * @param {number} [firstDay] the week's first day, 0 (Sunday) to 6; Monday by default
 * @returns {string[]} `YYYY-MM-DD`, oldest first
 */
export declare function monthCells(
  year: number,
  month: number,
  firstDay?: number,
): string[];
/**
 * Build every month heatmap under `root`: the month's title between ‹ Prev
 * and Next ›, Today, a line for the state, a six-week grid (one tab stop, the
 * keys of a date grid) and the legend. Today is the day in `timeZone`. Per
 * calendar: `data-kp-calendar-month="YYYY-MM"` (the month shown first, else
 * today's), `data-kp-calendar-selected="YYYY-MM-DD"`, `data-kp-time-zone`,
 * `data-kp-locale`, `data-kp-week-starts-on`, `data-kp-heading-level` (the
 * title's, 2 by default) and `data-kp-key` (handed to `decorate`).
 * @param {ParentNode} [root]
 * @param {CalendarOptions} [options]
 * @returns {() => void} detach: the listeners taken away; the calendars keep what they show
 */
export declare function attachCalendars(
  root?: ParentNode,
  options?: CalendarOptions,
): () => void;
/**
 * Every day's state, by `YYYY-MM-DD` (the whole history: a month change
 * needs no new call). Repaints in place; the focus and the selection stay,
 * so a live update never moves a reader. A calendar that was loading is
 * ready from here on.
 * @param {Element} el
 * @param {Record<string, CalendarDay>} days
 */
export declare function setCalendarDays(
  el: Element,
  days: Record<string, CalendarDay>,
): void;
/**
 * The calendar as a whole: `loading` (every day pulses at its final size),
 * `ready`, `empty` or `error` (no pulse; `words` under the month, an error's
 * after a dot in the destructive colour). The days say `words` too.
 * @param {Element} el
 * @param {CalendarState} state
 * @param {string} [words]
 */
export declare function setCalendarState(
  el: Element,
  state: CalendarState,
  words?: string,
): void;
/**
 * Select a day (or none, with null) and show its month; no event, the page
 * asked.
 * @param {Element} el
 * @param {string | null} iso `YYYY-MM-DD`
 */
export declare function calendarSelect(el: Element, iso: string | null): void;
/**
 * Show a month; no event, the page asked.
 * @param {Element} el
 * @param {{ year: number, month: number }} at month 1-12
 */
export declare function calendarMonth(
  el: Element,
  {
    year,
    month,
  }: {
    year: number;
    month: number;
  },
): void;
/**
 * The legend under the grid: a swatch and its words per tone, in the order
 * given; none hides it.
 * @param {Element} el
 * @param {{ tone: CalendarTone, label: string }[]} entries
 */
export declare function setCalendarLegend(
  el: Element,
  entries: {
    tone: CalendarTone;
    label: string;
  }[],
): void;
