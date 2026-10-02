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
import { mountJobPanel } from "./jobpanel.js";
import { commitBody, committedText, planView } from "./plan.js";
import { clearError, showError } from "/static/kp/js/forms.js";
import {
  BEFORE_STEP_EVENT,
  FINISH_EVENT,
  attachWizards,
  wizard,
} from "/static/kp/js/wizard.js";

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
    if (f.pattern) inp.pattern = f.pattern;
    if (f.required) inp.required = true;
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
 * The files' diffs, one open `details` each.
 * @param {ReturnType<typeof import("./plan.js").fileViews>} files
 * @returns {HTMLElement[]}
 */
export function diffBlocks(files) {
  /** @type {HTMLElement[]} */
  const out = [];
  for (const f of files) {
    // kp-themes' diff component [TH54]: a `<pre class="kp-diff">` of
    // `.kp-diff__line[data-kind]` lines, each a number, a sign (its own
    // column, so the change survives without colour) and the text. kp has
    // no hunk-header element, so `@@ … @@` stays a small local style.
    const table = h("pre", {
      class: "kp-diff",
      "aria-label": `Changes in ${f.path}`,
    });
    for (const hk of f.hunks) {
      table.append(h("div", { class: "diff-hunk kp-text-muted" }, hk.head));
      for (const l of hk.lines)
        table.append(
          h(
            "span",
            { class: "kp-diff__line", "data-kind": l.kind },
            h(
              "span",
              { class: "kp-diff__number" },
              l.no == null ? "" : String(l.no),
            ),
            h("span", { class: "kp-diff__sign" }, l.mark),
            h("span", null, l.text),
          ),
        );
    }
    out.push(
      h(
        "details",
        { class: "plan-file", open: "" },
        h(
          "summary",
          null,
          badge(f.badge),
          " ",
          h("span", { class: "mono" }, f.title),
        ),
        table,
      ),
    );
  }
  return out;
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
