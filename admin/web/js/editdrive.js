// Milestone follow (feat-platform-10): Live view plays Claude's steps on
// the edit forms. Each controller works the page's or dialog's own parts,
// found through drivehooks.js: the stack's settings tab (the settings form,
// the raw editor, add an app), its firewall tab with the rule dialog, the
// plan dialog every stack edit ends in, the new-stack wizard, the host
// settings page with its key and review dialogs, the batch dialog and the
// roll back dialog. A value is set the way a person sets it (the input's
// own events run the page's own handlers), a Review button is clicked so
// the page opens its own plan dialog, and the final press is only shown:
// it ran on the dashboard's server, once.

import { openBatch } from "./actiondialog.js";
import { handle, waitFor, waitHandle } from "./drivehooks.js";
import { openNewStack } from "./newstack.js";
import { openRollback } from "./rollbackdialog.js";
import { clearError, showError } from "/static/kp/js/forms.js";

/**
 * @typedef {import("./driveview.js").DriveForm} DriveForm
 * @typedef {{dialog: HTMLDialogElement | null, closed: Promise<void>,
 *   close: () => void,
 *   input: (name: string, id?: string) => HTMLElement | null,
 *   set: (name: string, value: string | boolean, id?: string) => void,
 *   button: (name: string) => HTMLElement | null,
 *   row: (op: string, target?: string) => HTMLElement | null,
 *   sync: (f: DriveForm) => Promise<void>}} DrivenControl
 */

/**
 * Set a field as a person does: the value, then its own input and change
 * events, so the page's handlers keep their model.
 * @param {Element} el
 * @param {unknown} value
 */
export function setInput(el, value) {
  if (el instanceof HTMLInputElement && el.type === "checkbox")
    el.checked = value === true;
  else if (
    el instanceof HTMLInputElement ||
    el instanceof HTMLSelectElement ||
    el instanceof HTMLTextAreaElement
  )
    el.value = value == null ? "" : String(value);
  else return;
  el.dispatchEvent(new Event("input", { bubbles: true }));
  el.dispatchEvent(new Event("change", { bubbles: true }));
}

/** @param {Element} el */
const valueOf = (el) =>
  el instanceof HTMLInputElement && el.type === "checkbox"
    ? el.checked
    : /** @type {HTMLInputElement} */ (el).value;

/** @param {string | undefined} id */
const byId = (id) =>
  id ? /** @type {HTMLElement | null} */ (document.getElementById(id)) : null;

/**
 * JSON with its keys in order: the server's objects and the page's are
 * the same model whatever order their keys came in.
 * @param {unknown} v
 * @returns {string}
 */
export function canon(v) {
  if (Array.isArray(v)) return `[${v.map(canon).join(",")}]`;
  if (v && typeof v === "object")
    return `{${Object.keys(v)
      .sort()
      .map(
        (k) =>
          `${JSON.stringify(k)}:${canon(/** @type {Record<string, unknown>} */ (v)[k])}`,
      )
      .join(",")}}`;
  return JSON.stringify(v ?? null);
}

/**
 * Each field's value from the state, where its input is on screen.
 * @param {DriveForm} f
 * @param {(x: import("./driveview.js").DriveField) => boolean} which
 */
function fill(f, which) {
  for (const x of f.fields) {
    if (!which(x)) continue;
    const el = byId(x.id);
    if (!el || valueOf(el) === x.value) continue;
    setInput(el, x.value);
  }
}

/** @param {DriveForm} f */
function mark(f) {
  for (const x of f.fields) {
    const el = byId(x.id);
    if (!el) continue;
    if (x.error) showError(el, x.error);
    else if (el.getAttribute("aria-invalid") === "true") clearError(el);
  }
}

/** The close button of a dialog. @param {HTMLDialogElement | null | undefined} d */
const closeOf = (d) =>
  /** @type {HTMLElement | null} */ (
    d?.querySelector(".kp-dialog__close") ?? null
  );

/** A controller that is never the viewer's to close. */
function base() {
  /** @type {() => void} */
  let done = () => {};
  const closed = new Promise((r) => (done = () => r(undefined)));
  return { closed, done };
}

/**
 * The stack's own edits: settings, raw, add-app, firewall.
 * @param {DriveForm} first
 * @returns {DrivenControl}
 */
function stackEdit(first) {
  const family = first.action;
  const stack = first.stack;
  const { closed, done } = base();
  let current = first;
  const fwKey = `firewall:${stack}`;
  const seKey = `stack-edit:${stack}`;
  const top = () => handle("rule")?.dialog ?? handle("plan")?.dialog ?? null;
  return {
    get dialog() {
      return top();
    },
    closed,
    close: () => {
      handle("rule")?.close();
      handle("plan")?.close();
      done();
    },
    input: (_n, id) => byId(id),
    set: (_n, v, id) => {
      const el = byId(id);
      if (el) setInput(el, v);
    },
    button: (name) => {
      const rule = handle("rule");
      if (rule) return name === "save" ? rule.save() : rule.cancel();
      if (name === "close") return closeOf(top());
      if (current.step_index === 0)
        return family === "firewall"
          ? (handle(fwKey)?.review() ?? null)
          : (handle(seKey)?.review(family) ?? null);
      const p = handle("plan");
      return name === "back" ? (p?.back() ?? null) : (p?.next() ?? null);
    },
    row: (op, target) => {
      const fw = handle(fwKey);
      if (!fw) return null;
      return op === "add" ? fw.add() : fw.rowButton(op, Number(target) - 1);
    },
    sync: async (f) => {
      current = f;
      const e = /** @type {import("./driveview.js").DriveEdit} */ (
        f.edit ?? { family: "", guarded: 0 }
      );
      const fw = family === "firewall" ? await waitHandle(fwKey) : null;
      const se = family === "firewall" ? null : await waitHandle(seKey);
      if (family === "raw") se?.openRaw();
      if (fw && e.model && canon(fw.model()) !== canon(e.model))
        fw.setModel(e.model);
      const rule = handle("rule");
      if (fw && e.sub && !rule) {
        fw.openRule(typeof e.sub.target === "number" ? e.sub.target : null);
        await waitHandle("rule", 2000);
      } else if (!e.sub && rule) rule.close();
      fill(f, (x) => x.step === f.steps[0] || x.step === "rule");
      if (f.step_index === 0) handle("plan")?.close();
      else {
        let p = handle("plan");
        if (!p) {
          (fw ? fw.review() : se?.review(family))?.click();
          p = await waitHandle("plan", 3000);
        }
        if (p) {
          await p.ready;
          const want = f.step === "commit" ? 1 : 0;
          if (p.step() !== want) await p.goTo(want);
          fill(f, (x) => x.step === "commit");
          p.runError(f.run_error);
          if (e.result) p.showCommitted(e.result);
        }
      }
      mark(f);
    },
  };
}

/**
 * The new-stack wizard.
 * @param {(href: string) => void} navigate
 * @returns {Promise<DrivenControl>}
 */
async function newStack(navigate) {
  const { closed, done } = base();
  void openNewStack(navigate);
  const first = await waitHandle("new-stack", 5000);
  first?.closed.then(done);
  const h = () => handle("new-stack");
  return {
    get dialog() {
      return h()?.dialog ?? null;
    },
    closed,
    close: () => h()?.close(),
    input: (_n, id) => byId(id),
    set: (_n, v, id) => {
      const el = byId(id);
      if (el) setInput(el, v);
    },
    button: (name) =>
      name === "close"
        ? closeOf(h()?.dialog)
        : name === "back"
          ? (h()?.back() ?? null)
          : (h()?.next() ?? null),
    row: () => null,
    sync: async (f) => {
      const w = h();
      if (!w) return;
      // One step at a time, each step's fields first: the wizard's own
      // checks and its reads (the data folders, the plan) run as they do
      // for a person.
      for (let guard = 0; guard < 8; guard += 1) {
        const at = w.step();
        const id = f.steps[at];
        if (id === "data")
          await waitFor(
            () =>
              f.fields
                .filter((x) => x.step === "data")
                .every((x) => byId(x.id)),
            3000,
          );
        fill(f, (x) => x.step === id);
        if (at === f.step_index) break;
        const moved = await w.goTo(at < f.step_index ? at + 1 : f.step_index);
        if (!moved) break;
      }
      if (f.step === "plan") {
        await waitFor(() => w.planReady(), 8000);
        fill(f, (x) => x.step === "plan");
      }
      w.runError(f.run_error);
      if (f.edit?.result) w.showCommitted(f.edit.result);
      mark(f);
    },
  };
}

/**
 * The host settings page with its key and review dialogs.
 * @returns {DrivenControl}
 */
function hostSettings() {
  const { closed, done } = base();
  const top = () =>
    handle("key")?.dialog ?? handle("host-review")?.dialog ?? null;
  /** @type {DriveForm | null} */
  let current = null;
  return {
    get dialog() {
      return top();
    },
    closed,
    close: () => {
      handle("key")?.close();
      handle("host-review")?.close();
      done();
    },
    input: (_n, id) => byId(id),
    set: (_n, v, id) => {
      const el = byId(id);
      if (el) setInput(el, v);
    },
    button: (name) => {
      const key = handle("key");
      if (key && ["save", "default", "cancel"].includes(name))
        return key[name]();
      if (name === "close") return closeOf(top());
      const review = handle("host-review");
      if (review) return name === "back" ? review.back() : review.save();
      return current?.step === "keys"
        ? (handle("host-settings")?.reviewButton() ?? null)
        : null;
    },
    row: (_op, target) =>
      handle("host-settings")?.rowButton(String(target ?? "")) ?? null,
    sync: async (f) => {
      current = f;
      const hs = await waitHandle("host-settings");
      if (!hs) return;
      await waitFor(() => hs.ready(), 8000);
      const e = /** @type {import("./driveview.js").DriveEdit} */ (
        f.edit ?? { family: "", guarded: 0 }
      );
      hs.setStaged(e.staged ?? {}, e.confirmed ?? []);
      const key = handle("key");
      if (e.sub && key?.key !== e.sub.target) {
        key?.close();
        hs.openKey(String(e.sub.target));
        await waitHandle("key", 2000);
      } else if (!e.sub && key) key.close();
      fill(f, (x) => x.step === "key");
      const review = handle("host-review");
      if (e.result) {
        review?.close();
        hs.showSaved(e.result);
      } else if (f.step === "review" && !review) hs.openReview();
      else if (f.step !== "review" && review) review.close();
      handle("host-review")?.runError(f.run_error);
      mark(f);
    },
  };
}

/**
 * The batch dialog.
 * @param {DriveForm} first
 * @returns {Promise<DrivenControl>}
 */
async function batch(first) {
  const { closed, done } = base();
  const action = first.id.replace(/^batch:/, "");
  void openBatch(action, first.stack.split(","));
  const b = await waitHandle("batch", 5000);
  b?.closed.then(done);
  return {
    dialog: b?.dialog ?? null,
    closed,
    close: () => handle("batch")?.close(),
    input: (_n, id) => byId(id),
    set: (_n, v, id) => {
      const el = byId(id);
      if (el) setInput(el, v);
    },
    button: (name) =>
      name === "close"
        ? closeOf(handle("batch")?.dialog)
        : (handle("batch")?.run() ?? null),
    row: () => null,
    sync: async (f) => {
      const x = handle("batch");
      if (!x) return;
      fill(f, () => true);
      mark(f);
      x.runError(f.run_error);
      if (f.edit?.result) x.showBatch(f.edit.result);
    },
  };
}

/**
 * The roll back dialog: a pick points at its row, and `next` is the row's
 * own button (the replay then opens the action's dialog, as that button
 * does).
 * @param {DriveForm} first
 * @returns {Promise<DrivenControl>}
 */
async function rollback(first) {
  const { closed, done } = base();
  void openRollback(first.stack);
  const r = await waitHandle("rollback", 5000);
  r?.closed.then(done);
  /** @type {Record<string, string>} */
  const chosen = { commit: "", unit: "" };
  const target = () =>
    chosen.commit
      ? (handle("rollback")?.rowButton("commit", chosen.commit) ?? null)
      : chosen.unit
        ? (handle("rollback")?.rowButton("unit", chosen.unit) ?? null)
        : null;
  return {
    dialog: r?.dialog ?? null,
    closed,
    close: () => handle("rollback")?.close(),
    input: () => target(),
    set: (name, v) => {
      chosen[name] = String(v);
      target()?.scrollIntoView({ block: "nearest" });
    },
    button: (name) =>
      name === "close" ? closeOf(handle("rollback")?.dialog) : target(),
    row: () => null,
    sync: async (f) => {
      await waitFor(
        () => handle("rollback")?.dialog.querySelector("table, .missing"),
        5000,
      );
      chosen.commit = String(f.values.commit ?? "");
      chosen.unit = String(f.values.unit ?? "");
      for (const b of handle("rollback")?.dialog.querySelectorAll(
        "button[data-commit], button[data-unit]",
      ) ?? [])
        b.closest("tr")?.classList.toggle("drive-focus", b === target());
    },
  };
}

/**
 * Open the edit form the state holds, the way its button opens it.
 * @param {DriveForm} f
 * @param {{navigate: (href: string) => void}} ctx
 * @returns {Promise<DrivenControl>}
 */
export async function openDriven(f, ctx) {
  switch (f.action) {
    case "new-stack":
      return newStack(ctx.navigate);
    case "host-settings":
      return hostSettings();
    case "batch":
      return batch(f);
    case "rollback":
      return rollback(f);
    default:
      return stackEdit(f);
  }
}
