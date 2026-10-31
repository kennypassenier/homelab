// Milestone edit's DOM pieces: a field drawn from its description
// (editforms.js), the plan (plan.js) as a readable block, and the plan
// dialog every edit ends in: a kp wizard with the plan first and the
// commit second, then the job it queued (feat-ops-6).

import { send } from "./act.js";
import {
  badge,
  notify,
  openDialog,
  refusalAlarm,
  refusalCallout,
} from "./actui.js";
import { commitFields, startValues } from "./editforms.js";
import { driven, register } from "./drivehooks.js";
import { h } from "./dom.js";
import { gridRow } from "./gridrow.js";
import { mountJobPanel } from "./jobpanel.js";
import { commitBody, committedText, planView } from "./plan.js";
import { attachDiffs, setDiff } from "/static/kp/dist/kp-themes.min.js";
import { clearError, showError } from "/static/kp/dist/kp-themes.min.js";
import {
  discardUnsaved,
  markUnsavedClean,
  setUnsavedSaving,
  unsaved,
} from "/static/kp/dist/kp-themes.min.js";
import {
  BEFORE_STEP_EVENT,
  FINISH_EVENT,
  attachWizards,
  wizard,
} from "/static/kp/dist/kp-themes.min.js";

/**
 * One field, drawn from its description; the input's id is the field's id,
 * so a replayed step (feat-platform-10) finds it.
 * @param {import("./editforms.js").EditField} f
 * @param {string | boolean} value
 * @param {(v: string | boolean) => void} onChange
 * @returns {{wrap: HTMLElement, input: HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement}}
 */
export function editField(f, value, onChange) {
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
    // The default is the saved state kp's unsaved bar compares against.
    input.defaultChecked = value === true;
    input.checked = value === true;
    input.addEventListener("change", () => onChange(input.checked));
    const wrap = h(
      "div",
      { class: "kp-field kp-field--check", "data-field": f.name },
      input,
      h("label", { class: "kp-field__label", for: f.id }, f.label),
      help,
    );
    return { wrap, input };
  }
  /** @type {HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement} */
  let input;
  if (f.kind === "choice") {
    const sel = h("select", {
      class: "kp-field__input",
      id: f.id,
      name: f.name,
      "aria-describedby": `${f.id}-hint`,
    });
    sel.append(
      ...(f.choices ?? []).map((c) => h("option", { value: c.value }, c.label)),
    );
    sel.value = String(value ?? "");
    for (const o of sel.options) o.defaultSelected = o.value === sel.value;
    input = sel;
  } else if (f.kind === "textarea") {
    const ta = h("textarea", {
      class: "kp-field__input",
      id: f.id,
      name: f.name,
      rows: "3",
      spellcheck: "false",
      "aria-describedby": `${f.id}-hint`,
    });
    ta.defaultValue = String(value ?? "");
    ta.value = String(value ?? "");
    input = ta;
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
    if (f.kind === "number") inp.inputMode = "numeric";
    if (f.placeholder) inp.placeholder = f.placeholder;
    // The attribute is checked with the browser's `v` flag, where a bare "/"
    // in a class is not allowed; the same pattern in JS (checkFields) is.
    if (f.pattern) inp.pattern = f.pattern.replace(/(?<!\\)\//g, "\\/");
    if (f.required) inp.required = true;
    inp.defaultValue = String(value ?? "");
    inp.value = String(value ?? "");
    input = inp;
  }
  // fix-182-dashboard-edits: a field the server fills in (a manual check's
  // generated `id`) is shown but never typed into — `readOnly` still
  // submits the field's value, `disabled` would not. `hidden` is for a
  // field that rides along (`replaces`) without being shown at all.
  if (f.readonly && "readOnly" in input) input.readOnly = true;
  if (f.hidden) {
    const wrap = h(
      "div",
      { class: "kp-field", "data-field": f.name, style: "display: none" },
      input,
    );
    return { wrap, input };
  }
  const read = () => {
    onChange(input.value);
    if (input.getAttribute("aria-invalid") === "true") clearError(input);
  };
  input.addEventListener("input", read);
  input.addEventListener("change", read);
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
 * One field as a row of a `.kp-formgrid` (Kenny, 2026-10-19: labels, fields
 * and words on shared tracks): the label, the control, and the help in the
 * value track. The control keeps the field's id (Live view and a replayed
 * step find it by it); `data-kp-row` and `data-kp-name` let kp's unsaved bar
 * track the row and name it in its list of changes. A required number with
 * bounds is kp's quantity (minus, plus, unit), a textarea grows with its
 * text (`data-kp-autosize`), and an error shows in the control's own slot.
 * @param {import("./editforms.js").EditField} f
 * @param {string | boolean} value
 * @param {(v: string | boolean) => void} onChange
 * @returns {{row: HTMLElement, input: HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement}}
 */
export function editRow(f, value, onChange) {
  const x = editField(f, value, onChange);
  if (f.hidden) return { row: x.wrap, input: x.input };
  const help = /** @type {HTMLElement | null} */ (
    x.wrap.querySelector(".kp-field__help")
  );
  help?.classList.remove("kp-field__help");
  const input = x.input;
  /** @type {HTMLElement} */
  let control = input;
  if (
    f.kind === "number" &&
    f.required &&
    f.min != null &&
    f.max != null &&
    input instanceof HTMLInputElement
  ) {
    input.classList.remove("kp-field__input");
    input.classList.add("kp-quantity__input");
    const q = h("div", {
      class: "kp-quantity",
      "data-kp-quantity": "",
      "data-kp-quantity-min": String(f.min),
      "data-kp-quantity-max": String(f.max),
      "data-kp-quantity-step": String(
        f.max > 1000 && f.min % 64 === 0 ? 64 : 1,
      ),
      "data-kp-quantity-unit": f.unit ?? "",
      "data-kp-quantity-label": f.label,
    });
    q.append(input);
    // The stepper writes the number and tells only its own event; the model
    // and the unsaved bar listen for an input.
    q.addEventListener("kp-quantity-change", () =>
      input.dispatchEvent(new Event("input", { bubbles: true })),
    );
    control = q;
  } else if (input instanceof HTMLTextAreaElement) {
    input.classList.add("kp-field__input--multiline");
    input.setAttribute("data-kp-autosize", "");
    input.setAttribute("data-kp-autosize-max", "12");
  } else if (f.unit && input instanceof HTMLInputElement) {
    input.classList.add("kp-field__input--short");
  }
  const slot = h("p", {
    class: "kp-field__error",
    "data-kp-field-error": "",
    hidden: "",
  });
  const field = h(
    "div",
    { class: f.kind === "check" ? "kp-field kp-field--check" : "kp-field" },
    control,
    slot,
  );
  const row = gridRow({
    label: f.label,
    labelFor: f.id,
    control: field,
    value: help ?? undefined,
  });
  row.dataset.kpRow = f.name;
  row.dataset.kpName = f.label;
  return { row, input };
}

/**
 * A titled group of rows for a `.kp-formgrid`: a heading and one sentence
 * saying what the group is, then its rows on the grid's tracks.
 * @param {{id?: string, title: string, desc: string, rows: HTMLElement[]}} g
 * @returns {HTMLElement}
 */
export function editGroup({ id, title, desc, rows }) {
  return h(
    "div",
    { class: "kp-formgrid__group edit-group", id: id ?? null },
    h("h3", { class: "kp-formgrid__heading" }, title),
    h(
      "p",
      { class: "measured edit-group__desc", "data-kp-formgrid-span": "all" },
      desc,
    ),
    ...rows,
  );
}

/**
 * A titled block that is not a form row (a table of rows with its Add
 * button): the same heading and sentence as `editGroup`, outside the grid;
 * its action (Add) stands on the block's end edge, in a grid cell of its own.
 * @param {{id?: string, title: string, desc: string, body: Node[], action?: HTMLElement}} g
 * @returns {HTMLElement}
 */
export function editBlock({ id, title, desc, body, action }) {
  return h(
    "section",
    { class: "edit-block", id: id ?? null, "aria-label": title },
    h(
      "div",
      { class: "edit-block__head" },
      h("h3", { class: "edit-block__title" }, title),
      h("p", { class: "measured edit-group__desc" }, desc),
      action ? h("div", { class: "edit-block__action" }, action) : "",
    ),
    ...body,
  );
}

/**
 * The card around an edit form: heading, one sentence, kp's unsaved bar in
 * its head (the bar's Save is the card's review; the old "Review and
 * commit" button and "Changed; not committed." words are the bar's now),
 * the grid of field groups, then the blocks.
 * @param {{id: string, form: string, title: string, desc: string,
 *   groups?: HTMLElement[], blocks?: HTMLElement[], bar?: boolean}} c
 * `bar: false` is a card that acts (adds something) instead of staging edits.
 * @returns {{node: HTMLElement, bar: HTMLElement}}
 */
export function editCard({
  id,
  form,
  title,
  desc,
  groups = [],
  blocks = [],
  bar: withBar = true,
}) {
  const bar = h("div", {
    class: "kp-unsaved edit-card__bar",
    "data-kp-unsaved": "",
    "data-kp-unsaved-for": `#${id}`,
    "data-kp-name": id,
    "data-kp-placement": "header",
    "data-kp-keep": "off",
    "data-kp-warn-leave": "off",
    "data-kp-undo-ms": "0",
    id: `${id}-bar`,
  });
  const node = h(
    "section",
    {
      class: "kp-card edit-card",
      "aria-label": title,
      id,
      "data-form": form,
    },
    h(
      "header",
      { class: "edit-card__head" },
      h("h2", { class: "edit-card__title" }, title),
      h("p", { class: "measured edit-card__desc" }, desc),
      ...(withBar ? [bar] : []),
    ),
    ...(groups.length
      ? [
          h(
            "div",
            {
              class: "kp-formgrid edit-card__grid",
              "data-kp-formgrid-columns": "never",
            },
            ...groups,
          ),
        ]
      : []),
    ...blocks,
  );
  return { node, bar };
}

/**
 * Wire a card's bar to its review. Save runs `review()`, which opens the plan
 * dialog and returns its handle (or null when it refused, having said why);
 * the bar rests again when that dialog closes without a commit, and a commit
 * reloads the tab, which draws the bars afresh.
 * @param {HTMLElement} bar
 * @param {() => ReturnType<typeof openPlanDialog> | null} review
 * @param {{revert?: (key: string) => boolean, discard?: () => void}} [hooks]
 * @returns {() => void}
 */
export function wireBar(bar, review, hooks = {}) {
  const onSave = () => {
    const d = review();
    if (!d) {
      queueMicrotask(() => setUnsavedSaving(bar, false));
      return;
    }
    d.closed.then(() => setUnsavedSaving(bar, false));
  };
  /** @param {Event} ev */
  const onRevert = (ev) => {
    const key = /** @type {CustomEvent} */ (ev).detail?.key;
    if (hooks.revert?.(String(key))) ev.preventDefault();
  };
  const onDiscard = () => hooks.discard?.();
  bar.addEventListener("kp-unsaved-save", onSave);
  bar.addEventListener("kp-unsaved-revert", onRevert);
  bar.addEventListener("kp-unsaved-discard", onDiscard);
  return () => {
    bar.removeEventListener("kp-unsaved-save", onSave);
    bar.removeEventListener("kp-unsaved-revert", onRevert);
    bar.removeEventListener("kp-unsaved-discard", onDiscard);
  };
}

/**
 * Mark a row of the page's own (a table of rows, a file) as changed or not,
 * for a bar that tracks by the page (the bar still tracks the fields).
 * @param {HTMLElement} row
 * @param {{was: string, now: string} | null} change null: not changed
 */
export function markRow(row, change) {
  if (!change) {
    row.removeAttribute("data-kp-changed");
    row.removeAttribute("data-kp-was");
    row.removeAttribute("data-kp-now");
    return;
  }
  row.dataset.kpWas = change.was;
  row.dataset.kpNow = change.now;
  row.setAttribute("data-kp-changed", "");
}

export { discardUnsaved, markUnsavedClean, unsaved };

/**
 * Show each error on its field; focus the first.
 * @param {Map<string, HTMLElement>} inputs
 * @param {Record<string, string>} errors
 * @returns {boolean} whether there were none
 */
export function markErrors(inputs, errors) {
  for (const [name, input] of inputs) {
    if (errors[name]) showError(input, errors[name]);
    else if (input.getAttribute("aria-invalid") === "true") clearError(input);
  }
  const first = Object.keys(errors).find((k) => inputs.has(k));
  if (first) inputs.get(first)?.focus();
  return Object.keys(errors).length === 0;
}

/**
 * The plan as a block: problems first, then what the machines would see,
 * what the host last applied, and each file's diff.
 * @param {import("./plan.js").Plan} plan
 */
export function planBlock(plan) {
  const v = planView(plan);
  const out = h("div", { class: "plan", "data-plan": plan.kind });
  if (v.syncNote)
    out.append(
      h(
        "div",
        { class: "kp-alert kp-alert--warning", role: "status" },
        v.syncNote,
      ),
    );
  if (v.blockedWhy)
    out.append(
      h(
        "div",
        {
          class: `kp-alert ${v.problems.length ? "kp-alert--destructive" : "kp-alert--info"}`,
          role: "alert",
        },
        h("strong", null, v.blockedWhy),
        ...(v.problems.length
          ? [h("ul", null, ...v.problems.map((p) => h("li", null, p)))]
          : []),
      ),
    );
  if (v.effects.length) {
    out.append(
      h("h3", null, "What homelab would change"),
      h(
        "ul",
        { class: "plan-effects" },
        ...v.effects.map((e) =>
          h(
            "li",
            { class: `plan-effect plan-effect--${e.tone}` },
            h("span", { class: "plan-effect__what" }, e.what),
            h("span", { class: "measured" }, ` · ${e.by}`),
            ...(e.detail.length
              ? [h("pre", { class: "plan-detail mono" }, e.detail.join("\n"))]
              : []),
          ),
        ),
      ),
    );
  }
  if (v.applied)
    out.append(
      h("p", { class: "measured plan-applied" }, v.applied),
      ...(v.appliedList.length
        ? [h("pre", { class: "plan-detail mono" }, v.appliedList.join("\n"))]
        : []),
    );
  if (v.files.length) out.append(h("h3", null, "The diff"));
  out.append(...diffBlocks(v.files));
  return out;
}

/**
 * The files' diffs as kp's diff component: a header per file (status, path,
 * +N −M, Copy path), unchanged lines folded, the changed words marked. Open
 * unless `opts.open` is false (the Update flow folds them, review item 7).
 * @param {ReturnType<typeof import("./plan.js").fileViews>} files
 * @param {{open: boolean}} [opts]
 * @returns {HTMLElement[]}
 */
export function diffBlocks(files, opts = { open: true }) {
  if (!files.length) return [];
  const root = h("div", {
    class: "kp-diffview",
    "data-kp-diffview": "",
    "data-kp-diff-open": opts.open ? "all" : "none",
    "data-kp-diff-label": "Changes to the files",
  });
  attachDiffs(root);
  setDiff(root, files);
  return [root];
}

/**
 * The plan dialog every edit ends in (feat-stacks-2): a kp wizard whose
 * first step reads and shows the plan, whose second asks the commit's
 * subject, a note and what follows, and whose finish commits. Then the
 * queued job's panel.
 * @param {{id: string, title: string, planUrl: string, planBody: unknown,
 *   commitUrl: string, commitBody: (values: import("./editforms.js").Values,
 *   plan: import("./plan.js").Plan) => unknown,
 *   onCommitted?: () => void}} o
 */
export function openPlanDialog(o) {
  const planBox = h(
    "div",
    { class: "plan-box", "aria-live": "polite" },
    h("p", { class: "measured" }, "Making the plan…"),
  );
  const commitBox = h("div", { class: "commit-box" });
  const runError = h("div");
  const next = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--primary",
      "data-kp-wizard-next": "",
      "data-kp-finish": "Commit",
      "data-kp-next": "Next",
      id: "edit-commit",
      disabled: "",
    },
    "Next",
  );
  const back = h(
    "button",
    { type: "button", class: "kp-button", "data-kp-wizard-back": "" },
    "Back",
  );
  const wiz = h(
    "div",
    {
      class: "kp-wizard act-wizard",
      "data-kp-wizard": "",
      "data-kp-validate": "false",
      "data-form": o.id,
    },
    h(
      "ol",
      { class: "kp-wizard__steps", "data-kp-wizard-steps": "" },
      h("li", { "data-kp-step-label": "", "data-step": "plan" }, "Plan"),
      h("li", { "data-kp-step-label": "", "data-step": "commit" }, "Commit"),
    ),
    h(
      "section",
      { "data-kp-step": "", "data-step": "plan", "aria-label": "Plan" },
      planBox,
    ),
    h(
      "section",
      {
        "data-kp-step": "",
        "data-step": "commit",
        "aria-label": "Commit",
        hidden: "",
      },
      commitBox,
      runError,
    ),
    h("div", { class: "kp-wizard__actions" }, back, next),
  );
  const body = h("div", { class: "act-body" }, wiz);
  const d = openDialog({
    title: o.title,
    body: [body],
    id: "plan-dialog",
    wide: true,
  });
  d.dialog.dataset.form = o.id;
  const detach = attachWizards(d.dialog, { focusStep: false });
  const handle = wizard(wiz);
  /** @type {() => void} */
  let stopPanel = () => {};
  /** @type {(v: boolean) => void} */
  let loaded = () => {};
  const ready = new Promise((r) => (loaded = r));
  // feat-platform-10: the Live view replay moves this very dialog.
  const unregister = register("plan", {
    dialog: d.dialog,
    ready,
    step: () => handle?.step() ?? 0,
    goTo: (/** @type {number} */ i) =>
      handle ? handle.goTo(i) : Promise.resolve(false),
    input: (/** @type {string} */ name) => inputs.get(name) ?? null,
    back: () => /** @type {HTMLElement} */ (back),
    next: () => /** @type {HTMLElement} */ (next),
    runError: (/** @type {import("./doctor.js").RouteError | null} */ e) =>
      runError.replaceChildren(...(e ? [refusalCallout(e)] : [])),
    showCommitted: (/** @type {any} */ r) => showCommitted(r),
    close: () => d.close(),
  });
  d.closed.then(() => {
    unregister();
    detach();
    stopPanel();
  });

  /** @type {import("./plan.js").Plan | null} */
  let plan = null;
  /** @type {import("./editforms.js").Values} */
  let values = {};
  /** @type {Map<string, HTMLElement>} */
  const inputs = new Map();

  void (async () => {
    const r = await send(
      "POST",
      o.planUrl,
      o.planBody,
      `the plan for ${o.title}`,
    );
    if (!r.ok) {
      planBox.replaceChildren(
        refusalCallout(r.error, "destructive", "No plan"),
      );
      loaded(false);
      return;
    }
    plan = /** @type {import("./plan.js").Plan} */ (r.body);
    planBox.replaceChildren(planBlock(plan));
    const fields = commitFields(plan.follow_ups, plan.subject);
    values = startValues({
      steps: [{ id: "commit", label: "Commit", fields }],
    });
    commitBox.replaceChildren(
      ...(plan.restarts_dashboard
        ? [
            h(
              "div",
              { class: "kp-alert kp-alert--warning", role: "status" },
              "Deploying admin restarts this dashboard: the page loses its link for a moment.",
            ),
          ]
        : []),
      ...fields.map((f) => {
        const x = editField(f, values[f.name], (v) => (values[f.name] = v));
        inputs.set(f.name, x.input);
        return x.wrap;
      }),
    );
    next.disabled = !plan.valid;
    if (!plan.valid) next.title = "Nothing can be committed; see the plan";
    loaded(true);
  })();

  wiz.addEventListener(BEFORE_STEP_EVENT, (e) => {
    const detail = /** @type {CustomEvent} */ (e).detail;
    if (detail.direction === "forward" && (!plan || !plan.valid))
      e.preventDefault();
  });

  let running = false;
  /** What the commit answered, in the dialog: the text, the job's panel.
   * @param {any} answer */
  const showCommitted = (answer) => {
    if (body.dataset.shownCommit) return;
    body.dataset.shownCommit = "1";
    const text = committedText(answer);
    notify(text, "success");
    o.onCommitted?.();
    const job = answer.follow?.job;
    const parts = /** @type {Node[]} */ ([
      h(
        "div",
        {
          class: "kp-alert kp-alert--success",
          role: "status",
          "data-committed": answer.committed.commit,
        },
        text,
      ),
    ]);
    if (answer.follow?.refused)
      parts.push(
        refusalCallout(answer.follow.refused, "warning", "Not queued"),
      );
    if (typeof job === "number") {
      const panel = mountJobPanel(job, { compact: true });
      stopPanel = panel.stop;
      parts.push(panel.element);
    }
    handle?.element.remove();
    const close = h(
      "button",
      { type: "button", class: "kp-button kp-button--primary" },
      "Close",
    );
    close.addEventListener("click", () => d.close());
    body.replaceChildren(
      ...parts,
      h("div", { class: "kp-dialog__actions" }, close),
    );
  };
  wiz.addEventListener(FINISH_EVENT, async () => {
    // Live view: the commit runs on the dashboard's server, once.
    if (running || !plan || driven()) return;
    if (!String(values.subject ?? "").trim()) {
      markErrors(inputs, { subject: "A commit needs a subject." });
      return;
    }
    running = true;
    next.disabled = true;
    runError.replaceChildren(
      h("p", { class: "measured" }, "Committing and pushing…"),
    );
    const r = await send(
      "POST",
      o.commitUrl,
      o.commitBody(values, plan),
      o.title,
    );
    running = false;
    next.disabled = false;
    if (!r.ok) {
      runError.replaceChildren(refusalCallout(r.error));
      await refusalAlarm(r.error, r.status);
      return;
    }
    showCommitted(r.body);
  });
  return d;
}

/**
 * The commit request of a stack edit.
 * @param {Record<string, unknown>} edit
 */
export const stackCommit =
  (edit) =>
  /** @param {import("./editforms.js").Values} values */
  (values) =>
    commitBody(edit, values);
