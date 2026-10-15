// The kp-themes module chassis-rs serves at /static/kp/js/strings.js (kp-themes 10.1.1),
// copied from kp-themes js/strings.d.ts: chassis serves the module, not its types.
export type Strings = {
  alertSuccess: string;
  alertWarning: string;
  alertInfo: string;
  alertError: string;
  /**
   * The word a screen reader hears before a critical problem on the attention band (setAttention) [scope-143]
   */
  attentionCritical: string;
  /**
   * The same, before a warning [scope-143]
   */
  attentionWarning: string;
  /**
   * The same, before an information [scope-143]
   */
  attentionInfo: string;
  /**
   * A ticking freshness line, "updated 12 s ago" (js/freshness.js) [scope-143]
   */
  agoText: (verb: string, duration: string) => string;
  /**
   * The same line with no moment yet, "not updated yet" [scope-143]
   */
  agoNever: (verb: string) => string;
  /**
   * A duration's seconds part, "12 s" [scope-143]
   */
  agoSeconds: (n: number) => string;
  /**
   * Its minutes part, "2 min" [scope-143]
   */
  agoMinutes: (n: number) => string;
  /**
   * Its hours part, "3 h" [scope-143]
   */
  agoHours: (n: number) => string;
  /**
   * Its days part, "1 day", "2 days" [scope-143]
   */
  agoDays: (n: number) => string;
  /**
   * The verb a freshness line uses when its element names none [scope-143]
   */
  agoVerb: string;
  /**
   * What a freshness line with `data-kp-ago-announce="state"` announces once it turns stale [scope-143]
   */
  agoStale: (text: string) => string;
  /**
   * What it announces once it is fresh again [scope-143]
   */
  agoFresh: (text: string) => string;
  /**
   * The console warning for a ticking line inside a live region [scope-143]
   */
  agoInLiveRegion: string;
  busy: string;
  close: string;
  /**
   * The accessible name of a collapsed navigation's toggle
   */
  menu: string;
  /**
   * The same toggle once the navigation is open
   */
  closeMenu: string;
  /**
   * The accessible name of a mega-menu button in the bar that holds no words of its own [scope-48]
   */
  navDisclosure: string;
  /**
   * The accessible name of a hidden side navigation's toggle
   */
  sidebar: string;
  /**
   * The same toggle once that side navigation is open
   */
  closeSidebar: string;
  /**
   * The accessible name of an empty slim-rail toggle while the rail is expanded
   */
  collapseRail: string;
  /**
   * The same toggle while the rail is collapsed to its icons
   */
  expandRail: string;
  /**
   * The name of the resize handle on a resizable side navigation
   */
  resizeSidebar: string;
  /**
   * The control that returns the reader to the top of the page
   */
  backToTop: string;
  previous: string;
  next: string;
  finish: string;
  back: string;
  removeNamed: (name: string) => string;
  noResults: string;
  oneResult: string;
  manyResults: (n: number) => string;
  noCommands: string;
  oneCommand: string;
  manyCommands: (n: number) => string;
  commandPlaceholder: string;
  commandsLabel: string;
  shortcutsLabel: string;
  /**
   * The visible word on the command palette's trigger in the bar [scope-48]
   */
  paletteTrigger: string;
  /**
   * The hotkey as that trigger prints it: ⌘K on a Mac, Ctrl K elsewhere [scope-48]
   */
  paletteHotkey: (key: string, mac: boolean) => string;
  tableSearch: string;
  tableSearchLabel: string;
  tableSelectAll: string;
  tableSelectRow: (key: string) => string;
  tableEmpty: string;
  tableRows: (n: number) => string;
  tableRowsFiltered: (shown: number, total: number) => string;
  tablePage: (at: number, of: number) => string;
  tableRegion: string;
  /**
   * The name of the "In" choice beside the search box
   */
  tableSearchScope: string;
  /**
   * The "In" choice's first option
   */
  tableSearchAllColumns: string;
  /**
   * The filter panel's toggle, with the number of active filters
   */
  tableFilters: (active: number) => string;
  /**
   * The filter panel's group name
   */
  tableFiltersLabel: string;
  /**
   * The list of removable filter pills
   */
  tableActiveFilters: string;
  /**
   * A choice filter's pill
   */
  tableFilterValue: (column: string, value: string) => string;
  /**
   * A range or date filter's pill; an open end is `tableFilterOpenEnd`
   */
  tableFilterRange: (column: string, from: string, to: string) => string;
  /**
   * The open end of a range in its pill
   */
  tableFilterOpenEnd: string;
  /**
   * The lower bound's accessible name
   */
  tableFilterFrom: (column: string) => string;
  /**
   * The upper bound's accessible name
   */
  tableFilterTo: (column: string) => string;
  /**
   * A pill's remove button
   */
  tableRemoveFilter: (label: string) => string;
  tableClearFilters: string;
  /**
   * The way out of the no-match state
   */
  tableClearSearch: string;
  /**
   * The "+ Add filter" button of a table in the add-filter mode, with the number of active filters
   */
  tableAddFilter: (active: number) => string;
  /**
   * The name of its menu of filterable columns
   */
  tableAddFilterMenu: string;
  /**
   * A column in that menu, not filtered yet
   */
  tableAddFilterItem: (column: string) => string;
  /**
   * A column in that menu that is filtered already: choosing it edits that filter
   */
  tableAddFilterItemActive: (column: string) => string;
  /**
   * The mark beside a filtered column in the menu
   */
  tableFilterMarked: string;
  /**
   * The editor's accessible name
   */
  tableFilterEditor: (column: string) => string;
  /**
   * The editor's list of choices, for a screen reader
   */
  tableFilterChoicesLegend: (column: string) => string;
  /**
   * A bound's visible label in the editor
   */
  tableFilterBound: (
    kind: "range" | "date",
    bound: "from" | "to",
    column: string,
  ) => string;
  /**
   * A choice filter's one pill in the add-filter mode
   */
  tableFilterChoicePill: (column: string, values: string[]) => string;
  /**
   * A range or date filter's pill in the add-filter mode; an open end is left out
   */
  tableFilterSpanPill: (
    column: string,
    from: string,
    to: string,
    kind: "range" | "date",
  ) => string;
  /**
   * A pill's own button, which reopens its editor
   */
  tableEditFilter: (label: string) => string;
  tableFilterApply: string;
  tableFilterCancel: string;
  /**
   * The add-filter mode's way out of every filter, shown from two pills
   */
  tableFilterClearAll: string;
  /**
   * A number bound that is not a number
   */
  tableFilterNotNumber: (label: string, value: string) => string;
  /**
   * A date bound the picker cannot read
   */
  tableFilterNotDate: (label: string, value: string) => string;
  /**
   * A range whose lower bound is above its upper
   */
  tableFilterBackwards: (from: string, to: string) => string;
  tableDensity: string;
  tableDensityComfortable: string;
  tableDensityCompact: string;
  tableRowsPerPage: string;
  /**
   * The status line: "Showing 1–25 of 60"
   */
  tableShowing: (
    from: number,
    to: number,
    count: number,
    total: number,
  ) => string;
  /**
   * The action bar's count
   */
  tableSelected: (n: number) => string;
  tableClearSelection: string;
  /**
   * The card layout's sort control
   */
  tableSortBy: string;
  /**
   * Its first option
   */
  tableSortNone: string;
  tableSortAscending: string;
  tableSortDescending: string;
  tableFailed: string;
  tableRetry: string;
  /**
   * One key of a multi-column sort in words; `kind` is text, number, date or order
   */
  tableSortKey: (
    column: string,
    direction: "ascending" | "descending",
    kind: string,
  ) => string;
  /**
   * The multi-sort summary, from the keys in order
   */
  tableSortedBy: (keys: string[]) => string;
  tableNotSorted: string;
  tableSortReset: string;
  tableBusyElapsed: (seconds: number) => string;
  /**
   * The column menu's button
   */
  tableColumns: string;
  /**
   * The column menu's name
   */
  tableColumnsLabel: string;
  /**
   * A column that cannot be hidden, in the menu
   */
  tableColumnLocked: (column: string) => string;
  tableShowAllColumns: string;
  tableColumnsShown: (shown: number, total: number) => string;
  /**
   * The expansion column's header, read by a screen reader only
   */
  tableDetailsColumn: string;
  /**
   * A row's expand button
   */
  tableRowDetails: (key: string) => string;
  /**
   * A group row's fold button: the group's name and how many of its rows match [J2]
   */
  tableGroupRows: (group: string, count: number) => string;
  /**
   * An editable cell's button
   */
  tableEdit: (column: string, key: string, value: string) => string;
  /**
   * The editor's accessible name
   */
  tableEditField: (column: string, key: string) => string;
  /**
   * Said when an editor opens
   */
  tableEditing: (column: string, key: string) => string;
  tableEdited: (
    key: string,
    column: string,
    before: string,
    after: string,
  ) => string;
  tableEditUndone: (key: string, column: string, value: string) => string;
  tableEditCancelled: string;
  tableEditRequired: string;
  /**
   * A rejected edit with no message of its own
   */
  tableEditInvalid: string;
  /**
   * The keyboard grid's line before the first cell is focused
   */
  tableGridStart: string;
  /**
   * Row 0 is the header row
   */
  tableGridPosition: (
    row: number,
    rows: number,
    column: string,
    text: string,
  ) => string;
  formRequired: string;
  formInvalid: string;
  formSummaryOne: string;
  formSummaryMany: (n: number) => string;
  fieldFallbackName: string;
  /**
   * The word beside a switch that is on [gap-11]
   */
  switchOn: string;
  /**
   * The word beside a switch that is off [gap-11]
   */
  switchOff: string;
  calendarOpen: string;
  calendarButton: string;
  dateFormatHint: string;
  previousMonth: string;
  nextMonth: string;
  monthTitle: (month: string, year: number) => string;
  /**
   * The month title's name: it opens the twelve months [scope-89]
   */
  chooseMonth: (title: string) => string;
  /**
   * The year title's name: it opens twelve years [scope-89]
   */
  chooseYear: (year: number) => string;
  previousYear: string;
  nextYear: string;
  /**
   * Twelve years back, in the year grid
   */
  previousYears: string;
  /**
   * Twelve years on, in the year grid
   */
  nextYears: string;
  /**
   * The month grid's name, and what is said when it opens
   */
  monthGrid: (year: number) => string;
  /**
   * The year grid's name, and what is said when it opens
   */
  yearGrid: (from: number, to: number) => string;
  /**
   * The year grid's title
   */
  yearRange: (from: number, to: number) => string;
  weekdays: string[];
  months: string[];
  /**
   * The month grid's cells, where a full name does not fit [scope-89]
   */
  monthsShort: string[];
  dayLabel: (day: number, month: string, year: number) => string;
  uploadZone: string;
  uploadTooLarge: (size: string) => string;
  uploadTooMany: (max: number) => string;
  uploadTotalTooLarge: (max: string) => string;
  uploadWrongType: (accept: string) => string;
  uploadProgress: (name: string) => string;
  wizardStep: (at: number, of: number) => string;
  copy: string;
  copied: string;
  copyBlocked: string;
  copiedAnnouncement: (value: string) => string;
  copyBlockedAnnouncement: string;
  undo: string;
  deleted: string;
  splitLabel: string;
  reorderHandle: (name: string) => string;
  reorderMoved: (name: string, at: number, of: number) => string;
  tileFallbackName: string;
  tileLabel: (
    name: string,
    column: number,
    row: number,
    w: number,
    h: number,
  ) => string;
  contrastMissing: (token: string) => string;
  contrastReport: (ratio: string, token: string, verdict: string) => string;
  colourHue: string;
  colourSaturation: string;
  colourLightness: string;
  contrastPasses: string;
  contrastFails: string;
  confirm: string;
  confirmAccept: string;
  confirmCancel: string;
  confirmDescription: string;
  /**
   * The button that closes an alarm that must be acknowledged, when the caller names none [scope-94]
   */
  alarmAction: string;
  /**
   * The button that stops an alarm's countdown and keeps it open [scope-94]
   */
  alarmKeepOpen: string;
  /**
   * The key hint beside the acknowledge button; hidden from a screen reader [scope-94]
   */
  alarmHint: string;
  /**
   * The whole seconds left under the countdown bar; hidden from a screen reader [scope-94]
   */
  alarmCountdown: (seconds: number) => string;
  /**
   * Said once when an alarm that closes by itself opens [scope-94]
   */
  alarmClosesBy: (seconds: number) => string;
  /**
   * Said when an alarm that must be acknowledged opens [scope-94]
   */
  alarmPressTo: (action: string) => string;
  /**
   * Said when Keep open stopped the countdown [scope-94]
   */
  alarmKeptOpen: (action: string) => string;
  save: string;
  mainNavigation: string;
  skipToContent: string;
  /**
   * The diagnostics row that lists unknown hook values [AR44]
   */
  diagnosticsEffects: string;
  diagnosticsEffectsNone: string;
  /**
   * The stamp a register may print on an emphasis reveal (the dossier) [AR35]
   */
  classified: string;
  /**
   * The neutral boot line of an arrival, for a theme that performs one and has no words of its own in `arrivalWordsByTheme` or `arrivalLinesByTheme` [SW2, scope-84]
   */
  arrivalLine: string;
  /**
   * A theme's own words for the counting boot, keyed by theme [scope-84]: its line, the word before the percentage, and the word that closes it. A word the entry leaves out is the neutral one.
   */
  arrivalWordsByTheme: Record<
    string,
    {
      line?: string;
      progress?: string;
      ready?: string;
    }
  >;
  /**
   * The lines a theme's own boot shows instead of that one line, in order, keyed by theme [S49, A11]. A `{count}` in a line is replaced by a number counting up to `--kp-arrival-count` (640 by default), which is how retro's memory test reads.
   */
  arrivalLinesByTheme: Record<string, string[]>;
  /**
   * The neutral word before the percentage on that line
   */
  arrivalProgress: string;
  /**
   * The neutral word that closes the boot line
   */
  arrivalReady: string;
  /**
   * The button that ends the arrival at once
   */
  arrivalSkip: string;
  /**
   * A live dimension label before the first measurement lands (blueprint) [S48]
   */
  measureLoading: string;
  /**
   * The size of the box the measurement frame holds (blueprint) [scope-18]
   */
  measureBox: (w: number, h: number) => string;
  /**
   * A time chart's plot, for a screen reader: what it is and its keys [scope-143]
   */
  chartPlot: (label: string) => string;
  /**
   * A chart's 24-hour spark line, for a screen reader [scope-143]
   */
  chartSpark: (label: string) => string;
  /**
   * The name of a chart's legend [scope-143]
   */
  chartSources: (label: string) => string;
  /**
   * A legend button's title: what hovering and clicking it do [scope-143]
   */
  chartSource: (label: string) => string;
  /**
   * The legend's way back to every source [scope-143]
   */
  chartShowAll: string;
  /**
   * Its title [scope-143]
   */
  chartShowAllTitle: string;
  /**
   * The hint under a legend, for a pointer [scope-143]
   */
  chartHintPointer: string;
  /**
   * The same hint, for a finger [scope-143]
   */
  chartHintTouch: string;
  /**
   * The word in a pinned tooltip's head [scope-143]
   */
  chartPinned: string;
  /**
   * The pinned tooltip's ✕, named [scope-143]
   */
  chartRelease: string;
  /**
   * The tooltip's foot: what the ▲/▼ column is [scope-143]
   */
  chartChange: string;
  /**
   * An event's link in a pinned tooltip [scope-143]
   */
  chartOpen: string;
  /**
   * An event marker's title [scope-143]
   */
  chartMark: (label: string, time: string) => string;
  /**
   * The zoom chip, before its Reset, from its two ends; used only when a consumer overrides it and not `chartZoomedSpan` [scope-143]
   */
  chartZoomed: (from: string, to: string) => string;
  /**
   * The zoom chip, before its Reset: `span` is the zoom as the chart's `range` time style prints it [scope-143]
   */
  chartZoomedSpan: (span: string) => string;
  /**
   * The zoom chip's button [scope-143]
   */
  chartReset: string;
  /**
   * Its title [scope-143]
   */
  chartResetTitle: string;
  /**
   * A rise, as the readout says it [scope-143]
   */
  chartUp: string;
  /**
   * A fall, as the readout says it [scope-143]
   */
  chartDown: string;
  /**
   * No change, as the readout says it [scope-143]
   */
  chartSame: string;
  /**
   * A spark line's value when no time is under the crosshair [scope-143]
   */
  chartNow: string;
  /**
   * A time chart with no readings in its window, in the plot at its height [scope-143]
   */
  chartEmpty: string;
  /**
   * Under a chart whose every source has one reading (with `onePointNote`) [scope-143]
   */
  chartOnePoint: string;
  /**
   * A loading chart (`data-kp-chart-loading`), for a screen reader [scope-143]
   */
  chartLoading: string;
  /**
   * A pinned tooltip's head once a live update moved the window past the pin [scope-143]
   */
  chartPinnedOutside: string;
  /**
   * A menu button's menu while its entries load: one disabled entry (js/menu-button.js) [scope-143]
   */
  menuLoading: string;
  /**
   * The title of a menu button with no entries, which stays and does nothing (`data-kp-menu-empty="disable"`) [scope-143]
   */
  menuEmpty: string;
  /**
   * The tour card's button to the next step (js/tour.js) [scope-143]
   */
  tourNext: string;
  /**
   * Its title [scope-143]
   */
  tourNextTitle: string;
  /**
   * The tour card's button to the step before [scope-143]
   */
  tourBack: string;
  /**
   * Its title [scope-143]
   */
  tourBackTitle: string;
  /**
   * The next button on the last step, which ends the tour [scope-143]
   */
  tourDone: string;
  /**
   * Its title [scope-143]
   */
  tourDoneTitle: string;
  /**
   * The tour card's button that ends the tour at once [scope-143]
   */
  tourSkip: string;
  /**
   * Its title [scope-143]
   */
  tourSkipTitle: string;
  /**
   * The tour card's count, "1 of 5"; `of` counts only the steps whose part is on the page [scope-143]
   */
  tourCount: (n: number, of: number) => string;
  /**
   * A key figure's trend axis: the word after the first point's clock when it is today ("07:00 today") [scope-143]
   */
  chartToday: string;
  /**
   * The same when it is yesterday ("14:40 yesterday") [scope-143]
   */
  chartYesterday: string;
  /**
   * A key figure's trend, for a screen reader: its keys [scope-143]
   */
  chartTrendKeys: string;
  /**
   * A meter's share, in the words a screen reader hears, when the page names it nothing else ("62% used") [scope-143]
   */
  meterUsed: string;
  /**
   * A meter with no share, for a screen reader [scope-143]
   */
  meterNotMeasured: string;
  /**
   * A loading meter, for a screen reader [scope-143]
   */
  meterMeasuring: string;
  /**
   * The calendar's busy text when the page gives none (`Loading…`)
   */
  loading?: string;
  /**
   * The word a busy picture spells on screen, deciphered letter by letter (cyberpunk's skeletons, busy bars and meters); `setStrings()` hands it to the stylesheet [scope-143]
   */
  loadingWord: string;
  /**
   * The name of a month heatmap's row of month buttons (js/calendar.js) [scope-143]
   */
  calendarNav: string;
  /**
   * A month heatmap's button to the month before; its name and title are `previousMonth` [scope-143]
   */
  calendarPrev: string;
  /**
   * Its button to the month after; its name and title are `nextMonth` [scope-143]
   */
  calendarNext: string;
  /**
   * Its button back to today's month, with today picked [scope-143]
   */
  calendarToday: string;
  /**
   * That button's title [scope-143]
   */
  calendarTodayTitle: string;
  /**
   * A day's name and title: the date (dd/mm/yyyy) and what the page says about it [scope-143]
   */
  calendarDay: (date: string, label: string) => string;
  /**
   * What a day still to come is, when the page says nothing about it [scope-143]
   */
  calendarFuture: string;
  /**
   * What every day is while the calendar loads [scope-143]
   */
  calendarLoading: string;
  /**
   * What a past day is when the page says nothing about it [scope-143]
   */
  calendarUnknown: string;
  /**
   * The network graph's way back to every node and every kind of link (js/graph.js) [scope-143]
   */
  graphShowAll: string;
  /**
   * Its title [scope-143]
   */
  graphShowAllTitle: string;
  /**
   * The hint under the graph, and the second half of its picture's accessible name: what a pointer, a click and the keys do [scope-143]
   */
  graphHint: string;
  /**
   * The name of the list of kinds of link over the graph [scope-143]
   */
  graphKinds: string;
  /**
   * A loading graph's sentence when the page gives none (`setGraphState(el, 'loading')`) [scope-143]
   */
  graphLoading: string;
  /**
   * The picture's name when the graph has no `aria-label` [scope-143]
   */
  graphUnnamed: string;
  breadcrumb: string;
  pagination: string;
  themePicker: string;
  themeSaveFailed: string;
  themeSaveRefused: string;
  contractDestructive: string;
  contractSemantic: string;
  themeGroupLight: string;
  themeGroupDark: string;
  themeUnknown: (requested: string, applied: string) => string;
  /**
   * The console warning when two elements of one component ask to be remembered under the same name
   */
  rememberClash: (name: string, component: string) => string;
  /**
   * The console error when a lazily loaded register did not arrive; the theme stays what it was [scope-50]
   */
  registerLoadFailed: (theme: string, href: string) => string;
  diagnosticsHeading: string;
  diagnosticsStylesheet: string;
  diagnosticsScript: string;
  diagnosticsVersion: string;
  diagnosticsThemes: string;
  diagnosticsVerdict: string;
  diagnosticsMatch: string;
  diagnosticsStylesheetBehind: (stylesheet: string, script: string) => string;
  diagnosticsScriptBehind: (stylesheet: string, script: string) => string;
  diagnosticsThemesDiffer: (version: string) => string;
  diagnosticsNoVersion: string;
  diagnosticsOnlyInStylesheet: (names: string) => string;
  diagnosticsOnlyInScript: (names: string) => string;
  diagnosticsUnknownVersion: string;
};
/**
 * @typedef {object} Strings
 * @property {string} alertSuccess
 * @property {string} alertWarning
 * @property {string} alertInfo
 * @property {string} alertError
 * @property {string} attentionCritical  The word a screen reader hears before a critical problem on the attention band (setAttention) [scope-143]
 * @property {string} attentionWarning  The same, before a warning [scope-143]
 * @property {string} attentionInfo  The same, before an information [scope-143]
 * @property {(verb: string, duration: string) => string} agoText  A ticking freshness line, "updated 12 s ago" (js/freshness.js) [scope-143]
 * @property {(verb: string) => string} agoNever  The same line with no moment yet, "not updated yet" [scope-143]
 * @property {(n: number) => string} agoSeconds  A duration's seconds part, "12 s" [scope-143]
 * @property {(n: number) => string} agoMinutes  Its minutes part, "2 min" [scope-143]
 * @property {(n: number) => string} agoHours  Its hours part, "3 h" [scope-143]
 * @property {(n: number) => string} agoDays  Its days part, "1 day", "2 days" [scope-143]
 * @property {string} agoVerb  The verb a freshness line uses when its element names none [scope-143]
 * @property {(text: string) => string} agoStale  What a freshness line with `data-kp-ago-announce="state"` announces once it turns stale [scope-143]
 * @property {(text: string) => string} agoFresh  What it announces once it is fresh again [scope-143]
 * @property {string} agoInLiveRegion  The console warning for a ticking line inside a live region [scope-143]
 * @property {string} busy
 * @property {string} close
 * @property {string} menu          The accessible name of a collapsed navigation's toggle
 * @property {string} closeMenu     The same toggle once the navigation is open
 * @property {string} navDisclosure  The accessible name of a mega-menu button in the bar that holds no words of its own [scope-48]
 * @property {string} sidebar       The accessible name of a hidden side navigation's toggle
 * @property {string} closeSidebar  The same toggle once that side navigation is open
 * @property {string} collapseRail  The accessible name of an empty slim-rail toggle while the rail is expanded
 * @property {string} expandRail    The same toggle while the rail is collapsed to its icons
 * @property {string} resizeSidebar The name of the resize handle on a resizable side navigation
 * @property {string} backToTop    The control that returns the reader to the top of the page
 * @property {string} previous
 * @property {string} next
 * @property {string} finish
 * @property {string} back
 * @property {(name: string) => string} removeNamed
 * @property {string} noResults
 * @property {string} oneResult
 * @property {(n: number) => string} manyResults
 * @property {string} noCommands
 * @property {string} oneCommand
 * @property {(n: number) => string} manyCommands
 * @property {string} commandPlaceholder
 * @property {string} commandsLabel
 * @property {string} shortcutsLabel
 * @property {string} paletteTrigger  The visible word on the command palette's trigger in the bar [scope-48]
 * @property {(key: string, mac: boolean) => string} paletteHotkey  The hotkey as that trigger prints it: ⌘K on a Mac, Ctrl K elsewhere [scope-48]
 * @property {string} tableSearch
 * @property {string} tableSearchLabel
 * @property {string} tableSelectAll
 * @property {(key: string) => string} tableSelectRow
 * @property {string} tableEmpty
 * @property {(n: number) => string} tableRows
 * @property {(shown: number, total: number) => string} tableRowsFiltered
 * @property {(at: number, of: number) => string} tablePage
 * @property {string} tableRegion
 * @property {string} tableSearchScope      The name of the "In" choice beside the search box
 * @property {string} tableSearchAllColumns The "In" choice's first option
 * @property {(active: number) => string} tableFilters  The filter panel's toggle, with the number of active filters
 * @property {string} tableFiltersLabel     The filter panel's group name
 * @property {string} tableActiveFilters    The list of removable filter pills
 * @property {(column: string, value: string) => string} tableFilterValue  A choice filter's pill
 * @property {(column: string, from: string, to: string) => string} tableFilterRange  A range or date filter's pill; an open end is `tableFilterOpenEnd`
 * @property {string} tableFilterOpenEnd    The open end of a range in its pill
 * @property {(column: string) => string} tableFilterFrom  The lower bound's accessible name
 * @property {(column: string) => string} tableFilterTo    The upper bound's accessible name
 * @property {(label: string) => string} tableRemoveFilter  A pill's remove button
 * @property {string} tableClearFilters
 * @property {string} tableClearSearch      The way out of the no-match state
 * @property {(active: number) => string} tableAddFilter  The "+ Add filter" button of a table in the add-filter mode, with the number of active filters
 * @property {string} tableAddFilterMenu    The name of its menu of filterable columns
 * @property {(column: string) => string} tableAddFilterItem  A column in that menu, not filtered yet
 * @property {(column: string) => string} tableAddFilterItemActive  A column in that menu that is filtered already: choosing it edits that filter
 * @property {string} tableFilterMarked     The mark beside a filtered column in the menu
 * @property {(column: string) => string} tableFilterEditor  The editor's accessible name
 * @property {(column: string) => string} tableFilterChoicesLegend  The editor's list of choices, for a screen reader
 * @property {(kind: 'range' | 'date', bound: 'from' | 'to', column: string) => string} tableFilterBound  A bound's visible label in the editor
 * @property {(column: string, values: string[]) => string} tableFilterChoicePill  A choice filter's one pill in the add-filter mode
 * @property {(column: string, from: string, to: string, kind: 'range' | 'date') => string} tableFilterSpanPill  A range or date filter's pill in the add-filter mode; an open end is left out
 * @property {(label: string) => string} tableEditFilter  A pill's own button, which reopens its editor
 * @property {string} tableFilterApply
 * @property {string} tableFilterCancel
 * @property {string} tableFilterClearAll   The add-filter mode's way out of every filter, shown from two pills
 * @property {(label: string, value: string) => string} tableFilterNotNumber  A number bound that is not a number
 * @property {(label: string, value: string) => string} tableFilterNotDate  A date bound the picker cannot read
 * @property {(from: string, to: string) => string} tableFilterBackwards  A range whose lower bound is above its upper
 * @property {string} tableDensity
 * @property {string} tableDensityComfortable
 * @property {string} tableDensityCompact
 * @property {string} tableRowsPerPage
 * @property {(from: number, to: number, count: number, total: number) => string} tableShowing  The status line: "Showing 1–25 of 60"
 * @property {(n: number) => string} tableSelected  The action bar's count
 * @property {string} tableClearSelection
 * @property {string} tableSortBy           The card layout's sort control
 * @property {string} tableSortNone         Its first option
 * @property {string} tableSortAscending
 * @property {string} tableSortDescending
 * @property {string} tableFailed
 * @property {string} tableRetry
 * @property {(column: string, direction: 'ascending' | 'descending', kind: string) => string} tableSortKey  One key of a multi-column sort in words; `kind` is text, number, date or order
 * @property {(keys: string[]) => string} tableSortedBy  The multi-sort summary, from the keys in order
 * @property {string} tableNotSorted
 * @property {string} tableSortReset
 * @property {(seconds: number) => string} tableBusyElapsed
 * @property {string} tableColumns          The column menu's button
 * @property {string} tableColumnsLabel     The column menu's name
 * @property {(column: string) => string} tableColumnLocked  A column that cannot be hidden, in the menu
 * @property {string} tableShowAllColumns
 * @property {(shown: number, total: number) => string} tableColumnsShown
 * @property {string} tableDetailsColumn    The expansion column's header, read by a screen reader only
 * @property {(key: string) => string} tableRowDetails  A row's expand button
 * @property {(group: string, count: number) => string} tableGroupRows  A group row's fold button: the group's name and how many of its rows match [J2]
 * @property {(column: string, key: string, value: string) => string} tableEdit  An editable cell's button
 * @property {(column: string, key: string) => string} tableEditField  The editor's accessible name
 * @property {(column: string, key: string) => string} tableEditing  Said when an editor opens
 * @property {(key: string, column: string, before: string, after: string) => string} tableEdited
 * @property {(key: string, column: string, value: string) => string} tableEditUndone
 * @property {string} tableEditCancelled
 * @property {string} tableEditRequired
 * @property {string} tableEditInvalid     A rejected edit with no message of its own
 * @property {string} tableGridStart        The keyboard grid's line before the first cell is focused
 * @property {(row: number, rows: number, column: string, text: string) => string} tableGridPosition  Row 0 is the header row
 * @property {string} formRequired
 * @property {string} formInvalid
 * @property {string} formSummaryOne
 * @property {(n: number) => string} formSummaryMany
 * @property {string} fieldFallbackName
 * @property {string} switchOn   The word beside a switch that is on [gap-11]
 * @property {string} switchOff  The word beside a switch that is off [gap-11]
 * @property {string} calendarOpen
 * @property {string} calendarButton
 * @property {string} dateFormatHint
 * @property {string} previousMonth
 * @property {string} nextMonth
 * @property {(month: string, year: number) => string} monthTitle
 * @property {(title: string) => string} chooseMonth  The month title's name: it opens the twelve months [scope-89]
 * @property {(year: number) => string} chooseYear    The year title's name: it opens twelve years [scope-89]
 * @property {string} previousYear
 * @property {string} nextYear
 * @property {string} previousYears  Twelve years back, in the year grid
 * @property {string} nextYears      Twelve years on, in the year grid
 * @property {(year: number) => string} monthGrid           The month grid's name, and what is said when it opens
 * @property {(from: number, to: number) => string} yearGrid The year grid's name, and what is said when it opens
 * @property {(from: number, to: number) => string} yearRange The year grid's title
 * @property {string[]} weekdays
 * @property {string[]} months
 * @property {string[]} monthsShort  The month grid's cells, where a full name does not fit [scope-89]
 * @property {(day: number, month: string, year: number) => string} dayLabel
 * @property {string} uploadZone
 * @property {(size: string) => string} uploadTooLarge
 * @property {(max: number) => string} uploadTooMany
 * @property {(max: string) => string} uploadTotalTooLarge
 * @property {(accept: string) => string} uploadWrongType
 * @property {(name: string) => string} uploadProgress
 * @property {(at: number, of: number) => string} wizardStep
 * @property {string} copy
 * @property {string} copied
 * @property {string} copyBlocked
 * @property {(value: string) => string} copiedAnnouncement
 * @property {string} copyBlockedAnnouncement
 * @property {string} undo
 * @property {string} deleted
 * @property {string} splitLabel
 * @property {(name: string) => string} reorderHandle
 * @property {(name: string, at: number, of: number) => string} reorderMoved
 * @property {string} tileFallbackName
 * @property {(name: string, column: number, row: number, w: number, h: number) => string} tileLabel
 * @property {(token: string) => string} contrastMissing
 * @property {(ratio: string, token: string, verdict: string) => string} contrastReport
 * @property {string} colourHue
 * @property {string} colourSaturation
 * @property {string} colourLightness
 * @property {string} contrastPasses
 * @property {string} contrastFails
 * @property {string} confirm
 * @property {string} confirmAccept
 * @property {string} confirmCancel
 * @property {string} confirmDescription
 * @property {string} alarmAction  The button that closes an alarm that must be acknowledged, when the caller names none [scope-94]
 * @property {string} alarmKeepOpen  The button that stops an alarm's countdown and keeps it open [scope-94]
 * @property {string} alarmHint  The key hint beside the acknowledge button; hidden from a screen reader [scope-94]
 * @property {(seconds: number) => string} alarmCountdown  The whole seconds left under the countdown bar; hidden from a screen reader [scope-94]
 * @property {(seconds: number) => string} alarmClosesBy  Said once when an alarm that closes by itself opens [scope-94]
 * @property {(action: string) => string} alarmPressTo  Said when an alarm that must be acknowledged opens [scope-94]
 * @property {(action: string) => string} alarmKeptOpen  Said when Keep open stopped the countdown [scope-94]
 * @property {string} save
 * @property {string} mainNavigation
 * @property {string} skipToContent
 * @property {string} diagnosticsEffects  The diagnostics row that lists unknown hook values [AR44]
 * @property {string} diagnosticsEffectsNone
 * @property {string} classified  The stamp a register may print on an emphasis reveal (the dossier) [AR35]
 * @property {string} arrivalLine  The neutral boot line of an arrival, for a theme that performs one and has no words of its own in `arrivalWordsByTheme` or `arrivalLinesByTheme` [SW2, scope-84]
 * @property {Record<string, { line?: string, progress?: string, ready?: string }>} arrivalWordsByTheme  A theme's own words for the counting boot, keyed by theme [scope-84]: its line, the word before the percentage, and the word that closes it. A word the entry leaves out is the neutral one.
 * @property {Record<string, string[]>} arrivalLinesByTheme  The lines a theme's own boot shows instead of that one line, in order, keyed by theme [S49, A11]. A `{count}` in a line is replaced by a number counting up to `--kp-arrival-count` (640 by default), which is how retro's memory test reads.
 * @property {string} arrivalProgress  The neutral word before the percentage on that line
 * @property {string} arrivalReady  The neutral word that closes the boot line
 * @property {string} arrivalSkip  The button that ends the arrival at once
 * @property {string} measureLoading  A live dimension label before the first measurement lands (blueprint) [S48]
 * @property {(w: number, h: number) => string} measureBox  The size of the box the measurement frame holds (blueprint) [scope-18]
 * @property {(label: string) => string} chartPlot  A time chart's plot, for a screen reader: what it is and its keys [scope-143]
 * @property {(label: string) => string} chartSpark  A chart's 24-hour spark line, for a screen reader [scope-143]
 * @property {(label: string) => string} chartSources  The name of a chart's legend [scope-143]
 * @property {(label: string) => string} chartSource  A legend button's title: what hovering and clicking it do [scope-143]
 * @property {string} chartShowAll  The legend's way back to every source [scope-143]
 * @property {string} chartShowAllTitle  Its title [scope-143]
 * @property {string} chartHintPointer  The hint under a legend, for a pointer [scope-143]
 * @property {string} chartHintTouch  The same hint, for a finger [scope-143]
 * @property {string} chartPinned  The word in a pinned tooltip's head [scope-143]
 * @property {string} chartRelease  The pinned tooltip's ✕, named [scope-143]
 * @property {string} chartChange  The tooltip's foot: what the ▲/▼ column is [scope-143]
 * @property {string} chartOpen  An event's link in a pinned tooltip [scope-143]
 * @property {(label: string, time: string) => string} chartMark  An event marker's title [scope-143]
 * @property {(from: string, to: string) => string} chartZoomed  The zoom chip, before its Reset, from its two ends; used only when a consumer overrides it and not `chartZoomedSpan` [scope-143]
 * @property {(span: string) => string} chartZoomedSpan  The zoom chip, before its Reset: `span` is the zoom as the chart's `range` time style prints it [scope-143]
 * @property {string} chartReset  The zoom chip's button [scope-143]
 * @property {string} chartResetTitle  Its title [scope-143]
 * @property {string} chartUp  A rise, as the readout says it [scope-143]
 * @property {string} chartDown  A fall, as the readout says it [scope-143]
 * @property {string} chartSame  No change, as the readout says it [scope-143]
 * @property {string} chartNow  A spark line's value when no time is under the crosshair [scope-143]
 * @property {string} chartEmpty  A time chart with no readings in its window, in the plot at its height [scope-143]
 * @property {string} chartOnePoint  Under a chart whose every source has one reading (with `onePointNote`) [scope-143]
 * @property {string} chartLoading  A loading chart (`data-kp-chart-loading`), for a screen reader [scope-143]
 * @property {string} chartPinnedOutside  A pinned tooltip's head once a live update moved the window past the pin [scope-143]
 * @property {string} menuLoading  A menu button's menu while its entries load: one disabled entry (js/menu-button.js) [scope-143]
 * @property {string} menuEmpty  The title of a menu button with no entries, which stays and does nothing (`data-kp-menu-empty="disable"`) [scope-143]
 * @property {string} tourNext  The tour card's button to the next step (js/tour.js) [scope-143]
 * @property {string} tourNextTitle  Its title [scope-143]
 * @property {string} tourBack  The tour card's button to the step before [scope-143]
 * @property {string} tourBackTitle  Its title [scope-143]
 * @property {string} tourDone  The next button on the last step, which ends the tour [scope-143]
 * @property {string} tourDoneTitle  Its title [scope-143]
 * @property {string} tourSkip  The tour card's button that ends the tour at once [scope-143]
 * @property {string} tourSkipTitle  Its title [scope-143]
 * @property {(n: number, of: number) => string} tourCount  The tour card's count, "1 of 5"; `of` counts only the steps whose part is on the page [scope-143]
 * @property {string} chartToday  A key figure's trend axis: the word after the first point's clock when it is today ("07:00 today") [scope-143]
 * @property {string} chartYesterday  The same when it is yesterday ("14:40 yesterday") [scope-143]
 * @property {string} chartTrendKeys  A key figure's trend, for a screen reader: its keys [scope-143]
 * @property {string} meterUsed  A meter's share, in the words a screen reader hears, when the page names it nothing else ("62% used") [scope-143]
 * @property {string} meterNotMeasured  A meter with no share, for a screen reader [scope-143]
 * @property {string} meterMeasuring  A loading meter, for a screen reader [scope-143]
 * @property {string} [loading]  The calendar's busy text when the page gives none (`Loading…`)
 * @property {string} loadingWord  The word a busy picture spells on screen, deciphered letter by letter (cyberpunk's skeletons, busy bars and meters); `setStrings()` hands it to the stylesheet [scope-143]
 * @property {string} calendarNav  The name of a month heatmap's row of month buttons (js/calendar.js) [scope-143]
 * @property {string} calendarPrev  A month heatmap's button to the month before; its name and title are `previousMonth` [scope-143]
 * @property {string} calendarNext  Its button to the month after; its name and title are `nextMonth` [scope-143]
 * @property {string} calendarToday  Its button back to today's month, with today picked [scope-143]
 * @property {string} calendarTodayTitle  That button's title [scope-143]
 * @property {(date: string, label: string) => string} calendarDay  A day's name and title: the date (dd/mm/yyyy) and what the page says about it [scope-143]
 * @property {string} calendarFuture  What a day still to come is, when the page says nothing about it [scope-143]
 * @property {string} calendarLoading  What every day is while the calendar loads [scope-143]
 * @property {string} calendarUnknown  What a past day is when the page says nothing about it [scope-143]
 * @property {string} graphShowAll  The network graph's way back to every node and every kind of link (js/graph.js) [scope-143]
 * @property {string} graphShowAllTitle  Its title [scope-143]
 * @property {string} graphHint  The hint under the graph, and the second half of its picture's accessible name: what a pointer, a click and the keys do [scope-143]
 * @property {string} graphKinds  The name of the list of kinds of link over the graph [scope-143]
 * @property {string} graphLoading  A loading graph's sentence when the page gives none (`setGraphState(el, 'loading')`) [scope-143]
 * @property {string} graphUnnamed  The picture's name when the graph has no `aria-label` [scope-143]

 * @property {string} breadcrumb
 * @property {string} pagination
 * @property {string} themePicker
 * @property {string} themeSaveFailed
 * @property {string} themeSaveRefused
 * @property {string} contractDestructive
 * @property {string} contractSemantic
 * @property {string} themeGroupLight
 * @property {string} themeGroupDark
 * @property {(requested: string, applied: string) => string} themeUnknown
 * @property {(name: string, component: string) => string} rememberClash  The console warning when two elements of one component ask to be remembered under the same name
 * @property {(theme: string, href: string) => string} registerLoadFailed  The console error when a lazily loaded register did not arrive; the theme stays what it was [scope-50]
 * @property {string} diagnosticsHeading
 * @property {string} diagnosticsStylesheet
 * @property {string} diagnosticsScript
 * @property {string} diagnosticsVersion
 * @property {string} diagnosticsThemes
 * @property {string} diagnosticsVerdict
 * @property {string} diagnosticsMatch
 * @property {(stylesheet: string, script: string) => string} diagnosticsStylesheetBehind
 * @property {(stylesheet: string, script: string) => string} diagnosticsScriptBehind
 * @property {(version: string) => string} diagnosticsThemesDiffer
 * @property {string} diagnosticsNoVersion
 * @property {(names: string) => string} diagnosticsOnlyInStylesheet
 * @property {(names: string) => string} diagnosticsOnlyInScript
 * @property {string} diagnosticsUnknownVersion
 */
/**
 * The defaults. English, by Kenny's decision of 2026-09-04 — the package
 * has to speak something, and English is the language a consumer is least
 * likely to have to replace.
 *
 * @type {Strings}
 */
export declare const DEFAULT_STRINGS: Strings;
/**
 * Replace some or all of the strings, for the framework-free channel and
 * for anything that reads them outside React.
 *
 * Merged rather than replaced: a consumer that wants one word does not
 * have to restate the other seventy, and a key added in a later version
 * keeps working instead of becoming `undefined` on their page.
 *
 * @param {Partial<Strings>} next
 * @returns {Strings} the merged result
 */
export declare function setStrings(next: Partial<Strings>): Strings;
/**
 * The words the stylesheets draw themselves, as custom properties
 * [scope-143]. A keyframe cannot read this module, so the word a busy
 * picture spells reaches it as `--kp-loading-word-0` to `-7`: the eight
 * ticks of cyberpunk's decipher, from all noise to the whole word. The
 * register keeps the English as each property's fallback, so a page that
 * never calls `setStrings()` draws what it always drew.
 *
 * @param {Strings} [strings] the strings to draw from, the current ones by default
 * @returns {Record<string, string>} property name to a CSS string value
 */
export declare function stringProperties(
  strings?: Strings,
): Record<string, string>;
/**
 * Write `stringProperties()` onto an element, so every stylesheet under it
 * draws the consumer's words. `setStrings()` does this on the document
 * root; call it yourself for a subtree that speaks another language.
 *
 * @param {HTMLElement} root
 * @param {Strings} [strings]
 */
export declare function applyStringProperties(
  root: HTMLElement,
  strings?: Strings,
): void;
/** @returns {Strings} the strings as they stand */
export declare function getStrings(): Strings;
/**
 * The strings a component should use: the global ones, with anything the
 * caller passed layered on top.
 *
 * @param {Partial<Strings>} [overrides]
 * @returns {Strings}
 */
export declare function resolveStrings(overrides?: Partial<Strings>): Strings;
