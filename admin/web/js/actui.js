// DOM pieces milestone act shares: the kp copy line (feat-stacks-7), a
// callout for a route's `{what, why, fix}`, the Alarm for a hard refusal,
// a toast, a form field drawn from its description (actionforms.js), and a
// kp dialog opened from code.

import { h } from "./dom.js";
import { showAlarm } from "/static/kp/js/alarm.js";
import { attachDialogs, openAtTop, toast } from "/static/kp/js/overlays.js";
import { attachPatterns } from "/static/kp/js/patterns.js";

let copyIds = 0;

/**
 * A command line with a kp copy button (feat-stacks-7).
 * @param {string} text
 * @param {string} [label] what the button copies, for a screen reader
 */
export function copyLine(text, label = "Copy the CLI command") {
  copyIds += 1;
  const id = `copy-${copyIds}`;
  const wrap = h(
    "span",
    { class: "kp-copyable cli-line" },
    h("code", { class: "kp-copyable__value mono", id }, text),
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--ghost kp-copyable__button",
        "data-kp-copy": id,
        "aria-label": label,
      },
      "Copy",
    ),
  );
  attachPatterns(wrap);
  return wrap;
}

/**
 * A kp callout for a refusal or an error (arch-errors).
 * @param {import("./doctor.js").RouteError} e
 * @param {"destructive" | "warning" | "info"} [tone]
 * @param {string} [lead] the first words, before `what`
 */
export function refusalCallout(e, tone = "destructive", lead = "Refused") {
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
    ),
  );
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
export const badge = (b) =>
  h("span", { class: `state ${b.tone}` }, h("span", null, b.label));

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
  title.id = `${spec.id ?? "act-dialog"}-title-${++copyIds}`;
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
