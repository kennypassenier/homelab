# kp-themes bug reports from the homelab dashboard (2026-09-29)

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
