// feat-stacks-3: a new stack with the kp wizard. The steps come from
// newStackWizard() in editforms.js (preset, name and number, size, data,
// plan and commit), so milestone `follow` can replay them field by field.
// The files come from the repository's presets through the client's own
// scaffold on the server; the plan shows them before the commit, and the
// commit can be followed by the first deploy of exactly that commit.

import { send } from "./act.js";
import { notify, openDialog, refusalAlarm, refusalCallout } from "./actui.js";
import {
  PLAN,
  checkNewStep,
  commitFields,
  dataFields,
  newStackBody,
  newStackWizard,
  presetSize,
  startValues,
} from "./editforms.js";
import { editField, markErrors, planBlock } from "./editui.js";
import { fetchJson, h } from "./dom.js";
import { mountJobPanel } from "./jobpanel.js";
import { committedText } from "./plan.js";
import { stackHref } from "./router.js";
import {
  BEFORE_STEP_EVENT,
  FINISH_EVENT,
  STEP_EVENT,
  attachWizards,
  wizard,
} from "/static/kp/js/wizard.js";

/**
 * @param {(href: string) => void} [navigate]
 */
export async function openNewStack(navigate) {
  const r = await fetchJson("/data/presets", "the presets");
  if (!r.ok) {
    await refusalAlarm(r.error, 0);
    return;
  }
  /** @type {import("./editforms.js").Preset[]} */
  const presets = r.body.presets ?? [];
  if (!presets.length) {
    await refusalAlarm(
      {
        what: "a new stack",
        why: r.body.sync_error
          ? `the working copy is not there: ${r.body.sync_error}`
          : "the repository holds no presets",
        fix: "the settings page shows the working copy's state",
      },
      409,
    );
    return;
  }
  const taken = {
    names: /** @type {string[]} */ (r.body.taken?.names ?? []),
    vmids: /** @type {number[]} */ (r.body.taken?.vmids ?? []),
  };
  const w = newStackWizard(presets, r.body.suggest_vmid ?? null);
  const values = startValues(w);
  /** @type {Map<string, HTMLElement>} */
  const inputs = new Map();
  /** @param {import("./editforms.js").EditField} f */
  const field = (f) => {
    const x = editField(f, values[f.name], (v) => {
      values[f.name] = v;
      if (f.name === "preset") {
        // The size follows the preset until the person changes it.
        const size = presetSize(presets.find((p) => p.name === v));
        for (const [k, s] of Object.entries(size)) {
          values[k] = s;
          const i = inputs.get(k);
          if (i instanceof HTMLInputElement) i.value = String(s);
        }
      }
    });
    inputs.set(f.name, x.input);
    return x.wrap;
  };
  const presetNote = h("p", { class: "measured", id: "new-preset-note" });
  const paintPreset = () => {
    const p = presets.find((x) => x.name === values.preset);
    presetNote.textContent = p
      ? `${p.apps.length ? `Apps: ${p.apps.join(", ")}.` : "No apps: an empty stack to fill later."}${p.gpu ? " Passes the GPU in." : ""}${p.vpn ? " Gives the container a tunnel device." : ""}`
      : "";
  };
  inputs.get("preset")?.addEventListener("change", paintPreset);
  const dataBox = h("div", { class: "new-data" });
  const planBox = h("div", { class: "plan-box", "aria-live": "polite" });
  const commitBox = h("div", { class: "commit-box" });
  const runError = h("div");

  const sections = w.steps.map((s, i) => {
    const sec = h("section", {
      "data-kp-step": "",
      "data-step": s.id,
      "aria-label": s.label,
    });
    if (i > 0) sec.hidden = true;
    if (s.id === "preset") sec.append(...s.fields.map(field), presetNote);
    else if (s.id === "data") sec.append(dataBox);
    else if (s.id === PLAN) sec.append(planBox, commitBox, runError);
    else sec.append(...s.fields.map(field));
    return sec;
  });
  paintPreset();
  const next = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--primary",
      "data-kp-wizard-next": "",
      "data-kp-finish": "Commit",
      "data-kp-next": "Next",
      id: "new-next",
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
      "data-form": w.id,
    },
    h(
      "ol",
      { class: "kp-wizard__steps", "data-kp-wizard-steps": "" },
      ...w.steps.map((s) =>
        h("li", { "data-kp-step-label": "", "data-step": s.id }, s.label),
      ),
    ),
    ...sections,
    h("div", { class: "kp-wizard__actions" }, back, next),
  );
  const body = h("div", { class: "act-body" }, wiz);
  const d = openDialog({
    title: "New stack",
    description:
      "From one of the repository's presets; committed under stacks/<name>/ and pushed, then deployed if you choose.",
    body: [body],
    id: "new-stack-dialog",
    wide: true,
  });
  d.dialog.dataset.form = w.id;
  const detach = attachWizards(d.dialog, { focusStep: false });
  const handle = wizard(wiz);
  /** @type {() => void} */
  let stopPanel = () => {};
  d.closed.then(() => {
    detach();
    stopPanel();
  });

  /** @type {import("./plan.js").Plan | null} */
  let plan = null;
  /** @type {Record<string, string | boolean>} */
  let commitValues = {};
  /** @type {Map<string, HTMLElement>} */
  const commitInputs = new Map();

  const loadData = async () => {
    dataBox.replaceChildren(
      h("p", { class: "measured" }, "Reading the data folders…"),
    );
    const x = await send(
      "POST",
      "/data/stacks-new/appdata",
      {
        preset: values.preset,
        name: String(values.name).trim(),
        vmid: Number(values.vmid),
      },
      "the data folders",
    );
    const paths = x.ok ? /** @type {string[]} */ (x.body.appdata ?? []) : [];
    for (const k of Object.keys(values))
      if (k.startsWith("nodata:") && !paths.includes(k.slice(7)))
        delete values[k];
    const fields = dataFields(paths);
    dataBox.replaceChildren(
      ...(fields.length
        ? [
            h(
              "p",
              null,
              "Each folder is backed up every night. Tick one only when its app keeps nothing of its own there.",
            ),
            ...fields.map((f) => {
              values[f.name] ??= false;
              return field(f);
            }),
          ]
        : [
            h(
              "p",
              { class: "measured" },
              "This preset binds no /appdata folder: nothing to back up.",
            ),
          ]),
    );
  };

  const loadPlan = async () => {
    plan = null;
    next.disabled = true;
    planBox.replaceChildren(h("p", { class: "measured" }, "Making the plan…"));
    commitBox.replaceChildren();
    const x = await send(
      "POST",
      "/data/stacks-new/plan",
      newStackBody(values),
      "the new stack's plan",
    );
    if (!x.ok) {
      planBox.replaceChildren(
        refusalCallout(x.error, "destructive", "No plan"),
      );
      return;
    }
    plan = /** @type {import("./plan.js").Plan} */ (x.body);
    planBox.replaceChildren(planBlock(plan));
    const fields = commitFields(plan.follow_ups, plan.subject);
    commitValues = startValues({
      steps: [{ id: "commit", label: "Commit", fields }],
    });
    commitInputs.clear();
    commitBox.replaceChildren(
      ...fields.map((f) => {
        const y = editField(
          f,
          commitValues[f.name],
          (v) => (commitValues[f.name] = v),
        );
        commitInputs.set(f.name, y.input);
        return y.wrap;
      }),
    );
    next.disabled = !plan.valid;
  };

  wiz.addEventListener(BEFORE_STEP_EVENT, (e) => {
    const detail = /** @type {CustomEvent} */ (e).detail;
    if (detail.direction !== "forward") return;
    const from = w.steps[detail.from]?.id;
    if (!from) return;
    if (!markErrors(inputs, checkNewStep(w, from, values, taken)))
      e.preventDefault();
  });
  wiz.addEventListener(STEP_EVENT, (e) => {
    const detail = /** @type {CustomEvent} */ (e).detail;
    const at = w.steps[detail.step]?.id;
    if (at !== PLAN) next.disabled = false;
    if (at === "data") void loadData();
    if (at === PLAN) void loadPlan();
  });

  let running = false;
  wiz.addEventListener(FINISH_EVENT, async () => {
    if (running || !plan?.valid) return;
    if (!String(commitValues.subject ?? "").trim()) {
      markErrors(commitInputs, { subject: "A commit needs a subject." });
      return;
    }
    running = true;
    next.disabled = true;
    runError.replaceChildren(
      h("p", { class: "measured" }, "Committing and pushing…"),
    );
    const follow = String(commitValues.follow ?? "none");
    const x = await send(
      "POST",
      "/data/stacks-new/commit",
      {
        stack: newStackBody(values),
        subject: String(commitValues.subject ?? "").trim(),
        ...(String(commitValues.note ?? "").trim()
          ? { note: String(commitValues.note).trim() }
          : {}),
        ...(follow !== "none" ? { follow } : {}),
      },
      "the new stack",
    );
    running = false;
    next.disabled = false;
    if (!x.ok) {
      runError.replaceChildren(refusalCallout(x.error));
      await refusalAlarm(x.error, x.status);
      return;
    }
    const text = committedText(x.body);
    notify(text, "success");
    const name = String(values.name).trim();
    const parts = /** @type {Node[]} */ ([
      h("div", { class: "kp-alert kp-alert--success", role: "status" }, text),
    ]);
    if (x.body.follow?.refused)
      parts.push(
        refusalCallout(x.body.follow.refused, "warning", "Not queued"),
      );
    if (typeof x.body.follow?.job === "number") {
      const panel = mountJobPanel(x.body.follow.job, { compact: true });
      stopPanel = panel.stop;
      parts.push(panel.element);
    }
    const open = h(
      "a",
      { class: "kp-button", href: stackHref(name, "settings") },
      `Open ${name}`,
    );
    open.addEventListener("click", (ev) => {
      if (!navigate) return;
      ev.preventDefault();
      d.close();
      navigate(stackHref(name, "settings"));
    });
    const close = h(
      "button",
      { type: "button", class: "kp-button kp-button--primary" },
      "Close",
    );
    close.addEventListener("click", () => d.close());
    handle?.element.remove();
    body.replaceChildren(
      ...parts,
      h("div", { class: "kp-dialog__actions" }, open, close),
    );
  });
}
