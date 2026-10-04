// fix-239 (Kenny, 2026-10-02: "waarom gebruik je live view niet?"): every
// button a page draws that opens a dialog or runs something is reachable
// through Live view, even when the page is renamed or the button moves.
//
// Two ways reach a button, and each button says which one it is, where it
// is built:
//
// - **a form** (`viaForm`): the button opens one of the dialogs the
//   dashboard's server models itself — a catalog action (`data-action`)
//   or an edit form — so `homelab ui open <form> [target]` reaches it and
//   the server checks every later step against that form's description;
// - **a page control** (`drivable`): everything else — a stale image's
//   Update, a schedule's Edit, Issue token, Snooze — is declared here, once,
//   by the page module that draws it (`declare`), and `homelab ui click
//   <control> [row]` makes the tab that follows click it exactly as a
//   person would: the same handler, the same dialog, the same requests.
//
// The declarations ARE the registry: a page that draws a control it never
// declared throws, and the whole-screen invariant
// (test-e2e/invariants.e2e.js) fails on any visible button whose click
// opens a dialog or sends a change while it carries neither mark.
//
// A control's `page` is the page it lives on (router.js's page names), so
// `ui click` finds it from anywhere: the tab goes there first when the
// control is not on screen. Rename or move the page and the declaration
// moves with the code that draws the button.

/**
 * One declared page control.
 * `opens`: "dialog" (the click opens one), "run" (it runs something), or
 * "view" (redesign-backups: it only changes what the page shows — pins a night,
 * filters, folds, sorts).
 * @typedef {{id: string, page: string, what: string,
 *   opens: "dialog" | "run" | "view", row?: string, bar?: boolean,
 *   rowOpens?: Record<string, "dialog" | "run" | "view">,
 *   at?: (row: string | null) => string | null,
 *   was?: (string | Was)[], shows?: string, reach?: Reach[],
 *   twins?: boolean}} Control
 *   `rowOpens` (redesign-final-42): a row whose press does something else
 *   than `opens` declares its own, keyed by the row's last part (`update`
 *   of `<stack>/update`) or the whole row — the stack hub's Update goes to
 *   the Update flow ("view") where Back up and Deploy open a dialog.
 *   `bar`: the control lives in the bar on every page (Help's "?"); like
 *   every declared control it is found wherever it is drawn.
 *   `at`: where the control is when it lives on a page per stack (the
 *   stack hub's), from its row; otherwise its page's own address.
 *   `was` (drive-reach, Kenny 2026-10-03: "Claude must always be able to
 *   reach every control, also after it is renamed or moved"): the ids this
 *   control had before, each with the press that picks it when the old
 *   control became an item of this one's menu. `ui click <old>` still
 *   works, and says what the control is called now.
 *   `shows`: when the control is on screen only in some state (a toast's
 *   Undo, a staged value's Write), that state in a few words, for the
 *   refusal and `homelab ui list`.
 *   `reach`: the Live view steps that bring it on screen from its page,
 *   `"*"` as a row meaning the first row on screen. A refusal names them.
 *   `twins` (review M6): the control is drawn more than once without rows
 *   on purpose (a drawer's x and its Cancel), each press doing the same;
 *   any other control found twice on screen is refused, never guessed.
 * @typedef {{id: string, press?: string}} Was
 * @typedef {{do: string, control?: string, row?: string, field?: string,
 *   text?: string, button?: string}} Reach
 * @typedef {{id: string, page: string, what: string, opens: string,
 *   rowOpens?: Record<string, string>, row: string | null, href: string | null, was: Was[],
 *   shows: string | null, reach: Reach[], twins: boolean}} CatalogEntry
 */

/** @type {Map<string, Control>} */
const CONTROLS = new Map();

/** An old id (a control's `was`) → the id it has now. @type {Map<string, string>} */
const ALIASES = new Map();
/**
 * drive-reach: an old id that became an item of its control's menu → the
 * press that picks it there.
 * @type {Map<string, string>}
 */
const PRESSES = new Map();

/** The id a control has now, for an id it may have had before. @param {string} id */
export const current = (id) => ALIASES.get(id) ?? id;

/** A control id: lower-case words joined by hyphens. */
const ID = /^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/;

/**
 * Declare a page control. Called at a page module's top level, so every
 * control is known as soon as the dashboard's code has loaded.
 * @param {Control} spec
 * @returns {string} the id, for `drivable`
 */
export function declare(spec) {
  if (!ID.test(spec.id))
    throw new Error(`a Live view control id is lower-case-words: ${spec.id}`);
  if (CONTROLS.has(spec.id) || ALIASES.has(spec.id))
    throw new Error(`the Live view control ${spec.id} is declared twice`);
  if (!spec.page || !spec.what)
    throw new Error(`the Live view control ${spec.id} needs a page and what`);
  /** @type {Was[]} */
  const was = (spec.was ?? []).map((w) =>
    typeof w === "string" ? { id: w } : w,
  );
  for (const w of was) {
    if (!ID.test(w.id))
      throw new Error(`an old control id is lower-case-words: ${w.id}`);
    if (CONTROLS.has(w.id) || ALIASES.has(w.id))
      throw new Error(`the Live view id ${w.id} is taken (was of ${spec.id})`);
  }
  for (const w of was) {
    ALIASES.set(w.id, spec.id);
    if (w.press) PRESSES.set(w.id, w.press);
  }
  CONTROLS.set(spec.id, Object.freeze({ ...spec, was }));
  return spec.id;
}

/** Every declared control, sorted by id. @returns {Control[]} */
export const controls = () =>
  [...CONTROLS.values()].sort((a, b) => a.id.localeCompare(b.id));

/**
 * drive-reach: what a name a driver used means now: the control itself, or
 * the control an old name moved to, with the press that picks it from that
 * control's menu. `null`: no control has or had that name.
 * @param {string} id
 * @returns {{control: Control, was: string | null, press: string | null} | null}
 */
export function resolve(id) {
  const c = CONTROLS.get(id);
  if (c) return { control: c, was: null, press: null };
  const to = ALIASES.has(id) ? CONTROLS.get(current(id)) : null;
  return to ? { control: to, was: id, press: PRESSES.get(id) ?? null } : null;
}

/** The words of an id or a sentence, lower case. @param {string} s */
const words = (s) =>
  s
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter(Boolean);

/**
 * Levenshtein distance, for a mistyped id, over code points (review M7:
 * the client's Rust copy counts `char`s, so a name with an emoji or an
 * accent scores the same on both sides; test/fixtures/drive-closest.json
 * holds them to it).
 * @param {string} a
 * @param {string} b
 */
export function distance(a, b) {
  const x = [...a];
  const y = [...b];
  let prev = Array.from({ length: y.length + 1 }, (_, i) => i);
  for (let i = 1; i <= x.length; i += 1) {
    const cur = [i];
    for (let j = 1; j <= y.length; j += 1)
      cur[j] = Math.min(
        prev[j] + 1,
        cur[j - 1] + 1,
        prev[j - 1] + (x[i - 1] === y[j - 1] ? 0 : 1),
      );
    prev = cur;
  }
  return prev[y.length];
}

/**
 * drive-reach: the declared controls closest to a name a driver used, best
 * first: by spelling (an id a typo or two away) and by words (a word of the
 * name in a control's id, its old ids, or what it does).
 * @param {string} name
 * @param {number} [n]
 * @param {Control[]} [among] every declared control by default
 * @returns {Control[]}
 */
export function closest(name, n = 5, among = controls()) {
  const want = words(name);
  const scored = among.map((c) => {
    const names = [c.id, ...(c.was ?? []).map((w) => idOf(w))];
    const typo = Math.min(...names.map((x) => distance(name, x)));
    const own = new Set(names.flatMap(words));
    const said = new Set(words(c.what));
    let hits = 0;
    for (const w of want) {
      if (own.has(w)) hits += 2;
      else if (
        said.has(w) ||
        [...own].some((o) => o.startsWith(w) || w.startsWith(o))
      )
        hits += 1;
    }
    const near = typo <= Math.max(2, Math.floor([...name].length / 4));
    return { c, score: hits * 10 + (near ? 30 - typo : 0) };
  });
  return scored
    .filter((x) => x.score > 0)
    .sort((a, b) => b.score - a.score || a.c.id.localeCompare(b.c.id))
    .slice(0, n)
    .map((x) => x.c);
}

/** @param {string | Was} w */
const idOf = (w) => (typeof w === "string" ? w : w.id);

/**
 * drive-reach: the exact `homelab ui` line that clicks `c`, its row as the
 * declared placeholder when none is given and it repeats per row.
 * @param {Control} c
 * @param {string | null} [row]
 */
export const clickLine = (c, row = null) =>
  `homelab ui click ${c.id}${c.row ? ` ${row ?? c.row}` : ""}`;

/**
 * drive-reach: a reach step as the `homelab ui` line that takes it.
 * @param {Reach} s
 */
export function reachLine(s) {
  const rest =
    s.do === "click"
      ? [s.control, s.row]
      : s.do === "press"
        ? [s.button]
        : [s.field, s.text];
  return ["homelab ui", s.do, ...rest.filter((x) => x != null && x !== "")]
    .join(" ")
    .replace(/ \*$/, " <row>");
}

/**
 * redesign-final-42: what pressing `c` on `row` does: the row's own
 * `rowOpens` (by the whole row, then its last part), else the control's.
 * @param {{opens: string, rowOpens?: Record<string, string>}} c
 * @param {string | null | undefined} row
 * @returns {string}
 */
export const opensFor = (c, row) =>
  (row != null &&
    (c.rowOpens?.[row] ?? c.rowOpens?.[row.split("/").pop() ?? ""])) ||
  c.opens;

/**
 * drive-reach: how a driver gets `c` on screen and clicks it, as the
 * `homelab ui` lines to send, in order.
 * @param {Control} c
 * @param {string | null} [row]
 */
export const howToReach = (c, row = null) =>
  [...(c.reach ?? []).map(reachLine), clickLine(c, row)].join("; then ");

/**
 * drive-reach: one control as the catalog (`drivecatalog.json`, built from
 * these declarations by scripts/drivecatalog.mjs) and `homelab ui list`
 * carry it: plain data, its page's address resolved.
 * @param {Control} c
 * @param {(c: Control) => string | null} href where it lives now
 * @returns {CatalogEntry}
 */
export const entry = (c, href) => ({
  id: c.id,
  page: c.page,
  what: c.what,
  opens: c.opens,
  // Only a control whose rows differ carries it (the catalog stays lean).
  ...(c.rowOpens ? { rowOpens: c.rowOpens } : {}),
  row: c.row ?? null,
  href: href(c),
  was: (c.was ?? []).map((w) => (typeof w === "string" ? { id: w } : w)),
  shows: c.shows ?? null,
  reach: c.reach ?? [],
  twins: c.twins === true,
});

/**
 * drive-reach: a page field Live view types into, picks or ticks (`ui type
 * <field>`), by its element id. Declared beside the field when a redesign
 * renamed it, its old ids in `was`, so `ui pick shell-vmid 104` still
 * reaches the field that replaced it. review M5: every page field Live
 * view may set is declared (the commit gate's static check holds the page
 * modules to it), and the client and the dashboard refuse an undeclared one
 * before a tab is asked. A field drawn once per row (a host.toml key) is
 * declared once with `row`, its elements' ids made by `fieldId`.
 * @typedef {{id: string, page: string, what: string, was?: string[],
 *   row?: string}} Field
 */

/** @type {Map<string, Field>} */
const FIELDS = new Map();
/** An old field id → the id the field has now. @type {Map<string, string>} */
const FIELD_ALIASES = new Map();

/** The id a page field has now, for an id it may have had before. @param {string} id */
export const currentField = (id) => FIELD_ALIASES.get(id) ?? id;

/**
 * Declare a page field (at a page module's top level, like `declare`).
 * @param {Field} spec
 * @returns {string} the id, for the element
 */
export function declareField(spec) {
  if (!ID.test(spec.id))
    throw new Error(`a Live view field id is lower-case-words: ${spec.id}`);
  if (FIELDS.has(spec.id) || FIELD_ALIASES.has(spec.id))
    throw new Error(`the Live view field ${spec.id} is declared twice`);
  if (!spec.page || !spec.what)
    throw new Error(`the Live view field ${spec.id} needs a page and what`);
  for (const old of spec.was ?? []) {
    if (!ID.test(old))
      throw new Error(`an old field id is lower-case-words: ${old}`);
    if (FIELDS.has(old) || FIELD_ALIASES.has(old))
      throw new Error(
        `the Live view field id ${old} is taken (was of ${spec.id})`,
      );
  }
  for (const old of spec.was ?? []) FIELD_ALIASES.set(old, spec.id);
  FIELDS.set(spec.id, Object.freeze({ ...spec, was: [...(spec.was ?? [])] }));
  return spec.id;
}

/** Every declared page field, by id. */
export const fields = () =>
  [...FIELDS.values()].sort((a, b) => a.id.localeCompare(b.id));

/**
 * The element id of a declared field's row (`fieldId("key", "ask-timeout")`
 * is `key-ask-timeout`).
 * @param {string} id a field declared with `row`
 * @param {string} row
 */
export function fieldId(id, row) {
  const f = FIELDS.get(id);
  if (!f) throw new Error(`the Live view field ${id} was never declared`);
  if (!f.row) throw new Error(`the Live view field ${id} has no rows`);
  return `${id}-${row}`;
}

/**
 * The declared field a name a driver used means: by its id, an old id, or
 * as one row of a field per row. `null`: no page declares it.
 * @param {string} name
 * @returns {Field | null}
 */
export function declaredField(name) {
  const f = FIELDS.get(currentField(name));
  if (f) return f;
  for (const g of FIELDS.values())
    if (g.row && name.startsWith(`${g.id}-`) && name.length > g.id.length + 1)
      return g;
  return null;
}

/**
 * redesign-final (coordinator, 2026-10-04): an old page field that became
 * a control repeating per row (a select turned into one press per choice:
 * Backups' Restore picker selects `bk-restore-*`, the rules page's
 * `notify-snooze-minutes`). `ui pick <old> <value>` and `ui type <old>
 * <value>` still land: they press that control's row `value`. `null` when
 * `field` is a declared field or no control's `was`.
 * @param {string} field
 * @param {string} value
 * @returns {{control: string, row: string} | null}
 */
export function fieldAsClick(field, value) {
  if (FIELDS.has(currentField(field)) || CONTROLS.has(field)) return null;
  const r = resolve(field);
  if (!r || !r.was || !r.control.row || r.press) return null;
  return { control: r.control.id, row: value };
}

/** @param {string} id @returns {Control | null} */
export const control = (id) => CONTROLS.get(current(id)) ?? null;

/**
 * Mark `el` as the declared control `id` — on row `row` when the control
 * repeats once per row (the row's own key, e.g. `<stack>/<app>/<service>`).
 * @template {HTMLElement} E
 * @param {E} el
 * @param {string} id
 * @param {string} [row]
 * @returns {E}
 */
export function drivable(el, id, row) {
  const c = CONTROLS.get(id);
  if (!c) throw new Error(`the Live view control ${id} was never declared`);
  if (c.row && row == null)
    throw new Error(`the Live view control ${id} repeats per row: name it`);
  el.dataset.drive = id;
  if (row != null) el.dataset.driveRow = row;
  return el;
}

/**
 * Name a button inside a page-level dialog for `ui click <name>` (or `ui
 * press <name>`) while that dialog is open: a dialog's buttons are found
 * among its own, never on the page, so they need no declaration. A button
 * left unnamed is still found by its visible label.
 * @template {HTMLElement} E
 * @param {E} el
 * @param {string} name
 * @returns {E}
 */
export function dialogControl(el, name) {
  if (!ID.test(name))
    throw new Error(`a dialog control name is lower-case-words: ${name}`);
  el.dataset.drive = name;
  return el;
}

/**
 * Mark `el` (a button, or a card holding several) as reached through the
 * server-modelled form `form` (`homelab ui open <form> …`).
 * @template {HTMLElement} E
 * @param {E} el
 * @param {string} form
 * @returns {E}
 */
export function viaForm(el, form) {
  el.dataset.driveForm = form;
  return el;
}

/**
 * Where a `ui click` lands, decided from what is on screen: the element,
 * or why there is none, in the words the driver reads back.
 * @param {{id: string, row: string | null}} want
 * @param {{id: string, row: string | null, label: string}[]} here the
 *   marked controls on screen now (a dialog's own buttons by label too)
 * @param {boolean} [byLabel] a button may be named by its visible label (in
 *   a dialog; a page's own controls answer to their declared id only)
 * @returns {{index: number} | {why: string, fix: string}}
 */
export function pick(want, here, byLabel = true) {
  want = { ...want, id: current(want.id) };
  const same = here
    .map((x, index) => ({ ...x, index }))
    .filter((x) => x.id === want.id);
  if (same.length === 0) {
    // In a dialog a button is found by its visible label too (the page's
    // own controls are found by their declared id only).
    const label = byLabel
      ? here
          .map((x, index) => ({ ...x, index }))
          .filter(
            (x) =>
              x.label.trim().toLowerCase() === want.id.trim().toLowerCase() &&
              want.row == null,
          )
      : [];
    if (label.length === 1) return { index: label[0].index };
    const ids = [...new Set(here.map((x) => x.id).filter(Boolean))].sort();
    return {
      why: `there is no control ${want.id} on screen`,
      fix: ids.length
        ? `the controls on screen are: ${ids.join(", ")}`
        : "nothing on screen can be clicked this way; homelab ui goto the page first",
    };
  }
  if (want.row == null) {
    // review M6: a control drawn twice without rows is one control only
    // when its declaration says so (`twins`: a drawer's x and its Cancel,
    // either press doing the same); otherwise which one is meant is not
    // guessed.
    if (same.length === 1) return { index: same[0].index };
    if (same.every((x) => x.row == null)) {
      if (CONTROLS.get(want.id)?.twins) return { index: same[0].index };
      return {
        why: `${want.id} is on screen ${same.length} times and is not declared as twins`,
        fix: "declare it with twins: true when every copy does the same, or give each its own id",
      };
    }
    const rows = same.map((x) => x.row ?? "").sort();
    return {
      why: `${want.id} is on ${same.length} rows`,
      fix: `name the row: homelab ui click ${want.id} <row>; its rows are: ${rows.join(", ")}`,
    };
  }
  const hit = same.find((x) => x.row === want.row);
  if (hit) return { index: hit.index };
  const rows = same.map((x) => x.row ?? "").sort();
  return {
    why: `${want.id} has no row ${want.row}`,
    fix: `its rows are: ${rows.join(", ")}`,
  };
}
