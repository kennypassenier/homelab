// feat-stacks-3, moved to a page by ux-stacknew-2 (Kenny, 2026-11-01: "the
// wizard steps themselves run on a full page, like Restore"): a new stack
// with the kp wizard. The steps come from newStackWizard() in editforms.js
// (preset, name and number, size, data, review), so milestone `follow` can
// replay them field by field. The files come from the repository's presets
// through the client's own scaffold on the server; the review (newreview.js,
// shared with the bundle route) shows them before the commit, and the commit
// can be followed by the first deploy of exactly that commit.

import { send } from "./act.js";
import { declare } from "./drivable.js";
import {
  PLAN,
  checkNewStep,
  dataFields,
  newStackBody,
  newStackWizard,
  presetSize,
  startValues,
} from "./editforms.js";
import { editField, markErrors } from "./editui.js";
import { register } from "./drivehooks.js";
import { fetchJson, h } from "./dom.js";
import { reviewPart } from "./newreview.js";
import { stage } from "./stage.js";
import { wizardShell } from "./newwizard.js";
import { failBox } from "./ui.js";
import {
  STEP_EVENT,
  BEFORE_STEP_EVENT,
} from "/static/kp/dist/kp-themes.min.js";

const TRY_AGAIN = declare({
  id: "new-stack-try-again",
  page: "newstack",
  opens: "view",
  what: "read the presets again",
  shows: "after the presets could not be read",
});

/** The page of one kind of new stack. @param {"preset" | "bundle" | "empty"} kind */
export const newStackHref = (kind, preset = "") =>
  `/stacks/new/${kind}${preset ? `?preset=${encodeURIComponent(preset)}` : ""}`;

/**
 * Open New stack's wizard: it is a page now, so this goes there (the Stacks
 * drawer, the Presets gallery, the palette and Live view all call it).
 * @param {(href: string) => void} [navigate]
 * @param {{preset?: string, empty?: boolean}} [opts] `preset`: the gallery
 *   card's "Use this preset", chosen (with its size) when the page opens;
 *   `empty`: the "Empty" route, a stack with no apps whose first app is
 *   added from its hub.
 */
export async function openNewStack(navigate, opts = {}) {
  const href = newStackHref(opts.empty ? "empty" : "preset", opts.preset);
  if (navigate) navigate(href);
  else location.assign(href);
}

/**
 * The preset / empty wizard, drawn into `holder`.
 * @param {HTMLElement} holder
 * @param {{navigate: (href: string) => void, empty: boolean,
 *   preset?: string | null}} o
 * @returns {Promise<() => void>} stop
 */
export async function mountPresetWizard(holder, o) {
  const { empty } = o;
  let stop = () => {};
  let gone = false;
  /** Show why the wizard cannot start, with Try again (re-runs the read). */
  const fail = (
    /** @type {{what: string, why: string, fix?: string | null}} */ err,
  ) => {
    holder.replaceChildren(
      failBox(err, {
        run: () => {
          holder.replaceChildren();
          holder.setAttribute("aria-busy", "true");
          void mountPresetWizard(holder, o).then((s) => {
            holder.removeAttribute("aria-busy");
            if (gone) s();
            else stop = s;
          });
        },
        drive: { id: TRY_AGAIN },
      }),
    );
    return () => {
      gone = true;
      stop();
    };
  };
  // The read has no answer yet: the package's busy plate, not a blank page.
  holder.replaceChildren(
    stage(h("p", { class: "measured" }, "Reading the presets…")),
  );
  const r = await fetchJson("/data/presets", "the presets");
  if (!r.ok) return fail(r.error);
  /** @type {import("./editforms.js").Preset[]} */
  const presets = r.body.presets ?? [];
  if (!presets.length && !empty)
    return fail({
      what: "a new stack",
      why: r.body.sync_error
        ? `the working copy is not there: ${r.body.sync_error}`
        : "the repository holds no presets",
      fix: "the settings page shows the working copy's state",
    });
  const taken = {
    names: /** @type {string[]} */ (r.body.taken?.names ?? []),
    vmids: /** @type {number[]} */ (r.body.taken?.vmids ?? []),
  };
  const w = newStackWizard(presets, r.body.suggest_vmid ?? null, { empty });
  const values = startValues(w);
  const picked = presets.find((p) => p.name === o.preset);
  if (empty) values.preset = "";
  else if (picked)
    Object.assign(values, { preset: picked.name }, presetSize(picked));
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

  /** @type {ReturnType<typeof wizardShell>} */
  let shell;
  const review = reviewPart({
    what: "the new stack",
    planUrl: "/data/stacks-new/plan",
    planBody: () => newStackBody(values),
    commitUrl: "/data/stacks-new/commit",
    commitBody: (cv) => ({
      stack: newStackBody(values),
      subject: String(cv.subject ?? "").trim(),
      ...(String(cv.note ?? "").trim() ? { note: String(cv.note).trim() } : {}),
      ...(String(cv.follow ?? "none") !== "none"
        ? { follow: String(cv.follow) }
        : {}),
    }),
    stackName: () => String(values.name).trim(),
    canCommit: (ok) => (shell.next.disabled = !ok),
    onCommitted: (a) => shell.showCommitted(a, String(values.name).trim()),
  });
  paintPreset();

  const sections = w.steps.map((s) => ({
    id: s.id,
    label: s.label,
    body:
      s.id === "preset"
        ? [...s.fields.map(field), presetNote]
        : s.id === "data"
          ? [dataBox]
          : s.id === PLAN
            ? review.els
            : s.fields.map(field),
  }));
  shell = wizardShell(holder, {
    id: w.id,
    steps: sections,
    navigate: o.navigate,
    onFinish: () => review.commit(),
  });
  // A gallery card already chose the preset: open on its name and number.
  if (picked) void shell.handle?.goTo(1);

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

  shell.wiz.addEventListener(BEFORE_STEP_EVENT, (e) => {
    const detail = /** @type {CustomEvent} */ (e).detail;
    if (detail.direction !== "forward") return;
    const from = w.steps[detail.from]?.id;
    if (!from) return;
    if (!markErrors(inputs, checkNewStep(w, from, values, taken)))
      e.preventDefault();
  });
  shell.wiz.addEventListener(STEP_EVENT, (e) => {
    const at = w.steps[/** @type {CustomEvent} */ (e).detail.step]?.id;
    if (at !== PLAN) shell.next.disabled = false;
    if (at === "data") void loadData();
    if (at === PLAN) void review.load();
  });

  // feat-platform-10: the Live view replay moves this very wizard.
  const unregister = register("new-stack", {
    dialog: holder,
    closed: shell.closed,
    step: () => shell.handle?.step() ?? 0,
    goTo: (/** @type {number} */ i) =>
      shell.handle ? shell.handle.goTo(i) : Promise.resolve(false),
    next: () => shell.next,
    back: () => shell.back,
    cancel: () => shell.cancel,
    planReady: () => review.planReady(),
    runError: review.runError,
    showCommitted: (/** @type {any} */ a) =>
      shell.showCommitted(a, String(values.name).trim()),
    close: () => shell.cancel.click(),
  });
  return () => {
    unregister();
    shell.stop();
  };
}
