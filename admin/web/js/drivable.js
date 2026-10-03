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
 *   opens: "dialog" | "run" | "view", row?: string,
 *   at?: (row: string | null) => string | null, was?: string[]}} Control
 *   `was`: the ids this control had before a redesign renamed or merged it
 *   (3.71.0 coordinator rule), kept as aliases so an old script still finds it.
 *   `at`: where the control is when it lives on a page per stack (the
 *   stack hub's), from its row; otherwise its page's own address.
 */

/** @type {Map<string, Control>} */
const CONTROLS = new Map();
/** An old id (a control's `was`) → the id it has now. @type {Map<string, string>} */
const ALIASES = new Map();

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
  for (const old of spec.was ?? []) {
    if (CONTROLS.has(old) || ALIASES.has(old))
      throw new Error(`the Live view id ${old} is taken (was of ${spec.id})`);
    ALIASES.set(old, spec.id);
  }
  CONTROLS.set(spec.id, Object.freeze({ ...spec }));
  return spec.id;
}

/** Every declared control, sorted by id. @returns {Control[]} */
export const controls = () =>
  [...CONTROLS.values()].sort((a, b) => a.id.localeCompare(b.id));

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
 * @returns {{index: number} | {why: string, fix: string}}
 */
export function pick(want, here) {
  want = { ...want, id: current(want.id) };
  const same = here
    .map((x, index) => ({ ...x, index }))
    .filter((x) => x.id === want.id);
  if (same.length === 0) {
    const label = here
      .map((x, index) => ({ ...x, index }))
      .filter(
        (x) =>
          x.label.trim().toLowerCase() === want.id.trim().toLowerCase() &&
          want.row == null,
      );
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
    if (same.length === 1) return { index: same[0].index };
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
