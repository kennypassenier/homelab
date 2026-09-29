// The kp-themes module chassis-rs serves at /static/kp/js/datatable.js (kp-themes
// 8.0.0), copied whole from kp-themes js/datatable.d.ts: chassis serves the
// module, not its types.
/** A detail row, directly after the row it belongs to. */
export declare const DETAIL = "[data-kp-row-detail]";
export type Direction = "ascending" | "descending" | "none";
export type Sort = {
  column: number;
  direction: Direction;
} | null;
export type SortKey = {
  column: number;
  direction: "ascending" | "descending";
};
export type FilterKind = "choice" | "range" | "date";
export type FilterValue =
  | string[]
  | {
      from?: string;
      to?: string;
    };
export type State = "ready" | "loading" | "failed";
export type Density = "comfortable" | "compact";
export type EditKind = "text" | "number" | "select" | "date";
export type View = {
  shown: number;
  total: number;
  page: number;
  pages: number;
  pageSize: number;
  query: string;
  scope: number | null;
  filters: Record<number, FilterValue>;
  sort: Sort;
  sorts: SortKey[];
  hidden: number[];
  expanded: string[];
  density: Density;
  state: State;
  keys: string[];
  pageKeys: string[];
};
export type DataRequest = {
  id: number;
  query: string;
  scope: number | null;
  sort: SortKey | null;
  sorts: SortKey[];
  filters: Record<number, FilterValue>;
  page: number;
  pageSize: number;
  signal: AbortSignal;
};
export type DataAnswer = {
  rows: ReadonlyArray<HTMLTableRowElement | Record<string, unknown>>;
  total: number;
};
export type EditDetail = {
  key: string;
  row: HTMLTableRowElement;
  cell: HTMLTableCellElement;
  column: number;
  label: string;
  value: string;
  previous: string;
  undo: boolean;
  reject: (message?: string) => void;
  waitUntil: (promise: Promise<unknown>) => void;
};
/** @typedef {'ascending' | 'descending' | 'none'} Direction */
/** @typedef {{ column: number, direction: Direction } | null} Sort */
/** One key of a sort on several columns. @typedef {{ column: number, direction: 'ascending' | 'descending' }} SortKey */
/** @typedef {'choice' | 'range' | 'date'} FilterKind */
/** A choice filter's ticked values, or a range's bounds as typed. @typedef {string[] | { from?: string, to?: string }} FilterValue */
/** @typedef {'ready' | 'loading' | 'failed'} State */
/** @typedef {'comfortable' | 'compact'} Density */
/** @typedef {'text' | 'number' | 'select' | 'date'} EditKind */
/**
 * @typedef {{ shown: number, total: number, page: number, pages: number, pageSize: number, query: string, scope: number | null,
 *   filters: Record<number, FilterValue>, sort: Sort, sorts: SortKey[], hidden: number[], expanded: string[], density: Density, state: State,
 *   keys: string[], pageKeys: string[] }} View
 */
/**
 * What a server-backed table asks its `load` for [Kenny, 2026-09-13].
 * @typedef {{ id: number, query: string, scope: number | null, sort: SortKey | null, sorts: SortKey[], filters: Record<number, FilterValue>,
 *   page: number, pageSize: number, signal: AbortSignal }} DataRequest
 */
/**
 * What `load` answers: one page of rows — table rows, or objects read through each header's `data-kp-field` — and how many
 * rows there are in all.
 * @typedef {{ rows: ReadonlyArray<HTMLTableRowElement | Record<string, unknown>>, total: number }} DataAnswer
 */
/**
 * An edit about to be saved. `reject(message)` refuses it with a message the cell shows; `waitUntil(promise)` holds the
 * save until the promise settles, and a promise that resolves to a string or to false refuses it too.
 * @typedef {{ key: string, row: HTMLTableRowElement, cell: HTMLTableCellElement, column: number, label: string, value: string,
 *   previous: string, undo: boolean, reject: (message?: string) => void, waitUntil: (promise: Promise<unknown>) => void }} EditDetail
 */
/** Fired on the wrapper when the visible set changes. A contract value [TH26]. Detail: View, plus `rows` and `pageRows` (the elements). */
export declare const VIEW_EVENT = "kp-datatable-view";
/** Fired when the selection changes; the detail carries the selected row keys and rows. */
export declare const SELECT_EVENT = "kp-datatable-select";
/** Fired when the sort changes: `{ column, direction }` or null — the first key; the view carries all of them as `sorts`. */
export declare const SORT_EVENT = "kp-datatable-sort";
/** Fired when the failed state's retry control is pressed; the app loads again and sets the state. */
export declare const RETRY_EVENT = "kp-datatable-retry";
/** Fired when the reader shows or hides a column: `{ hidden }`, the column indices. */
export declare const COLUMNS_EVENT = "kp-datatable-columns";
/** Fired when a row opens or closes: `{ key, row, open, cell, expanded }`; `cell` is the detail cell, for an app that fills it then. */
export declare const EXPAND_EVENT = "kp-datatable-expand";
/** Fired when a server-backed table needs rows: the DataRequest, plus `respond(answer)` and `fail(error)`. */
export declare const REQUEST_EVENT = "kp-datatable-request";
/** Fired, cancelable, when an edited value is about to be saved (EditDetail); `preventDefault()` refuses it. */
export declare const EDIT_EVENT = "kp-datatable-edit";
/** Rows per page when the consumer does not say [Kenny, 2026-09-13]. Per table as `data-kp-page-size`. */
export declare const PAGE_SIZE = 25;
/** The page sizes on offer when the consumer does not say. Per table as `data-kp-page-sizes="10,25"`, or `none`. */
export declare const PAGE_SIZES: readonly number[];
/** How long a server-backed table waits after the last key before it asks [research/datatable, "300 ms server"]. */
export declare const SERVER_DEBOUNCE_MS = 300;
/** Rows Page Up and Page Down move in the keyboard grid, as the approved mock does. */
export declare const GRID_PAGE_ROWS = 5;
export type Compare = (
  a: string,
  b: string,
  kind: string,
  locale: string,
) => number;
export type Filter = (row: HTMLTableRowElement, query: string) => boolean;
/** @typedef {(a: string, b: string, kind: string, locale: string) => number} Compare */
/** @typedef {(row: HTMLTableRowElement, query: string) => boolean} Filter */
/**
 * Compare two cells the way their column asks to be compared.
 *
 * A number column sorted as text puts 100 before 20, which is the single
 * most common data-table bug and the reason the column declares its type
 * rather than the code guessing from the first row. Numbers and text are
 * read in the locale: "1.284,50" is a number in Dutch and "1,284.50" in
 * English, and both sort correctly on their own page.
 *
 * @type {Compare}
 */
export declare function compare(
  a: string,
  b: string,
  kind: string,
  locale: string,
): number;
/**
 * Compare two cells by a declared order, "Low,Medium,High,Critical" [gap-13].
 *
 * Severity sorted as text reads Critical, High, Low, Medium: alphabetical
 * and meaningless. A value the order does not name sorts after every value
 * it does, by the column's own comparison, so a stray "Unknown" neither
 * disappears nor jumps to the top.
 *
 * @param {readonly string[]} order
 * @param {string} a
 * @param {string} b
 * @param {(a: string, b: string) => number} fallback
 */
export declare function compareByOrder(
  order: readonly string[],
  a: string,
  b: string,
  fallback: (a: string, b: string) => number,
): number;
/**
 * The next sort keys after a header press [Kenny, 2026-09-13, #multi-sort].
 *
 * A plain press sorts on that column alone, turning it round when it already
 * was the only key. With `add` (Shift) the column joins the keys as the
 * last one, turns round when it is a key already, and leaves the keys on
 * its third press. Both channels call this, so they cannot disagree on
 * what a press does.
 *
 * @template K
 * @param {{ column: K, direction: 'ascending' | 'descending' }[]} keys
 * @param {K} column
 * @param {{ add?: boolean, cycle?: 'two' | 'three' }} [options]
 * @returns {{ column: K, direction: 'ascending' | 'descending' }[]}
 */
export declare function nextSorts<K>(
  keys: {
    column: K;
    direction: "ascending" | "descending";
  }[],
  column: K,
  {
    add,
    cycle,
  }?: {
    add?: boolean;
    cycle?: "two" | "three";
  },
): {
  column: K;
  direction: "ascending" | "descending";
}[];
/**
 * Whether a filter holds anything; an empty one filters nothing.
 * @param {FilterKind} kind
 * @param {FilterValue | undefined} value
 */
export declare function filterActive(
  kind: FilterKind,
  value: FilterValue | undefined,
): boolean;
/**
 * The one pill a filter shows in the add-filter mode [Kenny, 2026-09-14]:
 * the column and every value the filter holds, or null when it holds
 * nothing. Both channels call this, so their pills read the same.
 *
 * @param {FilterKind} kind
 * @param {string} column the column's name
 * @param {FilterValue | undefined} value
 * @param {import('./strings.js').Strings} s
 * @returns {string | null}
 */
export declare function filterPillLabel(
  kind: FilterKind,
  column: string,
  value: FilterValue | undefined,
  s: import("./strings.js").Strings,
): string | null;
/**
 * Read the two bounds of an add-filter editor into a filter value, or say
 * what is wrong with them [Kenny, 2026-09-14, #filter-add]: a number bound
 * that is not a number, a date the picker cannot read, a range that runs
 * backwards. A value of null with no error means both bounds are empty, and
 * applying that removes the filter. Both channels call this.
 *
 * @param {'range' | 'date'} kind
 * @param {Record<'from' | 'to', { text: string, iso?: string, label: string }>} bounds  `text` as the field shows it; `iso` a date picker's value
 * @param {import('./strings.js').Strings} s
 * @returns {{ value: { from: string, to: string } | null, error?: string, bound?: 'from' | 'to' }}
 */
export declare function readFilterBounds(
  kind: "range" | "date",
  bounds: Record<
    "from" | "to",
    {
      text: string;
      iso?: string;
      label: string;
    }
  >,
  s: import("./strings.js").Strings,
): {
  value: {
    from: string;
    to: string;
  } | null;
  error?: string;
  bound?: "from" | "to";
};
/**
 * Whether a cell's text passes a column's filter [gap-13].
 *
 * A range reads the cell in the page's locale, the way the sort does; a
 * date reads it as the date the cell writes, and the upper bound takes
 * the whole of its day, so "to 12 August" includes 12 August.
 *
 * @param {FilterKind} kind
 * @param {FilterValue | undefined} value
 * @param {string} text the cell's text
 * @param {string} locale
 */
export declare function matchesFilter(
  kind: FilterKind,
  value: FilterValue | undefined,
  text: string,
  locale: string,
): boolean;
/**
 * The pills an active filter shows, each with the value that removes it.
 *
 * @param {FilterKind} kind
 * @param {string} column the column's name
 * @param {FilterValue | undefined} value
 * @param {import('./strings.js').Strings} s
 * @returns {{ label: string, without: FilterValue | undefined }[]}
 */
export declare function filterPills(
  kind: FilterKind,
  column: string,
  value: FilterValue | undefined,
  s: import("./strings.js").Strings,
): {
  label: string;
  without: FilterValue | undefined;
}[];
/**
 * Keep the leading columns of a table in place while its box scrolls
 * sideways [Kenny, 2026-09-13, #sticky]. Measures the header cells and
 * writes, on every cell of the first `count` columns, `data-kp-fixed`
 * (`last` on the one that draws the hairline) and the offset it sticks at
 * as `--kp-datatable-fixed-start`; the stylesheet does the sticking. Call
 * it again after the columns change; both channels do.
 *
 * @param {HTMLTableElement} table
 * @param {number} count how many leading columns stay; 0 takes the marks away
 */
export declare function syncFixedColumns(
  table: HTMLTableElement,
  count: number,
): void;
/**
 * Mark a data table while its scroll box is scrolled away from the top
 * [scope-90]: `data-kp-scrolled` on `element` while `box.scrollTop` is
 * above 0, removed at the top. The stylesheet draws the sticky header's
 * reach (`fix-32`) only under it, so a caption right above the header is
 * not painted over at rest. One decision per frame, on a passive listener.
 * Both channels use this one.
 *
 * @param {HTMLElement} element the `.kp-datatable`
 * @param {HTMLElement} box its `.kp-table-wrap`, the box that scrolls
 * @returns {() => void} detach, which removes the attribute
 */
export declare function watchScrolled(
  element: HTMLElement,
  box: HTMLElement,
): () => void;
export type GridHandle = {
  /**
   * re-read the rows and cells after they changed; keeps one cell in the tab order
   */
  sync: () => void;
  /**
   * move to a cell, by row index in the table and column
   */
  focus: (row: number, column: number) => void;
  detach: () => void;
};
/**
 * @typedef {object} GridHandle
 * @property {() => void} sync  re-read the rows and cells after they changed; keeps one cell in the tab order
 * @property {(row: number, column: number) => void} focus  move to a cell, by row index in the table and column
 * @property {() => void} detach
 */
/**
 * A roving focus grid over a table [Kenny, 2026-09-13, #keyboard; APG grid].
 *
 * Tab enters on one cell and leaves with the next Tab; inside, the arrow
 * keys move a cell at a time, Home and End go to the start and end of the
 * row, Ctrl + Home and Ctrl + End to the first and last cell, and Page Up
 * and Page Down `pageRows` rows. A cell that holds exactly one control — a
 * sort button, a checkbox, an edit button — hands the focus to that
 * control, so Enter and Space do what the control does. Hidden rows and
 * columns are stepped over, and keys pressed inside an open editor belong
 * to the editor. Both channels use this one.
 *
 * @param {HTMLTableElement} table
 * @param {{ pageRows?: number, onMove?: (position: { row: number, rows: number, column: number, cell: HTMLTableCellElement }) => void }} [options]
 * @returns {GridHandle}
 */
export declare function attachGrid(
  table: HTMLTableElement,
  {
    pageRows,
    onMove,
  }?: {
    pageRows?: number;
    onMove?: (position: {
      row: number;
      rows: number;
      column: number;
      cell: HTMLTableCellElement;
    }) => void;
  },
): GridHandle;
export type DataTableHandle = {
  element: HTMLElement;
  view: () => View;
  /**
   * sort on one column, or null for none
   */
  sort: (sort: Sort) => void;
  /**
   * sort on several columns, the first key first
   */
  sortBy: (keys: readonly SortKey[]) => void;
  page: (page: number) => void;
  pageSize: (size: number) => void;
  query: (query: string) => void;
  /**
   * limit the search to one column, or null for all
   */
  scope: (column: number | null) => void;
  /**
   * set one column's filter; null clears it
   */
  filter: (column: number, value: FilterValue | null) => void;
  clearFilters: () => void;
  /**
   * open a column's filter editor, or close it with null; in the panel mode, open or close the panel
   */
  editFilter: (column: number | null) => void;
  /**
   * the columns to hide; a locked column stays
   */
  hideColumns: (columns: readonly number[]) => void;
  /**
   * the rows to open, by key
   */
  expand: (keys: readonly string[]) => void;
  density: (density: Density) => void;
  /**
   * loading, failed, or ready again
   */
  state: (state: State) => void;
  /**
   * the status line's words while the table loads, kept across refresh(), shown only while loading, cleared with no text [fix-80]; with `since` (a time or a Date) the table counts how long it has been loading by itself, in a part the live region does not announce [fix-84]
   */
  busy: (
    words?:
      | string
      | null
      | {
          text?: string | null;
          since?: number | Date | null;
        },
  ) => void;
  /**
   * the failed state with the app's reason in the failed slot's words, and Try again under them; any table, not only a server's [fix-85]
   */
  fail: (reason?: string | null) => void;
  /**
   * ask the server again for what the table shows
   */
  reload: () => void;
  /**
   * open a cell's editor
   */
  edit: (key: string, column: number) => void;
  cancelEdit: () => void;
  /**
   * the rows the table shows, in order: every page of the view, or this page
   */
  rows: (which?: "view" | "page") => HTMLTableRowElement[];
  selected: () => string[];
  select: (keys: readonly string[]) => void;
  /**
   * re-read the rows after the consumer added or removed some, and the filter panel's choice lists with them
   */
  refresh: () => void;
};
/** The handle for an attached table, for code that did not call attach. @param {Element} element */
export declare function dataTable(element: Element): DataTableHandle | null;
export type DataTableOptions = {
  locale?: string;
  compare?: Compare;
  filter?: Filter;
  /**
   * Default 0, and 300 for a server-backed table.
   */
  debounceMs?: number;
  sortCycle?: "two" | "three";
  /**
   * Shift + click adds a sort key. Default false; per table `data-kp-sort-multi`.
   */
  multiSort?: boolean;
  /**
   * How the header-declared filters are set: a panel of every filter, or "+ Add filter" with one pill per filter. Default panel; per table `data-kp-filter-mode`.
   */
  filterMode?: "panel" | "add";
  pagerClassName?: string;
  pageLabel?: (at: number, of: number) => string;
  regions?: boolean;
  pageSizes?: readonly number[];
  removeGlyph?: string;
  /**
   * A row's detail, built when it first opens.
   */
  detail?: (row: HTMLTableRowElement) => Node | string | null | undefined;
  /**
   * The rows of a `data-kp-server` table.
   */
  load?: (request: DataRequest) => Promise<DataAnswer> | DataAnswer;
  /**
   * The key of a row `load` answered as an object. Default the field `data-kp-row-key-field` names, else the key column's `data-kp-field`, else `key`, else `id`.
   */
  rowKey?: (row: Record<string, unknown>) => string;
  /**
   * Hears an edit before it is saved; a string or false (or a promise of one) refuses it.
   */
  onEdit?: (detail: EditDetail) => unknown;
  /**
   * Default ▸.
   */
  expandGlyph?: string;
  /**
   * Default ▾.
   */
  collapseGlyph?: string;
  /**
   * Default 5.
   */
  gridPageRows?: number;
};
/**
 * @typedef {object} DataTableOptions
 * @property {string} [locale]
 * @property {Compare} [compare]
 * @property {Filter} [filter]
 * @property {number} [debounceMs]  Default 0, and 300 for a server-backed table.
 * @property {'two' | 'three'} [sortCycle]
 * @property {boolean} [multiSort]  Shift + click adds a sort key. Default false; per table `data-kp-sort-multi`.
 * @property {'panel' | 'add'} [filterMode]  How the header-declared filters are set: a panel of every filter, or "+ Add filter" with one pill per filter. Default panel; per table `data-kp-filter-mode`.
 * @property {string} [pagerClassName]
 * @property {(at: number, of: number) => string} [pageLabel]
 * @property {boolean} [regions]
 * @property {readonly number[]} [pageSizes]
 * @property {string} [removeGlyph]
 * @property {(row: HTMLTableRowElement) => Node | string | null | undefined} [detail]  A row's detail, built when it first opens.
 * @property {(request: DataRequest) => Promise<DataAnswer> | DataAnswer} [load]  The rows of a `data-kp-server` table.
 * @property {(row: Record<string, unknown>) => string} [rowKey]  The key of a row `load` answered as an object. Default the field `data-kp-row-key-field` names, else the key column's `data-kp-field`, else `key`, else `id`.
 * @property {(detail: EditDetail) => unknown} [onEdit]  Hears an edit before it is saved; a string or false (or a promise of one) refuses it.
 * @property {string} [expandGlyph]   Default ▸.
 * @property {string} [collapseGlyph] Default ▾.
 * @property {number} [gridPageRows]  Default 5.
 */
/**
 * Attach every data table under `root`.
 *
 * @param {ParentNode} root
 * @param {DataTableOptions} [options]
 *   Defaults, each also settable per table: `data-kp-locale`, `data-kp-debounce`, `data-kp-sort-cycle`, `data-kp-page-size`,
 *   `data-kp-page-sizes` (`none` for no choice). `regions` upgrades the scroll wrapper to a named region [TH95]; off leaves the
 *   wrapper untouched.
 * @returns {(() => void) & { handles: DataTableHandle[] }} detach
 */
export declare function attachDataTables(
  root?: ParentNode,
  {
    locale: localeOption,
    compare: compareFn,
    filter: filterFn,
    debounceMs,
    sortCycle,
    multiSort,
    filterMode,
    pagerClassName,
    pageLabel,
    regions,
    pageSizes: pageSizesOption,
    removeGlyph,
    detail: detailFn,
    load: loadFn,
    rowKey: rowKeyFn,
    onEdit,
    expandGlyph,
    collapseGlyph,
    gridPageRows,
  }?: DataTableOptions,
): (() => void) & {
  handles: DataTableHandle[];
};
