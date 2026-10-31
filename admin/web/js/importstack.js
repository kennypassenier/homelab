// TUI parity (`homelab import <bundle.yml> <new-name> <vmid>`): a bundle an
// export wrote becomes a new stack, through the same plan dialog and commit
// every edit ends in. The bundle is pasted or uploaded (a file read in the
// browser, never sent anywhere but here); it never carries secrets, and one
// that names a .env is refused by the server. Claude drives the same form:
// homelab ui open import.

import { notify, openDialog } from "./actui.js";
import { fetchJson, h } from "./dom.js";
import { driven, register } from "./drivehooks.js";
import { IMPORT_FIELDS, importBody, importErrors } from "./editforms.js";
import { editField, markErrors, openPlanDialog } from "./editui.js";
import { commitBody } from "./plan.js";
import { stackHref } from "./router.js";
import {
  FILE_EVENT,
  REMOVE_EVENT,
  attachUploads,
  setDone,
  setError,
} from "/static/kp/dist/kp-themes.min.js";

/**
 * Open the import dialog.
 * @param {(href: string) => void} navigate
 */
export async function openImport(navigate) {
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
  const review = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--primary",
      id: "import-review",
    },
    "Review",
  );
  const note = h("p", { class: "measured", role: "status" });
  const d = openDialog({
    title: "Import a stack",
    description:
      "A bundle an export wrote (Export bundle on a stack's page, or homelab export) becomes a new stack: its name, number, address and /appdata paths are replaced by the new ones. Secrets are never in a bundle: add them through latch before the first deploy.",
    body: [
      h(
        "div",
        { class: "kp-field" },
        h("span", { class: "kp-field__label" }, "Upload a bundle"),
        drop,
      ),
      ...fields,
      note,
      h("div", { class: "kp-dialog__actions" }, review),
    ],
    id: "import-dialog",
    wide: true,
  });
  d.dialog.dataset.form = "import";
  const detachUpload = attachUploads(d.dialog);
  /** @type {{names: string[], vmids: number[]}} */
  let taken = { names: [], vmids: [] };
  void fetchJson("/data/presets", "the taken names and numbers").then((r) => {
    if (!r.ok) return;
    taken = r.body.taken ?? taken;
    if (!r.body.working_copy)
      note.textContent =
        "The dashboard has no working copy yet, so nothing can be imported.";
  });
  review.addEventListener("click", () => {
    // Live view: the plan and the commit run on the dashboard's server;
    // the replay opens the plan dialog itself.
    if (!driven()) {
      const errors = importErrors(values, taken);
      markErrors(inputs, errors);
      if (Object.keys(errors).length) return;
    }
    const body = importBody(values);
    openPlanDialog({
      id: "import",
      title: `Import · ${body.name}`,
      planUrl: "/data/stacks-import/plan",
      planBody: body,
      commitUrl: "/data/stacks-import/commit",
      commitBody: (v) => commitBody(body, v),
      onCommitted: () => {
        notify(`stacks/${body.name} is committed.`, "success", {
          label: "Open it",
          onClick: () => navigate(stackHref(body.name, "settings")),
        });
      },
    });
  });
  // feat-platform-10: the Live view replay works this very dialog.
  const unregister = register("import", {
    dialog: d.dialog,
    closed: d.closed,
    review: () => review,
    close: () => d.close(),
  });
  d.closed.then(() => {
    unregister();
    detachUpload();
  });
  return d;
}
