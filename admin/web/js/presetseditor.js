// feat-preset-1 (D): editing presets/ itself — a preset's metadata, its
// app files, a new preset, removing one. The Presets page's preset-name
// buttons and its "New preset…" button both open this one dialog; every
// change goes through the same edit plan → diff → commit flow a stack's
// forms use (`editui.js`'s `openPlanDialog`), scoped to `presets/` instead
// of `stacks/<stack>/` (`admin/src/core/presetedit.rs`,
// `WorkingCopy::transact_presets`). Kenny never hand-edits a preset's YAML
// either (owner goal, 2026-09-30). Live view parity: `homelab ui open
// preset <name>` and `homelab ui open new-preset` both drive this dialog
// (`editdrive.js`'s `presetEdit`), through the `preset-editor` handle.

import { notify, openDialog } from "./actui.js";
import { errorBox, fetchJson, h } from "./dom.js";
import { openPlanDialog } from "./editui.js";
import { register } from "./drivehooks.js";

/**
 * @typedef {{description: string, ram_mb: number, cores: number | null,
 *   disk_gb: number | null, features: string | null,
 *   unprivileged: boolean | null, gpu: boolean, vpn: boolean}} PresetMetaView
 * @typedef {{name: string, head: {commit: string, subject: string, at: number} | null,
 *   sync_error: string | null, exists: boolean, meta: PresetMetaView | null,
 *   files: Record<string, string>,
 *   file_templates: Record<string, string>}} PresetEditRead
 */

const NAME_PATTERN = /^[a-z0-9][a-z0-9-]*$/;

/**
 * @param {string} name
 * @param {AbortSignal} signal
 */
async function readPreset(name, signal) {
  return fetchJson(
    `/data/presets/${encodeURIComponent(name)}/edit`,
    `the ${name} preset`,
    signal,
  );
}

/**
 * @param {Record<string, unknown>} edit
 */
function planCommit(edit) {
  return /** @param {import("./editforms.js").Values} values */ (values) => {
    const subject = String(values.subject ?? "").trim();
    const note = String(values.note ?? "").trim();
    return {
      edit,
      ...(subject ? { subject } : {}),
      ...(note ? { note } : {}),
    };
  };
}

/**
 * @param {string} id
 * @param {string} title
 * @param {Record<string, unknown>} edit
 * @param {() => void} onCommitted
 */
function reviewEdit(id, title, edit, onCommitted) {
  openPlanDialog({
    id,
    title,
    planUrl: "/data/presets/plan",
    planBody: { edit },
    commitUrl: "/data/presets/commit",
    commitBody: planCommit(edit),
    onCommitted,
  });
}

/**
 * A labelled text input, for the metadata form's plain fields.
 * @param {string} id
 * @param {string} label
 * @param {string} value
 * @param {{type?: string, help?: string}} [opts]
 */
function field(id, label, value, opts = {}) {
  const input = h("input", {
    class: "kp-field__input",
    id,
    type: opts.type ?? "text",
  });
  input.value = value;
  return {
    input,
    wrap: h(
      "div",
      { class: "kp-field" },
      h("label", { class: "kp-field__label", for: id }, label),
      input,
      opts.help ? h("p", { class: "kp-field__help" }, opts.help) : "",
    ),
  };
}

/**
 * @param {string} id
 * @param {string} label
 * @param {boolean} checked
 */
function checkField(id, label, checked) {
  const input = h("input", {
    type: "checkbox",
    id,
    class: "kp-field__check",
  });
  input.checked = checked;
  return {
    input,
    wrap: h(
      "div",
      { class: "kp-field kp-field--check" },
      input,
      h("label", { class: "kp-field__label", for: id }, label),
    ),
  };
}

/**
 * The metadata card: `preset.yml`'s own fields, plus — for a new preset —
 * the name field itself (kp dialog + validation, replacing a `prompt()`).
 * @param {string | null} name null: a new preset, named in the form
 * @param {PresetMetaView | null} meta
 * @param {(name: string) => void} reload
 */
function metaCard(name, meta, reload) {
  const m = meta ?? {
    description: "",
    ram_mb: 1024,
    cores: null,
    disk_gb: null,
    features: null,
    unprivileged: null,
    gpu: false,
    vpn: false,
  };
  const nameField =
    name === null
      ? field("new-preset-name", "Preset name", "", {
          help: "Lowercase letters, digits and dashes, not starting with '_'.",
        })
      : null;
  const description = field("preset-description", "Description", m.description);
  const ramMb = field("preset-ram-mb", "Memory (MB)", String(m.ram_mb), {
    type: "number",
  });
  const cores = field(
    "preset-cores",
    "Cores (blank: the stack default)",
    m.cores == null ? "" : String(m.cores),
    { type: "number" },
  );
  const diskGb = field(
    "preset-disk-gb",
    "Disk (GB, blank: the stack default)",
    m.disk_gb == null ? "" : String(m.disk_gb),
    { type: "number" },
  );
  const features = field(
    "preset-features",
    "LXC features (blank: the stack default)",
    m.features ?? "",
  );
  const gpu = checkField(
    "preset-gpu",
    "Pass the host GPU through (VAAPI)",
    m.gpu,
  );
  const vpn = checkField(
    "preset-vpn",
    "Give a /dev/net/tun device (VPN clients)",
    m.vpn,
  );
  const status = h("span", { class: "measured state-word", role: "status" });
  const save = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--primary",
      id: "preset-meta-review",
    },
    "Review and commit…",
  );
  save.addEventListener("click", () => {
    const useName = name ?? nameField?.input.value.trim() ?? "";
    if (!useName || !NAME_PATTERN.test(useName)) {
      notify(
        "Give the preset a name: lowercase letters, digits and dashes, not starting with '-'.",
        "info",
      );
      nameField?.input.focus();
      return;
    }
    const num = (/** @type {string} */ s) =>
      s.trim() === "" ? null : Number(s);
    const edit = {
      kind: "meta",
      name: useName,
      meta: {
        description: description.input.value.trim(),
        ram_mb: Number(ramMb.input.value) || 0,
        cores: num(cores.input.value),
        disk_gb: num(diskGb.input.value),
        features: features.input.value.trim() || null,
        unprivileged: m.unprivileged,
        gpu: gpu.input.checked,
        vpn: vpn.input.checked,
      },
    };
    reviewEdit(
      `edit:preset-meta:${useName}`,
      `${useName}: preset.yml`,
      edit,
      () => reload(useName),
    );
  });
  return h(
    "section",
    {
      class: "kp-card edit-card",
      "aria-label": "preset.yml",
      id: "preset-meta",
    },
    h("h3", null, "preset.yml"),
    h(
      "div",
      { class: "edit-grid" },
      ...(nameField ? [nameField.wrap] : []),
      description.wrap,
      ramMb.wrap,
      cores.wrap,
      diskGb.wrap,
      features.wrap,
      gpu.wrap,
      vpn.wrap,
    ),
    h("div", { class: "row-buttons" }, save, " ", status),
  );
}

/**
 * The file editor card: every file the preset holds, one at a time,
 * create, delete or rename — the stack Files card's shape
 * (`admin/src/core/stackedit_files.rs`), reusing its starter templates,
 * for a preset's app files instead of a stack's.
 * @param {string} name
 * @param {Record<string, string>} files
 * @param {Record<string, string>} templates
 * @param {() => void} reload
 */
function filesCard(name, files, templates, reload) {
  const known = Object.keys(files)
    .filter((f) => f !== "preset.yml")
    .sort();
  const sel = h(
    "select",
    { class: "kp-field__input", id: "preset-file-select" },
    h("option", { value: "" }, "New file…"),
    ...known.map((f) => h("option", { value: f }, f)),
  );
  const pathInput = h("input", {
    class: "kp-field__input mono",
    id: "preset-file-path",
    type: "text",
    placeholder: "myapp/docker-compose.yml",
  });
  const templateNames = Object.keys(templates).sort();
  const templateSel = h(
    "select",
    { class: "kp-field__input", id: "preset-file-template", hidden: "" },
    h("option", { value: "" }, "Blank"),
    ...templateNames.map((k) => h("option", { value: k }, k)),
  );
  const templateField = h(
    "div",
    { class: "kp-field", id: "preset-file-template-field", hidden: "" },
    h(
      "label",
      { class: "kp-field__label", for: "preset-file-template" },
      "Start from",
    ),
    templateSel,
  );
  const area = h("textarea", {
    class: "kp-field__input mono raw-text",
    id: "preset-file-text",
    rows: "16",
    spellcheck: "false",
  });
  const renameInput = h("input", {
    class: "kp-field__input mono",
    id: "preset-file-rename-to",
    type: "text",
  });
  const renameField = h(
    "div",
    { class: "kp-field", id: "preset-file-rename-field", hidden: "" },
    h(
      "label",
      { class: "kp-field__label", for: "preset-file-rename-to" },
      "New path",
    ),
    renameInput,
  );
  const show = () => {
    const f = sel.value;
    pathInput.value = f;
    pathInput.disabled = Boolean(f);
    area.value = f ? (files[f] ?? "") : "";
    templateField.hidden = Boolean(f);
    templateSel.hidden = Boolean(f);
    renameField.hidden = !f;
    renameInput.value = f;
  };
  sel.addEventListener("change", show);
  templateSel.addEventListener("change", () => {
    if (!sel.value) area.value = templates[templateSel.value] ?? "";
  });
  show();
  const save = h(
    "button",
    { type: "button", class: "kp-button", id: "preset-file-save" },
    "Review and commit…",
  );
  const rename = h(
    "button",
    { type: "button", class: "kp-button", id: "preset-file-rename" },
    "Rename…",
  );
  const del = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--destructive",
      id: "preset-file-delete",
    },
    "Delete this file…",
  );
  save.addEventListener("click", () => {
    const path = pathInput.value.trim();
    if (!path) {
      notify("Give the file a path first.", "info");
      return;
    }
    const edit = { kind: "file", name, path, content: area.value };
    reviewEdit(
      `edit:preset-file:${name}:${path}`,
      `${name}: ${path}`,
      edit,
      reload,
    );
  });
  rename.addEventListener("click", () => {
    if (!sel.value) return;
    const to = renameInput.value.trim();
    if (!to || to === sel.value) {
      notify("Give the file a different new path first.", "info");
      return;
    }
    const edit = { kind: "rename_file", name, from: sel.value, to };
    reviewEdit(
      `edit:preset-rename-file:${name}:${sel.value}`,
      `${name}: ${sel.value} → ${to}`,
      edit,
      reload,
    );
  });
  del.addEventListener("click", () => {
    if (!sel.value) return;
    const edit = { kind: "remove_file", name, path: sel.value };
    reviewEdit(
      `edit:preset-remove-file:${name}:${sel.value}`,
      `${name}: remove ${sel.value}`,
      edit,
      reload,
    );
  });
  return h(
    "section",
    { class: "kp-card edit-card", "aria-label": "Files", id: "preset-files" },
    h("h3", null, "Files"),
    h(
      "div",
      { class: "kp-field" },
      h(
        "label",
        { class: "kp-field__label", for: "preset-file-select" },
        "File",
      ),
      sel,
    ),
    h(
      "div",
      { class: "kp-field" },
      h("label", { class: "kp-field__label", for: "preset-file-path" }, "Path"),
      pathInput,
    ),
    templateField,
    area,
    renameField,
    h("div", { class: "row-buttons" }, save, " ", rename, " ", del),
  );
}

/**
 * Open the presets editor. `name`: an existing preset's name, or `null`
 * for a new one (named in the form itself — Live view parity: `homelab ui
 * open preset <name>` and `homelab ui open new-preset` both land here).
 * @param {string | null} name
 * @param {() => void} onChanged called after every committed edit, so the
 *   Presets page can reload its table
 */
export async function openPresetEditor(name, onChanged) {
  const abort = new AbortController();
  const body = h(
    "p",
    { class: "measured" },
    name ? "Reading the preset…" : "A new preset.",
  );
  const d = openDialog({
    id: name ? `preset-editor-${name}` : "new-preset-editor",
    title: name ?? "New preset",
    wide: true,
    body: [body],
  });
  d.dialog.dataset.form = name ? "preset" : "new-preset";
  d.closed.then(() => abort.abort());
  let current = name;
  /** @param {string} [renamedTo] after a create, the name to keep reading */
  const load = async (renamedTo) => {
    if (renamedTo) current = renamedTo;
    if (!current) {
      body.replaceChildren(
        metaCard(null, null, (created) => {
          onChanged();
          void load(created);
        }),
      );
      return;
    }
    const r = await readPreset(current, abort.signal);
    if (abort.signal.aborted) return;
    if (!r.ok) {
      body.replaceChildren(errorBox(r.error));
      return;
    }
    const e = /** @type {PresetEditRead} */ (r.body);
    const reload = () => void load();
    const removeBtn = h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--destructive",
        id: "preset-remove",
      },
      "Remove this preset…",
    );
    removeBtn.addEventListener("click", () => {
      reviewEdit(
        `edit:preset-remove:${current}`,
        `Remove ${current}`,
        { kind: "remove", name: current },
        () => {
          onChanged();
          d.close();
        },
      );
    });
    const parts = [
      h(
        "p",
        { class: "measured" },
        e.exists
          ? `presets/${current}/ — ${Object.keys(e.files).length} file(s).`
          : `presets/${current}/ does not exist yet: saving preset.yml below creates it.`,
      ),
      metaCard(current, e.meta, () => {
        onChanged();
        reload();
      }),
    ];
    if (e.exists) {
      parts.push(
        filesCard(current, e.files, e.file_templates ?? {}, () => {
          onChanged();
          reload();
        }),
      );
      parts.push(h("div", { class: "row-buttons" }, removeBtn));
    }
    body.replaceChildren(...parts);
  };
  // feat-platform-10: the Live view replay works this very dialog —
  // `preset <name>` and `new-preset` both open it (driveedit.rs's
  // `EditKind::Preset`/`NewPreset`), so one handle name serves both.
  const unregister = register("preset-editor", {
    dialog: d.dialog,
    closed: d.closed,
    review: () =>
      /** @type {HTMLElement | null} */ (
        d.dialog.querySelector("#preset-meta-review")
      ),
    // feat-preset-1: the Files card's own three buttons, and "Remove this
    // preset…" — each its own small plan/commit, on the same dialog as
    // the metadata form (`button`/`sync` in editdrive.js's `presetEdit`
    // pick the right one by the server's `edit.action`).
    fileSave: () =>
      /** @type {HTMLElement | null} */ (
        d.dialog.querySelector("#preset-file-save")
      ),
    fileRename: () =>
      /** @type {HTMLElement | null} */ (
        d.dialog.querySelector("#preset-file-rename")
      ),
    fileDelete: () =>
      /** @type {HTMLElement | null} */ (
        d.dialog.querySelector("#preset-file-delete")
      ),
    remove: () =>
      /** @type {HTMLElement | null} */ (
        d.dialog.querySelector("#preset-remove")
      ),
    close: () => d.close(),
  });
  d.closed.then(unregister);
  await load();
}
