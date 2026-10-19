/** Fired when a date is chosen, typed or set. A contract value [TH26]: `{ iso, date, source }`. */
export declare const DATE_EVENT = "kp-date-change";
/** Fired when the panel opens or closes: `{ open }`. */
export declare const OPEN_EVENT = "kp-date-open";
/** Fired when the visible month changes: `{ year, month }` (month 0-11). */
export declare const MONTH_EVENT = "kp-date-month";
/** @param {Date} date */
export declare function toISO(date: Date): string;
/**
 * A date as a locale writes it. `toDutch` remains for the consumers that
 * imported it in 1.x; it is `formatDate(date, 'nl-NL')`.
 *
 * @param {Date} date
 * @param {string} [locale]
 */
export declare function formatLocalDate(date: Date, locale?: string): string;
/** @deprecated since 3.0.0 — `formatLocalDate(date, 'nl-NL')`. @param {Date} date */
export declare function toDutch(date: Date): string;
/**
 * Read what someone typed: ISO always, and the locale's own order for
 * the numeric form. "31-02-2026" is refused rather than rounded to 3
 * March, in every locale.
 *
 * @param {string} text
 * @param {string} [locale]
 * @returns {Date | null}
 */
export declare function parseDate(text: string, locale?: string): Date | null;
export type DatePickerHandle = {
  element: HTMLElement;
  get: () => Date | null;
  /**
   * a Date, an ISO string, or null to clear
   */
  set: (date: Date | string | null) => void;
  open: () => void;
  close: () => void;
};
/**
 * @typedef {object} DatePickerHandle
 * @property {HTMLElement} element
 * @property {() => Date | null} get
 * @property {(date: Date | string | null) => void} set a Date, an ISO string, or null to clear
 * @property {() => void} open
 * @property {() => void} close
 */
/**
 * Keep an open calendar inside the window [Kenny's note of 2026-09-13]. The
 * panel hangs from the picker's inline start, so a picker near the inline
 * end of the window — the last filter of a data table — opened its calendar
 * past the edge: measured at x=1374 of a 1280px page. When it would cross
 * that edge it hangs from the picker's inline end instead and opens toward
 * the inline start; when that is worse (a picker narrower than its calendar
 * in a window narrower than both), the side that overflows less is kept.
 * `data-kp-align="end"` says which side was taken. A panel in the top layer
 * (js/top-layer.js) is placed in window coordinates under its picker; one
 * that is not keeps hanging from the picker and only the side changes.
 *
 * @param {HTMLElement} panel an open `.kp-datepicker__panel`
 */
export declare function placeDatePanel(panel: HTMLElement): void;
/**
 * Open a calendar above every container it sits in (js/top-layer.js: a
 * card's clip-path cut every day of it away in four themes), inside the
 * window, and following its field while it is open. Both channels open
 * with it. Returns the way back down.
 *
 * @param {HTMLElement} panel an open `.kp-datepicker__panel`
 * @returns {() => void} lower
 */
export declare function raiseDatePanel(panel: HTMLElement): () => void;
export type DateView = "days" | "months" | "years";
/** The first year of the block of twelve that holds `year`. @param {number} year */
export declare function yearBlockStart(year: number): number;
/**
 * A day of a month, the day kept where the month has it and the month's last
 * day otherwise: 31 January to February lands on the 28th, not in March.
 *
 * @param {number} year @param {number} month may run past 0-11 @param {number} day
 */
export declare function clampToMonth(
  year: number,
  month: number,
  day: number,
): Date;
/**
 * Whether a span of days lies wholly before the minimum or after the maximum:
 * a month or a year that still holds one day to choose stays a way in.
 *
 * @param {Date} from first day @param {Date} to last day @param {Date | null} min @param {Date | null} max
 */
export declare function outsideRange(
  from: Date,
  to: Date,
  min: Date | null,
  max: Date | null,
): boolean;
/**
 * How far a key moves the focus in a month or year grid, in cells; null for a
 * key the grid does not take. Arrows by a cell and a row, Home and End to the
 * ends of the row, PageUp and PageDown by twelve — the day grid's keys, one
 * size up. A move past the drawn grid crosses into the next year or block.
 *
 * @param {string} key @param {number} index the focused cell, 0-11
 * @returns {number | null}
 */
export declare function jumpMove(key: string, index: number): number | null;
/**
 * The size of the day view, to hold while the month or year grid shows, so
 * the panel does not jump under the pointer. Read as the computed sizes, in
 * each element's own box-sizing, so writing them back as minimums lands on
 * the same box whatever a register set.
 *
 * @param {HTMLElement} panel an open `.kp-datepicker__panel` showing days
 * @returns {{ inline: string, block: string, gridInline: string, gridBlock: string }}
 */
export declare function measureDateView(panel: HTMLElement): {
  inline: string;
  block: string;
  gridInline: string;
  gridBlock: string;
};
/** The handle for an attached picker. @param {Element} element */
export declare function datePicker(element: Element): DatePickerHandle | null;
/**
 * @param {ParentNode} root
 * @param {{ locale?: string, dateFormat?: 'rule52' | 'locale', weekStartsOn?: number, closeOnSelect?: boolean, refocus?: boolean, isDateDisabled?: (date: Date) => boolean, renderDay?: (button: HTMLButtonElement, date: Date) => void, previousGlyph?: string, nextGlyph?: string }} [options]
 *   Defaults; per picker: `data-kp-locale`, `data-kp-date-format` (`rule52`, the default: dd/mm/yyyy whatever the locale; `locale`: the locale's own numeric order), `data-kp-week-starts-on`, `data-kp-min`, `data-kp-max` (ISO), `data-kp-disabled-days` (0-6, comma-separated), `data-kp-close-on-select="false"`, `data-kp-previous-glyph`, `data-kp-next-glyph`.
 * @returns {(() => void) & { handles: DatePickerHandle[] }} detach
 */
export declare function attachDatePickers(
  root?: ParentNode,
  {
    locale: localeOption,
    dateFormat: formatOption,
    weekStartsOn: weekOption,
    closeOnSelect,
    refocus,
    isDateDisabled,
    renderDay,
    previousGlyph,
    nextGlyph,
  }?: {
    locale?: string;
    dateFormat?: "rule52" | "locale";
    weekStartsOn?: number;
    closeOnSelect?: boolean;
    refocus?: boolean;
    isDateDisabled?: (date: Date) => boolean;
    renderDay?: (button: HTMLButtonElement, date: Date) => void;
    previousGlyph?: string;
    nextGlyph?: string;
  },
): (() => void) & {
  handles: DatePickerHandle[];
};
