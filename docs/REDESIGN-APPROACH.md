# How the 3.71.0 dashboard redesign was run

Written 2026-10-04 for other projects that start a UI rewrite (JobTracker
first). It describes the process and the guards, not the pages; the page
rules themselves live in the design language
(`~/.local/share/homelab/redesign-3.71/DESIGN_LANGUAGE.md`). The
house-wide principles every UI follows are
`~/Projects/dev-procedure/UI_PRINCIPLES.md` (standing rule 53); this file
is the homelab case study behind them.

## 1 · Steps

1. **Audit before design.** One pass over every existing page at 1894 px
   and 390 px, in a light and a dark theme, listing what each page is for,
   what a person does there most, and every defect (cut text, sideways
   scroll, two headers, raw ids, unclear actions). The audit is the brief
   for the design; nothing is redesigned without a row in it.
2. **Structure first.** Decide the information architecture before any
   pixels: six areas, which page lives where, which old addresses redirect
   where. The redirect table and the renamed-control aliases are built
   from that decision, not afterwards.
3. **One shared design language, then demos.** A single
   `DESIGN_LANGUAGE.md` (tokens only, page anatomy, type scale, grid,
   states) that every demo page is built to, so the pages read as one
   product. Demos are static HTML on real captured data (never invented
   shapes), screenshotted in every theme.
4. **Kenny approves the demos in one form.** Every page and every choice
   in one interactive form, each item with a plain-language explanation and
   the screenshot. Once a demo is approved, it is the spec: **build it
   exactly.** A deviation is either built anyway or put back to Kenny; it
   is never silently "registered".
5. **Shared kit before pages.** One kit (`admin/web/js/ui.js`: pageHeader,
   section, KPI meter, toggleGroup, segSwitch, toast, rowMenu/moreMenu,
   sortHead + sortstate, chip, dot, meter, failBox, rowKeys, drawer)
   landed and was reviewed before any page moved onto it. Six page kits
   that had grown their own copies were folded into it.
6. **Build per page on its own branch**, one helper per page, each with a
   senior review and a fix round before it merges into the integration
   branch.
7. **A final review over the integrated whole**, by a fresh reviewer, with
   findings ranked critical / high / medium / low / cross-page coherence.
   Every finding is fixed before the release go (no "later" list).

## 2 · Grid and layout

- Every page is a 12-column grid. Cards span 12 / 8 / 6 / 4 / 3 columns at
  ≥ 1200 px, 6 / 12 on a tablet, 12 on a phone. One phone breakpoint
  token for the whole app.
- Page anatomy, top to bottom: header row (title + one-sentence
  description left, one primary action + at most two secondary + an
  overflow menu right; the freshness line sits right after the title),
  a KPI strip for live pages, an attention band only when something needs
  a person, then content cards ordered by use.
- Loading, empty, error and filled states occupy the same box: the
  skeleton has the final geometry, so nothing jumps.
- kp-themes tokens only: no hex colours, no raw px spacing
  (`--kp-space-*`), no new fonts. A status colour is text only on its own
  status plate (`--x` background + `--x-foreground`), never on a card.
- Tables are kp datatables: sortable, multi-sort with a plain click
  (no Shift) plus "Reset sort", remembered per table.

## 3 · Generic checks (one per fault class)

Rule: when a defect is found, ask what class it belongs to and build one
check that finds every instance on every page, failing first on the
defect. Never fix only the one spot.

| Class | Where |
|---|---|
| Cut text, letter-per-line wraps, row overlap, two page headers, one date format (dd/mm/yyyy HH:MM, 24 h), "0 problems" without a red chip, internal ids in user text, one freshness slot, exact counts (linked via `data-count-of`), a description under the title, the title row, monospace size, links that resolve, no sideways scroll at 390 px, the action dialog field grid | `admin/web/test-e2e/layoutaudit.js`: ONE walk over every page and every action dialog at 1894 and 390 px reports every class in one pass (`invariants.e2e.js`, redesign-final-gen; parked classes in `test-e2e/parked.json`) |
| Text contrast WCAG AA on every banner, badge and chip, in every theme the picker offers | `invariants.e2e.js` (redesign-final-contrast) |
| Row selection and focus survive a live refresh (rows updated in place by stable id) | `invariants.e2e.js` (redesign-final-extra-live-selection) |
| Names matched through one normalising key (spaces, capitals, unicode) | `namekey.test.js` + Rust `names_tests` |
| No test, harness or demo host reads the real clock | `admin/web/scripts/check-test-clock.mjs`, one injected clock (`test-e2e/clock.js`) |
| Every Live view control is declared and known to the catalog (old names resolve for click and field verbs) | static node tests (`drivecatalog.test.js`, `drivable.test.js`) and the client's own check (`client/src/ui_catalog.rs`); the press-every-control sweep and its stamp were removed on 2026-10-04 (Kenny: know every control, do not press them all) |

## 4 · Review and gate setup

- **Pre-commit:** fmt/lint/secrets, the drive catalog test, and the
  whole-screen cases of the pages the commit changes (mapping derived
  from the import graph, not a list).
- **Merge / integration:** the merge check runs the affected cases and
  refuses the merge naming each failure; a fast-forward goes through it
  too (`post-merge` undoes a failing one).
- **Test harness:** a free port per run, a check that the server answering
  is the one the run started, and the server killed on every exit path.
  An orphaned demo server once answered a run on a fixed port.
- **Release gate:** the full suite, once. Every test run reports its measured
  duration.
- **Proofs:** every guard is proven by a commit that breaks the thing it
  guards and is refused, in a throwaway clone.

## 5 · Mistakes Kenny corrected (avoid them)

- Reporting a branch as green while cases were red on it. Read the run
  before claiming; never "probably", never "I'll check later".
- Guessing Live view control names. Every control is declared; renamed
  ones keep a `was` alias.
- Treating a demo deviation as "registered" instead of building it.
- Overriding an explicit Kenny rule with demo text (dates: his
  dd/mm/yyyy HH:MM won over the demo's "Sat 3 Oct").
- Slow gates: a full 4-minute sweep per commit and a 15-minute merge check
  were both cut back to "only what changed" plus one full run at the gate.
- Too many helpers in parallel overloaded the machine and hit the account
  limit: two slots per queue (cargo and whole-screen), one helper at a
  time when Kenny says so.
- Several pieces of work open and uncommitted at once. Commit each finding
  on its own as soon as its tests pass.
- Progress updates with bare codes or file counts. Every update says what
  finished, what runs, what is next, in plain words, with the counters
  (N of M) moving.
- Clock-dependent demo data and tests that pass or fail with the hour of
  the day.
