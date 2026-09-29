# kp-themes bug reports from the homelab dashboard (2026-09-29)

## Status after chassis-rs 2.4.1 (kp-themes 8.0.0, via 7.3.0)

The dashboard moved to chassis-rs v2.4.1 on 2026-09-29; it vendors
kp-themes 8.0.0 (for web pages identical to 7.3.0, which carries the fixes).
Each workaround below was removed and the native behaviour measured in one
Playwright run (Chromium, the dashboard built from the tree reading the real
host read-only, 1280 and 1440 px). Every report is **resolved in 8.0.0**;
no workaround is left in `app.css` or `dom.js`.

| # | Report | kp fix | Status | Measurement without the workaround |
|---|--------|--------|--------|-------------------------------------|
| 1 | Bar ghost buttons take the page's ink | fix-77 | resolved in 8.0.0 | all 22 themes × 1280/1440 pass at rest: lowest bell/help 5.0:1 (shade-dark), theme icon 6.27:1 (shade-light) |
| 2 | Select picker runs off screen, labels right-aligned | fix-78 | resolved in 8.0.0 | Answer dialog, 32 options at 1280: list 630 px wide (x 363-993), inside the window; labels start 10 px from the option's start edge, wrap to 2-3 lines, check mark at the end |
| 3 | Switch words change its width | fix-79 | resolved in 8.0.0 | four switches, checked vs unchecked: box 32 px both ways, the label after it moved 0 px |
| 4 | First load: skeleton with no sign of progress | fix-80 | resolved in 8.0.0 | Today's first load at 1.5/4.5/8.5 s: kp's spinner, the page's words and kp's counter ("1 s … 8 s so far"), status line in the same place (height 20 px) throughout |
| 5 | Bar ghost buttons: hover plate is the page's | fix-82 | resolved in 8.0.0 | hover and keyboard focus (`:focus-visible`) of bell, help, theme, Go to, menu toggle in 22 themes × 1280/1440: 0 fails; lowest ghost hover 5.33:1 (phantom), theme icon hover 4.26:1 (lapis, icon ≥ 3:1) |
| 6 | Multi-sort summary shares the search's row | fix-83 | resolved in 8.0.0 | history table at 1440, five sort keys: search, sort line, Reset the sort and the first row moved 0 px; the line stays 45 px high; the reset keeps its place hidden while unsorted |
| 7 | busy(text) not released; counter by hand | fix-80, fix-84 | resolved in 8.0.0 | `busy({ text, since })`: kp ticks the counter itself in an `aria-hidden` part; a refresh keeps the rows (opacity 0.55) and says "Showing the rows from …" |
| 8 | Empty and failed slots half there without a server | fix-85 | resolved in 8.0.0 | incidents answered 502 (route mock): kp's own failed slot, the reason in three lines, Try again reads again (103 rows, slot hidden); incidents answered empty: the "nothing yet" part only; a search matching nothing: the "nothing matches" part with Clear, 61 rows back after it |

The dashboard now uses: kp's sort line and reset (no sort slot of its own),
`busy({ text, since })` (no spinner or counter of its own; `tablestate.js
busyWords` only writes the words), `fail(reason)` in kp's own failed slot
(`tablestate.js failReason`: what, why, what to do; `white-space: pre-line`
in `app.css` keeps the three lines), and the empty slot's
`data-kp-datatable-empty-none` / `-nomatch` parts (the page writes the
"nothing matches" words from the view). What stays in `app.css` is layout of
the dashboard's own: the search taking the toolbar's width, tabular digits in
the status line, the slots' margins inside the frame, and the version text in
the bar wearing the bar's ink (`.kp-nav .link`, the dashboard's own element).

---

## The reports as filed

Found while fixing Kenny's UI findings of 2026-09-29 03:40 in the homelab
admin dashboard (kp-themes as vendored by chassis-rs 2.4.0; source checked
at kp-themes 1d6ef4ac). Each is worked around in
`homelab/admin/web/css/app.css` only (unlayered rules, scoped to the app);
nothing in kp-themes was changed.

---

## 1. Ghost buttons in the nav bar take the page's ink, not the bar's

- **Component:** nav bar (`.kp-nav`) with `.kp-button--ghost` inside it
  (the icon buttons a consumer puts in the bar: a bell, a "?" button).
- **Where:**
  - `css/components.css:683-687` — `.kp-button--ghost { color: var(--foreground) }`
  - `css/cyberpunk-register.css:550-554` — `--kp-btn-fg: var(--foreground)` for the ghost
  - `css/cyberpunk-register.css:264-266` — the bar is `--surface-hero-bg` (yellow) with `--surface-hero-fg` ink
  - `css/nostromo-register.css:109-113` (bar on `--sidebar-background` / `--sidebar-foreground`) vs `:432-436` (ghost `color: var(--foreground)`)
  - `css/forest-register.css:458-462` (ghost `color: var(--primary)`)
- **Repro:** `<nav class="kp-nav">…<button class="kp-button kp-button--ghost">?</button></nav>`
  in cyberpunk. The "?" is the page's light foreground on the yellow bar.
- **Measured** (text/icon colour vs the bar's plate, WCAG ratio, before the
  workaround): cyberpunk 1.20:1 (bell and "?"), nostromo 1.00:1, forest "?"
  1.53:1, shade-dark "?" 3.65:1. kp's own theme-picker trigger in the same
  bar reads 15.6:1 in cyberpunk, because it inherits the bar's colour.
- **Expected:** every control in the bar is drawn in the bar's ink
  (≥ 4.5:1 text, ≥ 3:1 icon) in all 22 themes, as the theme trigger is.
- **Suggested fix:** in `components.css` (and each register that repaints
  the ghost), `.kp-nav .kp-button--ghost { --kp-btn-fg: currentColor; color: inherit; }`
  (the nav's `color` is the bar ink every register already contrast-gates),
  and add a ghost button inside `.kp-nav` to the contrast gate's fixtures.
  The same applies to muted text in the bar (`--muted-foreground` on
  cyberpunk's yellow read 2.56:1, nostromo 1.82:1): a bar needs its own
  muted token or `color: inherit`.
- **Workaround in the dashboard:** `.kp-nav .kp-button--ghost { --kp-btn-fg: currentColor; color: inherit }` and `.kp-nav .link { color: inherit }`.

---

## 2. The select's open list (base-select) runs off the screen and right-aligns every label

- **Component:** native `select.kp-field__input` with `appearance: base-select` (Chromium 135+).
- **Where:** `css/components.css:1011-1045`
  - `:1016-1025` `::picker(select)` has a max block size but no max inline size;
    the picker's UA width is at least the anchor's and otherwise its content's
    max-content width, and options do not wrap (UA `white-space: nowrap`).
  - `:1041-1044` `option::checkmark { margin-inline-start: auto }` — in
    Chromium the check mark is the option's FIRST flex item, so the auto
    margin pushes the check mark and the label to the end: every label is
    right-aligned.
- **Repro:** a select of 32 options of 60-200 characters (the dashboard's
  "Answer a manual check" form) in a 40rem dialog at 1280×900. Open it.
- **Measured:** picker 983 px wide from x=298 to the viewport edge; each
  option 973 px, content wider still, labels right-aligned and cut at the
  viewport's right edge (≈60 % of a long label visible); Kenny: scrolling the
  list then shows a different cut (≈80 %).
- **Expected:** the list is at most a sensible width (the dialog's, or
  `min(40rem, 100vw - 2rem)`), an option wraps inside it, the label starts
  at the start edge and the check mark sits at the end.
- **Suggested fix:**
  ```css
  select.kp-field__input::picker(select) {
      max-inline-size: var(--kp-picker-max-width, min(40rem, calc(100vw - 2rem)));
  }
  select.kp-field__input option { white-space: normal; overflow-wrap: anywhere; }
  select.kp-field__input option::checkmark { order: 1; margin-inline-start: auto; }
  ```
  (`order: 1` is how Chromium's own base-select examples put the mark at the end.)
- **Workaround in the dashboard:** exactly the three rules above, inside
  `@supports (appearance: base-select)`.

---

## 3. The switch's On/Off words change the switch's width

- **Component:** switch (`.kp-switch` with `.kp-switch__state`, filled by `attachSwitches` in `js/forms.js:405-440`).
- **Where:** `css/components.css:1276-1290`
  - `:1277` `.kp-switch__state { min-inline-size: var(--kp-switch-state-min, 2rem) }`
  - `:1283-1290` the word that does not apply is `display: none`.
- **Repro:** a `.kp-switch` with a label after the state word; set
  strings whose words are wider than 2rem (`setStrings({switchOn: "Enabled",
  switchOff: "Disabled"})`, or a translation such as "Activé"/"Désactivé"),
  or raise the text size; toggle it. The label after the word moves by the
  difference between the two words. Measured with the English defaults in
  all 22 themes: "On" 14-18 px, "Off" 15-21 px, box 32 px, label did not
  move (so today it holds by the 2rem floor, not by construction).
- **Expected:** toggling never moves anything (Kenny, 2026-09-29: after a
  click, a second click without moving the mouse must hit the same element).
- **Suggested fix:** keep both words laid out in one grid cell and hide the
  one that does not apply with `visibility`, so the box is always as wide as
  the wider word, in any face or language:
  ```css
  .kp-switch__state { display: inline-grid; }
  .kp-switch__on, .kp-switch__off { grid-area: 1 / 1; display: inline; }
  .kp-switch__on, .kp-switch:has(:checked) .kp-switch__off { visibility: hidden; }
  .kp-switch:has(:checked) .kp-switch__on { visibility: visible; }
  ```
  and drop the 2rem floor (or keep it as a knob on top).
- **Workaround in the dashboard:** the rules above (see `app.css`, "kp's switch words").

---

## 4. Datatable: a first load shows a skeleton with no sign of progress

- **Component:** datatable (`js/datatable.js`).
- **Where:** `js/datatable.js:1511-1531` (`syncSkeleton`: three pulsing
  rows on a first load) and `:1675-1685` (the status line gets a spinner only
  when `state === 'loading' && all.length > 0`, i.e. on a refresh).
- **Repro:** a datatable with `data-kp-state="loading"` and no rows, left
  loading for a minute and a half (the host's `homelab today` takes 93 s).
- **Observed:** three grey bars pulse and the status line says the busy
  word; nothing tells the reader that anything is still happening or how
  long it has taken. Kenny read it as "it seems to load, but nothing loads".
  (The dashboard's own fault was worse: the request died at 30 s, see its
  report, but the table looked the same while it did.)
- **Expected:** the first load carries the same spinner the refresh does,
  and a consumer can put its own progress text in the status line (for
  example "Asking the host… 42 s so far") without it being overwritten on
  the next `refresh()`.
- **Suggested fix:** add the spinner in the first-load case too, and read
  an optional `data-kp-busy-text` (or a `busy(text)` handle method) for the
  status line while `state === 'loading'`.
- **Workaround in the dashboard:** the page shows its own elapsed line
  ("… 42 s so far; the host needs about a minute and a half") above the
  table and keeps a table it fills only on request hidden until the first
  request. Not a CSS override; nothing of kp's is patched for this one.

---

# Second round (2026-09-29, Kenny's findings of 05:39)

Checked against kp-themes main at 3bee5403 (fix-77..80 are on main, not
released; chassis-rs 2.4.0 still vendors kp-themes 7.2.0). Each item is
worked around in the dashboard (`admin/web/css/app.css`,
`admin/web/js/dom.js` `tableBlock`) and marked there as a shim to remove.

## 5. Nav-bar ghost buttons: the hover plate is the page's, not the bar's

- **Component:** nav bar (`.kp-nav`) with `.kp-button--ghost` inside it;
  fix-77 made the resting state inherit the bar's ink, the hover state was
  left out.
- **Where (main 3bee5403):**
  - `css/nostromo-register.css:445-447` — hover `background: var(--card)`
  - `css/solstice-register.css:275-277` — hover `background: var(--muted)`
  - `css/high-contrast-register.css:1177-1181` — hover insets in
    `--kp-hc-bar-ink` (`var(--background)`), and `:408-409` the `::after` bar
  - `css/phantom-register.css:538-548` — hover sweeps a `--primary` plate
    (`::after`) under the button and sets `color: var(--primary-foreground)`
  - `css/components.css:698-701` — the package hover plate
    `hsl(from var(--foreground) h s l / 0.08)`, the page's foreground
- **Repro:** `<nav class="kp-nav">…<button class="kp-button kp-button--ghost">?</button></nav>`
  with the bar's ink inherited (fix-77); hover the button in nostromo,
  high-contrast, phantom.
- **Measured** (the button's ink against the rendered plate under it,
  hovered, 1280 and 1440 px, homelab dashboard): nostromo 1.12:1,
  high-contrast 1.00:1, phantom 3.56:1 (white "?" on the red sweep). At rest
  all 22 themes pass (lowest shade-dark 5.0:1).
- **Expected:** a control in the bar reads at ≥ 4.5:1 (text) / ≥ 3:1 (icon)
  in every state, hover and focus included.
- **Suggested fix:** in `components.css` (and each register that paints a
  ghost hover), inside the bar:
  ```css
  .kp-nav :is(.kp-button--ghost, .kp-icon-button):is(:hover, :focus-visible):not(:disabled) {
      background: transparent;
      border-color: currentColor;   /* the hover is a frame in the bar's ink */
      box-shadow: none;
  }
  .kp-nav :is(.kp-button--ghost, .kp-icon-button):is(:hover, :focus-visible)::before,
  .kp-nav :is(.kp-button--ghost, .kp-icon-button):is(:hover, :focus-visible)::after {
      background: transparent;      /* no register sweep plate */
  }
  ```
  or give the bar its own hover token derived from the bar ink, and add the
  hovered state to `tests/nav-ghost.spec.mjs`.
- **Workaround in the dashboard:** exactly those rules (app.css, after
  "The bar's icon buttons … wear the bar's own ink").

## 6. Datatable: the multi-sort summary shares the search's row

- **Component:** datatable with `data-kp-sort-multi`.
- **Where:** `js/datatable.js:1259-1268` — without a consumer's
  `[data-kp-datatable-sort-summary]` the summary is made and
  `ensureTopBar().prepend(sortSummary)`: it sits in the toolbar before the
  search, and `.kp-datatable__sort-summary { flex: 1 1 auto }`
  (`css/components.css`, "multi-sort summary") makes it grow with its text.
- **Repro:** a datatable with a search and `data-kp-sort-multi`; Shift+click
  four headers.
- **Measured:** at 1440 px the search box was 743 px unsorted ("Not
  sorted." 615 px beside it) and shrank with every key added; the search box
  moved under the pointer on every sort (Kenny: "als ik nu veel dingen
  sort, verschuift de UI ook"). The demo's toolbar gives the search most of
  the width.
- **Expected:** sorting never moves the toolbar or the rows; the search keeps
  its width.
- **Suggested fix:** create the summary as its own line under the toolbar
  (a `.kp-datatable__bar` of its own, one line, `text-overflow: ellipsis`,
  full text in `title`), with the sort reset (`data-kp-datatable-sort-reset`)
  at its end, hidden by `visibility` while nothing is sorted so the line
  never changes height. The reset button has no automatic visibility today
  (`:2806` only handles its click).
- **Workaround in the dashboard:** `tableBlock` supplies the summary and the
  reset in their own bar (`.table-sortline`), toggles the reset's
  `data-off` from the view event; measured: search 1267 of 1390 px (91 %),
  search, sort line and first row did not move across five sort keys.

## 7. Datatable: busy(text) and the first-load spinner are on main but not released

- **Component:** datatable loading state (fix-80, main 8c2c7f8a).
- **Where (7.2.0 as vendored by chassis 2.4.0):** `js/datatable.js:1675-1685`
  — spinner only when `state === 'loading' && all.length > 0`, words always
  `strings.busy` ("Working…"), rewritten on every render.
- **Observed:** Kenny, 05:39: "Er staat nog altijd gewoon Working... en de
  default skeleton dingen. Is te statisch."
- **Request:** release fix-80 and have chassis-rs vendor it. Two additions
  that every consumer of a slow read would otherwise build again:
  1. `busy({ text, since })`: with a start time the table ticks the counter
     itself ("42 s so far") instead of the consumer calling `busy()` every
     second (which re-renders the whole table each second).
  2. The counter outside the live region's announcement (or a polite
     announcement only when the words change), so a screen reader is not
     told every second.
- **Shim in the dashboard:** `tableBlock` → `setBusyText(text)` calls the
  handle's `busy(text)` when it exists; otherwise it paints exactly what
  fix-80 paints (spinner, then the words) after each kp render (the
  `kp-datatable-view` event). Remove when chassis vendors a kp-themes with
  `busy()`.

## 8. Datatable: the empty and failed slots are half there without a server

- **Component:** datatable (framework-free), `data-kp-datatable-empty`,
  `data-kp-datatable-failed`, `data-kp-datatable-retry`.
- **Where:**
  - `js/datatable.js:1338-1350` — the failed slot (with its retry button) is
    made only for a `data-kp-server` table; a table whose consumer loads the
    rows itself and calls `state('failed')` shows nothing but "Showing 0 of 0".
  - `:1702` — one empty slot for "nothing at all" and "nothing matches";
    the demo's `#states` draws them apart (different words, different way
    out), marked "mock".
  - The failed slot's text is the dictionary's `tableFailed`; there is no
    way to hand it the reason ("the host did not answer, HTTP 502").
- **Expected:** every table that can fail has the failed slot with the
  reason and Try again; the empty slot says "there is nothing yet" and "no
  row matches the search/filters (N hidden) — clear them" apart.
- **Suggested fix:** make the failed slot for any table (not only server
  mode), add `fail(reason?: string | Node)` to the handle, and read two
  optional children of the empty slot (`data-kp-datatable-empty-none`,
  `data-kp-datatable-empty-nomatch`) shown by `view().total === 0`.
- **Workaround in the dashboard:** `tableBlock` puts its own failed slot
  (label, why, what to do, `data-kp-datatable-retry`) and an empty slot whose
  words follow the view event (`tablestate.js emptyWords`); kp shows, hides
  and wires both (native), the dashboard only writes the words.
