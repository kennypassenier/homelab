# kp-themes 10.1.1 adoption inventory for the admin dashboard

Read-only research for release 3.74.1 (chassis-rs v3.7.1, which vendors
kp-themes 10.1.1). Kenny's ask: use the new kp-themes components, replace the
dashboard's home-made copies where possible, use every kp-themes feature that
fits, and remove each workaround that exists because a component was missing.

Scope read: `admin/web/js`, `admin/web/css` (app.css 5838 lines, pages/*.css
7696 lines), `admin/web/test`, `admin/web/test-e2e`, `docs/` (REGISTER.md,
`docs/admin/*`), against `/home/kenny/Projects/kp-themes` (tag v10.1.1:
CHANGELOG.md, MIGRATION.md, docs/USER_GUIDE.md, `js/`, `css/components.css`) and
`/home/kenny/Projects/chassis-rs` (CHANGELOG 3.7.1, `crates/chassis/static/kp`).

Conventions. Evidence is `file:line` relative to `admin/web/` unless it starts
with `docs/`, `admin/` or `kp:` (= kp-themes repo). Size is small / medium /
large for the swap. **(inferred)** marks a claim I did not verify by running the
page or reading the exact kp code path; everything else was read in the source.
Nothing was run or built (no tests, no browser): behaviour notes are from code.

State of adoption today (verified): the dashboard imports 23 kp modules
(`datatable, forms, overlays, motion, wizard, palette, log, kpi, update, tiles,
theme-picker, theme-core, tables, progressbar, patterns, menu-button, freshness,
effects, components, chart, attention, alarm, actions`). It never imports
`calendar, graph, tour, remember, datepicker, combobox (attachSelects), listbox,
structure, sidenav, top-layer, upload, loading-pieces, strings, locale, as-of`.
It does not use `js/auto.js` (every module is attached by hand), so a module that
auto.js would start for a page is only active here if the dashboard calls it.

---

## 1. Home-made components that kp-themes 10.1.1 replaces

### kpa-1 KPI tiles and strip (large)
- Home-made: `js/ui.js:335-516` (`kpi`, `kpiStrip`, `ctxBar`), CSS `css/app.css:3435-3551`,
  `4562-4625` (`.nx-kpis`, `.nx-kpi*`, `--kpi-n` + two media queries at `3440-3456`),
  `.nx-ops` variants `4928-5075`, `.bk-page` `5120-5160`. Eight callers of `kpiStrip`:
  `pages/fleetview.js:237`, `backups.js:358`, `stack.js:831`, `firewall.js:329`,
  `overview.js:370`, `metrics.js:465,952`, `activity.js:549`.
- kp replacement: `.kp-kpis` + `.kp-kpi` (`kp:css/components.css`, USER_GUIDE.md:930-1050),
  `kp:js/kpi.js` `attachKpiStrips` (columns by the strip's own width, `data-kp-kpis-columns`),
  `attachKpiToggles` (`button.kp-kpi--toggle`, event `kp-kpi-toggle`, hint `data-kp-when`),
  `setMeter` / `meterParts` / `meterText`, `drawSparkline` (already used by `ui.js:949`).
  Parts: `__label __value (small unit) __note __trend __delta __spark __meter __link`.
- Differences / risk: the label no longer ends in an ellipsis, it wraps (10.1.0), so tiles
  get taller; homelab's own label is `white-space:nowrap; text-overflow:ellipsis`
  (`css/app.css:3472-3481`). A toggle tile must stay a declared Live view control
  (`ui.js:372-377` `drive(e, k.toggle.drive)`), so the `decorate`/data hook must re-declare it.
  `ctxMeter`, `ctxParts` (dot-led context), `tone` and the `target` scroll link have no 1:1 kp part
  **(inferred: map to `__note`/`__trend` and `kp-health`)**. `evidence of pinning`: 43 `nx-kpi`
  hits in `test-e2e/invariants.e2e.js`, plus `test-e2e/layoutaudit.js`, `test/uikit_dom.test.js`,
  `test/reviewmetrics.test.js`, `test/drivecatalog.test.js`.

### kpa-2 Stat tiles (small-medium)
- Home-made: `js/dom.js:597-740` (`statGrid`, `fillStatGrid`, `statTile`), CSS `css/app.css:167-200`
  (`.stat-grid`, `.stat-tile*`). 9 hits in 2 files (batch preview in `actiondialog.js`).
- kp: `.kp-kpis`/`.kp-kpi` without trend, or `.kp-spec` (spec sheet, `kp:docs/USER_GUIDE.md` "data" page).
  Same family as kpa-1; do together.

### kpa-3 Meter, share bar, in-tile meter (small)
- Home-made: `ui.js:2022-2062` (`meter`, `shareBar`), `ui.js:296-316` (`meterParts`), CSS
  `css/app.css:4269-4291`, `4331-4340` (`.nx-meter`, `.nx-share*`).
- kp: `.kp-meter` (+ `.kp-meter--inline`, mark/tick, tone, loading) in `kp:css/components.css:1308-1400`;
  `kp:js/kpi.js` `setMeter(el, {value, mark, tone, label, markLabel, loading})`.
- Risk: `test/uikit.test.js` pins `meterParts` clamping (dashboard copy); kp has its own
  `meterParts(value, mark)` with a different signature.

### kpa-4 Chips, dots, state pills (medium, wide fan-out)
- Home-made: `ui.js:1987-2020` (`chip`, `dot`), `dom.js:120` (`badgeCell`, `td.state.ok|warn|bad`),
  `actui.js:171` (`badge`), CSS `css/app.css:283-300` (`.state`), `4221-4268` (`.nx-chip*`),
  `.nx-dot*`, `.hk-dot*` (`css/pages/host.css:60-100`).
- kp: `.kp-badge--success|info|warning|destructive` (`kp:css/components.css` ~979-994),
  `.kp-health` + `data-state="ok|warn|down"` + `.kp-health__dot` (components.css:7398-7420),
  `.kp-tag`/`.kp-tag-list`.
- Risk: the dashboard colours chips with `--success`/`--warning` + `*-foreground` pairs
  (`css/app.css:4247-4262`, comment "the kp-themes pairing that holds in every theme"); kp badges
  already do that. Contrast invariants (`invariants.e2e.js` redesign-final-contrast, two themes
  `dark`/`light`) must be re-run. `live` tone and `tone=""` neutral have no 1:1 kp variant
  **(inferred; neutral = plain `.kp-badge`)**.

### kpa-5 Home-made spinners and busy marks (small; this is gap-themeuni-1)
- Home-made: `.nx-spin` (`js/chrome.js:305`, CSS `css/app.css:3185-3196`, keyframes `nx-rot`
  declared twice: `3193` and `4517`), the running-step ring `.nx-check li[data-s="run"] .nx-check__st`
  (`css/app.css:4480-4520`), the step bar `.ac-step--whole[data-s="run"] i::after` with `@keyframes
  ac-busy` (`css/pages/activity.css:172-210`, `js/pages/activity.js:1077-1094`).
- kp: `.kp-spinner` (already used once: `js/pages/host.js:1197`), `.kp-progressbar[data-kp-indeterminate]`
  (`kp:js/progressbar.js`, already used by `dom.js:754`).
- Why: each theme draws its own spinner/bar in `kp.signature`; a `border + rotate` circle is the same in
  every theme, which is exactly Kenny's 2026-10-09 complaint (REGISTER.md gap-themeuni-1, now
  "moved to kp-themes"; kp 10.0.0 delivered the per-theme grammar, the dashboard only has to use the parts).
- Behaviour: the 10 px circle in the running pill needs `.kp-spinner` sizing (check `--kp-spinner-size`
  **(inferred)**).

### kpa-6 Hand-drawn shimmer skeletons and pulses (small)
- Home-made: `.sch-sk` (`css/pages/schedules.css:90-110`, used `js/pages/schedules.js:311,327,338`),
  `.sx .sk` (`css/pages/secrets.css:56-70`, `js/pages/secrets.js:190`), `bk-heat__cell--load` pulse
  (`css/app.css:5346-5352`), `hk-pulse` (`css/pages/host.css:80-100`), `sx-pulse`
  (`css/pages/secrets.css:21-33`), `.hl-ln--sk` (`css/pages/activity.css:742-750`).
- kp: `.kp-skeleton` (+ `--block`, `--circle`; already used by `skeletonLines/Table/Block`,
  `ui.js:787-835`), `data-kp-state="loading"`. Pastel/forest/nostromo/titanium each draw their own
  loading picture (CHANGELOG 10.0.0), a grey linear-gradient shimmer overrides that.
- The live dot pulse (`hk-pulse`, `sx-pulse`) belongs to `.kp-ago` / freshness (kpa-8).

### kpa-7 Freshness line with a dot (small)
- Home-made: `ui.js:140-192` (`liveStatus`: dot + `agoEl`), `.nx-live` CSS; `format.js:141` `agoText`
  and `format.js:19` `humanDuration` duplicate `kp:js/freshness.js` `agoText` / `humanDuration`.
- kp: `[data-kp-ago]` + `data-kp-stale-after` (stale plate drawn by the theme) already powers `ago.js`.
  The extra dot/"busy words"/fail state is the dashboard's; the dot could be `.kp-health`.
  `humanDuration` copies are used by `tablestate.js:6`, `updateflow.js:10`, `stacksview.js:10`,
  `activityview.js:10`, `pages/restore.js:16`; their wording is pinned by `test/format.test.js`
  (kp's `humanDuration` gives "2 min 5 s", homelab's drops a zero second part **(inferred equal, verify)**).

### kpa-8 State word that keeps its width (small) -- a workaround, see kpa-31
- `dom.js:532` `stateWord`, CSS `css/app.css:1547-1560` (`.state-word`, `data-size`), 1 caller.

### kpa-9 Hover card / tooltip (medium)
- Home-made: `ui.js:1447-1519` (`tipPlace`, `hoverCard`, `pageTip`), CSS `css/app.css:4763-4795`
  (`.nx-tip`); 3 files, 14 `pageTip` hits; pinned by `test/uikit.test.js` (`tipPlace`) and
  `invariants.e2e.js` (2 hits).
- kp: `.kp-tooltip-anchor` + `[role=tooltip]` + `attachTooltips` (`kp:js/overlays.js:540-650`:
  open/close delay, Esc, reverse-entrance leave, `aria-describedby`).
- Risk: the dashboard keeps ONE shared tooltip node filled on demand (`fill()` per target); kp wants a
  tooltip element per anchor. A rich, lazily built card is not a drop-in **(inferred)**.

### kpa-10 Row menu (medium)
- Home-made: `ui.js:1671-1830` (`menuPlace`, `menuKey`, `rowMenu`: non-modal `<dialog role=menu>`,
  own arrow-key walk, own outside-click), CSS `css/app.css:4845-4887` (`.nx-rowmenu`); 2 callers;
  pinned by `uikit.test.js` / `uikit_dom.test.js` and 7 `nx-rowmenu` hits in `invariants.e2e.js`.
- kp: `.kp-menu-button` + `.kp-menu` + `setMenu/openMenu/closeMenu` (`kp:js/menu-button.js`), which
  `moreMenu` (`ui.js:1840`) already drives. Adds viewport flip, `--kp-menu-anchor-x`, `--kp-menu-count`,
  rich entries with hints, `kp-menu__item--destructive`, the theme's open/close motion.
- Behaviour: kp's menu is a popover, not a dialog; the Live view `dialogControl` naming of each item
  must move to `decorate(part, info)` (as `moreMenu` does at `ui.js:1887-1891`).

### kpa-11 Drawers and sheets (medium-large)
- Home-made: `ui.js:2161-2226` (`drawer`), `chrome.js:370-383` (job drawer), `chrome.js:488-502`
  (phone "More" sheet), `helptour.js:367-412` (Help), `pages/overview.js:1301,1364`, `css/pages/secrets.css:373`
  (`.sx-drawer`), CSS `css/app.css:3289-3334` (`dialog.nx-drawer`, `__head/__body/__foot`),
  `css/pages/help.css`.
- kp: `dialog.kp-drawer.kp-dialog` with `.kp-drawer__head/__desc/__body/__foot` and the `.kp-help` card
  with `.kp-help__list` (USER_GUIDE.md:1655-1700; ports the dashboard's own Help panel).
- Risk: kp's drawer is an end-edge panel; the phone More sheet is a bottom sheet and the close-button
  placement fix in `help.css:5-14` (absolute corner) hints the kp markup differs **(inferred)**.

### kpa-12 Help tour (medium)
- Home-made: `helptour.js:75-300` (`tourSteps` with `desktop`/`phone` selectors, `shouldTour`, `startTour`,
  `.tour-target` ring, card positioning), CSS in `css/pages/help.css`, `TOURED_KEY = "homelab-toured"`
  (`helptour.js:32`, localStorage at `:148,157`).
- kp: `kp:js/tour.js` `startTour(steps, {start, remember, returnFocus, decorate, strings})`,
  `shouldStartTour({search, remembered, automated})` (`?tour` / `?tour=3`), `tourRemembered`,
  `forgetTour`; card `.kp-tour`, part `[data-kp-tour-target]` (ring + dim); steps whose target is missing are
  dropped from the count; goes inside an open modal dialog.
- Behaviour change: kp remembers under `kp-remember:tour:<name>:done` (js/remember.js), so every
  existing browser sees the first-visit tour once more unless the old key is read once and mapped.
  `flows.test.js` pins `tourSteps`/`shouldTour`; `invariants.e2e.js` has 23 `tour` hits. Phone-vs-desktop
  targets become a `target: () => ...` function.

### kpa-13 Backup calendar (large)
- Home-made: `js/backupcalendar.js` (328 lines, pure model: month cells, ratio tone), `js/pages/backupcalendar.js`
  (652 lines; grid `:152-205`, nav `:343`, legend `:326-337`, selection/keys), CSS `css/app.css:2370-2672`
  (about 300 lines, `.backup-cal*`). The register row fix-214 literally says Kenny wanted "a datepicker per
  month". Guard: `test/backupcalendar.test.js`, 18 `backup-cal` hits in `invariants.e2e.js`.
- kp: `section.kp-calendar[data-kp-calendar]` + `kp-calendar-layout` (`kp:js/calendar.js`,
  USER_GUIDE.md:1262-1340): `setCalendarState('loading'|'ready'|'empty'|'error', words)`,
  `setCalendarDays(el, {'YYYY-MM-DD': {tone, count, label}})`, `setCalendarLegend`, event
  `kp-calendar-pick {date}`, `calendarSelect/calendarMonth`, keys (arrows, Home/End, PageUp/Down, Enter),
  six-week grid, `data-kp-week-starts-on="1"`, timezone Europe/Brussels default (same as `format.js ZONE`).
  Tones are the same words the dashboard already computes: `ok warn bad muted future before none`
  (`js/backupcalendar.js:47,82-87`).
- Keep: the pure model (`dayDetail`, ratio verdict) and the per-stack progress chips; drop: grid, nav,
  legend, keyboard, skeleton cells (kp pulses every day while `loading`, which also matches the existing
  "grid at final geometry" rule).
- Risk: today the detail panel is beside the grid; `.kp-calendar-layout` does that under 45rem fallback.

### kpa-14 Topology / Map graph (large)
- Home-made: `js/topology.js` (411 lines: circle layout, hues, legend, hover isolation), `js/mapview.js` (391),
  `js/pages/mapgraph.js` (304), `pages/fleetview.js` (graph vs list switch), CSS `css/app.css:2672-2905`,
  `css/pages/map.css` (451 lines). Tests: `topology.test.js`, `mapview.test.js`, `fwview.test.js`;
  59 topology/`.mp-` hits in `invariants.e2e.js`.
- kp: `figure.kp-graph[data-kp-graph]` (`kp:js/graph.js`, USER_GUIDE.md:1493-1580): hub + ring A-Z, per-node
  evenly spread `hue` (or own), `weight` 0..1 sizes a node, `flag:'mismatch'`, `external`, edge `kind`s with
  `style` solid/dash/dot/long-dash and `colour`, kind toggles + Show all, hover isolates links, click picks,
  arrow-key walk, `setGraphData` live update, `setGraphState`, `decorate`, event `kp-graph-change`.
  This is the shape fix-207 asked for (own hue per stack, hover isolates), now upstream.
- Gaps to check before swapping **(inferred)**: the Firewall page's traffic ring (`topology.js:149-192`,
  radius grows with bytes/s) maps to `weight`; "named / open / planned / route" edge kinds map to `kinds[]`;
  `stackHues()` is also used by `backups.js:49,88` for swatches (keep that export).

### kpa-15 Logs that are not `.kp-log` (medium; this is gap-kplog-1)
- Already kp: job panel `jobpanel.js:51-64,338`, Activity running jobs `pages/activity.js:953` (+ `attachLogs`).
- Not yet kp-log:
  - bulk updater / Update flow: `<pre class="uf-log">` filled with `textContent` (`pages/update.js:372-376`,
    `:911-916`), CSS `css/pages/update.css:206`. Reuse `jobpanel.js logLineEl` (data already has level/source/ts).
  - Stack Logs tab (Loki): `<pre class="sh-log">` with space-padded columns (`pages/stack.js:1521-1527`,
    `:1610-1617`), CSS `css/pages/stack.css:437-490`.
  - Host-log explorer lines `.hl-ln` (`pages/log.js:156-170,381-412,563`), CSS `css/pages/activity.css:724-880`,
    `role="log"` already set; 10 `hl-ln` hits in `invariants.e2e.js`.
  - Not log lines (leave): console exec output `pages/shell.js:411`, snapshot file view `pages/backups.js:2036`,
    incident text `incident.js:35`, plan detail `editui.js:190`.
- kp: `.kp-log` > `.kp-log__line[data-kp-severity] > __time __source[data-kp-source] __level __message`,
  `attachLogs(root)` colours a source with `--chart-N` (same hash as kp-tui), `data-kp-current`.
- Differences: click-to-expand detail row (`hl-ln` has `aria-expanded`) is not part of kp-log; keep a button
  inside the message cell or the detail row below **(inferred)**. The level/source filters stay the dashboard's
  (kp-log has no filter UI).

### kpa-16 Native date and time inputs (small)
- `pages/schedules.js:1067-1076` `input type=date` and `type=time`; `pages/notifications.js:181` `type=time`.
- kp: `div.kp-datepicker[data-kp-datepicker]` > `input[data-kp-date-input]` + `[data-kp-date-open]` +
  `[data-kp-date-panel]` (`kp:js/datepicker.js:1-60`); dd/mm/yyyy by default (10.1.0), ISO in
  `data-kp-date-value`, `data-kp-min/max`, `data-kp-disabled-days`. kp has no time picker: both `type=time`
  inputs stay native.
- Why: a native date input prints in the browser's locale (rule 52 says dd/mm/yyyy everywhere).
  `schedules.js:101` validates "Pick the date." and `schedules.js` reads `.value` ISO: keep, read
  `dataset.kpDateValue`. Live view must still reach the input (`SCHED_DATE` id on the `input`).

### kpa-17 Home-made sort (medium) -- partly workaround, see kpa-32
- `ui.js:1226-1420` (`sortHead`, `sortableTable`, `sortMark`, `SORT_HINT`), `js/sortstate.js` (119),
  `sortkeys.js` (42), localStorage under `sortStoreKey` (`sortstate.js:85-115`). Callers: `pages/host.js:437,1293`
  (two plain tables), `pages/backups.js:1394` (repository table), `pages/schedules.js:737,1522-1535` (own sort,
  own `sched-sort` key), `pages/stack.js:1948` and `pages/overview.js` tables (kp already).
- kp: `kp-datatable` with `data-kp-sort` headers, `data-kp-sort-multi`, `data-kp-remember`, sort summary and
  Reset (the other 11 tables already use it via `dom.js tableBlock` / `ui.js dataTable`).
- Swap: turn the 4 remaining hand-built tables into `tableBlock`/`dataTable` (loses nothing: pagination off).
  Tests: `uikit.test.js` (`sortValue`), `uikit_dom.test.js`, `backupsview.test.js`, `format.test.js`.

### kpa-18 Toolbar, toggle groups, filter chips (medium, partial)
- `ui.js:558-670` (`toolbar`), `1031-1220` (`toggleGroup`, `segSwitch`), `1422-1440` (`filterChip`), CSS
  `css/app.css:3554-3683`, `4647-4760`; 12 files use `segSwitch`.
- kp has no segmented control or list toolbar (only the datatable's own bar with search/filter pills,
  `kp:js/datatable.js` `filterPills`, `data-kp-filter="choice|range|date"`). Where the toolbar sits on a table
  that is already a kp-datatable, kp's search/filter/column menu can replace it **(inferred)**; the rest
  stays (see kpa-33).

### kpa-19 Misc small blocks (small each)
- Steps: `ui.js:2498-2528` (`stepper`, `.nx-steps` `css/app.css:4401-4450`) -> `.kp-wizard__steps` look
  (wizard already used by `actiondialog.js`); live checklist `ui.js:2533-2565` (`.nx-check`): kp has
  `.kp-timeline` (`kp:css/components.css:7428`) but no step-state list **(inferred, keep)**.
- Dismissible note: `ui.js:2573-2607` (`explainNote`, localStorage + `leave`) -> `.kp-alert` with
  `data-kp-dismiss` (`kp:js/overlays.js:466-480` `attachDismissals`, event `kp-alert-dismiss`) **(inferred; memory
  of the dismissal is the dashboard's)**.
- Shortcut line: `ui.js:1920-1937` (`kbd`, `keyRow`, `.nx-keys` `css/app.css:4913-4927`, callers in 4 pages)
  -> `.kp-shortcuts` / `.kp-help__list` (kp has `.kp-shortcuts__row`, components.css:6446-6490) **(inferred partial)**.
- Fail box: `ui.js:2069` (`failBox`) is already `.kp-alert--destructive`; `failBand` `2352`: same. Empty:
  `emptyState` is already `.kp-empty`. No change.
- Pre-existing duplicate toast helper: `actui.js:157` `notify()` wraps kp `toast` a second time beside `ui.js:1615`
  `toast()` (two classes of toast code paths) -> one helper (small).
- Dead code from the earlier kp swap (remove, no replacement needed): `css/app.css:3913-4162` the whole `.tc-*`
  time-chart block (comment still says "timechart.js", no JS/test references: verified by grep), `css/app.css:2200-2370`
  `.metrics-head*`, `.metrics-toolbar*`, `.chart__*`, `.chart-ranges` (no JS/test references), `css/app.css:2054-2200`
  the old start page `.start-tile*`/`.start-dot*`/`.start-legend*`, `.shell-*`, `.health-strip*`, `.kv-grid*`,
  `.status-*` and about 150 more unreferenced classes in `app.css` (static scan of `js/**`, tests, index.html; classes
  built from template strings were excluded by hand, so spot-check each block before deleting). Per-file counts of
  unreferenced selectors: app.css 166, stacks.css 15, map.css 12, activity.css 12, stack.css 5, firewall.css 5.

### kpa-20 Tabs, accordions, tooltips already kp
- `dom.js:794` `tabRow` is `.kp-tabs__list` + `.kp-tab` as links (one address per tab): fine, no change.
  `<details>` folds are `kp-accordion__item--bare`. Wizard, palette, progressbar, attention, chart, freshness,
  datatable, tables, actions, tiles, menu-button: already kp.

---

## 2. Workarounds that exist because kp-themes lacked something

Evidence searched: code comments for workaround/until/lacks/kp-has-no/[TH..], `docs/deployment/REGISTER.md`,
`docs/admin/*`, `unfinished-features.md`. Few comments say "workaround" outright; most are discoverable by
comparing the code with the kp CHANGELOG.

### kpa-30 `rowtoggle.js` (dead code, remove) -- shipped in kp 8.1.1
- `js/rowtoggle.js` (55 lines) opens an expandable row when clicked anywhere; `main.js:24,444` installs it;
  it already stands down when `--kp-themes-version >= 8.1.1` (`rowtoggle.js:30-50`). Register fix-196
  (`docs/deployment/REGISTER.md:734`). Vendored kp is 10.1.1 (10.0.0 added Enter/Space on a row), so the module
  never installs.
- Remove: the module, `main.js` import + call, `test/rowtoggle.test.js`.

### kpa-31 `stateWord` / `.state-word` -- shipped in kp 9.2.0
- `dom.js:532`, `css/app.css:1540-1560` (Kenny 2026-09-29 "layout never shifts"). kp: `.kp-state-word`
  `data-kp-words="a\nb"` + `setStateWord(el, word)` (`kp:js/components.js:1173`, CHANGELOG 9.2.0 "A state word
  that keeps its width"; `update()` accepts it). Replace the 1 caller, delete the CSS block.

### kpa-32 `sortclick.js` -- NOT provided by 10.1.1 (keep, ask kp)
- `js/sortclick.js` (93 lines) rewrites a plain header click into a Shift-click so each click adds a sort key
  (Kenny: "multi-sort without Shift"). kp 10.1.1 still only adds a key on Shift (`kp:js/datatable.js:2856,2868`;
  USER_GUIDE.md:1888). Not removable; candidate request: `data-kp-sort-multi="click"`.

### kpa-33 Home-made controls kp has no component for (keep; candidates to request)
- Segmented/toggle control (`toggleGroup`, `segSwitch`, kpa-18), toast hold/linger/close (`ui.js:1575-1612`, kp toast has
  `ms:0`, `action`, `max` but no pause-on-hover), bar fold into a menu on narrow/wide-font themes
  (`main.js:413-433` `fitBar` + `css/app.css:1905-1915,3222`; kp only has `data-kp-nav-compact` sticky logic,
  `kp:js/components.js:644-765`), diff hunk header (`css/app.css:1217-1220`, `editui.js:220-232`: "kp has no
  hunk-header element"; `kp-diff__line` only, no `__hunk`), running-job pill, bottom tab bar for the phone areas.

### kpa-34 Register rows that ask kp-themes for something
- `gap-kplog-1` (REGISTER.md:970, "later", Kenny 2026-10-09): every log becomes `.kp-log`. kp-log shipped 9.x
  (kp MINI_ROUNDS gap-14, built 2026-09-20). Ready to implement: kpa-15. After it, mark done.
- `gap-themeuni-1` (REGISTER.md:971, "closed ... moved to kp-themes"): per-theme spinner/bar/shape. kp 10.0.0
  delivered every theme's grammar; what remains on the dashboard side is kpa-5/kpa-6 (stop drawing its own).
- `feat-kpall-2` (REGISTER.md:965) "Hand-made copies of what kp-themes 10 ships": done for chart, freshness,
  progressbar, attention, sparkline, More menu, table regions, row actions, tiles; the copies still left are
  kpa-1, 3, 4, 9, 10, 11, 12, 13, 14, 15, 17.
- `docs/admin/FEATURES.md:10`: "Components the UI needs and kp-themes lacks are requested from kp-themes."
  `docs/admin/DECISIONS.md:22`: charts "may move there later" (done: kp chart.js) -- text outdated.
- `docs/admin/KP_THEMES_REPORTS.md`: eight reports all marked resolved in kp 8.0.0 with workarounds already
  removed; the only residue is the `.kp-datatable__*` margins and the version text colour (`css/app.css:1628-1680`,
  `1640-1650`), and the `.kp-nav .kp-nav__search { margin-inline-start: 0 }` override (`css/app.css:51-60`, fix-176).
  Re-check whether kp's own `.kp-nav__search` (`kp:css/components.css:3676`) still carries the auto margin
  **(not verified)**.
- Possibly redundant overrides to re-measure after the swap **(inferred, not verified)**:
  `a.kp-button { text-decoration-line: none }` (`css/app.css:93-97`), `.kp-button { white-space: nowrap }`
  (`css/app.css:1562`), `.notify-title` ellipsis (`css/app.css:1613`, kp has `.kp-cell-truncate`),
  `.nx-ops .kp-attention__*` and `.bk-page .kp-attention__*` overrides (`css/app.css:4959-4966,5150-5160`).

### kpa-35 Stale text to update with the pin
- "22 themes": `css/app.css:1505`, `scripts/merge-cases.mjs:98`, `docs/REDESIGN-APPROACH.md:82`, `docs/admin/INVENTORY.md:56`
  (also says kp 7.2.0), `docs/admin/REALIZATION_PLAN.md:109`. The package has 16 themes now (counted in
  `crates/chassis/static/kp/js/theme-registry.js`: formal light dark cyberpunk synthwave pastel terminal forest
  blueprint solstice brutalism deco phantom grotesk nostromo titanium).
- `admin/web/types/kp/*.d.ts` are copies "from kp-themes 10.0.0"; the 25 checked differ from the 10.1.1 `js/*.d.ts` only by
  prettier formatting plus the header (actions.d.ts verified in full; others by line count **(inferred)**). New modules
  need new stubs (see kpa-40..44) and `jsconfig.json` `paths` already maps `/static/kp/js/*`.

---

## 3. kp-themes 9.x / 10.x features the dashboard does not use yet

All of these exist in the vendored chassis 3.7.1 asset set (47 modules incl. `loading-pieces.js`).

### kpa-40 `attachLoadingPieces` is never attached (small, 10.1.0)
- `kp:js/loading-pieces.js`: pastel's pom-pom caterpillar for skeleton rows, busy progress track, meter,
  spinner, busy calendar days, busy table panel, busy menu. `kp:js/auto.js:145-151` starts it; the dashboard does
  not use auto.js and has no `attachLoadingPieces` call (grep: none). In pastel the loading pictures are
  incomplete **(inferred)**. Fix: `attachLoadingPieces(document)` once in `main.js` next to `attachMotion`
  (it watches the page; no-op for the other 15 themes). Register-level CHANGELOG 10.1.0 / 10.0.0 "scope-141".

### kpa-41 Drawn selects (`attachSelects`) (small-medium)
- Every `select.kp-field__input` (54 `kp-field__input` hits, e.g. the 32-option "Answer a manual check" dialog)
  stays the browser's list; in Firefox that list is the platform's, not the theme's. USER_GUIDE.md:3448: the
  package draws a themed listbox over each single select with `attachSelects` (native select keeps the value, so
  `drivable()` and `.value` code is unaffected **(inferred)**). Call `attachSelects(root)` per mounted page/dialog or
  from a `MutationObserver` as `main.js:458-463` does for tables. `data-kp-select="native"` opts out.

### kpa-42 Remembered state (`js/remember.js`, `data-kp-remember`) (small)
- The dashboard stores by hand: `sortstate.js:85-115`, `pages/home.js:135-155` (folded/starred), `pages/overview.js:423,1569`
  (view), `pages/schedules.js:1522-1535`, `logs.js:190-208` (levels), `pages/update.js:177-189`, `ui.js:2575-2600`
  (dismissed notes). kp remembers disclosures, tabs, tree, sidenav groups, table columns/sort per name
  (`attachRemembered`, `forgetRememberedExcept`; 10.0.0 "Remembered disclosures on a page that rebuilds them").
  Only the kp-datatable tables use it today (4 `data-kp-remember` hits). The two kinds of memory use different keys
  (`homelab-*`/`sched-sort` vs `kp-remember:*`).

### kpa-43 Other unused modules that fit
- `calendar.js` -> kpa-13; `graph.js` -> kpa-14; `tour.js` -> kpa-12; `datepicker.js` -> kpa-16; `combobox.js` (searchable
  pickers: the stack and app pickers of the Answer/Restore dialogs, `snapshotpicker.js`, `fieldChoices` in
  `actionforms.js`) **(inferred)**.
- `upload.js` (`.kp-upload`): `importstack.js:26` `input type=file` + paste area (drop zone, progress, reject
  events) **(inferred; the bundle flow also pastes YAML)**.
- `structure.js` (tree/reorder/split): `pages/presets.js`/`presetseditor.js` list editing and the stack hub's
  sections **(inferred, low value)**.
- `patterns.js` copy / undo / inline confirm: only `attachPatterns` is used once (`chrome.js`/`actui.js` copy line);
  `data-kp-confirm` (two-step button) is unused; typed-name confirmation stays the dashboard's (kp has dialog and
  inline modes only: `kp:js/components.js:50-56`).
- `attachTooltips` -> kpa-9; `attachScrollbars`, `attachDismissals`, `attachToTop` (`.kp-to-top`, "Back to top on every
  page", 9.2.0) are not attached; `attachToTop` suits long pages such as Settings/Backups.
- Tile motion/size already via `attachMotion`; `leave()`/`update()` are used (`dom.js patchKeyed/setLive`).
- 9.2.0 features not used: `.kp-action-list` role columns on lists (only `.kp-row-actions` is used), `.kp-page-header`
  overflow "More" (used), `.kp-attention` (used), `.kp-chart-group` shared crosshair (not checked **(inferred)**),
  `kp-progressbar--md|--lg` (9.3.0 size classes in the 10.0.0 section) for the larger bars on the batch/job pages.
- 10.1.0 changes picked up with no code: `--kp-press: hold` (deco seal) via `attachEffects` (`chrome.js`),
  `--kp-lines: keep`, `--kp-update-time` (formal), busy-button words `span.kp-button__label` (forms.js).

---

## 4. 10.1.x consumer changes and where the dashboard is touched

### kpa-50 Date picker writes dd/mm/yyyy (CHANGELOG 10.1.0; chassis 3.7.1)
- The dashboard has no kp date picker, so nothing changes today. Time chart tooltips were already rule 52 (10.0.0).
  Adopting kpa-16 gets dd/mm/yyyy by default; `data-kp-date-format="locale"` is not wanted here (rule 52).

### kpa-51 Key-figure labels wrap (10.1.0)
- Dashboard tiles are `.nx-kpi` (kpa-1), not `.kp-kpi`, so they keep the one-line ellipsis label
  (`css/app.css:3472-3481`; wraps only under a media query at `:3506`). Switching to `.kp-kpi` makes tiles
  taller and changes the layout-audit expectations (`test-e2e/layoutaudit.js`, `invariants.e2e.js` KPI row height
  checks).

### kpa-52 Side navigation defaults to the edge tab (`data-kp-sidenav-opener`)
- No `.kp-sidenav` anywhere in `admin/web` (grep: none). The areas bar is `.kp-nav`; phone navigation is the
  home-made bottom tab bar + `nx-sheet` (`css/app.css:3214-3334`, `chrome.js:488`). No effect. If the sheet is ever
  replaced by `.kp-sidenav`, set `data-kp-sidenav-opener="launcher"` to keep a visible button.

### kpa-53 Removed themes `high-contrast`, `sepia`, `retro` (also `lapis`, `shade-*` earlier)
- No theme name appears in `admin/web/js`, `css`, `html` or Rust `admin/src` (grep). The picker and the palette
  commands read the list from `theme-core.js THEMES` (`chrome.js:57,145`, `commands.js:147-158`), so they shrink
  by themselves (16 themes).
- Stored choice: `chrome.js:75-77` `chooseTheme` writes `localStorage["theme"]` (kp STORAGE_KEY). `static/theme-boot.js`
  from chassis maps only `lapis, shade-dark, shade-light` and the old pre-8 names (file read:
  `crates/chassis/static/theme-boot.js`); `high-contrast`, `sepia` and `retro` are NOT mapped, so the boot script
  writes `data-theme="sepia"` on `<html>` before first paint. No stylesheet defines it, so the page paints with the
  default tokens **(inferred: the formal/root tokens; not run)**. kp's `currentTheme()` (called at `chrome.js:146`)
  reports the unknown name once (console + `kp-theme-unknown`, sessionStorage-throttled) and returns `formal`; the
  stored value is left (`kp:js/theme-core.js:207-215,314`), so the picker shows `formal` selected while `localStorage`
  still says `sepia` until the viewer picks one. MIGRATION.md suggests `light` for sepia, `formal`/`light` for
  high-contrast and retro.
- Options: (a) ask chassis to extend `renamed` in theme-boot.js (a global fix, in chassis); (b) in the dashboard
  call `initializeTheme()` or `applyTheme(storedTheme())` + `storeTheme()` once at start so the stored name is
  rewritten.
- Tests: only `dark light formal deco cyberpunk` are used in `test-e2e/invariants.e2e.js` (lines 97-105, 1963, 2393, 2910,
  4142, 4914); the contrast cases enumerate "every theme the picker offers" (docs/REDESIGN-APPROACH.md:71) so they
  shrink automatically. `client/tests/tui_snapshot_tests.rs:3307` "retro" is a wizard-name string, unrelated.

### kpa-54 Menus gain `--kp-menu-anchor-x` and `--kp-menu-count`
- Written by `menu-button.js` on every menu button; only deco (inlay under the opener) and formal (menu line count)
  read them. `moreMenu` (`ui.js:1840`) and the page-header More menus already benefit with no change; the home-made
  `rowMenu` does not (kpa-10).

### kpa-55 `js/loading-pieces.js` joins the webapp asset set
- Vendored by chassis 3.7.1 but needs the explicit attach (kpa-40). Without it nothing breaks; pastel just draws part
  of its loading pictures.

### kpa-56 Other 10.1.x things worth a regression look
- Overlay placement by layout box (phantom turned parts), sidenav off-screen when closed in four themes,
  React `DatePicker` default, tooltips/graph loading ghost, `kp-button__label` span in busy buttons (forms.js),
  `.kp-menu__item--destructive` keeps its ink (the dashboard's `rowMenu` `.danger` styling is its own).
  None need dashboard code.

---

## 5. Guard tests that pin home-made components (update or retire with each swap)

Unit (`admin/web/test`, run by `npm run check`; the `kp-register.mjs` hook maps `/static/kp/` to the chassis checkout
pinned in Cargo.lock):

| Test file | Pins | Affects |
|-----------|------|---------|
| `rowtoggle.test.js` | `kpHandlesRowClicks`, `rowToggleFor` | kpa-30 (delete) |
| `uikit.test.js` | `meterParts`, `toggleValue`, `tipPlace`, `menuPlace`, `toastsToDrop`, `sortValue`, `chartColour`, `hueColour` | kpa-3, 9, 10, 11, 17 |
| `uikit_dom.test.js` | ui.js blocks in a mini DOM: rowMenu, segSwitch, chip/dot/meter, kpiStrip, sortable, toast, drawer | kpa-1, 3, 4, 10, 11, 17 |
| `backupcalendar.test.js` | month cells, ratio tones, `dayDetail` | kpa-13 (keep the model tests) |
| `topology.test.js`, `mapview.test.js`, `fwview.test.js`, `reviewmetrics.test.js` | layout, hues, edge kinds, KPI tiles | kpa-14, kpa-1 |
| `hostlogview.test.js` | filter counts, levels | kpa-15 (filter stays) |
| `flows.test.js` | `tourSteps`, `shouldTour` | kpa-12 |
| `format.test.js`, `read.test.js`, `backupsview.test.js` | `humanDuration`, `agoText`, `humanBytes` | kpa-7 |
| `liveloop.test.js`, `uikit.test.js` | toast replace / duration rules | kpa-11 |
| `finalreview_h3.test.js` | `stepper`/`checkList` | kpa-19 |
| `drivecatalog.test.js`, `drivable.test.js` | declared Live view controls (kpi, drawer) | kpa-1, 11 |
| `schedules.test.js` | schedule form fields incl. date | kpa-16 |

E2E (`admin/web/test-e2e`, whole-screen, not run here): `invariants.e2e.js` has class-name pins for `nx-kpi` (43),
`nx-rowmenu` (7), `backup-cal` (18), `hl-ln` (10), `sh-log` (3), `nx-chip` (7), `nx-check/nx-steps` (12), topology / `.mp-`
(59), `sch-sk`/`.sk` (50), `nx-seg` (9), `nx-toast` (3), `tour` (23); `layoutaudit.js` pins `nx-kpi`, `nx-chip`;
`secrets.e2e.js` pins `.sk`. `docs/INVARIANTS.md` rows 24 (one topology), 59 (theme picker) apply. Per
`docs/deployment/REGISTER.md` rule "merge-cases", the 22-theme contrast case runs only when a stylesheet or the
kp pin changes (`scripts/merge-cases.mjs:98`): this release changes both.

---

## 6. Summary table

| ID | Area | Home-made (where) | kp-themes 10.1.1 replacement | Size | Kind |
|----|------|-------------------|------------------------------|------|------|
| kpa-1 | KPI tiles | ui.js:335-516, app.css:3435-3551 | `.kp-kpis/.kp-kpi`, kpi.js | large | swap |
| kpa-2 | Stat tiles | dom.js:597-740 | `.kp-kpi` / `.kp-spec` | small-med | swap |
| kpa-3 | Meter, share bar | ui.js:2022-2062 | `.kp-meter`, `setMeter` | small | swap |
| kpa-4 | Chips, dots, state pills | ui.js:1987, app.css:283,4221 | `.kp-badge--*`, `.kp-health` | medium | swap |
| kpa-5 | Spinners, busy marks | chrome.js:305, app.css:3185,4480, activity.css:172 | `.kp-spinner`, `.kp-progressbar` | small | swap (gap-themeuni-1) |
| kpa-6 | Shimmer skeletons | schedules.css:90, secrets.css:56 | `.kp-skeleton` | small | swap |
| kpa-7 | Freshness, durations | ui.js:140, format.js:19,141 | `[data-kp-ago]`, `humanDuration` | small | swap |
| kpa-8 | State word | dom.js:532 | `.kp-state-word`, `setStateWord` | small | workaround removal |
| kpa-9 | Hover card | ui.js:1447-1519 | `attachTooltips` | medium | swap (risky) |
| kpa-10 | Row menu | ui.js:1671-1830 | `menu-button.js` | medium | swap |
| kpa-11 | Drawers, Help | ui.js:2161, chrome.js:370,488, helptour.js:367 | `.kp-drawer`, `.kp-help` | med-large | swap |
| kpa-12 | Help tour | helptour.js:75-300 | `tour.js` | medium | swap |
| kpa-13 | Backup calendar | backupcalendar.js x2, app.css:2370-2672 | `calendar.js` | large | swap |
| kpa-14 | Topology graph | topology.js, mapview.js, map.css | `graph.js` | large | swap |
| kpa-15 | Non-kp logs | update.js:372, stack.js:1521, log.js:156 | `.kp-log` | medium | swap (gap-kplog-1) |
| kpa-16 | Date input | schedules.js:1067 | `.kp-datepicker` | small | swap |
| kpa-17 | Home-made sort | ui.js:1226-1420, sortstate.js | `kp-datatable` | medium | swap |
| kpa-18 | Toolbar, segmented | ui.js:558-1220 | none (partial datatable bar) | medium | keep / partial |
| kpa-19 | Steps, notes, key line, dead CSS | ui.js:2498-2607, app.css:3913-4162 | wizard steps, dismissals, `.kp-shortcuts` | small | mixed + dead code |
| kpa-20 | Tabs, accordions etc. | dom.js:794 | already kp | none | none |
| kpa-30 | Row click expand | rowtoggle.js | kp 8.1.1+ built in | small | dead code |
| kpa-31 | State word | dom.js:532 | `.kp-state-word` (9.2.0) | small | workaround |
| kpa-32 | Plain-click multi sort | sortclick.js | not shipped | n/a | keep, request |
| kpa-33 | No kp component | segmented, toast hold, bar fold, diff hunk | not shipped | n/a | keep, request |
| kpa-34 | Register asks | REGISTER gap-kplog-1, gap-themeuni-1 | shipped | n/a | close rows |
| kpa-35 | Stale text, type stubs | "22 themes", types/kp | 16 themes, 10.1.1 stubs | small | docs |
| kpa-40 | Loading pieces | not attached | `attachLoadingPieces` | small | new feature |
| kpa-41 | Drawn selects | not attached | `attachSelects` | small-med | new feature |
| kpa-42 | Remembered state | many localStorage keys | `data-kp-remember` | small | new feature |
| kpa-43 | Other unused modules | -- | combobox, upload, structure, to-top, tooltips | varies | optional |
| kpa-50 | Date picker dd/mm/yyyy | none used | -- | none | note |
| kpa-51 | KPI label wrap | nx-kpi ellipsis | wraps in `.kp-kpi` | with kpa-1 | note |
| kpa-52 | Sidenav opener | no sidenav | -- | none | note |
| kpa-53 | Removed themes | theme-boot.js maps only lapis/shade | stored sepia/retro/high-contrast fall back | small | decision needed |
| kpa-54 | Menu anchor/count | rowMenu lacks them | menu-button.js | with kpa-10 | note |
| kpa-55 | loading-pieces asset | -- | needs attach | with kpa-40 | note |

Suggested order (for Kenny's confirmation, not decided here): first the safe removals and wiring (kpa-30, 31,
19-dead CSS, 40, 41, 53, 35), then the one-for-one swaps with an existing kp twin (kpa-15, 16, 5, 6, 12, 13, 14),
then the wide-fan-out visual ones last (kpa-1/2/3/4, 11, 10, 9, 17), because each of those moves class names that
the whole-screen invariants and the contrast case pin.

---

## 7. Implementation (helper A)

Nothing was run (no tests, no browser, no cargo). `tsc -p jsconfig.json` and
`prettier --check .` are clean on the whole `admin/web` tree. Pages and page
stylesheets that only call a swapped component were edited where the swap
needed it (listed under "Files outside the owned list"). Helper B's section 8
requests are applied (types, `.backup-cal*` rules, calendar unit tests, e2e
pins, `.nx-spin` / `nx-check` ring, `sx-pulse`).

### Per item

- **kpa-1 KPI tiles: done.** `ui.js kpi()` / `kpiStrip()` draw `.kp-kpis[data-kp-kpis-columns="all 3 2 1"]`
  of `.kp-kpi` (`__label __value(small) __trend __hint(data-kp-when) __spark __meter`).
  Tone `warn`/`bad` is `data-kp-tone="warning|destructive"`; the toggle tile is
  `button.kp-kpi--toggle` and listens to kp's `kp-kpi-toggle` event (the page keeps
  the filter; `set` writes `aria-pressed` back); `ctxParts` / `ctxTone` are kp-health
  dots, `ctxMeter` a `.kp-meter--inline`; the fourth row is `svg.kp-kpi__spark`
  (drawn by kp's `drawSparkline`) or `.kp-kpi__meter`. `main.js` attaches
  `attachKpiStrips` and `attachKpiToggles`. Host's eight tiles are kp's trend tile
  (`.kp-kpi--trend`, `setTrendData`, `attachTrendCharts`): the home-made hover
  chip, axis and crosshair are gone. Apply's plan tiles are `.kp-kpi--toggle`
  (`data-kp-kpi-owned`: the page keeps which column is on); the Stack hub's
  Compare tile is a `.kp-kpi`. Deleted: `.nx-kpi*`, `.nx-kpis`, the `.nx-ops` and
  `.bk-page` tile variants, and the page overrides in metrics / firewall / stack /
  host / apply css. Look changes (kp wins): labels wrap, a toned figure sits on its
  status plate, a taller tile.
- **kpa-2 stat tiles: done.** `dom.js statGrid/statTile` and the batch totals
  (`actiondialog.js`) are `.kp-kpis` / `.kp-kpi` (label + value, no trend).
- **kpa-3 meter, share bar: done.** `ui.js meter()` / `shareBar()` are `.kp-meter`
  written by kp's `setMeter` (a share past 100 % now fills the bar and shows kp's
  over-mark instead of being clamped; not measured has no `aria-valuenow`). The
  dashboard `meterParts` is deleted. Host's own `.hk-bar` memory bar is `shareBar`.
- **kpa-4 chips, dots, state pills: done.** `chip()` is `.kp-badge[--success|warning|destructive|info]`
  (a page's own tone, Claude's chip, is a `data-tone` hook); `dot()` is
  `.kp-health[data-state]` + `.kp-health__dot` (bad is kp's `down`); `badgeCell` and
  `actui.js badge` are kp badges (`dom.js stateBadge`); Host's `hk-dot` and Backups'
  hand-made `nx-dot` spans are `dot()`. Two small additions kp lacks, in `app.css`:
  `.kp-health[data-state="info"]` (the Map's legend colour) and a badge that never
  wraps (`--kp-badge-wrap: normal`, `nowrap`) as chips used to. Not done in this
  round: re-run the contrast case (`redesign-final-contrast`), see "Run these".
- **kpa-5 spinners, busy marks: done.** The running pill shows `.kp-spinner`
  (`.nx-pill__spin`), the running `nx-check` step too (ring and both `nx-rot`
  keyframes deleted); the Activity bar is helper B's `progressBar(label, null)`.
- **kpa-6 skeletons: done for mine.** `bk-heat__cell--load` uses kp's `kp-pulse` on
  `--muted`; `hk-pulse` went with `hk-dot`; helper B did the others.
- **kpa-7 freshness: done.** `liveStatus` is `.kp-health.nx-live` (dot + kp's ticking
  line + a `role=status` words span; `data-state` is `ok` / `down`, `aria-busy`
  while busy); `format.js humanDuration` / `agoText` delegate to kp's `freshness.js`
  (the dashes for null / negative stay). `sx-pulse` deleted (its dot is gone).
  Home's "watched every minute" and the Console's chip are still hand-made `.nx-live`
  with the old `::before` dot (kept in `app.css` as `.nx-live:not(.kp-health)::before`).
- **kpa-8 / kpa-31 state word: done.** `dom.js stateWord()` and the eight status
  spans in `editpanels.js` / `presetseditor.js` are `.kp-state-word` with
  `data-kp-words`; the `.state-word` CSS is deleted.
- **kpa-9 hover card: partial, kept on purpose.** kp's `attachTooltips` needs a
  tooltip element inside every anchor, found once at attach; the dashboard's one
  shared card is filled on demand for table cells and chips that are rebuilt every
  30 s, so behaviour would be lost. The card now wears kp's look
  (`.kp-popover.kp-tooltip.nx-tip`: border, plate, radius and the theme's entrance
  come from kp; only the placement stays ours).
- **kpa-10 row menu: done.** `rowMenu()` keeps its signature but wraps the row's
  button once in a `.kp-menu-button` with kp's rich menu (`setMenu`, `openMenu`,
  `attachMenuButtons`); entries are named for Live view by `decorate`
  (`dialogControl`). `menuPlace`, `menuKey` and the 260 px dialog are deleted. It
  gains `--kp-menu-anchor-x` / `--kp-menu-count` (kpa-54) and the theme's open and
  close. The first call opens it; the button's own click is kp's from then on.
- **kpa-11 drawers, help: done.** `ui.js drawer()`, the job drawer, the phone More
  sheet and the Help panel are `dialog.kp-drawer.kp-dialog` (`__head __desc __body
  __foot`, `kp-icon-button kp-dialog__close`); Help's cards are `.kp-help` with
  `.kp-help__list` and kp's `kbd`. `.nx-drawer*` is deleted; `stacks.css` sets
  `--kp-drawer-width` for the plan sheet; `help.css` is one rule. A drawer foot
  with several buttons sits side by side (one small rule in `app.css`).
- **kpa-17 home-made sort: kept (partial), with the reason.** The three tables that
  still use `sortHead` / `sortableTable` (Host's Containers and Settings, Backups'
  repositories) cannot become a `kp-datatable` without losing behaviour: their rows
  are filtered by the page itself (`tr.hidden`; kp's table rewrites row visibility),
  Backups groups them by stack with folds and detail rows and keeps the sort in the
  address, and each header is a declared Live view control
  (`sort-host-containers`, `sort-host-settings`, Backups' `SORT`) in
  `drivecatalog.json`, which is not in my files. Needs its own round: move the
  page filters onto kp's `filter` / `query`, the groups onto
  `data-kp-row-group`, and the catalog rows to `table-reset-sort`. Every other
  table is already a kp-datatable. `sortclick.js` stays (kpa-32).
- **kpa-19 misc: partial.** Done: `stepper()` is `.kp-wizard__steps` (`data-state`
  done / current); `explainNote()` is a `.kp-alert--info` with `.kp-alert__close`
  (kp's `attachDismissals` in `main.js` hides it, its remembering stays the
  dashboard's localStorage key). Kept: `checkList` (kp has no step-state list; its
  running ring is now kp's spinner) and `keyRow` / `kbd` (kp's `.kp-shortcuts` is a
  boxed sheet and `kbd` is only styled inside `.kp-help`; ask kp for an inline key
  line). Dead CSS: deleted every selector whose classes no file references. Checked
  by a script over `js/**`, `test/**`, `test-e2e/**`, `index.html`, `css/pages/*`,
  Rust `admin/src` and `core/src`, with token boundaries and `${…}` /
  `parts:` template prefixes (kept: `.topology__*`, `.nx-spark-line`, `.nx-done__mark--*`,
  `.bk-heat__cell--*`, `.chart__empty` which `topology.js` still uses). Gone: the
  `.tc-*` block, `.metrics-head*`, `.metrics-toolbar*`, `.chart*`, `.start-*`,
  `.health-*`, `.shell-*`, `.status-*`, `.logs-*`, `.notice-*`, `.apply-group`,
  `.client-manage`, `.passkeys-list`, `.stale__*` and the like (142 blocks, app.css
  5838 → 4205 lines with the swaps). `.backup-cal*` rules (B's request) are gone.
- **kpa-30 rowtoggle: done.** `rowtoggle.js`, its test and the `main.js` call deleted.
- **kpa-35 stale text, stubs: done for my files.** `types/kp/*.d.ts` are the 10.1.1
  files (header "kp-themes 10.1.1") for the 25 old modules and new stubs for
  `tour`, `calendar`, `datepicker`, `remember`, `loading-pieces`, `combobox`,
  `strings`, `listbox`, `top-layer`. The "22 themes" comment in `app.css` is
  reworded. Not mine: `scripts/merge-cases.mjs:98`, `docs/REDESIGN-APPROACH.md:82`,
  `docs/admin/INVENTORY.md:56` (also says kp 7.2.0), `docs/admin/REALIZATION_PLAN.md:109`.
- **kpa-40 loading pieces: done.** `attachLoadingPieces(document)` in `main.js`.
- **kpa-41 drawn selects: done.** `attachSelects(document)` plus a body observer for
  pages and dialogs drawn later (`main.js`); the native select keeps the value.
- **kpa-42 remembered state: kept, with the reason.** Tables already remember sort
  through `data-kp-remember`. The rest of my hand-made state is either read by
  another file (`home.js`, `overview.js`, `update.js`, `schedules.js`, not mine) or a
  `<details>` whose open state the page sets from the address, Live view or a
  search (a remembered state would fight it and leak between e2e cases). The
  dismissed note keeps its own key (kp remembers no alert).
- **kpa-53 removed themes: done.** New `js/themesettle.js settleStoredTheme()`:
  if the stored theme is not in kp's list it applies `DEFAULT_THEME` and stores it
  (`applyTheme`, `storeTheme`), before the picker is attached (`chrome.js`). No
  theme name appears in it. Test: `test/themesettle.test.js`.
- **kpa-54 / kpa-55:** the row menu gains the anchor and count properties; loading
  pieces are attached (above).

### Files outside the owned list that I edited (only to switch to the kp part)

`js/actui.js`, `js/actiondialog.js`, `js/editpanels.js`, `js/presetseditor.js`,
`js/helptour.js` (Help panel only), `js/pages/host.js` (dots, share bar, trend
tiles), `js/pages/apply.js` (plan tiles), `js/pages/stack.js` (the Compare tile's
class and slot, one line each), `js/pages/backups.js` and `js/pages/restore.js`
and `js/pages/secrets.js` (status dots → `dot()`), `js/pages/schedules.js` (one
selector for the open row menu), `css/pages/{host,apply,metrics,firewall,stack,stacks,help,secrets}.css`.
The uncommitted `data-kp-action` edits in `backups.js` / `stack.js` are untouched.

### Tests updated (not run)

- Unit: `uikit.test.js` (meterParts, menuPlace removed), `uikit_dom.test.js` (chip /
  dot / meter / drawer assertions, row-menu guard, new cases for kpa-1, 2, 4, 7, 8,
  9, 19), `backupcalendar.test.js` (B's request: `calendarWeeks` / `shiftMonth`
  gone, `dayCell` and `historyCells` added), new `themesettle.test.js`,
  `support/minidom.mjs` (`replaceWith`, `toggleAttribute`). `rowtoggle.test.js` deleted.
- E2E pins: `invariants.e2e.js` and `layoutaudit.js` (`.nx-kpi*` → `.kp-kpi*`,
  `.nx-chip` → `.kp-badge`, `.nx-dot--*` → `.kp-health[data-state]`, `.nx-steps` →
  `.kp-wizard__steps`, `dialog.nx-rowmenu[open]` → `.nx-rowmenu > .kp-menu:not([hidden])`,
  the calendar, the tour, the host log and the stack Logs pins from B's list),
  `secrets.e2e.js` (`.sk` → `.kp-skeleton`).

### Run these first (judgement calls the tests will confirm)

1. The contrast case on `dark` / `light` (kp badges, health dots, toned tiles).
2. Row menu cases (`invariants.e2e.js` near the Schedules menu): the menu is now at
   least 19 rem wide; a second click closes after kp's close animation.
3. The calendar colour case now expects kp's day plates (`--success` etc.).
4. `attachSelects` against Live view's select filling (`pagedrive.js` sets `.value`
   and fires `change`).

### Requests for files I do not own

- `docs/` texts and `scripts/merge-cases.mjs` carrying "22 themes" (listed under kpa-35).
- `js/drivecatalog.json` and the three pages of kpa-17 for the sort round.
- `js/pages/home.js:176` and `pages/shell.js:212` could use `liveStatus`-style
  `.kp-health` instead of the leftover `.nx-live::before` dot.
- `REGISTER.md`: gap-themeuni-1, gap-kplog-1 and feat-kpall-2 can be closed once the
  contrast case and the row-menu cases pass.

## 8. Implementation (helper B)

Scope: logs, tour, calendar, graph, date picker, spinners and skeletons in the
files helper B owns. Nothing was run (no tests, no browser); `tsc` was checked
against the 10.1.1 `.d.ts` files (see "Requests" for the three missing stubs)
and `prettier --check` is clean on every file listed.

### Per item

- **kpa-15 / gap-kplog-1 logs: done.** All three are `.kp-log` now, fed by
  kp's `attachLogs`.
  - Update flow `uf-log` (`js/pages/update.js`, `css/pages/update.css`): the
    last 14 job lines drawn by `jobpanel.js logLineEl` (time, source, level,
    message), `role="log"`, `data-kp-arrive="none"`; the CSS keeps only a
    seven-line floor.
  - Stack Logs tab `sh-log` (`js/pages/stack.js` `logsTab`, `css/pages/stack.css`):
    one `.kp-log__line` per Loki line, severity from `levelGroup`; the space
    padding (`padEnd`) is gone, kp's grid aligns the columns. `data-src` and
    `data-lvl` stay on the line, so the filter pins keep working. The fixed
    height and the states (empty, error, skeleton) span the grid.
  - Host log `hl-ln` (`js/pages/log.js`, `css/pages/activity.css`): `.hl-lines`
    is the `.kp-log` grid; a line is `p.kp-log__line[data-seq]` (kp's order is
    time, source, level, message; the stack swatch stays inside the source).
    A `display: contents` line has no box, so the click-to-open button is the
    message cell (`span.kp-log__message[role=button][aria-expanded]`, Enter and
    Space as before); a click on any cell of the line still opens it
    (listener on the line). The open line carries kp's `data-kp-current`. The
    detail pane `.hl-more` spans the grid. kp's level plate replaces the
    warn/error row tints and the left bar. Live lines call `attachLogs` on the
    new line only (no rescan of 2000 lines).
- **kpa-12 tour: done.** `js/helptour.js` hands the six steps to kp's
  `startTour` (`target` = the phone or desktop selector chosen at start,
  `remember: "homelab"`, `decorate` names the buttons for Live view as
  `tour-next`, `tour-skip` and the new `tour-back`). `tourSteps()` keeps its
  `{desktop, phone, title, text}` shape and `shouldTour()` is now a wrapper of
  kp's `shouldStartTour`, so `flows.test.js` still holds. A browser that has the
  old `homelab-toured` key counts as toured (read once, never written); new
  endings are written under `kp-remember:tour:homelab:done`. Home-made card,
  placement, ring, Esc handling and the `.tour*` CSS are gone (`help.css`).
  New for the user: a Back button, arrow keys, the dimmed page, a card that
  keeps one height.
- **kpa-13 backup calendar: done.** `js/pages/backupcalendar.js` draws kp's
  `section.kp-calendar` in `.kp-calendar-layout` with the detail pane beside it.
  Same information per day: tone, `n/m` count, the title/aria words (stacks
  backed up, missing names, "in the future", "before the fleet's oldest known
  backup", "no stack keeps data"), the legend (six tones), today, the picked day
  and the detail list; month, picked day stay in `?y=&m=&day=`. The model gained
  `dayCell` and `historyCells` (every day from a year before the oldest
  snapshot through today's month, in kp's `setCalendarDays` shape; Brussels time
  as `format.js ZONE` and kp's default). Loading = kp's `loading` state (pulse at
  final size), no readable stack = `error`, no stacks = `empty`. The page's own
  month nav, grid, legend, keyboard code, skeleton cells and `localIso` are gone;
  the detail pane CSS moved to the new `css/pages/backupcalendar.css`. Removed
  from the model: `calendarWeeks`, `shiftMonth` (dead after the swap). The detail
  heading is now dd/mm/yyyy (rule 52).
- **kpa-14 topology / Map graph: kept, with a reason.** `graph.js` is a literal
  port of the Map's own layout (hub in the middle, 1.35 ellipse, `graphBends`,
  radius 14 / 10..24 by weight): positions would be identical. But three things
  of the Map's look would change, and Kenny's rule is that the network graph's
  look never changes: (1) the node colour: the Map paints `oklch(0.62 0.15
  <stack hue>)` in every theme, kp paints `oklch(from var(--chart-1) l max(c,
  .12) <hue>)`, so lightness and chroma follow the theme and the stack colours
  differ; (2) the label: the Map writes the name centred under the node, kp puts
  it beside/over/under by angle and cuts it with an ellipsis (`fitGraphLabels`);
  (3) the Map's "Show all" button and hint sit inside the picture, kp's sit in a
  bar above and under it. The Firewall page's `topology.js` figure (circle
  layout, traffic ring, firewall rings, four node kinds) is a different layout
  altogether. Ask for kp: a fixed node colour option (`--kp-graph-node-colour`
  is a knob; confirm it can be set per graph to the Map's oklch), a label mode
  "under the node, never cut", and the show-all/hint placement. With those the
  Map page can move; `mapgraph.js`'s `NODE`/`SHOW_ALL` controls map to
  `decorate` kinds `node` and `show-all`.
- **kpa-16 date input: done.** `js/pages/schedules.js`: `div.kp-datepicker`
  (`span` in the sentence) with `input#sched-date` (text, shows dd/mm/yyyy),
  the calendar button and the panel; `attachDatePickers`. The server still gets
  the ISO date: `date.dataset.kpDateValue ?? ""` replaces `date.value`
  (`YYYY-MM-DD`, same as `input type=date`); empty or unparseable text gives
  `""`, so "Pick the date." still triggers. The draft date is set with
  `datePicker(el).set(iso)`. Repaint listens to `kp-date-change` (typed,
  chosen and set). Live view fills `#sched-date` as before: `pagedrive.js` sets
  `.value` and fires `input`/`change`, and the picker reads an ISO text.
  The time inputs stay native (kp has no time picker).
- **kpa-5 spinners / busy bars: partial.** Done: the Activity running-job bar
  without a step count (`js/pages/activity.js`, `css/pages/activity.css`): the
  home-made sliding `ac-busy` bar is kp's `progressBar(label, null)`
  (`data-kp-indeterminate`), built once per job row so the theme's busy picture
  does not restart on every event. Not in my files: `.nx-spin` (`chrome.js:305`,
  `app.css:3185-3196`), the `nx-check` run ring (`app.css:4480-4520`), the second
  `nx-rot` keyframes: helper A.
- **kpa-6 skeletons: partial.** Done: `.sch-sk` (`schedules.js`,
  `schedules.css`) and `.sx .sk` (`secrets.js` helper `sk`, `secrets.css`) are
  `.kp-skeleton`; the `hl-ln--sk` host-log rows use `.kp-skeleton` in the
  `.hl-sk` row. Kept, and why: the live-dot pulse `sx-pulse`
  (`css/pages/secrets.css:22-34`) belongs to the freshness dot (`ui.js
  liveStatus`, kpa-7), not to a skeleton, so it goes with that item. Not in my
  files: `bk-heat__cell--load`, `hk-pulse`.

### Requests for helper A's files

Types (`types/kp/`, copy the file from `~/Projects/kp-themes/js/<name>.d.ts`):
`tour.d.ts`, `calendar.d.ts`, `datepicker.d.ts`. Without them `tsc` fails on
`js/helptour.js:24`, `js/pages/backupcalendar.js:77` and `js/pages/schedules.js:50`
(and `helptour.js:185` for the `decorate` parameters). They refer to
`./strings.js`; `skipLibCheck` covers that.

`css/app.css`: delete the `.backup-cal*` rules, `2370-2494` (legend, grid, cells,
skeleton cells; the block starts at the "feat-overview-10: the backup calendar"
comment) and `2592-2671` (nav, chips-wrap, layout, detail). Keep `2495-2591`
(`.perstack-*`, `.backups__*`, `.snapfile*`: the Backups page uses them). Line
numbers are of the file as read at 2026-10-15; re-find by the selectors.

Unit tests:
- `test/backupcalendar.test.js:11` and `:15`: remove `calendarWeeks` and
  `shiftMonth` from the import (both removed from `js/backupcalendar.js`).
  Delete the tests `:83-98` (`calendarWeeks pads…`, `calendarWeeks of an empty…`),
  `:303-313` (`monthDays feeds calendarWeeks…`: keep a `monthDays` length check
  if wanted) and `:321-345` (the three `shiftMonth` tests). Add tests for the new
  `dayCell` and `historyCells` (tone, `n/m` count, missing names, a day before
  the oldest snapshot reads `before`, a later day is absent so kp draws `future`).
- `test/flows.test.js:425-450`: still green; `steps[n].phone`/`shouldTour` kept.

E2E pins (`test-e2e/invariants.e2e.js` unless noted). A `.kp-log__line` is
`display: contents` and has no box, so Playwright's `waitFor()` and `isVisible()`
need a child: `.kp-log__message` (or `.kp-log__time`).
- Backup calendar: `:185` `.backup-cal__cell--skeleton` -> `.kp-calendar__day[data-kp-tone="loading"]`
  (or `.kp-calendar[data-kp-state="loading"]`); `:458`, `:584`
  `.backup-cal__cell:not(--skeleton):not(--pad)` -> `.kp-calendar__day[data-kp-date]`;
  `:513` `.backup-cal__grid` -> `.kp-calendar__grid`; `:534` `#backup-cal-heading`
  -> `.kp-calendar__title`; `:557` `.backup-cal__grid` wait -> `.kp-calendar__grid`;
  `:559` `.backup-cal__grid > *` (counts cells) -> the grid is always six weeks,
  42 `td`s: `.kp-calendar__grid tbody td`; `:588` `.backup-cal__cell--today`
  -> `.kp-calendar__day[data-kp-today]`; `:590-594` `.backup-cal__detail` and
  `.backup-cal__detail-row` stay (the pane kept its classes);
  `:2397-2411` `.backup-cal__cell--ok|warn|bad` colours -> `.kp-calendar__day[data-kp-tone="ok|warn|bad"]`
  (kp paints them as status plates; recheck what the case measures).
  The page now has the month nav in `.kp-calendar__nav` (Prev, Next, Today).
- Help tour: `:3729` `.tour` -> `.kp-tour`; `:3734` `tour.locator("h3")` ->
  `.kp-tour__title`; `:3736` `footer span` first -> `.kp-tour__count`; `:3740`
  `.tour-target` -> `[data-kp-tour-target]`; `:3761` `.tour` count 0 -> wait for
  it (the card plays its entrance backwards before it is removed); `:3763`
  `localStorage.getItem("homelab-toured")` -> `"kp-remember:tour:homelab:done"`
  (value `true`); `:3749` the Next/Done button names are unchanged; `:8769`
  `dialog.tour` -> `dialog.kp-tour` (`aria-live` still null,
  `aria-labelledby` still set).
- Host log: `.hl-out .hl-ln[data-seq]` at `:5771, 5822, 5825, 5856, 5965, 6015,
  6048, 6104, 6248` -> `.hl-out .kp-log__line[data-seq]` for counts; for
  `waitFor()`, click and hover use `.hl-out .kp-log__line[data-seq] [role=button]`;
  `:5971` `.hl-ln[data-seq="9000000"]` -> `.kp-log__line[data-seq="9000000"]`.
  The detail row and `.hl-out`, `.hl-more`, `.hl-newpill` keep their classes.
- Stack Logs: `:7790` `.sh-log .sh-log__line` -> `.sh-log .kp-log__message`
  (waitFor) / `.sh-log .kp-log__line` (count); `:7803` and `:7808`
  `.sh-log [data-lvl=…]`, `.sh-log [data-src=…]` unchanged (attributes kept).
- `test-e2e/secrets.e2e.js:54` `.sk` -> `.kp-skeleton`.
- Schedules: the `#sched-date` field now holds dd/mm/yyyy text; any case that
  fills it should write `20/10/2026` or an ISO date (both parse). No case
  reads it today (searched `sched-date`, `type=date`).
- Update flow: nothing names `.uf-log` in the tests (searched).

### Left for a later look
- Date picker emoji button (`📅`) follows kp's own example; swap if a theme
  renders it badly.
