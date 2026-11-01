// TUI parity (`homelab import <bundle.yml> <new-name> <vmid>`): a bundle an
// export wrote becomes a new stack, through the same review and commit every
// New stack route ends on (newreview.js). ux-stacknew-2 (Kenny, 2026-11-01):
// it runs on a page of its own, /stacks/new/bundle, like the other routes.
// The bundle is pasted or uploaded (a file read in the browser, never sent
// anywhere but here); it never carries secrets, and one that names a .env is
// refused by the server. Claude drives the same form: homelab ui open import.

import { fetchJson, h } from "./dom.js";
import { driven, register } from "./drivehooks.js";
import { IMPORT_FIELDS, importBody, importErrors } from "./editforms.js";
import { editField, markErrors } from "./editui.js";
import { reviewPart } from "./newreview.js";
import { wizardShell } from "./newwizard.js";
import { commitBody } from "./plan.js";
import {
  BEFORE_STEP_EVENT,
  FILE_EVENT,
  REMOVE_EVENT,
  STEP_EVENT,
  attachUploads,
  setDone,
  setError,
} from "/static/kp/dist/kp-themes.min.js";

/**
 * Open the import route: it is a page now, so this goes there.
 * @param {(href: string) => void} [navigate]
 */
export async function openImport(navigate) {
  if (navigate) navigate("/stacks/new/bundle");
  else location.assign("/stacks/new/bundle");
}

/**
 * The bundle route's wizard, drawn into `holder`: step one reads the bundle
 * and the new name and number, step two is the review every route shares.
 * @param {HTMLElement} holder
 * @param {{navigate: (href: string) => void}} o
 * @returns {() => void} stop
 */
export function mountBundleWizard(holder, o) {
  /** @type {import("./editforms.js").Values} */
  const values = { bundle: "", name: "", vmid: "" };
  /** @type {Map<string, HTMLElement>} */
  const inputs = new Map();
  // kp's upload zone: choose or drop one bundle; the file is read here in the
  // browser (never sent anywhere but the plan), refusals say why on the row.
  const file = h("input", {
    type: "file",
    class: "kp-sr-only",
    id: "import-file",
    accept: ".yml,.yaml,text/yaml,application/yaml",
    "data-kp-upload-input": "",
  });
  const drop = h(
    "div",
    {
      class: "kp-upload",
      "data-kp-upload": "",
      "data-kp-max-bytes": String(1024 * 1024),
      "data-kp-max-files": "1",
    },
    file,
    h(
      "label",
      {
        class: "kp-upload__zone",
        for: "import-file",
        "data-kp-upload-zone": "",
      },
      "Drop a bundle (.yml) here or choose it. The file is read in this browser; its text appears below.",
    ),
    h("ul", { class: "kp-upload__list", "data-kp-upload-list": "" }),
  );
  const fields = IMPORT_FIELDS.map((f) => {
    const x = editField(f, "", (v) => (values[f.name] = v));
    inputs.set(f.name, x.input);
    return x.wrap;
  });
  const bundleEl = () =>
    /** @type {HTMLTextAreaElement} */ (inputs.get("bundle"));
  const setBundle = (/** @type {string} */ text) => {
    const el = bundleEl();
    el.value = text;
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
  };
  let fromFile = false;
  drop.addEventListener(FILE_EVENT, async (e) => {
    const { file: f, item } = /** @type {CustomEvent} */ (e).detail;
    try {
      const text = await f.text();
      setBundle(text);
      fromFile = true;
      setDone(item, "Read; review the text below.");
    } catch {
      setError(item, "This browser could not read the file.");
    }
  });
  drop.addEventListener(REMOVE_EVENT, () => {
    // Taking the file away takes its text away, but never text typed by hand.
    if (fromFile) setBundle("");
    fromFile = false;
  });

  const note = h("p", { class: "measured", role: "status" });
  /** @type {{names: string[], vmids: number[]}} */
  let taken = { names: [], vmids: [] };
  void fetchJson("/data/presets", "the taken names and numbers").then((r) => {
    if (!r.ok) return;
    taken = r.body.taken ?? taken;
    if (!r.body.working_copy)
      note.textContent =
        "The dashboard has no working copy yet, so nothing can be imported.";
  });

  /** @type {ReturnType<typeof wizardShell>} */
  let shell;
  const review = reviewPart({
    what: "the imported stack",
    planUrl: "/data/stacks-import/plan",
    planBody: () => importBody(values),
    commitUrl: "/data/stacks-import/commit",
    commitBody: (cv) => commitBody(importBody(values), cv),
    stackName: () => String(values.name).trim(),
    canCommit: (ok) => (shell.next.disabled = !ok),
    onCommitted: (a) => shell.showCommitted(a, importBody(values).name),
  });
  shell = wizardShell(holder, {
    id: "import",
    navigate: o.navigate,
    onFinish: () => review.commit(),
    steps: [
      {
        id: "bundle",
        label: "Bundle",
        body: [
          h(
            "div",
            { class: "kp-field" },
            h("span", { class: "kp-field__label" }, "Upload a bundle"),
            drop,
          ),
          ...fields,
          note,
        ],
      },
      { id: "review", label: "Review", body: review.els },
    ],
  });
  const detachUpload = attachUploads(holder);
  shell.wiz.addEventListener(BEFORE_STEP_EVENT, (e) => {
    if (/** @type {CustomEvent} */ (e).detail.direction !== "forward") return;
    // Live view: the server holds a driven step to the same checks.
    if (driven()) return;
    const errors = importErrors(values, taken);
    markErrors(inputs, errors);
    if (Object.keys(errors).length) e.preventDefault();
  });
  shell.wiz.addEventListener(STEP_EVENT, (e) => {
    const step = /** @type {CustomEvent} */ (e).detail.step;
    if (step === 0) shell.next.disabled = false;
    if (step === 1) void review.load();
  });
  // feat-platform-10: the Live view replay works this very wizard.
  const unregister = register("import", {
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
      shell.showCommitted(a, importBody(values).name),
    close: () => shell.cancel.click(),
  });
  return () => {
    unregister();
    detachUpload();
    shell.stop();
  };
}
