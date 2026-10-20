// DOM pieces milestone act shares: the kp command block (feat-stacks-7), a
// callout for a route's `{what, why, fix}`, the Alarm for a hard refusal,
// a toast, a form field drawn from its description (actionforms.js), and a
// kp dialog opened from code.

import { h, stateBadge } from "./dom.js";
import { declare, drivable } from "./drivable.js";
import { showAlarm } from "/static/kp/js/alarm.js";
import { attachDialogs, openAtTop, toast } from "/static/kp/js/overlays.js";
import { attachCommands } from "/static/kp/js/command.js";
import { buildTypeConfirm } from "/static/kp/js/components.js";

let dialogIds = 0;

/**
 * A CLI command as kp's command block: the terminal it runs in (the
 * workstation's WSL shell), the line behind its prompt, and the package's own
 * Copy button that confirms.
 * @param {string} text
 * @param {{wrap?: "wrap" | "scroll"}} [opts] scroll: one line that scrolls
 *   sideways (a row of a list)
 */
export function cliCommand(text, opts = {}) {
  const el = h(
    "div",
    {
      class: "kp-command cli-line",
      "data-kp-command": "wsl",
      ...(opts.wrap === "scroll" ? { "data-kp-command-wrap": "scroll" } : {}),
    },
    h("pre", { class: "kp-command__code" }, h("code", null, text)),
  );
  attachCommands(el);
  return el;
}

/**
 * A command with its own lead-in label on the same row (Kenny, 2026-10-01:
 * "The same from a workstation:" used to sit on a row of its own above the
 * line it describes). Wraps only when the row itself runs out of width.
 * @param {string} label the lead-in text, e.g. "The same from a workstation:"
 * @param {string} text the command line to copy
 */
export function labeledCliCommand(label, text) {
  return h(
    "div",
    { class: "cli-row" },
    h("span", { class: "measured cli-row__label" }, label),
    cliCommand(text),
  );
}

/**
 * A kp callout for a refusal or an error (arch-errors).
 * @param {import("./doctor.js").RouteError} e
 * @param {"destructive" | "warning" | "info"} [tone]
 * @param {string} [lead] the first words, before `what`
 */
export function refusalCallout(e, tone = "destructive", lead = "Refused") {
  const fix = fixOf(e);
  return h(
    "div",
    {
      class: `kp-alert kp-alert--${tone} refusal`,
      role: tone === "destructive" ? "alert" : "status",
      "data-kp-semantic": "",
    },
    h(
      "span",
      { class: "kp-alert__body" },
      h("strong", null, `${lead}: ${e.what}`),
      h("span", { class: "refusal-line" }, `Why: ${e.why}`),
      ...(e.fix
        ? [h("span", { class: "refusal-line" }, `What to do: ${e.fix}`)]
        : []),
      ...(fix ? [fixButton(fix)] : []),
    ),
  );
}

// fix-stale-refresh-1: a refusal whose fix is a dashboard action offers
// that action's own dialog right there.
const REFUSAL_FIX = declare({
  id: "refusal-fix",
  page: "update",
  opens: "dialog",
  row: "<stack>",
  what: "the button under a refusal whose fix is an action (Deploy a stack that already names the newer version)",
  shows: "under a refusal that names an action",
});

/**
 * The action a refusal names: the host's `fix_action`, or, from a host that
 * does not send one, the words "deploy <stack> to run it".
 * @param {import("./doctor.js").RouteError} e
 * @returns {{action: string, stack: string} | null}
 */
export function fixOf(e) {
  if (e.fix_action) return e.fix_action;
  const m = /^deploy (\S+) to run it$/.exec(e.fix ?? "");
  return m ? { action: "deploy", stack: m[1] } : null;
}

/** @param {{action: string, stack: string}} f */
function fixButton(f) {
  const b = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--sm refusal-fix",
      "data-action": f.action,
      title: `Open the ${f.action} dialog for ${f.stack}; nothing runs until you confirm`,
    },
    `${capital(f.action)} ${f.stack}…`,
  );
  drivable(b, REFUSAL_FIX, f.stack);
  b.addEventListener("click", () => {
    // Imported on use: the action dialog imports this module.
    void import("./actiondialog.js").then((m) =>
      m.openAction(f.stack, f.action),
    );
  });
  return b;
}

/**
 * kp-themes' Alarm for a refusal that stops the run (a 400 or 409 on the
 * press itself).
 * @param {import("./doctor.js").RouteError} e
 * @param {number} status
 */
export function refusalAlarm(e, status) {
  return showAlarm({
    title: `Refused: ${e.what}`,
    detail: `${capital(e.why)}.${e.fix ? ` What to do: ${e.fix}.` : ""}`,
    code: `HTTP ${status}`,
    mode: "ack",
    action: "Understood",
  });
}

/** @param {string} s */
const capital = (s) => (s ? s[0].toUpperCase() + s.slice(1) : s);

/**
 * A kp toast with a tone.
 * @param {string} text
 * @param {"success" | "warning" | "info" | "error"} tone
 * @param {{label: string, onClick: () => void}} [action]
 */
export function notify(text, tone, action) {
  return toast(text, {
    className: `kp-toast kp-toast--${tone}`,
    live: tone === "error" ? "assertive" : "polite",
    ms: tone === "error" ? 10000 : 6000,
    action,
    max: 4,
  });
}

/**
 * A state badge like the tables use.
 * @param {{label: string, tone: string}} b
 */
export const badge = (b) => stateBadge(b);

/**
 * One form field, drawn from its description. The input's id is the
 * field's id, so a replayed step (feat-platform-10) finds it.
 * @param {import("./actionforms.js").Field} f
 * @param {string | boolean} value
 * @param {{value: string, label: string, disabled?: boolean}[]} [choices]
 * @returns {{wrap: HTMLElement, input: HTMLInputElement | HTMLSelectElement}}
 */
export function fieldEl(f, value, choices = []) {
  const help = h(
    "span",
    { class: "kp-field__help", id: `${f.id}-hint` },
    f.help,
  );
  if (f.kind === "check") {
    const input = h("input", {
      class: "kp-field__check",
      type: "checkbox",
      id: f.id,
      name: f.name,
      "aria-describedby": `${f.id}-hint`,
    });
    input.checked = value === true;
    const wrap = h(
      "div",
      {
        class: `kp-field kp-field--check${f.danger ? " field-danger" : ""}`,
        "data-field": f.name,
      },
      input,
      h("label", { class: "kp-field__label", for: f.id }, f.label),
      help,
    );
    return { wrap, input };
  }
  // A name typed to confirm is kp's own block: the rule, the field and the
  // match line in one footprint. Its input takes the field's id, so a Live
  // view step and the label find it as before (gap 123: no `id` option).
  if (f.kind === "typed" && f.expect) {
    const { block, input } = buildTypeConfirm(document, { name: f.expect });
    const label = block.querySelector("label");
    input.id = f.id;
    input.name = f.name;
    if (label) label.htmlFor = f.id;
    input.value = typeof value === "string" ? value : "";
    block.dataset.field = f.name;
    return { wrap: block, input };
  }
  /** @type {HTMLInputElement | HTMLSelectElement} */
  let input;
  if (f.kind === "choice") {
    const sel = h("select", {
      class: "kp-field__input",
      id: f.id,
      name: f.name,
      "aria-describedby": `${f.id}-hint`,
    });
    sel.append(
      ...choices.map((c) =>
        h(
          "option",
          { value: c.value, ...(c.disabled ? { disabled: "" } : {}) },
          c.label,
        ),
      ),
    );
    // A preset value the list does not hold (yet) still shows.
    if (
      typeof value === "string" &&
      value &&
      !choices.some((c) => c.value === value)
    )
      sel.append(h("option", { value }, value));
    sel.value = typeof value === "string" ? value : "";
    if (f.required) sel.required = true;
    input = sel;
  } else {
    const inp = h("input", {
      class: "kp-field__input",
      type: "text",
      id: f.id,
      name: f.name,
      autocomplete: "off",
      spellcheck: "false",
      "aria-describedby": `${f.id}-hint`,
    });
    if (f.placeholder) inp.placeholder = f.placeholder;
    if (f.pattern) inp.pattern = f.pattern;
    if (f.required) inp.required = true;
    if (f.kind === "typed" && f.expect && f.required)
      inp.pattern = f.expect.replace(/[.*+?^${}()|[\]\\-]/g, "\\$&");
    inp.value = typeof value === "string" ? value : "";
    input = inp;
  }
  const wrap = h(
    "div",
    { class: "kp-field", "data-field": f.name },
    h("label", { class: "kp-field__label", for: f.id }, f.label),
    input,
    help,
  );
  return { wrap, input };
}

/**
 * A kp dialog, opened from code, removed from the page when it closes.
 * @param {{title: string, description?: string, body: Node[],
 *   wide?: boolean, id?: string}} spec
 * @returns {{dialog: HTMLDialogElement, close: () => void,
 *   closed: Promise<void>}}
 */
export function openDialog(spec) {
  const closeBtn = h(
    "button",
    {
      type: "button",
      class: "kp-icon-button kp-dialog__close",
      "aria-label": "Close",
      "data-kp-dialog-close": "",
    },
    "×",
  );
  const dialog = h(
    "dialog",
    {
      class: `kp-dialog act-dialog${spec.wide ? " act-dialog--wide" : ""}`,
      "aria-labelledby": "",
      ...(spec.id ? { id: spec.id } : {}),
    },
    h("h2", { class: "kp-dialog__title" }, spec.title),
    ...(spec.description
      ? [h("p", { class: "kp-dialog__description" }, spec.description)]
      : []),
    closeBtn,
    h("div", { class: "kp-dialog__body" }, ...spec.body),
  );
  const title = /** @type {HTMLElement} */ (
    dialog.querySelector(".kp-dialog__title")
  );
  title.id = `${spec.id ?? "act-dialog"}-title-${++dialogIds}`;
  dialog.setAttribute("aria-labelledby", title.id);
  document.body.append(dialog);
  const detach = attachDialogs(dialog);
  /** @type {() => void} */
  let done = () => {};
  const closed = new Promise((resolve) => {
    done = () => resolve(undefined);
  });
  dialog.addEventListener("close", () => {
    detach();
    dialog.remove();
    done();
  });
  dialog.showModal();
  openAtTop(dialog);
  return { dialog, close: () => dialog.close(), closed };
}
