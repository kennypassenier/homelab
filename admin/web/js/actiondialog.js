// feat-stacks-4, feat-stacks-5, feat-stacks-7: the action dialog. A kp
// dialog holding a kp wizard, drawn step by step from the action's
// description in actionforms.js: the options, then the review with the
// exact CLI line (copyable), the deploy guard's refusal with its Force
// choice, "restarts the dashboard", and the typed name where the action
// asks for it. The press starts the job; the dialog then follows it
// (feat-ops-6).

import { act, catalogReady, onAct, send } from "./act.js";
import {
  REVIEW,
  actionForm,
  batchBody,
  batchConfirmErrors,
  batchConfirmText,
  batchForm,
  buildArgs,
  checkValues,
  fieldChoices,
  formFields,
  initialValues,
  previewArgs,
} from "./actionforms.js";
import {
  badge,
  copyLine,
  fieldEl,
  notify,
  openDialog,
  refusalAlarm,
  refusalCallout,
} from "./actui.js";
import { fetchJson, h } from "./dom.js";
import { driven, register } from "./drivehooks.js";
import { stackDetail } from "./fleet.js";
import { batchView, jobBadge } from "./jobs.js";
import { mountJobPanel } from "./jobpanel.js";
import { current } from "./store.js";
import { clearError, showError } from "/static/kp/js/forms.js";
import {
  BEFORE_STEP_EVENT,
  FINISH_EVENT,
  STEP_EVENT,
  attachWizards,
  wizard,
} from "/static/kp/js/wizard.js";

/**
 * @typedef {{apps?: string[], units?: string[],
 *   commits?: {commit: string, subject: string}[]}} Sources
 * @typedef {{preset?: import("./actionforms.js").Values, sources?: Sources,
 *   openRollback?: (stack: string) => void, driven?: boolean}} OpenOpts
 * @typedef {{form: import("./actionforms.js").ActionForm,
 *   dialog: HTMLDialogElement, closed: Promise<void>, close: () => void,
 *   input: (name: string, id?: string) => HTMLInputElement | HTMLSelectElement | null,
 *   set: (name: string, value: string | boolean, id?: string) => void,
 *   step: () => number, goTo: (index: number) => Promise<boolean>,
 *   button: (name: string) => HTMLElement | null,
 *   errors: (byName: Record<string, string>) => void,
 *   runError: (e: import("./doctor.js").RouteError | null) => void,
 *   showJob: (job: number) => void}} ActionController
 *   feat-platform-10: what the driven replay (drive.js) does to a dialog.
 */

/**
 * The lists the form's choices come from: the fleet's apps, and the
 * stack's commits and native units when a field asks for them.
 * @param {import("./actionforms.js").ActionForm} form
 * @param {Sources} given
 * @returns {Promise<Sources>}
 */
async function readSources(form, given) {
  const out = { ...given };
  const fields = formFields(form);
  if (!out.apps && fields.some((f) => f.source === "apps"))
    out.apps =
      stackDetail(current().fleet, form.stack)?.apps.map((a) => a.name) ?? [];
  const wantsRepo = fields.some(
    (f) =>
      (f.source === "units" && !out.units) ||
      (f.source === "commits" && !out.commits),
  );
  if (wantsRepo) {
    const r = await fetchJson(
      `/data/actions/${encodeURIComponent(form.stack)}/rollback-options`,
      "the roll-back options",
    );
    if (r.ok) {
      out.units ??= r.body.native_units ?? [];
      out.commits ??= r.body.commits ?? [];
    } else {
      out.units ??= [];
      out.commits ??= [];
    }
  }
  return out;
}

/**
 * Open one action's dialog on one stack (or on the host target).
 * `driven` (feat-platform-10): Claude drives it from the dashboard's
 * server; the dialog shows each step and never sends the press itself.
 * @param {string} stack
 * @param {string} action
 * @param {OpenOpts} [opts]
 * @returns {Promise<ActionController | null>}
 */
export async function openAction(stack, action, opts = {}) {
  const catalog = await catalogReady();
  const entry = catalog?.actions.find((a) => a.action === action);
  if (!catalog || !entry) {
    notify(`The dashboard does not know the action "${action}".`, "error");
    return null;
  }
  // Deploying an earlier commit starts from the list of commits.
  if (action === "deploy-commit" && !opts.preset?.commit && opts.openRollback) {
    opts.openRollback(stack);
    return null;
  }
  const form = actionForm(entry, {
    stack,
    selfStack: catalog.self_stack,
    hostTarget: catalog.host_target,
  });
  if (form.refused) {
    await refusalAlarm({ what: form.title, why: form.refused, fix: "" }, 403);
    return null;
  }
  const sources = await readSources(form, opts.sources ?? {});
  const values = initialValues(form, opts.preset);
  return drawActionDialog(form, values, sources, opts.driven === true);
}

/**
 * The dialog for one form.
 * @param {import("./actionforms.js").ActionForm} form
 * @param {import("./actionforms.js").Values} values
 * @param {Sources} sources
 * @param {boolean} driven
 * @returns {ActionController}
 */
function drawActionDialog(form, values, sources, driven) {
  /** @type {Map<string, HTMLInputElement | HTMLSelectElement>} */
  const inputs = new Map();
  /** @type {Map<string, HTMLElement>} */
  const wraps = new Map();
  /** @param {import("./actionforms.js").Field} f */
  const field = (f) => {
    const x = fieldEl(
      f,
      values[f.name],
      f.kind === "choice" ? fieldChoices(f, sources) : [],
    );
    inputs.set(f.name, x.input);
    wraps.set(f.name, x.wrap);
    const read = () => {
      values[f.name] =
        x.input instanceof HTMLInputElement && x.input.type === "checkbox"
          ? x.input.checked
          : x.input.value;
      if (x.input.getAttribute("aria-invalid") === "true") clearError(x.input);
    };
    x.input.addEventListener("input", read);
    x.input.addEventListener("change", read);
    return x.wrap;
  };

  // The review step's parts: the preview, the guard, the typed name.
  const previewBox = h("div", { class: "act-preview", "aria-live": "polite" });
  const guardBox = h("div", { class: "act-guard" });
  const restartsBox = h(
    "div",
    {
      class: "kp-alert kp-alert--warning act-restarts",
      role: "status",
      hidden: "",
      "data-kp-semantic": "",
    },
    h(
      "span",
      { class: "kp-alert__body" },
      h("span", { class: "kp-alert__label" }, "Restarts the dashboard: "),
      "this page loses its link for a moment; the job's end is read back after the restart.",
    ),
  );
  const runError = h("div", { class: "act-run-error" });
  const reviewStep = /** @type {import("./actionforms.js").Step} */ (
    form.steps.find((s) => s.id === REVIEW)
  );
  const forceField = reviewStep.fields.find((f) => f.when === "guard");
  const otherReview = reviewStep.fields.filter((f) => f.when !== "guard");
  const forceWrap = forceField ? field(forceField) : null;
  if (forceWrap) forceWrap.hidden = true;

  const labels = form.steps.map((s) =>
    h("li", { "data-kp-step-label": "", "data-step": s.id }, s.label),
  );
  const sections = form.steps.map((s, i) => {
    const sec = h("section", {
      "data-kp-step": "",
      "data-step": s.id,
      "aria-label": s.label,
    });
    if (i > 0) sec.hidden = true;
    if (s.id === REVIEW) {
      sec.append(
        h("p", { class: "act-what" }, capital(form.what), "."),
        previewBox,
        guardBox,
        ...(forceWrap ? [forceWrap] : []),
        restartsBox,
        ...otherReview.map(field),
        runError,
      );
    } else sec.append(...s.fields.map(field));
    return sec;
  });
  const back = h(
    "button",
    { type: "button", class: "kp-button", "data-kp-wizard-back": "" },
    "Back",
  );
  const next = h(
    "button",
    {
      type: "button",
      class: `kp-button ${form.destructive ? "kp-button--destructive" : "kp-button--primary"}`,
      "data-kp-wizard-next": "",
      "data-kp-finish": form.submit,
      "data-kp-next": "Next",
      id: "act-run",
    },
    "Next",
  );
  const wiz = h(
    "div",
    {
      class: "kp-wizard act-wizard",
      "data-kp-wizard": "",
      "data-kp-validate": "false",
      "data-form": form.id,
    },
    ...(form.steps.length > 1
      ? [
          h(
            "ol",
            { class: "kp-wizard__steps", "data-kp-wizard-steps": "" },
            ...labels,
          ),
        ]
      : []),
    ...sections,
    h(
      "div",
      { class: "kp-wizard__actions" },
      ...(form.steps.length > 1 ? [back] : []),
      next,
    ),
  );
  const body = h("div", { class: "act-body" }, wiz);
  const d = openDialog({
    title: form.title,
    body: [body],
    id: "action-dialog",
  });
  d.dialog.dataset.form = form.id;
  if (driven) d.dialog.dataset.driven = "";
  const detachWizard = attachWizards(d.dialog, { focusStep: false });
  const handle = wizard(wiz);
  /** @type {() => void} */
  let stopPanel = () => {};
  d.closed.then(() => {
    detachWizard();
    stopPanel();
  });

  /** @type {import("./doctor.js").RouteError | null} */
  let guard = null;
  let previewSeq = 0;
  const preview = async () => {
    const seq = ++previewSeq;
    previewBox.replaceChildren(
      h("p", { class: "measured" }, "Reading what this would do…"),
    );
    const r = await send(
      "POST",
      form.previewPath,
      previewArgs(form, values),
      `the preview of ${form.title}`,
    );
    if (seq !== previewSeq) return;
    if (!r.ok) {
      guard = null;
      previewBox.replaceChildren(
        refusalCallout(r.error, "warning", "No preview"),
      );
      return;
    }
    const p = r.body;
    previewBox.replaceChildren(
      p.cli
        ? h(
            "div",
            { class: "act-cli" },
            h("p", { class: "measured" }, "The same from a workstation:"),
            copyLine(p.cli),
          )
        : h(
            "p",
            { class: "measured act-cli-none" },
            `No CLI line: ${p.cli_unavailable ?? "the dashboard could not build it"}.`,
          ),
    );
    restartsBox.hidden = !p.restarts_dashboard;
    guard = p.guard ?? null;
    guardBox.replaceChildren(
      ...(guard
        ? [refusalCallout(guard, "warning", "The deploy guard refuses")]
        : []),
    );
    // Force is only offered while the guard refuses; once ticked, the
    // preview asks again with force and the guard stands aside.
    if (forceWrap) forceWrap.hidden = !guard && values.force !== true;
  };
  inputs.get("force")?.addEventListener("change", () => void preview());

  /** @param {"options" | "review"} step */
  const stepErrors = (step) => {
    const errors = checkValues(form, values, step);
    for (const [name, input] of inputs) {
      if (errors[name]) showError(input, errors[name]);
      else if (input.getAttribute("aria-invalid") === "true") clearError(input);
    }
    const first = Object.keys(errors)[0];
    if (first) inputs.get(first)?.focus();
    return !first;
  };

  wiz.addEventListener(BEFORE_STEP_EVENT, (e) => {
    // Driven: the dashboard's server checked the step already.
    if (driven) return;
    const detail = /** @type {CustomEvent} */ (e).detail;
    const from = form.steps[detail.from]?.id;
    if (detail.direction === "forward" && from && !stepErrors(from))
      e.preventDefault();
  });
  wiz.addEventListener(STEP_EVENT, (e) => {
    const detail = /** @type {CustomEvent} */ (e).detail;
    if (form.steps[detail.step]?.id === REVIEW) void preview();
  });
  if (form.steps.length === 1) void preview();

  let running = false;
  /** @param {number} job */
  const showJob = (job) => {
    if (body.dataset.job) return;
    const panel = mountJobPanel(job, { compact: true });
    stopPanel = panel.stop;
    handle?.element.remove();
    body.replaceChildren(
      panel.element,
      h(
        "div",
        { class: "kp-dialog__actions" },
        h(
          "a",
          {
            class: "kp-button",
            href: `/app/jobs?job=${job}`,
            "data-close-on-nav": "",
          },
          "Open in Jobs",
        ),
        h(
          "button",
          {
            type: "button",
            class: "kp-button kp-button--primary",
            "data-kp-dialog-close": "",
          },
          "Close",
        ),
      ),
    );
    body.dataset.job = String(job);
    body
      .querySelector("[data-close-on-nav]")
      ?.addEventListener("click", () => d.close());
    body
      .querySelector("[data-kp-dialog-close]")
      ?.addEventListener("click", () => d.close());
  };
  wiz.addEventListener(FINISH_EVENT, async () => {
    // Driven: the press runs on the dashboard's server, once; this tab
    // only shows the job it started (drive.js calls showJob).
    if (driven || running) return;
    if (!stepErrors(REVIEW)) return;
    if (guard && values.force !== true) {
      runError.replaceChildren(
        refusalCallout(
          {
            what: form.title,
            why: "the deploy guard refuses this deploy",
            fix: "pull the working copy first, or tick Force if undoing the host's last deploy is the point",
          },
          "destructive",
        ),
      );
      inputs.get("force")?.focus();
      return;
    }
    running = true;
    next.disabled = true;
    runError.replaceChildren(h("p", { class: "measured" }, "Sending…"));
    const r = await send(
      "POST",
      form.runPath,
      buildArgs(form, values),
      form.title,
    );
    running = false;
    next.disabled = false;
    if (!r.ok) {
      runError.replaceChildren(refusalCallout(r.error));
      await refusalAlarm(r.error, r.status);
      return;
    }
    showJob(/** @type {number} */ (r.body.job));
  });

  return {
    form,
    dialog: d.dialog,
    closed: d.closed,
    close: () => d.close(),
    input: (name) => inputs.get(name) ?? null,
    set: (name, value) => {
      const x = inputs.get(name);
      if (!x) return;
      const check = x instanceof HTMLInputElement && x.type === "checkbox";
      if (check) x.checked = value === true;
      else x.value = String(value);
      x.dispatchEvent(
        new Event(
          check || x instanceof HTMLSelectElement ? "change" : "input",
          {
            bubbles: true,
          },
        ),
      );
    },
    step: () => handle?.step() ?? 0,
    goTo: (i) => (handle ? handle.goTo(i) : Promise.resolve(false)),
    button: (name) =>
      name === "back"
        ? back
        : name === "close"
          ? /** @type {HTMLElement | null} */ (
              d.dialog.querySelector(".kp-dialog__close")
            )
          : next,
    errors: (byName) => {
      for (const [name, input] of inputs) {
        if (byName[name]) showError(input, byName[name]);
        else if (input.getAttribute("aria-invalid") === "true")
          clearError(input);
      }
    },
    runError: (e) =>
      runError.replaceChildren(
        ...(e ? [refusalCallout(e, "destructive")] : []),
      ),
    showJob,
  };
}

/** @param {string} s */
const capital = (s) => (s ? s[0].toUpperCase() + s.slice(1) : s);

/**
 * feat-stacks-5: one action on several stacks. The review lists every
 * stack with its CLI line and its guard, asks each typed name the action
 * needs, and follows the batch once it runs.
 * @param {string} action
 * @param {string[]} stacks
 */
export async function openBatch(action, stacks) {
  const catalog = await catalogReady();
  const entry = catalog?.actions.find((a) => a.action === action);
  if (!catalog || !entry || stacks.length === 0) return;
  const form = batchForm(entry, stacks, catalog.self_stack);
  if (form.refused) {
    await refusalAlarm({ what: form.title, why: form.refused, fix: "" }, 403);
    return;
  }
  /** @type {import("./actionforms.js").Values} */
  const values = {};
  /** @type {Map<string, HTMLInputElement | HTMLSelectElement>} */
  const inputs = new Map();
  const sharedFields = form.shared.map((f) => {
    const x = fieldEl(f, f.kind === "check" ? false : "");
    values[f.name] = f.kind === "check" ? false : "";
    inputs.set(f.name, x.input);
    x.input.addEventListener("change", () => {
      values[f.name] =
        x.input instanceof HTMLInputElement && x.input.type === "checkbox"
          ? x.input.checked
          : x.input.value;
    });
    return x.wrap;
  });
  const confirmFields = form.confirms.map((f) => {
    const x = fieldEl(f, "");
    const key = `confirm:${f.stack}`;
    values[key] = "";
    inputs.set(key, x.input);
    x.input.addEventListener("input", () => {
      values[key] = x.input.value;
      if (x.input.getAttribute("aria-invalid") === "true") clearError(x.input);
    });
    return x.wrap;
  });
  const list = h("ul", { class: "batch-preview" });
  for (const s of stacks)
    list.append(
      h(
        "li",
        { "data-stack": s },
        h("strong", null, s),
        h("span", { class: "measured" }, " reading…"),
      ),
    );
  const runError = h("div");
  const run = h(
    "button",
    {
      type: "button",
      class: `kp-button ${entry.scope === "all" ? "kp-button--destructive" : "kp-button--primary"}`,
      id: "batch-run",
    },
    form.submit,
  );
  const cancel = h(
    "button",
    { type: "button", class: "kp-button", "data-kp-dialog-close": "" },
    "Cancel",
  );
  const body = h(
    "div",
    { class: "act-body", "data-form": form.id },
    h("p", { class: "act-what" }, capital(form.what), "."),
    ...(form.restartsDashboard
      ? [
          h(
            "div",
            { class: "kp-alert kp-alert--warning", role: "status" },
            `Restarts the dashboard: ${catalog.self_stack} is among the stacks.`,
          ),
        ]
      : []),
    h(
      "p",
      { class: "measured" },
      "Checked whole first, then run one by one in this order:",
    ),
    list,
    ...sharedFields,
    ...confirmFields,
    runError,
    h("div", { class: "kp-dialog__actions" }, cancel, run),
  );
  const d = openDialog({
    title: form.title,
    body: [body],
    id: "batch-dialog",
    wide: true,
  });
  d.dialog.dataset.form = form.id;
  /** @type {() => void} */
  let stop = () => {};
  /** The batch runs: its progress in the dialog's place.
   * @param {{batch: number, jobs?: {job: number, stack: string}[]}} answer */
  const showBatch = (answer) => {
    if (body.dataset.batch) return;
    body.dataset.batch = String(answer.batch);
    const panel = mountBatchPanel(answer.batch, answer.jobs ?? []);
    stop = panel.stop;
    body.replaceChildren(
      panel.element,
      h(
        "div",
        { class: "kp-dialog__actions" },
        h(
          "button",
          {
            type: "button",
            class: "kp-button kp-button--primary",
            "data-kp-dialog-close": "",
          },
          "Close",
        ),
      ),
    );
    body
      .querySelector("[data-kp-dialog-close]")
      ?.addEventListener("click", () => d.close());
  };
  // feat-platform-10: the Live view replay works this very dialog.
  const unregister = register("batch", {
    dialog: d.dialog,
    closed: d.closed,
    run: () => run,
    runError: (/** @type {import("./doctor.js").RouteError | null} */ e) =>
      runError.replaceChildren(...(e ? [refusalCallout(e)] : [])),
    showBatch,
    close: () => d.close(),
  });
  d.closed.then(() => {
    unregister();
    stop();
  });

  // Each stack's own preview: its CLI line and whether its guard refuses.
  let guarded = 0;
  await Promise.all(
    stacks.map(async (s) => {
      const r = await send(
        "POST",
        `/data/actions/${encodeURIComponent(s)}/${action}/preview`,
        entry.confirm ? { confirm: s } : {},
        `the preview of ${entry.label} on ${s}`,
      );
      const li = list.querySelector(`[data-stack="${CSS.escape(s)}"]`);
      if (!li) return;
      const parts = /** @type {Node[]} */ ([h("strong", null, s)]);
      if (!r.ok)
        parts.push(
          h("span", { class: "measured" }, ` no preview: ${r.error.why}`),
        );
      else {
        if (r.body.cli)
          parts.push(copyLine(r.body.cli, `Copy the CLI command for ${s}`));
        else
          parts.push(
            h(
              "span",
              { class: "measured" },
              ` no CLI line: ${r.body.cli_unavailable ?? "—"}`,
            ),
          );
        if (r.body.guard) {
          guarded += 1;
          parts.push(
            refusalCallout(r.body.guard, "warning", "The deploy guard refuses"),
          );
        }
      }
      li.replaceChildren(...parts);
    }),
  );
  const force = inputs.get("force");
  if (force)
    /** @type {HTMLElement} */ (force.closest(".kp-field")).hidden =
      guarded === 0;

  run.addEventListener("click", async () => {
    // Live view: the batch runs on the dashboard's server, once.
    if (driven()) return;
    const wrong = batchConfirmErrors(form, values);
    for (const c of form.confirms) {
      const input = inputs.get(`confirm:${c.stack}`);
      if (!input) continue;
      if (wrong.includes(c.stack)) showError(input, batchConfirmText(c.stack));
      else clearError(input);
    }
    if (wrong.length) {
      inputs.get(`confirm:${wrong[0]}`)?.focus();
      return;
    }
    run.disabled = true;
    const r = await send(
      "POST",
      "/data/actions/batch",
      batchBody(form, values),
      form.title,
    );
    run.disabled = false;
    if (!r.ok) {
      runError.replaceChildren(refusalCallout(r.error));
      await refusalAlarm(r.error, r.status);
      return;
    }
    showBatch(r.body);
  });
}

/**
 * A batch's progress from `action_batch`, and the running job's own panel.
 * @param {number} batch
 * @param {{job: number, stack: string}[]} jobs
 */
export function mountBatchPanel(batch, jobs) {
  const bar = h("progress", {
    class: "kp-progress",
    max: "100",
    value: "0",
    "aria-label": "Batch progress",
  });
  const text = h("span", { class: "kp-progress__value" });
  const rows = h("ul", { class: "batch-jobs" });
  const running = h("div", { class: "batch-running" });
  /** @type {{job: number, stop: () => void} | null} */
  let panel = null;
  const element = h(
    "section",
    {
      class: "batch-panel",
      "aria-label": `Batch ${batch}`,
      "data-batch": String(batch),
    },
    h(
      "div",
      { class: "kp-progress-group" },
      h(
        "div",
        { class: "kp-progress__wrap" },
        h("span", { class: "kp-progress__label" }, `Batch ${batch}`),
        bar,
        text,
      ),
    ),
    rows,
    running,
  );
  const paint = () => {
    const b = act.batches.get(batch) ?? {
      batch,
      jobs: jobs.map((j) => ({
        job: j.job,
        stack: j.stack,
        state: act.jobs.find((x) => x.job === j.job)?.state ?? "queued",
        message: null,
      })),
      done: 0,
      ok: 0,
      failed: 0,
      deferred: 0,
    };
    const v = batchView(b);
    bar.value = v.percent;
    text.textContent = v.text;
    rows.replaceChildren(
      ...v.rows.map((r) => {
        const live = act.jobs.find((x) => x.job === r.job);
        const b2 = live ? jobBadge(live.state) : r.badge;
        return h(
          "li",
          { "data-job": String(r.job) },
          badge(b2),
          ` ${r.stack} `,
          h("a", { href: `/app/jobs?job=${r.job}` }, `job ${r.job}`),
          ...(r.message
            ? [h("span", { class: "measured" }, ` · ${r.message}`)]
            : []),
        );
      }),
    );
    // The job running now gets the full panel.
    const now = v.rows
      .map((r) => act.jobs.find((x) => x.job === r.job))
      .find((j) => j && (j.state === "running" || j.state === "queued"));
    if (now && panel?.job !== now.job) {
      panel?.stop();
      const p = mountJobPanel(now.job);
      panel = { job: now.job, stop: p.stop };
      running.replaceChildren(p.element);
    }
  };
  const offB = onBatch(paint);
  paint();
  return {
    element,
    stop: () => {
      offB();
      panel?.stop();
    },
  };
}

/** @param {() => void} f */
function onBatch(f) {
  const a = onAct("batch", f);
  const b = onAct("jobs", f);
  return () => {
    a();
    b();
  };
}
