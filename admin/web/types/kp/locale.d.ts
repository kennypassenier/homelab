/**
 * The locale to use: the explicit one; else the nearest `lang` attribute
 * above the element, because a document that declares its language has
 * already answered; else the browser's.
 *
 * @param {string | undefined} [explicit]
 * @param {Element | null} [element] where to look for `lang`
 * @returns {string}
 */
export declare function resolveLocale(
  explicit?: string | undefined,
  element?: Element | null,
): string;
/**
 * The day the week starts on, 0 = Sunday … 6 = Saturday.
 *
 * Intl.Locale's week info where the browser has it (Chrome and Safari
 * do, Firefox is arriving); Monday where it does not, because most of
 * the world's calendars agree and the ones that do not are the ones
 * whose browsers ship the API.
 *
 * @param {string | undefined} [locale]
 * @param {number | undefined} [explicit] a consumer's own answer wins
 * @returns {number}
 */
export declare function weekStartsOn(
  locale?: string | undefined,
  explicit?: number | undefined,
): number;
export type DateFormat = "locale" | "rule52";
/** How a numeric date is written: the reader's locale, or dd/mm/yyyy (rule 52). @typedef {'locale' | 'rule52'} DateFormat */
/**
 * The order and separator a locale writes a numeric date in, read from
 * Intl once rather than assumed. `parts` is e.g. ['day','month','year'].
 * `format` is `'locale'` (what Intl says; the default of this function) or
 * `'rule52'` (dd/mm/yyyy whatever the locale, Kenny's rule 52).
 *
 * @param {string | undefined} [locale]
 * @param {DateFormat} [format]
 * @returns {{ parts: ('day' | 'month' | 'year')[], separator: string, hint: string }}
 */
export declare function datePattern(
  locale?: string | undefined,
  format?: DateFormat,
): {
  parts: ("day" | "month" | "year")[];
  separator: string;
  hint: string;
};
/**
 * A date as the locale writes it: numeric, two-digit day and month; with
 * `format` `'rule52'`, dd/mm/yyyy in Latin digits whatever the locale.
 *
 * @param {Date} date
 * @param {string | undefined} [locale]
 * @param {DateFormat} [format]
 * @returns {string}
 */
export declare function formatDate(
  date: Date,
  locale?: string | undefined,
  format?: DateFormat,
): string;
/**
 * Read what a person typed, in their locale's order — plus ISO, which
 * everyone accepts. Returns null for anything that is not a real date:
 * "31-02-2026" is refused rather than rounded to 3 March.
 *
 * @param {string} text
 * @param {string | undefined} [locale]
 * @param {DateFormat} [format]
 * @returns {Date | null}
 */
export declare function parseDate(
  text: string,
  locale?: string | undefined,
  format?: DateFormat,
): Date | null;
/**
 * The month and weekday names a calendar shows, in the locale it is drawn
 * for [gap-11].
 *
 * The date picker wrote a Dutch date under an English month, because its
 * names came from the string dictionary and the dictionary is English.
 * Intl knows the names in every language the browser does, so a locale
 * that is not English reads them from Intl. The dictionary still decides
 * in two cases: when a consumer has set names of their own (their words
 * win, which is the way out [KT6]), and when the locale is English, where
 * the dictionary already is the answer — and where Intl's "Mon" would
 * quietly change the two-letter "Mo" every English page shows.
 *
 * The short month names fill the month grid [scope-89], where twelve full
 * names in three columns ran out of their cells in eleven themes
 * ("September", 83px of text in a 75px cell). They follow the same rule, and
 * a consumer who set full names of their own but no short ones sees their
 * full names there rather than the package's English abbreviations.
 *
 * @param {{ months: string[], weekdays: string[], monthsShort?: string[] }} dictionary the strings in force; weekdays Monday first
 * @param {{ months: string[], weekdays: string[], monthsShort?: string[] }} defaults the package's own dictionary, to tell a consumer's names from it
 * @param {string | undefined} [locale]
 * @returns {{ months: string[], monthsShort: string[], weekdays: string[] }} weekdays Sunday first, the way Date#getDay counts
 */
export declare function calendarNames(
  dictionary: {
    months: string[];
    weekdays: string[];
    monthsShort?: string[];
  },
  defaults: {
    months: string[];
    weekdays: string[];
    monthsShort?: string[];
  },
  locale?: string | undefined,
): {
  months: string[];
  monthsShort: string[];
  weekdays: string[];
};
/**
 * A byte count as the locale writes numbers: "1,5 MB" in Dutch, "1.5 MB"
 * in English. Base 1000 with SI units, because a person reading a file
 * size is not counting sectors.
 *
 * @param {number} bytes
 * @param {string | undefined} [locale]
 * @param {{ base?: 1000 | 1024, units?: string[] }} [options]
 * @returns {string}
 */
export declare function formatBytes(
  bytes: number,
  locale?: string | undefined,
  {
    base,
    units,
  }?: {
    base?: 1000 | 1024;
    units?: string[];
  },
): string;
/**
 * Read a number the way the locale writes it: "1.284,50" in Dutch,
 * "1,284.50" in English. Intl tells us which glyph is the decimal.
 *
 * @param {string} text
 * @param {string | undefined} [locale]
 * @returns {number}
 */
export declare function parseNumber(
  text: string,
  locale?: string | undefined,
): number;
/**
 * A collator for sorting text as the locale sorts it.
 *
 * @param {string | undefined} [locale]
 * @returns {Intl.Collator}
 */
export declare function collator(locale?: string | undefined): Intl.Collator;
