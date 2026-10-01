// The stack page's two edit tabs. Settings (feat-stacks-2): the settings
// form, any file of the stack in the raw editor, and a preset's app added
// (feat-stacks-3). Firewall (feat-firewall-1): the stack's rules as a kp
// datatable with add, edit, move and delete, and its options. Every change
// ends in the plan dialog (editui.js): the plan first, then the commit,
// then optionally a deploy of exactly that commit.

import { notify, openDialog, refusalCallout } from "./actui.js";
import {
  addAppBody,
  appsAddBlankField,
  appsBody,
  appsRemoveField,
  changesSomething,
  checkFields,
  checkRowFields,
  checkRowFromValues,
  checkRowProblems,
  checkRowSummary,
  checksBody,
  dataMountFields,
  dataMountFromValues,
  dataMountProblems,
  dataMountSummary,
  firewallBody,
  firewallChanged,
  firewallModel,
  latchBody,
  latchFileFields,
  latchFileFromValues,
  latchFileProblems,
  latchFileSummary,
  latchSecretField,
  latchSecretsProblems,
  logFileFields,
  logFileFromValues,
  logFileSummary,
  manualRow,
  manualRowFields,
  manualRowFromValues,
  manualRowSummary,
  moveRule,
  originJson,
  parseJsonList,
  probeRowFields,
  probeRowFromValues,
  probeRowProblems,
  probeRowSummary,
  retentionRowFields,
  retentionRowFromValues,
  retentionRowProblems,
  retentionRowSummary,
  rowBody,
  rowModel,
  rowsChanged,
  ruleFields,
  ruleFromValues,
  ruleProblems,
  ruleSummary,
  settingsBody,
  settingsExtBody,
  settingsExtForm,
  settingsForm,
  startValues,
  storageFields,
  storageFromValues,
  storageSummary,
  tileProblems,
  tileRowFields,
  tileRowFromValues,
  tileRowProblems,
  tileRowSummary,
  tilesEditBody,
} from "./editforms.js";
import {
  editField,
  markErrors,
  openPlanDialog,
  stackCommit,
} from "./editui.js";
import { badgeCell, errorBox, fetchJson, h, tableBlock, td } from "./dom.js";
import { driven, handle, register } from "./drivehooks.js";
import { attachDataTables, dataTable } from "/static/kp/js/datatable.js";
import { attachSwitches, showError } from "/static/kp/js/forms.js";

/**
 * @typedef {{unit: string, path: string, unit_file_path: string,
 *   manifest: NativeManifestView | null}} NativeView
 * @typedef {{stack_name: string, vmid: number, hostname: string, unit: string,
 *   binary: string, env_file: string | null, data_dirs: string[],
 *   update_cmd: string | null, stateless: boolean, restore_note: string | null,
 *   release_repo: string | null, release_asset: string | null,
 *   backup_from_newest: string | null, backup_pause: boolean | "chassis",
 *   update_policy: "manual" | "auto" | "self",
 *   metrics: boolean | null}} NativeManifestView
 */

/**
 * @typedef {{stack: string, head: {commit: string, subject: string, at: number} | null,
 *   sync_error: string | null, texts: Record<string, string>,
 *   manifest: import("./editforms.js").ManifestView | null,
 *   manifest_error: string | null, images: Record<string, string>,
 *   file_templates: Record<string, string>, self_stack: string,
 *   natives: NativeView[],
 *   presets: {name: string, description: string, apps: string[]}[],
 *   checks?: Record<string, import("./editforms.js").ChecksView>}} EditRead
 */

/**
 * Read what the editor needs.
 * @param {string} stack
 * @param {AbortSignal} signal
 */
async function readEdit(stack, signal) {
  return fetchJson(
    `/data/stacks/${encodeURIComponent(stack)}/edit`,
    `the files of ${stack}`,
    signal,
  );
}

/**
 * The working copy's line above an editor.
 * @param {EditRead} e
 */
function headLine(e) {
  const p = h("p", { class: "measured edit-head" });
  p.textContent = e.head
    ? `Editing the working copy at ${e.head.commit.slice(0, 10)} · ${e.head.subject}`
    : "The working copy has no commit yet.";
  const out = [p];
  if (e.sync_error)
    out.push(
      h(
        "div",
        { class: "kp-alert kp-alert--warning", role: "status" },
        `Not brought up to date with the remote: ${e.sync_error}.`,
      ),
    );
  return out;
}

/**
 * @param {HTMLElement} panel
 * @param {{name: string}} params
 * @returns {() => void}
 */
export function settingsTab(panel, params) {
  const stack = params.name;
  const abort = new AbortController();
  panel.replaceChildren(
    h("p", { class: "measured" }, "Reading the stack's files…"),
  );
  /** @type {() => void} */
  let unregister = () => {};
  /** @type {() => void} */
  let detachTables = () => {};
  /** @type {() => void} */
  let detachRows = () => {};
  const load = async () => {
    const r = await readEdit(stack, abort.signal);
    if (!r.ok) {
      panel.replaceChildren(errorBox(r.error));
      return;
    }
    const e = /** @type {EditRead} */ (r.body);
    const parts = /** @type {Node[]} */ ([...headLine(e)]);
    if (e.manifest) parts.push(settingsCard(stack, e, load));
    else
      parts.push(
        h(
          "div",
          { class: "kp-alert kp-alert--warning", role: "status" },
          `The settings form needs a readable lxc-compose.yml: ${e.manifest_error ?? "none"}. The raw editor below still works.`,
        ),
      );
    parts.push(filesCard(stack, e, load));
    if (e.manifest && e.presets.length) parts.push(addAppCard(stack, e, load));
    let settingsExt = null;
    let apps = null;
    let latch = null;
    let tiles = null;
    if (e.manifest) {
      settingsExt = settingsExtCard(stack, e, load);
      parts.push(settingsExt.node);
      apps = appsEditCard(stack, e, load);
      parts.push(apps.node);
      latch = latchEditCard(stack, e, load);
      parts.push(latch.node);
      tiles = tilesEditCard(stack, e, load);
      parts.push(tiles.node);
    }
    if (e.manifest && (e.manifest.native_only || e.natives.length))
      parts.push(nativeCard(stack, e, load));
    panel.replaceChildren(...parts);
    // feat-platform-10: the row tables' kp datatables need their wraps in
    // the live document first (drawFirewall's own order, at the panel
    // level here since several tables share this one tab).
    detachTables();
    detachTables = attachDataTables(panel);
    settingsExt?.bind();
    apps?.bind();
    latch?.bind();
    tiles?.bind();
    detachRows();
    detachRows = () => {
      settingsExt?.detach();
      apps?.detach();
      latch?.detach();
      tiles?.detach();
    };
    // feat-platform-10: the Live view replay presses these very buttons,
    // and opens/acts on the row tables' rows.
    unregister();
    unregister = register(`stack-edit:${stack}`, {
      openRaw: () => {
        const d = panel.querySelector("#raw-editor");
        if (d instanceof HTMLDetailsElement) d.open = true;
      },
      review: (/** @type {string} */ family) =>
        /** @type {HTMLElement | null} */ (
          panel.querySelector(
            {
              raw: "#raw-review",
              "add-app": "#add-app-review",
              "settings-ext": "#settings-ext-review",
              apps: "#apps-review",
              latch: "#latch-review",
              tiles: "#tiles-review",
              native: "#native-review",
              "add-native": "#add-native-review",
            }[family] ?? "#settings-review",
          )
        ),
      // feat-platform-10: a row op's target is "<list>:<n>" (or just
      // "<list>" for add) — each `rowTable` self-registers under
      // `row-table:<list>:<stack>` (see its own `register` call), so this
      // only needs to find the right one and hand the op on.
      row: (
        /** @type {string} */ op,
        /** @type {string | undefined} */ target,
      ) => {
        const [list, n] = String(target ?? "").split(":");
        const t = handle(`row-table:${list}:${stack}`);
        if (!t) return null;
        return op === "add" ? t.add() : t.rowButton(op, Number(n) - 1);
      },
      openRow: (/** @type {string} */ list, /** @type {number | null} */ i) =>
        handle(`row-table:${list}:${stack}`)?.openDialogFor(i),
      // feat-native-1: the native step's second button, beside "next".
      remove: () =>
        /** @type {HTMLElement | null} */ (
          panel.querySelector("#native-remove")
        ),
    });
  };
  void load().catch(() => {});
  return () => {
    abort.abort();
    unregister();
    detachTables();
    detachRows();
  };
}

/**
 * @param {string} stack
 * @param {EditRead} e
 * @param {() => Promise<void>} reload
 */
function settingsCard(stack, e, reload) {
  const m = /** @type {import("./editforms.js").ManifestView} */ (e.manifest);
  const form = settingsForm(stack, m, e.images);
  const values = startValues(form);
  /** @type {Map<string, HTMLElement>} */
  const inputs = new Map();
  const fields = form.steps[0].fields.map((f) => {
    const x = editField(f, values[f.name], (v) => {
      values[f.name] = v;
      status.textContent = changesSomething(settingsBody(form, values))
        ? "Changed; not committed."
        : "";
    });
    inputs.set(f.name, x.input);
    return x.wrap;
  });
  // Sized for its one text, so it never reflows the row when it appears.
  const status = h("span", {
    class: "measured state-word",
    "data-size": "Changed; not committed.",
    role: "status",
  });
  const review = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--primary",
      id: "settings-review",
    },
    "Review and commit…",
  );
  review.addEventListener("click", () => {
    const errors = { ...checkFields(form, values), ...tileProblems(values) };
    if (!markErrors(inputs, errors)) return;
    const edit = settingsBody(form, values);
    if (!changesSomething(edit)) {
      notify("Nothing is changed yet.", "info");
      return;
    }
    openPlanDialog({
      id: form.id,
      title: `Settings · ${stack}`,
      planUrl: `/data/stacks/${encodeURIComponent(stack)}/plan`,
      planBody: { edit },
      commitUrl: `/data/stacks/${encodeURIComponent(stack)}/commit`,
      commitBody: stackCommit(edit),
      onCommitted: () => void reload(),
    });
  });
  return h(
    "section",
    {
      class: "kp-card edit-card",
      "aria-label": "Settings",
      id: "settings-form",
      "data-form": form.id,
    },
    h("h2", null, "Settings"),
    h(
      "p",
      { class: "measured" },
      `CT ${m.vmid} · ${m.hostname} · ${m.ip}. Written into stacks/${stack}/lxc-compose.yml and the apps' compose files, with every comment kept.`,
    ),
    h("div", { class: "edit-grid" }, ...fields),
    h("div", { class: "row-buttons" }, review, " ", status),
  );
}

/**
 * feat-stacks-files: every text file of the stack, open, edited, created,
 * deleted or renamed. Editing an existing file still ends as a `raw` edit
 * (unchanged since feat-stacks-2, and still what Live view drives with
 * `homelab ui open raw <stack>`); create/delete/rename end as a `files`
 * edit (stackedit_files.rs) — new ops, not yet driven from the CLI.
 * @param {string} stack
 * @param {EditRead} e
 * @param {() => Promise<void>} reload
 */
function filesCard(stack, e, reload) {
  const files = Object.keys(e.texts).sort();
  const templates = e.file_templates ?? {};
  /** @type {"edit" | "create"} */
  let mode = "edit";

  const sel = h(
    "select",
    { class: "kp-field__input", id: "raw-file" },
    ...files.map((f) => h("option", { value: f }, f)),
  );
  sel.value = files.includes("lxc-compose.yml")
    ? "lxc-compose.yml"
    : (files[0] ?? "");

  const pathInput = h("input", {
    class: "kp-field__input mono",
    id: "files-new-path",
    type: "text",
    placeholder: "app/checks.yml",
    hidden: "",
  });
  const templateNames = Object.keys(templates).sort();
  const templateSel = h(
    "select",
    { class: "kp-field__input", id: "files-new-template", hidden: "" },
    h("option", { value: "" }, "Blank"),
    ...templateNames.map((k) => h("option", { value: k }, k)),
  );
  const pathField = h(
    "div",
    { class: "kp-field", id: "files-new-path-field", hidden: "" },
    h("label", { class: "kp-field__label", for: "files-new-path" }, "Path"),
    pathInput,
  );
  const templateField = h(
    "div",
    { class: "kp-field", id: "files-new-template-field", hidden: "" },
    h(
      "label",
      { class: "kp-field__label", for: "files-new-template" },
      "Start from",
    ),
    templateSel,
  );
  const fileField = h(
    "div",
    { class: "kp-field", id: "files-file-field" },
    h("label", { class: "kp-field__label", for: "raw-file" }, "File"),
    sel,
  );

  const renameInput = h("input", {
    class: "kp-field__input mono",
    id: "files-rename-to",
    type: "text",
  });
  const renameField = h(
    "div",
    { class: "kp-field", id: "files-rename-field", hidden: "" },
    h(
      "label",
      { class: "kp-field__label", for: "files-rename-to" },
      "New path",
    ),
    renameInput,
    h(
      "button",
      { type: "button", class: "kp-button", id: "files-rename-go" },
      "Rename…",
    ),
  );

  const area = h("textarea", {
    class: "kp-field__input mono raw-text",
    id: "raw-text",
    rows: "18",
    spellcheck: "false",
    "aria-label": "The file's text",
  });

  const newBtn = h(
    "button",
    { type: "button", class: "kp-button", id: "files-new" },
    "New file…",
  );
  const renameBtn = h(
    "button",
    { type: "button", class: "kp-button", id: "files-rename" },
    "Rename this file…",
  );
  const deleteBtn = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--destructive",
      id: "files-delete",
    },
    "Delete this file…",
  );
  const cancelBtn = h(
    "button",
    { type: "button", class: "kp-button", id: "files-cancel", hidden: "" },
    "Cancel",
  );
  const review = h(
    "button",
    { type: "button", class: "kp-button kp-button--primary", id: "raw-review" },
    "Review and commit…",
  );

  const showFile = () => (area.value = e.texts[sel.value] ?? "");
  showFile();
  sel.addEventListener("change", showFile);

  const setMode = (/** @type {"edit" | "create"} */ m) => {
    mode = m;
    const creating = m === "create";
    fileField.hidden = creating;
    pathField.hidden = !creating;
    templateField.hidden = !creating;
    newBtn.hidden = creating;
    renameBtn.hidden = creating;
    deleteBtn.hidden = creating;
    cancelBtn.hidden = !creating;
    renameField.hidden = true;
    if (creating) {
      pathInput.value = "";
      templateSel.value = "";
      area.value = "";
      pathInput.focus();
    } else {
      showFile();
    }
  };

  newBtn.addEventListener("click", () => setMode("create"));
  cancelBtn.addEventListener("click", () => setMode("edit"));
  templateSel.addEventListener(
    "change",
    () => (area.value = templates[templateSel.value] ?? ""),
  );

  renameBtn.addEventListener("click", () => {
    renameField.hidden = !renameField.hidden;
    if (!renameField.hidden) {
      renameInput.value = sel.value;
      renameInput.focus();
      renameInput.select();
    }
  });
  renameField
    .querySelector("#files-rename-go")
    ?.addEventListener("click", () => {
      const to = renameInput.value.trim();
      if (!to || to === sel.value) {
        notify("Give the file another path first.", "info");
        return;
      }
      const edit = { kind: "files", op: "rename", from: sel.value, to };
      openPlanDialog({
        id: `edit:files:rename:${stack}`,
        title: `Rename ${sel.value} · ${stack}`,
        planUrl: `/data/stacks/${encodeURIComponent(stack)}/plan`,
        planBody: { edit },
        commitUrl: `/data/stacks/${encodeURIComponent(stack)}/commit`,
        commitBody: stackCommit(edit),
        onCommitted: () => void reload(),
      });
    });

  deleteBtn.addEventListener("click", () => {
    const edit = { kind: "files", op: "delete", path: sel.value };
    openPlanDialog({
      id: `edit:files:delete:${stack}`,
      title: `Delete ${sel.value} · ${stack}`,
      planUrl: `/data/stacks/${encodeURIComponent(stack)}/plan`,
      planBody: { edit },
      commitUrl: `/data/stacks/${encodeURIComponent(stack)}/commit`,
      commitBody: stackCommit(edit),
      onCommitted: () => void reload(),
    });
  });

  review.addEventListener("click", () => {
    if (mode === "create") {
      const path = pathInput.value.trim();
      if (!path) {
        notify("Give the new file a path first.", "info");
        return;
      }
      const edit = { kind: "files", op: "create", path, content: area.value };
      openPlanDialog({
        id: `edit:files:create:${stack}`,
        title: `${path} · ${stack}`,
        planUrl: `/data/stacks/${encodeURIComponent(stack)}/plan`,
        planBody: { edit },
        commitUrl: `/data/stacks/${encodeURIComponent(stack)}/commit`,
        commitBody: stackCommit(edit),
        onCommitted: () => void reload(),
      });
      return;
    }
    const edit = { kind: "raw", path: sel.value, content: area.value };
    if (area.value === e.texts[sel.value]) {
      notify("The file is as it was.", "info");
      return;
    }
    openPlanDialog({
      id: `edit:raw:${stack}`,
      title: `${sel.value} · ${stack}`,
      planUrl: `/data/stacks/${encodeURIComponent(stack)}/plan`,
      planBody: { edit },
      commitUrl: `/data/stacks/${encodeURIComponent(stack)}/commit`,
      commitBody: stackCommit(edit),
      onCommitted: () => void reload(),
    });
  });

  return h(
    "details",
    { class: "kp-card edit-card", id: "raw-editor", open: "" },
    h("summary", null, h("strong", null, "Files")),
    h(
      "p",
      { class: "measured" },
      "Every text file of the stack: open, edit, create, delete or rename. Secrets (.env) are not here: they live in latch.",
    ),
    fileField,
    pathField,
    templateField,
    area,
    h(
      "div",
      { class: "row-buttons" },
      review,
      newBtn,
      renameBtn,
      deleteBtn,
      cancelBtn,
    ),
    renameField,
  );
}

/**
 * feat-stacks-3: a preset's app into this stack.
 * @param {string} stack
 * @param {EditRead} e
 * @param {() => Promise<void>} reload
 */
function addAppCard(stack, e, reload) {
  const sel = h(
    "select",
    { class: "kp-field__input", id: "add-app-preset" },
    ...e.presets.map((p) =>
      h(
        "option",
        { value: p.name },
        `${p.name} · ${p.description} (${p.apps.join(", ")})`,
      ),
    ),
  );
  // feat-tiles-3 (B): one optional hostname per app the chosen preset
  // brings in — blank means no tile for that app. Kept to a hostname
  // only (name = the app, group = "Own"); the Tiles form on this same tab
  // is where it is refined afterward.
  const tileArea = h("div");
  /** @type {Map<string, HTMLInputElement>} */
  const tileInputs = new Map();
  const renderTiles = () => {
    const preset = e.presets.find((p) => p.name === sel.value);
    tileInputs.clear();
    if (!preset || !preset.apps.length) {
      tileArea.replaceChildren();
      return;
    }
    tileArea.replaceChildren(
      h("p", { class: "kp-field__label" }, "Tiles (optional, blank = none)"),
      ...preset.apps.map((app) => {
        const id = `add-app-tile-${app}`;
        const input = /** @type {HTMLInputElement} */ (
          h("input", {
            class: "kp-field__input",
            type: "text",
            id,
            placeholder: `${app}.example.org`,
          })
        );
        tileInputs.set(app, input);
        return h(
          "div",
          { class: "kp-field" },
          h("label", { class: "kp-field__label", for: id }, `${app} hostname`),
          input,
        );
      }),
    );
  };
  sel.addEventListener("change", renderTiles);
  renderTiles();
  const review = h(
    "button",
    { type: "button", class: "kp-button", id: "add-app-review" },
    "Review and commit…",
  );
  review.addEventListener("click", () => {
    /** @type {Record<string, string>} */
    const tiles = {};
    for (const [app, input] of tileInputs.entries()) {
      const hostname = input.value.trim();
      if (hostname) tiles[app] = hostname;
    }
    const edit = addAppBody(sel.value, tiles);
    openPlanDialog({
      id: `edit:add-app:${stack}`,
      title: `Add ${sel.value} · ${stack}`,
      planUrl: `/data/stacks/${encodeURIComponent(stack)}/plan`,
      planBody: { edit },
      commitUrl: `/data/stacks/${encodeURIComponent(stack)}/commit`,
      commitBody: stackCommit(edit),
      onCommitted: () => void reload(),
    });
  });
  return h(
    "section",
    { class: "kp-card edit-card", "aria-label": "Add an app", id: "add-app" },
    h("h2", null, "Add an app"),
    h(
      "p",
      { class: "measured" },
      "A preset's app joins this stack: its files, its entry in apps and its /appdata folder.",
    ),
    h(
      "div",
      { class: "kp-field" },
      h("label", { class: "kp-field__label", for: "add-app-preset" }, "Preset"),
      sel,
    ),
    tileArea,
    h("div", { class: "row-buttons" }, review),
  );
}

// ── feat-publish-1 (B): publish an app through the gateway ──────────────

/**
 * The Apps tab's "Publish" dialog: a hostname and the container port
 * become `gateway_route` (or `extra_routes`, forced with "keep in a file
 * of its own") plus the router/service/loadBalancer fragment in
 * `traefik-routes.yml`, and optionally a tile for the same hostname — all
 * one `StackEdit::PublishApp` commit, through the same plan-then-commit
 * dialog every other form uses.
 * @param {string} stack
 * @param {string} app
 * @param {() => void} onCommitted
 */
export function openPublishDialog(stack, app, onCommitted) {
  /**
   * @param {string} id @param {string} label @param {HTMLElement} input
   * @param {string} [help]
   */
  const field = (id, label, input, help) =>
    h(
      "div",
      { class: "kp-field" },
      h("label", { class: "kp-field__label", for: id }, label),
      input,
      help ? h("p", { class: "kp-field__help" }, help) : "",
    );
  /** @param {string} id @param {string} label @param {HTMLElement} input */
  const check = (id, label, input) =>
    h(
      "div",
      { class: "kp-field kp-field--check" },
      input,
      h("label", { class: "kp-field__label", for: id }, label),
    );
  const hostname = /** @type {HTMLInputElement} */ (
    h("input", {
      class: "kp-field__input",
      type: "text",
      id: "publish-hostname",
      placeholder: `${app}.example.org`,
    })
  );
  const port = /** @type {HTMLInputElement} */ (
    h("input", {
      class: "kp-field__input",
      type: "number",
      id: "publish-port",
      min: "1",
      max: "65535",
    })
  );
  const external = /** @type {HTMLInputElement} */ (
    h("input", { type: "checkbox", id: "publish-external" })
  );
  const separateFile = /** @type {HTMLInputElement} */ (
    h("input", { type: "checkbox", id: "publish-separate-file" })
  );
  const createTile = /** @type {HTMLInputElement} */ (
    h("input", { type: "checkbox", id: "publish-create-tile" })
  );
  const tileName = /** @type {HTMLInputElement} */ (
    h("input", {
      class: "kp-field__input",
      type: "text",
      id: "publish-tile-name",
      placeholder: app,
    })
  );
  const tileGroup = /** @type {HTMLInputElement} */ (
    h("input", {
      class: "kp-field__input",
      type: "text",
      id: "publish-tile-group",
      placeholder: "Own",
    })
  );
  const errorBoxEl = h("div");
  const cancel = h(
    "button",
    { type: "button", class: "kp-button kp-button--ghost" },
    "Cancel",
  );
  const save = h(
    "button",
    { type: "button", class: "kp-button kp-button--primary" },
    "Review and commit…",
  );
  const d = openDialog({
    title: `Publish ${app} · ${stack}`,
    body: [
      field("publish-hostname", "Hostname", hostname, "Where this app opens."),
      field(
        "publish-port",
        "Container port",
        port,
        "The port the app's own container answers on.",
      ),
      check(
        "publish-external",
        "This backend is not one of the fleet's own managed stacks",
        external,
      ),
      check(
        "publish-separate-file",
        "Keep this route in a file of its own (extra_routes) instead of adding to traefik-routes.yml",
        separateFile,
      ),
      h("h3", { class: "kp-dialog__subtitle" }, "Tile (optional)"),
      check(
        "publish-create-tile",
        "Also create a tile for this hostname",
        createTile,
      ),
      field(
        "publish-tile-name",
        "Name",
        tileName,
        "Blank: the app's own name.",
      ),
      field("publish-tile-group", "Group", tileGroup, 'Blank: "Own".'),
      errorBoxEl,
      h("div", { class: "kp-dialog__actions" }, cancel, save),
    ],
    id: "publish-dialog",
  });
  // feat-platform-10: `ui open publish <stack>/<app>` drives this dialog
  // directly (it is opened by a click, not by a page already showing it),
  // the same shape `openNewStack`/`openPresetEditor` register under.
  const unregister = register("publish", {
    dialog: d.dialog,
    review: () => save,
    close: () => d.close(),
  });
  d.closed.then(unregister);
  cancel.addEventListener("click", () => d.close());
  save.addEventListener("click", () => {
    // Live view: the fields are kept on the dashboard's server.
    if (driven()) return;
    const host = hostname.value.trim();
    const p = Number(port.value.trim());
    if (!host || !Number.isInteger(p) || p < 1 || p > 65535) {
      errorBoxEl.replaceChildren(
        refusalCallout(
          {
            what: "the publish form",
            why: "a hostname and a port from 1 to 65535 are both needed",
            fix: "",
          },
          "warning",
          "Not yet",
        ),
      );
      return;
    }
    /** @type {Record<string, unknown>} */
    const edit = {
      kind: "publish_app",
      app,
      hostname: host,
      port: p,
      external: external.checked,
      separate_file: separateFile.checked,
    };
    if (createTile.checked) {
      edit.tile = {
        name: tileName.value.trim() || app,
        group: tileGroup.value.trim() || "Own",
      };
    }
    d.close();
    openPlanDialog({
      id: `edit:publish-app:${stack}:${app}`,
      title: `Publish ${app} · ${stack}`,
      planUrl: `/data/stacks/${encodeURIComponent(stack)}/plan`,
      planBody: { edit },
      commitUrl: `/data/stacks/${encodeURIComponent(stack)}/commit`,
      commitBody: stackCommit(edit),
      onCommitted: () => onCommitted(),
    });
  });
}

// ── feat-stacks-9: network, lxc flags, storage, on_demand, retention ────

/**
 * @param {string} stack
 * @param {EditRead} e
 * @param {() => Promise<void>} reload
 */
function settingsExtCard(stack, e, reload) {
  const m = /** @type {import("./editforms.js").ManifestView} */ (e.manifest);
  const form = settingsExtForm(stack, m);
  const values = startValues(form);
  /** @type {Map<string, HTMLElement>} */
  const inputs = new Map();
  const fields = form.steps[0].fields.map((f) => {
    const x = editField(f, values[f.name], (v) => {
      values[f.name] = v;
      repaintStatus();
    });
    inputs.set(f.name, x.input);
    return x.wrap;
  });
  const startRetention = rowModel(m.retention ?? []);
  const retention = rowTable({
    remember: "stack-retention",
    caption: `Retention of ${stack}`,
    nothing: "No tiers of its own yet; the fleet default applies.",
    columns: [
      { label: "Every", sort: "text" },
      { label: "Kept for", sort: "text" },
    ],
    toCells: (row) => [
      td(`${row.every_days}d`),
      td(row.span_days != null ? `${row.span_days}d` : "forever"),
    ],
    summary: retentionRowSummary,
    fieldsFor: retentionRowFields,
    fromValues: retentionRowFromValues,
    problems: retentionRowProblems,
    addLabel: "Add a tier…",
    driveKey: `row-table:retention:${stack}`,
    rows: startRetention,
    onChange: () => repaintStatus(),
  });
  const status = h("span", {
    class: "measured state-word",
    "data-size": "Changed; not committed.",
    role: "status",
  });
  const repaintStatus = () => {
    const changed =
      changesSomething(settingsExtBody(form, values, null)) ||
      rowsChanged(retention.getRows(), startRetention);
    status.textContent = changed ? "Changed; not committed." : "";
  };
  const review = h(
    "button",
    { type: "button", class: "kp-button", id: "settings-ext-review" },
    "Review and commit…",
  );
  review.addEventListener("click", () => {
    const errors = checkFields(form, values);
    if (!markErrors(inputs, errors)) return;
    const edit = settingsExtBody(
      form,
      values,
      rowsChanged(retention.getRows(), startRetention)
        ? retention.getRows()
        : null,
    );
    if (!changesSomething(edit)) {
      notify("Nothing is changed yet.", "info");
      return;
    }
    openPlanDialog({
      id: form.id,
      title: `Network & hardware · ${stack}`,
      planUrl: `/data/stacks/${encodeURIComponent(stack)}/plan`,
      planBody: { edit },
      commitUrl: `/data/stacks/${encodeURIComponent(stack)}/commit`,
      commitBody: stackCommit(edit),
      onCommitted: () => void reload(),
    });
  });
  const node = h(
    "section",
    {
      class: "kp-card edit-card",
      "aria-label": "Network & hardware",
      id: "settings-ext-form",
      "data-form": form.id,
    },
    h("h2", null, "Network & hardware"),
    h(
      "p",
      { class: "measured" },
      "The address, the lxc flags and the storage a rebuild would use, and this stack's own snapshot retention. Address/vmid clashes with another stack are caught at Review. Unprivileged, gpu, vpn and Proxmox storage only ever apply at a rebuild of the container.",
    ),
    h("div", { class: "edit-grid" }, ...fields),
    h("h3", null, "retention:"),
    ...retention.wrap,
    h("div", { class: "row-buttons" }, review, " ", status),
  );
  return {
    node,
    bind: () => retention.bind(),
    detach: () => retention.detach(),
  };
}

// ── feat-stacks-10 / feat-stacks-11: the row tables behind storage,
// data_mounts, log_files and latch_files ─────────────────────────────────
//
// One kp datatable + an Add/Edit dialog per list, the Firewall tab's rule
// table generalised to any row shape: `origin` (the old row's index, or
// null for a new row) rides on every row exactly the way `FirewallEdit`'s
// `RuleEdit.origin` does, so `rowBody` below is `firewallBody`'s
// `r.origin, rule: cleanRule(r.rule)` with the nesting dropped (the server
// structs are flat).

/**
 * @typedef {{wrap: Node[], add: HTMLElement,
 *   bind: () => void, getRows: () => import("./editforms.js").RowEdit[],
 *   setRows: (rows: import("./editforms.js").RowEdit[]) => void,
 *   openDialogFor: (i: number | null) => void, detach: () => void}} RowTable
 */

/**
 * The dialog behind a row table's Add and Edit.
 * @param {{title: string, fields: import("./editforms.js").EditField[],
 *   saveLabel: string,
 *   fromValues: (v: import("./editforms.js").Values) => Record<string, unknown>,
 *   problems?: (v: import("./editforms.js").Values) => Record<string, string>,
 *   datalists?: Record<string, string[]>,
 *   done: (row: Record<string, unknown>) => void}} opts
 */
function openRowDialog({
  title,
  fields,
  saveLabel,
  fromValues,
  problems,
  datalists,
  done,
}) {
  const steps = [{ id: "row", label: "Row", fields }];
  const values = startValues({ steps });
  /** @type {Map<string, HTMLElement>} */
  const inputs = new Map();
  const errBox = h("div");
  const wraps = fields.map((f) => {
    const x = editField(f, values[f.name], (v) => (values[f.name] = v));
    inputs.set(f.name, x.input);
    // A picker that also takes free text (e.g. a tile's group): a
    // <datalist> of what is already used, offered alongside typing.
    const options = datalists?.[f.name];
    if (options?.length && x.input instanceof HTMLInputElement) {
      const listId = `${f.id}-list`;
      x.input.setAttribute("list", listId);
      x.wrap.append(
        h(
          "datalist",
          { id: listId },
          ...options.map((/** @type {string} */ o) =>
            h("option", { value: o }),
          ),
        ),
      );
    }
    return x.wrap;
  });
  const save = h(
    "button",
    { type: "button", class: "kp-button kp-button--primary", id: "row-save" },
    saveLabel,
  );
  const cancel = h(
    "button",
    { type: "button", class: "kp-button", "data-kp-dialog-close": "" },
    "Cancel",
  );
  const d = openDialog({
    title,
    description: "Nothing is written until the plan is reviewed and committed.",
    body: [
      h("div", { class: "edit-grid" }, ...wraps),
      errBox,
      h("div", { class: "kp-dialog__actions" }, cancel, save),
    ],
    id: "row-dialog",
  });
  const unregister = register("row", {
    dialog: d.dialog,
    save: () => save,
    cancel: () => cancel,
    close: () => d.close(),
  });
  d.closed.then(unregister);
  cancel.addEventListener("click", () => d.close());
  save.addEventListener("click", () => {
    // Live view: the row is kept on the dashboard's server.
    if (driven()) return;
    const errors = {
      ...checkFields({ steps }, values),
      ...(problems ? problems(values) : {}),
    };
    if (!markErrors(inputs, errors)) {
      errBox.replaceChildren(
        refusalCallout(
          { what: "the row", why: Object.values(errors).join(" "), fix: "" },
          "warning",
          "Not yet",
        ),
      );
      return;
    }
    done(fromValues(values));
    d.close();
  });
}

/**
 * @param {{remember: string, caption: string, nothing: string,
 *   columns: import("./dom.js").Column[],
 *   toCells: (row: Record<string, unknown>) => (Node | string)[],
 *   summary: (row: Record<string, unknown>) => string,
 *   fieldsFor: (row: Record<string, unknown> | null) => import("./editforms.js").EditField[],
 *   fromValues: (v: import("./editforms.js").Values) => Record<string, unknown>,
 *   problems?: (v: import("./editforms.js").Values) => Record<string, string>,
 *   datalists?: Record<string, string[]>,
 *   addLabel: string, driveKey: string,
 *   rows: import("./editforms.js").RowEdit[], onChange: () => void}} opts
 * @returns {RowTable}
 */
function rowTable({
  remember,
  caption,
  nothing,
  columns,
  toCells,
  summary,
  fieldsFor,
  fromValues,
  problems,
  datalists,
  addLabel,
  driveKey,
  rows: initialRows,
  onChange,
}) {
  let rows = initialRows;
  /** @type {ReturnType<typeof dataTable> | null} */
  let table = null;
  const t = tableBlock({
    remember,
    caption,
    search: "Search rows",
    nothing,
    columns: [...columns, { label: "Change", sort: "text" }],
  });
  const paint = () => {
    t.tbody.replaceChildren(
      ...rows.map((r, i) => {
        const btn = (
          /** @type {string} */ label,
          /** @type {string} */ act,
          disabled = false,
        ) => {
          const b = h(
            "button",
            {
              type: "button",
              class: "kp-button kp-button--sm kp-button--ghost",
              "data-act": act,
              "data-i": String(i),
              "aria-label": `${label} row ${i + 1}: ${summary(r.row)}`,
            },
            label,
          );
          if (disabled) b.disabled = true;
          return b;
        };
        return h(
          "tr",
          {
            "data-kp-row-key": `${i}`,
            class: r.origin === null ? "rule-new" : "",
          },
          ...toCells(r.row),
          h(
            "td",
            { class: "row-buttons" },
            btn("Edit", "edit"),
            btn("Up", "up", i === 0),
            btn("Down", "down", i === rows.length - 1),
            btn("Delete", "delete"),
          ),
        );
      }),
    );
    table?.refresh();
  };
  const setRows = (/** @type {import("./editforms.js").RowEdit[]} */ next) => {
    rows = next;
    paint();
    onChange();
  };
  const openDialogFor = (/** @type {number | null} */ i) => {
    const row = i == null ? null : rows[i].row;
    openRowDialog({
      title: i == null ? addLabel : `Edit · ${summary(row ?? {})}`,
      fields: fieldsFor(row),
      saveLabel: i == null ? "Add the row" : "Change the row",
      fromValues,
      problems,
      datalists,
      done: (newRow) => {
        if (i == null) setRows([...rows, { origin: null, row: newRow }]);
        else setRows(rows.map((r, j) => (j === i ? { ...r, row: newRow } : r)));
      },
    });
  };
  t.tbody.addEventListener("click", (ev) => {
    const b = /** @type {Element} */ (ev.target).closest("button[data-act]");
    if (!(b instanceof HTMLButtonElement)) return;
    const i = Number(b.dataset.i);
    switch (b.dataset.act) {
      case "up":
      case "down": {
        const j = i + (b.dataset.act === "up" ? -1 : 1);
        if (j < 0 || j >= rows.length) return;
        const next = [...rows];
        [next[i], next[j]] = [next[j], next[i]];
        setRows(next);
        break;
      }
      case "delete":
        setRows(rows.filter((_, j) => j !== i));
        break;
      case "edit":
        openDialogFor(i);
        break;
    }
  });
  const add = h(
    "button",
    { type: "button", class: "kp-button", id: `${driveKey}-add` },
    addLabel,
  );
  add.addEventListener("click", () => openDialogFor(null));
  paint();
  // feat-platform-10: Live view's row op and the row dialog it opens.
  const detach = register(driveKey, {
    add: () => add,
    rowButton: (/** @type {string} */ op, /** @type {number} */ i) =>
      /** @type {HTMLElement | null} */ (
        t.tbody.querySelector(`button[data-act="${op}"][data-i="${i}"]`)
      ),
    openDialogFor,
  });
  return {
    wrap: [h("div", { class: "row-buttons" }, add), t.wrap],
    add,
    bind: () => {
      table = dataTable(t.wrap);
      table?.refresh();
    },
    getRows: () => rows,
    setRows,
    openDialogFor,
    detach,
  };
}

// ── feat-stacks-10: apps & storage ───────────────────────────────────────

/**
 * @param {string} stack
 * @param {EditRead} e
 * @param {() => Promise<void>} reload
 */
function appsEditCard(stack, e, reload) {
  const m = /** @type {import("./editforms.js").ManifestView} */ (e.manifest);
  const apps = m.apps ?? [];
  /** @type {Map<string, HTMLInputElement>} */
  const removeBoxes = new Map();
  const removeList = h(
    "div",
    { class: "edit-grid" },
    ...apps.map((a) => {
      const x = editField(appsRemoveField(a), false, () => {});
      removeBoxes.set(a, /** @type {HTMLInputElement} */ (x.input));
      return x.wrap;
    }),
  );
  const addBlankField = appsAddBlankField();
  let addBlankText = "";
  const addBlankX = editField(addBlankField, "", (v) => {
    addBlankText = String(v);
  });

  const startStorage = rowModel(m.storage ?? []);
  const storage = rowTable({
    remember: "stack-storage",
    caption: `Storage of ${stack}`,
    nothing: "No storage entries yet.",
    columns: [
      { label: "Host path", sort: "text" },
      { label: "Mount point", sort: "text" },
      { label: "App", sort: "text", filter: "choice" },
      { label: "No data", sort: "text" },
      { label: "No backup", sort: "text" },
    ],
    toCells: (row) => [
      td(String(row.host_path ?? ""), "mono"),
      td(String(row.mount_point ?? ""), "mono"),
      td(row.app ? String(row.app) : "(the stack)"),
      td(row.no_data ? "yes" : "—"),
      td(row.no_backup ? String(row.no_backup) : "—"),
    ],
    summary: storageSummary,
    fieldsFor: (row) => storageFields(m, row),
    fromValues: storageFromValues,
    addLabel: "Add a storage entry…",
    driveKey: `row-table:storage:${stack}`,
    rows: startStorage,
    onChange: () => repaintStatus(),
  });
  const startDataMounts = rowModel(m.data_mounts ?? []);
  const dataMounts = rowTable({
    remember: "stack-data-mounts",
    caption: `Data mounts of ${stack}`,
    nothing: "No data mounts yet.",
    columns: [
      { label: "Host path", sort: "text" },
      { label: "Mount point", sort: "text" },
      { label: "Note", sort: "text" },
      { label: "Rotate", sort: "text" },
    ],
    toCells: (row) => {
      const rotate =
        /** @type {{files?: string, keep?: number} | undefined} */ (row.rotate);
      return [
        td(String(row.host_path ?? ""), "mono"),
        td(String(row.mount_point ?? ""), "mono"),
        td(row.note ? String(row.note) : "—"),
        td(rotate ? `${rotate.files} (keep ${rotate.keep ?? 14})` : "—"),
      ];
    },
    summary: dataMountSummary,
    fieldsFor: dataMountFields,
    fromValues: dataMountFromValues,
    problems: dataMountProblems,
    addLabel: "Add a data mount…",
    driveKey: `row-table:data_mounts:${stack}`,
    rows: startDataMounts,
    onChange: () => repaintStatus(),
  });
  const startLogFiles = rowModel(m.log_files ?? []);
  const logFiles = rowTable({
    remember: "stack-log-files",
    caption: `Log files of ${stack}`,
    nothing:
      "No extra log files yet (docker's and the journal's ship regardless).",
    columns: [
      { label: "Path", sort: "text" },
      { label: "Job", sort: "text", filter: "choice" },
    ],
    toCells: (row) => [
      td(String(row.path ?? ""), "mono"),
      td(String(row.job ?? "")),
    ],
    summary: logFileSummary,
    fieldsFor: logFileFields,
    fromValues: logFileFromValues,
    addLabel: "Add a log file…",
    driveKey: `row-table:log_files:${stack}`,
    rows: startLogFiles,
    onChange: () => repaintStatus(),
  });

  const status = h("span", {
    class: "measured state-word",
    "data-size": "Changed; not committed.",
    role: "status",
  });
  const repaintStatus = () => {
    const changed =
      [...removeBoxes.values()].some((b) => b.checked) ||
      addBlankText.trim() !== "" ||
      rowsChanged(storage.getRows(), startStorage) ||
      rowsChanged(dataMounts.getRows(), startDataMounts) ||
      rowsChanged(logFiles.getRows(), startLogFiles);
    status.textContent = changed ? "Changed; not committed." : "";
  };
  addBlankX.input.addEventListener("input", repaintStatus);

  const review = h(
    "button",
    { type: "button", class: "kp-button", id: "apps-review" },
    "Review and commit…",
  );
  review.addEventListener("click", () => {
    const remove = [...removeBoxes.entries()]
      .filter(([, box]) => box.checked)
      .map(([a]) => a);
    const addBlank = addBlankText
      .split(",")
      .map((s) => s.trim())
      .filter((s) => s !== "");
    const edit = appsBody({
      remove,
      addBlank,
      storage: rowsChanged(storage.getRows(), startStorage)
        ? storage.getRows()
        : null,
      dataMounts: rowsChanged(dataMounts.getRows(), startDataMounts)
        ? dataMounts.getRows()
        : null,
      logFiles: rowsChanged(logFiles.getRows(), startLogFiles)
        ? logFiles.getRows()
        : null,
    });
    if (!changesSomething(edit)) {
      notify("Nothing is changed yet.", "info");
      return;
    }
    openPlanDialog({
      id: `edit:apps:${stack}`,
      title: `Apps & storage · ${stack}`,
      planUrl: `/data/stacks/${encodeURIComponent(stack)}/plan`,
      planBody: { edit },
      commitUrl: `/data/stacks/${encodeURIComponent(stack)}/commit`,
      commitBody: stackCommit(edit),
      onCommitted: () => void reload(),
    });
  });
  const node = h(
    "section",
    {
      class: "kp-card edit-card",
      "aria-label": "Apps & storage",
      id: "apps-form",
    },
    h("h2", null, "Apps & storage"),
    h(
      "p",
      { class: "measured" },
      "Removing an app drops it from apps:, its directory and its storage entries — data already on the container is untouched. A blank app gets a directory and a minimal docker-compose.yml to fill in.",
    ),
    apps.length
      ? h(
          "div",
          { class: "kp-field" },
          h("span", { class: "kp-field__label" }, "Remove an app"),
          removeList,
        )
      : h("p", { class: "measured" }, "This stack has no apps yet."),
    addBlankX.wrap,
    h("h3", null, "storage:"),
    ...storage.wrap,
    h("h3", null, "data_mounts:"),
    ...dataMounts.wrap,
    h("h3", null, "log_files:"),
    ...logFiles.wrap,
    h("div", { class: "row-buttons" }, review, " ", status),
  );
  return {
    node,
    bind: () => {
      storage.bind();
      dataMounts.bind();
      logFiles.bind();
    },
    detach: () => {
      storage.detach();
      dataMounts.detach();
      logFiles.detach();
    },
  };
}

// ── feat-stacks-11: latch_secrets and latch_files ────────────────────────

/**
 * @param {string} stack
 * @param {EditRead} e
 * @param {() => Promise<void>} reload
 */
function latchEditCard(stack, e, reload) {
  const m = /** @type {import("./editforms.js").ManifestView} */ (e.manifest);
  const apps = m.apps ?? [];
  const latch = m.latch ?? { latch_secrets: [], latch_files: [] };
  /** @type {Map<string, HTMLInputElement>} */
  const secretBoxes = new Map();
  const secretsList = h(
    "div",
    { class: "edit-grid" },
    ...apps.map((a) => {
      const x = editField(
        latchSecretField(a),
        latch.latch_secrets.includes(a),
        () => repaintStatus(),
      );
      secretBoxes.set(a, /** @type {HTMLInputElement} */ (x.input));
      return x.wrap;
    }),
  );
  const startFiles = rowModel(latch.latch_files);
  const files = rowTable({
    remember: "stack-latch-files",
    caption: `latch_files of ${stack}`,
    nothing: "No latch_files yet.",
    columns: [
      { label: "From", sort: "text" },
      { label: "Destination", sort: "text" },
      { label: "Mode", sort: "text" },
      { label: "Owner", sort: "text" },
      { label: "Restarts", sort: "text", filter: "choice" },
    ],
    toCells: (row) => [
      td(String(row.from ?? ""), "mono"),
      td(String(row.dest ?? ""), "mono"),
      td(String(row.mode ?? ""), "mono"),
      td(row.owner ? String(row.owner) : "—"),
      td(row.restarts ? String(row.restarts) : "—"),
    ],
    summary: latchFileSummary,
    fieldsFor: (row) => latchFileFields(m, row),
    fromValues: latchFileFromValues,
    problems: latchFileProblems,
    addLabel: "Add a latch file…",
    driveKey: `row-table:latch_files:${stack}`,
    rows: startFiles,
    onChange: () => repaintStatus(),
  });

  const status = h("span", {
    class: "measured state-word",
    "data-size": "Changed; not committed.",
    role: "status",
  });
  const repaintStatus = () => {
    const wantSecrets = apps.filter((a) => secretBoxes.get(a)?.checked);
    const changed =
      JSON.stringify([...wantSecrets].sort()) !==
        JSON.stringify([...latch.latch_secrets].sort()) ||
      rowsChanged(files.getRows(), startFiles);
    status.textContent = changed ? "Changed; not committed." : "";
  };

  const review = h(
    "button",
    { type: "button", class: "kp-button", id: "latch-review" },
    "Review and commit…",
  );
  review.addEventListener("click", () => {
    const wantSecrets = apps.filter((a) => secretBoxes.get(a)?.checked);
    const secretErrors = latchSecretsProblems(wantSecrets);
    if (Object.keys(secretErrors).length) {
      notify(Object.values(secretErrors)[0], "warning");
      return;
    }
    const secretsChanged =
      JSON.stringify([...wantSecrets].sort()) !==
      JSON.stringify([...latch.latch_secrets].sort());
    const edit = latchBody({
      secrets: secretsChanged ? wantSecrets : null,
      files: rowsChanged(files.getRows(), startFiles) ? files.getRows() : null,
    });
    if (!changesSomething(edit)) {
      notify("Nothing is changed yet.", "info");
      return;
    }
    openPlanDialog({
      id: `edit:latch:${stack}`,
      title: `Latch · ${stack}`,
      planUrl: `/data/stacks/${encodeURIComponent(stack)}/plan`,
      planBody: { edit },
      commitUrl: `/data/stacks/${encodeURIComponent(stack)}/commit`,
      commitBody: stackCommit(edit),
      onCommitted: () => void reload(),
    });
  });
  const node = h(
    "section",
    { class: "kp-card edit-card", "aria-label": "Latch", id: "latch-form" },
    h("h2", null, "Latch"),
    h(
      "p",
      { class: "measured" },
      "Which apps' .env comes from latch, and which secret FILES this stack gets from latch. A value must never contain '${': latch --expand parses every file it is given, so one unresolvable placeholder would break every stack's secrets.",
    ),
    apps.length
      ? h(
          "div",
          { class: "kp-field" },
          h("span", { class: "kp-field__label" }, "latch_secrets"),
          secretsList,
        )
      : h("p", { class: "measured" }, "This stack has no apps yet."),
    h("h3", null, "latch_files:"),
    ...files.wrap,
    h("div", { class: "row-buttons" }, review, " ", status),
  );
  return {
    node,
    bind: () => files.bind(),
    detach: () => files.detach(),
  };
}

// ── feat-tiles-1 (B): the stack's tiles: map, create/edit/rename/delete ──

/** @param {string} id @param {string} label @param {HTMLElement} input */
function labeledField(id, label, input) {
  return h(
    "div",
    { class: "kp-field" },
    h("label", { class: "kp-field__label", for: id }, label),
    input,
  );
}

/**
 * @param {string} stack
 * @param {EditRead} e
 * @param {() => Promise<void>} reload
 */
function tilesEditCard(stack, e, reload) {
  const m = /** @type {import("./editforms.js").ManifestView} */ (e.manifest);
  const tiles = m.tiles ?? {};
  const groups = [...new Set(Object.values(tiles).map((t) => t.group))].sort();
  const originals = /** @type {Record<string, Record<string, unknown>>} */ (
    tiles
  );
  const startRows = Object.entries(tiles).map(([key, t]) => ({
    origin: key,
    row: /** @type {Record<string, unknown>} */ ({ key, ...t }),
  }));
  const status = h("span", {
    class: "measured state-word",
    "data-size": "Changed; not committed.",
    role: "status",
  });
  const table = rowTable({
    remember: "stack-tiles",
    caption: `Tiles of ${stack}`,
    nothing: "No tiles yet.",
    columns: [
      { label: "Hostname", sort: "text" },
      { label: "Name", sort: "text" },
      { label: "Group", sort: "text", filter: "choice" },
      { label: "Order", sort: "number" },
    ],
    toCells: (row) => [
      td(String(row.key ?? ""), "mono"),
      td(String(row.name ?? "")),
      td(String(row.group ?? "")),
      td(row.order != null ? String(row.order) : "100"),
    ],
    summary: tileRowSummary,
    fieldsFor: (row) => tileRowFields(groups, row),
    fromValues: tileRowFromValues,
    problems: tileRowProblems,
    datalists: { group: groups },
    addLabel: "Add a tile…",
    driveKey: `row-table:tiles:${stack}`,
    rows: startRows,
    onChange: () => repaintStatus(),
  });
  const repaintStatus = () => {
    const changed = tilesEditBody(table.getRows(), originals).tiles.length > 0;
    status.textContent = changed ? "Changed; not committed." : "";
  };
  const review = h(
    "button",
    { type: "button", class: "kp-button", id: "tiles-review" },
    "Review and commit…",
  );
  review.addEventListener("click", () => {
    const edit = tilesEditBody(table.getRows(), originals);
    if (edit.tiles.length === 0) {
      notify("Nothing is changed yet.", "info");
      return;
    }
    openPlanDialog({
      id: `edit:tiles:${stack}`,
      title: `Tiles · ${stack}`,
      planUrl: `/data/stacks/${encodeURIComponent(stack)}/plan`,
      planBody: { edit },
      commitUrl: `/data/stacks/${encodeURIComponent(stack)}/commit`,
      commitBody: stackCommit(edit),
      onCommitted: () => void reload(),
    });
  });
  const node = h(
    "section",
    { class: "kp-card edit-card", "aria-label": "Tiles", id: "tiles-form" },
    h("h2", null, "Tiles"),
    h(
      "p",
      { class: "measured" },
      "This stack's tiles on the dashboard's start page, keyed by the hostname each one opens. Renaming a tile's hostname keeps it; \"probe\" is never set here — the client fills it in at deploy time.",
    ),
    ...table.wrap,
    h("div", { class: "row-buttons" }, review, " ", status),
  );
  return {
    node,
    bind: () => table.bind(),
    detach: () => table.detach(),
  };
}

// ── feat-checks-1 (B): checks.yml, editable per app ──────────────────────

/**
 * The Checks tab's edit section: one app's whole `checks.yml`, picked from
 * a dropdown — the file sits beside its app, not the stack as a whole, so
 * this is its own top-level tab mount (like `firewallTab`/`settingsTab`)
 * rather than another card of the Settings tab.
 * @param {HTMLElement} panel
 * @param {{name: string}} params
 * @returns {() => void}
 */
export function checksEditTab(panel, params) {
  const stack = params.name;
  const abort = new AbortController();
  panel.replaceChildren(
    h("p", { class: "measured" }, "Reading the stack's files…"),
  );
  /** @type {() => void} */
  let detach = () => {};
  const load = async () => {
    const r = await readEdit(stack, abort.signal);
    if (!r.ok) {
      panel.replaceChildren(errorBox(r.error));
      return;
    }
    const e = /** @type {EditRead} */ (r.body);
    const apps = e.manifest?.apps ?? [];
    if (!e.manifest) {
      panel.replaceChildren(
        h(
          "div",
          { class: "kp-alert kp-alert--warning", role: "status" },
          `The checks form needs a readable lxc-compose.yml: ${e.manifest_error ?? "none"}.`,
        ),
      );
      return;
    }
    if (!apps.length) {
      panel.replaceChildren(
        h("p", { class: "measured" }, "This stack has no apps yet."),
      );
      return;
    }
    detach();
    const card = checksEditCard(stack, e, apps, load);
    panel.replaceChildren(card.node);
    card.bind();
    detach = card.detach;
  };
  void load().catch(() => {});
  return () => {
    abort.abort();
    detach();
  };
}

/**
 * @param {string} stack
 * @param {EditRead} e
 * @param {string[]} apps
 * @param {() => Promise<void>} reload
 */
function checksEditCard(stack, e, apps, reload) {
  const checksByApp = e.checks ?? {};
  const select = h(
    "select",
    { class: "kp-field__input", id: "checks-app" },
    ...apps.map((a) => h("option", { value: a }, a)),
  );
  const body = h("div");
  const busyField = /** @type {HTMLInputElement} */ (
    h("input", {
      class: "kp-field__input",
      type: "text",
      id: "checks-busy",
      placeholder: "shell command run in the container, blank = none",
    })
  );
  const urlField = /** @type {HTMLInputElement} */ (
    h("input", {
      class: "kp-field__input",
      type: "text",
      id: "checks-url",
      placeholder: "https://…, blank = derived from the route file",
    })
  );
  const status = h("span", {
    class: "measured state-word",
    "data-size": "Changed; not committed.",
    role: "status",
  });
  /** @type {{checksTable?: RowTable, manualTable?: RowTable, probesTable?: RowTable,
   *   startChecks?: import("./editforms.js").RowEdit[],
   *   startManual?: import("./editforms.js").RowEdit[],
   *   startProbes?: import("./editforms.js").RowEdit[],
   *   view?: import("./editforms.js").ChecksReadView}} */
  let current = {};
  /** @type {() => void} */
  let detachCurrent = () => {};
  const repaintStatus = () => {
    const v = current.view;
    if (
      !v ||
      !current.checksTable ||
      !current.manualTable ||
      !current.probesTable
    ) {
      status.textContent = "";
      return;
    }
    const changed =
      rowsChanged(current.checksTable.getRows(), current.startChecks ?? []) ||
      rowsChanged(current.manualTable.getRows(), current.startManual ?? []) ||
      rowsChanged(current.probesTable.getRows(), current.startProbes ?? []) ||
      busyField.value.trim() !== (v.busy_check?.command ?? "") ||
      urlField.value.trim() !== (v.url ?? "");
    status.textContent = changed ? "Changed; not committed." : "";
  };
  /** @param {string} app */
  const render = (app) => {
    detachCurrent();
    const view = checksByApp[app] ?? { checks: [], manual: [], probes: [] };
    if ("error" in view) {
      body.replaceChildren(
        h(
          "div",
          { class: "kp-alert kp-alert--warning", role: "status" },
          `${app}/checks.yml does not read: ${view.error}. Fix it in the raw editor first (Settings tab).`,
        ),
      );
      current = {};
      return;
    }
    const startChecks = rowModel(view.checks ?? []);
    const checksTable = rowTable({
      remember: "stack-checks",
      caption: `Checks of ${app}`,
      nothing: "No measured checks yet.",
      columns: [
        { label: "Name", sort: "text" },
        { label: "Command", sort: "text", cls: "mono" },
        { label: "Layer", sort: "text", filter: "choice" },
      ],
      toCells: (row) => [
        td(String(row.name ?? "")),
        td(String(row.command ?? ""), "mono"),
        td(String(row.layer ?? "")),
      ],
      summary: checkRowSummary,
      fieldsFor: checkRowFields,
      fromValues: checkRowFromValues,
      problems: checkRowProblems,
      addLabel: "Add a check…",
      driveKey: `row-table:checks:${stack}:${app}`,
      rows: startChecks,
      onChange: () => repaintStatus(),
    });
    const startManual = rowModel((view.manual ?? []).map(manualRow));
    const manualTable = rowTable({
      remember: "stack-manual",
      caption: `Manual questions of ${app}`,
      nothing: "No manual questions yet.",
      columns: [{ label: "Question", sort: "text", cls: "wide" }],
      toCells: (row) => [td(manualRowSummary(row))],
      summary: manualRowSummary,
      fieldsFor: manualRowFields,
      fromValues: manualRowFromValues,
      addLabel: "Add a question…",
      driveKey: `row-table:manual:${stack}:${app}`,
      rows: startManual,
      onChange: () => repaintStatus(),
    });
    const startProbes = rowModel(view.probes ?? []);
    const probesTable = rowTable({
      remember: "stack-probes",
      caption: `Probes of ${app}`,
      nothing: "No nightly probes yet.",
      columns: [
        { label: "Name", sort: "text" },
        { label: "Command", sort: "text", cls: "mono" },
        { label: "Healthy", sort: "text" },
        { label: "Layer", sort: "text", filter: "choice" },
      ],
      toCells: (row) => [
        td(String(row.name ?? "")),
        td(String(row.command ?? ""), "mono"),
        td(probeRowSummary(row).replace(/^.*\(/, "").replace(")", "")),
        td(String(row.layer ?? "")),
      ],
      summary: probeRowSummary,
      fieldsFor: probeRowFields,
      fromValues: probeRowFromValues,
      problems: probeRowProblems,
      addLabel: "Add a probe…",
      driveKey: `row-table:probes:${stack}:${app}`,
      rows: startProbes,
      onChange: () => repaintStatus(),
    });
    busyField.value = view.busy_check?.command ?? "";
    urlField.value = view.url ?? "";
    body.replaceChildren(
      h("h3", null, "checks:"),
      ...checksTable.wrap,
      h("h3", null, "manual:"),
      ...manualTable.wrap,
      h("h3", null, "probes:"),
      ...probesTable.wrap,
      labeledField("checks-busy", "Busy check command", busyField),
      labeledField("checks-url", "Link", urlField),
    );
    const detachTablesLocal = attachDataTables(body);
    checksTable.bind();
    manualTable.bind();
    probesTable.bind();
    current = {
      checksTable,
      manualTable,
      probesTable,
      startChecks,
      startManual,
      startProbes,
      view,
    };
    detachCurrent = () => {
      checksTable.detach();
      manualTable.detach();
      probesTable.detach();
      detachTablesLocal();
    };
    repaintStatus();
  };
  select.value = apps[0];
  select.addEventListener("change", () => render(select.value));
  busyField.addEventListener("input", repaintStatus);
  urlField.addEventListener("input", repaintStatus);
  const review = h(
    "button",
    { type: "button", class: "kp-button", id: "checks-review" },
    "Review and commit…",
  );
  review.addEventListener("click", () => {
    const app = select.value;
    const v = current.view;
    if (
      !v ||
      !current.checksTable ||
      !current.manualTable ||
      !current.probesTable
    ) {
      notify("Fix the raw file first, in the Settings tab.", "warning");
      return;
    }
    const edit = checksBody({
      app,
      checks: rowsChanged(
        current.checksTable.getRows(),
        current.startChecks ?? [],
      )
        ? current.checksTable.getRows()
        : null,
      manual: rowsChanged(
        current.manualTable.getRows(),
        current.startManual ?? [],
      )
        ? current.manualTable.getRows()
        : null,
      probes: rowsChanged(
        current.probesTable.getRows(),
        current.startProbes ?? [],
      )
        ? current.probesTable.getRows()
        : null,
      busyCheck: busyField.value,
      url: urlField.value,
    });
    const busyChanged =
      busyField.value.trim() !== (v.busy_check?.command ?? "");
    const urlChanged = urlField.value.trim() !== (v.url ?? "");
    if (
      !edit.checks &&
      !edit.manual &&
      !edit.probes &&
      !busyChanged &&
      !urlChanged
    ) {
      notify("Nothing is changed yet.", "info");
      return;
    }
    openPlanDialog({
      id: `edit:checks:${stack}:${app}`,
      title: `checks.yml · ${app} · ${stack}`,
      planUrl: `/data/stacks/${encodeURIComponent(stack)}/plan`,
      planBody: { edit },
      commitUrl: `/data/stacks/${encodeURIComponent(stack)}/commit`,
      commitBody: stackCommit(edit),
      onCommitted: () => void reload(),
    });
  });
  const node = h(
    "section",
    { class: "kp-card edit-card", "aria-label": "Checks", id: "checks-form" },
    h("h2", null, "Checks"),
    h(
      "p",
      { class: "measured" },
      'What "healthy" means for one app: measured checks, manual questions, nightly probes, the busy check and the link it opens at. An app with no checks.yml yet gets one created on Review and commit.',
    ),
    labeledField("checks-app", "App", select),
    body,
    h("div", { class: "row-buttons" }, review, " ", status),
  );
  // feat-platform-10: `ui open checks <stack>/<app>` picks the app (see
  // `pick("checks-app", app)` in driveedit.rs), then row ops address
  // `checks`/`manual`/`probes` the same way `stack-edit:<stack>` does for
  // apps/latch — `row-table:<list>:<stack>:<app>`, the SELECTED app.
  const unregister = register(`checks-edit:${stack}`, {
    review: () => review,
    row: (
      /** @type {string} */ op,
      /** @type {string | undefined} */ target,
    ) => {
      const [list, n] = String(target ?? "").split(":");
      const t = handle(`row-table:${list}:${stack}:${select.value}`);
      if (!t) return null;
      return op === "add" ? t.add() : t.rowButton(op, Number(n) - 1);
    },
    openRow: (/** @type {string} */ list, /** @type {number | null} */ i) =>
      handle(`row-table:${list}:${stack}:${select.value}`)?.openDialogFor(i),
  });
  return {
    node,
    bind: () => render(apps[0]),
    detach: () => {
      detachCurrent();
      unregister();
    },
  };
}

// ── feat-native-1 (D): native services ──────────────────────────────────

/**
 * A labelled text field, for the native form's plain (non-`editforms.js`)
 * fields — one `service.yml` key each, its own stable id for Live view.
 * @param {string} id
 * @param {string} label
 * @param {string} value
 * @param {{textarea?: boolean, help?: string, placeholder?: string}} [opts]
 */
function nativeField(id, label, value, opts = {}) {
  const input = opts.textarea
    ? h("textarea", {
        class: "kp-field__input mono",
        id,
        rows: "3",
        spellcheck: "false",
      })
    : h("input", {
        class: "kp-field__input mono",
        id,
        type: "text",
        placeholder: opts.placeholder ?? "",
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
 * A `<select>` field for a fixed picker (`update_policy`, `metrics`).
 * @param {string} id
 * @param {string} label
 * @param {{value: string, label: string}[]} options
 * @param {string} current
 */
function nativeChoice(id, label, options, current) {
  const input = h(
    "select",
    { class: "kp-field__input", id },
    ...options.map((o) => h("option", { value: o.value }, o.label)),
  );
  input.value = current;
  return {
    input,
    wrap: h(
      "div",
      { class: "kp-field" },
      h("label", { class: "kp-field__label", for: id }, label),
      input,
    ),
  };
}

/**
 * Every native unit's `service.yml` form: one unit picker, one set of
 * fixed-id fields below it (Live view parity: the field ids do not change
 * when the picked unit does).
 * @param {string} stack
 * @param {NativeView[]} natives
 * @param {() => Promise<void>} reload
 */
function nativeUnitForm(stack, natives, reload) {
  const sel = h(
    "select",
    { class: "kp-field__input", id: "native-unit" },
    ...natives.map((n) => h("option", { value: n.unit }, n.unit)),
  );
  const warn = h("div", {
    class: "kp-alert kp-alert--warning",
    role: "status",
    hidden: "",
  });
  const binary = nativeField("native-binary", "Binary", "");
  const envFile = nativeField("native-env-file", "Environment file", "", {
    help: "Empty: the unit has no EnvironmentFile.",
    placeholder: "/appdata/…/….env",
  });
  const dataDirs = nativeField(
    "native-data-dirs",
    "Data directories (one per line)",
    "",
    { textarea: true, help: "What the nightly backup archives." },
  );
  const updateCmd = nativeField("native-update-cmd", "Update command", "", {
    textarea: true,
    help: "Empty: the homelab runs no verb of its own for this service.",
  });
  const stateless = h("input", {
    type: "checkbox",
    id: "native-stateless",
    class: "kp-field__check",
  });
  const restoreNote = nativeField("native-restore-note", "Restore note", "", {
    textarea: true,
    help: "Printed in the DR runbook under this unit.",
  });
  const releaseRepo = nativeField("native-release-repo", "Release repo", "", {
    placeholder: "owner/name",
  });
  const releaseAsset = nativeField(
    "native-release-asset",
    "Release asset",
    "",
    { help: "Empty: the unit name." },
  );
  const backupNewest = nativeField(
    "native-backup-newest",
    "Backup from newest",
    "",
    { help: "An absolute glob with a '*', e.g. /appdata/…/backup-*.db." },
  );
  const backupPause = nativeChoice(
    "native-backup-pause",
    "Pause for the nightly backup",
    [
      { value: "false", label: "Off" },
      { value: "true", label: "Homelab stops the unit" },
      {
        value: "chassis",
        label: "Chassis kit (backup-pause, falls back to stop)",
      },
    ],
    "false",
  );
  const updatePolicy = nativeChoice(
    "native-update-policy",
    "Update policy",
    [
      { value: "manual", label: "Manual (the homelab never updates it)" },
      { value: "auto", label: "Auto (nightly release check)" },
      { value: "self", label: "Self (the unit's own update_cmd, nightly)" },
    ],
    "manual",
  );
  const metrics = nativeChoice(
    "native-metrics",
    "Metrics",
    [
      { value: "measured", label: "Measured (default)" },
      { value: "not_measured", label: "Not measured (noted, not drift)" },
    ],
    "measured",
  );
  const status = h("span", { class: "measured state-word", role: "status" });
  const review = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--primary",
      id: "native-review",
    },
    "Review and commit…",
  );
  const removeBtn = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--destructive",
      id: "native-remove",
    },
    "Remove this unit…",
  );
  const pathLine = h("p", { class: "measured" });
  const show = () => {
    const n = natives.find((x) => x.unit === sel.value) ?? natives[0];
    pathLine.textContent = n
      ? `${n.path} · unit file ${n.unit_file_path} (edited in the raw editor).`
      : "";
    const m = n?.manifest;
    warn.hidden = Boolean(m);
    warn.textContent = m
      ? ""
      : `${n?.path ?? "this unit's service.yml"} does not read as a native service: use the raw editor below.`;
    binary.input.value = m?.binary ?? "";
    envFile.input.value = m?.env_file ?? "";
    dataDirs.input.value = (m?.data_dirs ?? []).join("\n");
    updateCmd.input.value = m?.update_cmd ?? "";
    stateless.checked = m?.stateless ?? false;
    restoreNote.input.value = m?.restore_note ?? "";
    releaseRepo.input.value = m?.release_repo ?? "";
    releaseAsset.input.value = m?.release_asset ?? "";
    backupNewest.input.value = m?.backup_from_newest ?? "";
    backupPause.input.value =
      m?.backup_pause === "chassis"
        ? "chassis"
        : m?.backup_pause
          ? "true"
          : "false";
    updatePolicy.input.value = m?.update_policy ?? "manual";
    metrics.input.value = m?.metrics === false ? "not_measured" : "measured";
  };
  sel.addEventListener("change", show);
  show();
  const edit = () => ({
    kind: "native",
    unit: sel.value,
    binary: binary.input.value.trim(),
    env_file: envFile.input.value,
    data_dirs: dataDirs.input.value
      .split("\n")
      .map((s) => s.trim())
      .filter(Boolean),
    update_cmd: updateCmd.input.value,
    stateless: stateless.checked,
    restore_note: restoreNote.input.value,
    release_repo: releaseRepo.input.value,
    release_asset: releaseAsset.input.value,
    backup_from_newest: backupNewest.input.value,
    backup_pause: /** @type {"false" | "true" | "chassis"} */ (
      backupPause.input.value
    ),
    update_policy: /** @type {"manual" | "auto" | "self"} */ (
      updatePolicy.input.value
    ),
    metrics: /** @type {"measured" | "not_measured"} */ (metrics.input.value),
  });
  review.addEventListener("click", () => {
    openPlanDialog({
      id: `edit:native:${stack}`,
      title: `Native services · ${stack}`,
      planUrl: `/data/stacks/${encodeURIComponent(stack)}/plan`,
      planBody: { edit: edit() },
      commitUrl: `/data/stacks/${encodeURIComponent(stack)}/commit`,
      commitBody: stackCommit(edit()),
      onCommitted: () => void reload(),
    });
  });
  removeBtn.addEventListener("click", () => {
    const e = { kind: "remove_native", unit: sel.value };
    openPlanDialog({
      id: `edit:remove-native:${stack}`,
      title: `Remove ${sel.value} · ${stack}`,
      planUrl: `/data/stacks/${encodeURIComponent(stack)}/plan`,
      planBody: { edit: e },
      commitUrl: `/data/stacks/${encodeURIComponent(stack)}/commit`,
      commitBody: stackCommit(e),
      onCommitted: () => void reload(),
    });
  });
  return h(
    "section",
    {
      class: "kp-card edit-card",
      "aria-label": "Native services",
      id: "native-form",
    },
    h("h3", null, "Edit a native unit"),
    h(
      "div",
      { class: "kp-field" },
      h("label", { class: "kp-field__label", for: "native-unit" }, "Unit"),
      sel,
    ),
    pathLine,
    warn,
    h(
      "div",
      { class: "edit-grid" },
      binary.wrap,
      envFile.wrap,
      releaseRepo.wrap,
      releaseAsset.wrap,
      updatePolicy.wrap,
      metrics.wrap,
      backupNewest.wrap,
      backupPause.wrap,
      h(
        "div",
        { class: "kp-field kp-field--check" },
        stateless,
        h(
          "label",
          { class: "kp-field__label", for: "native-stateless" },
          "Stateless (no data_dirs, by design)",
        ),
      ),
    ),
    dataDirs.wrap,
    updateCmd.wrap,
    restoreNote.wrap,
    h("div", { class: "row-buttons" }, review, " ", removeBtn, " ", status),
  );
}

/**
 * The "add a native unit" form.
 * @param {string} stack
 * @param {() => Promise<void>} reload
 */
function addNativeCard(stack, reload) {
  const unit = nativeField("add-native-unit", "Unit name", "", {
    placeholder: "worker",
    help: "The systemd unit name, without .service. Its service.yml and unit file go under <unit>/.",
  });
  const binary = nativeField("add-native-binary", "Binary", "", {
    placeholder: "/opt/worker/bin/worker",
  });
  const envFile = nativeField("add-native-env-file", "Environment file", "", {
    placeholder: "/appdata/…/….env",
  });
  const dataDirs = nativeField(
    "add-native-data-dirs",
    "Data directories (one per line)",
    "",
    { textarea: true },
  );
  const notifyChk = h("input", {
    type: "checkbox",
    id: "add-native-notify",
    class: "kp-field__check",
  });
  notifyChk.checked = true;
  const review = h(
    "button",
    { type: "button", class: "kp-button", id: "add-native-review" },
    "Review and commit…",
  );
  review.addEventListener("click", () => {
    const e = {
      kind: "add_native",
      unit: unit.input.value.trim(),
      binary: binary.input.value.trim(),
      env_file: envFile.input.value,
      data_dirs: dataDirs.input.value
        .split("\n")
        .map((s) => s.trim())
        .filter(Boolean),
      notify: notifyChk.checked,
    };
    if (!e.unit || !e.binary) {
      notify("A unit needs at least a name and a binary path.", "info");
      return;
    }
    openPlanDialog({
      id: `edit:add-native:${stack}`,
      title: `Add a native unit · ${stack}`,
      planUrl: `/data/stacks/${encodeURIComponent(stack)}/plan`,
      planBody: { edit: e },
      commitUrl: `/data/stacks/${encodeURIComponent(stack)}/commit`,
      commitBody: stackCommit(e),
      onCommitted: () => void reload(),
    });
  });
  return h(
    "section",
    {
      class: "kp-card edit-card",
      "aria-label": "Add a native unit",
      id: "add-native",
    },
    h("h2", null, "Add a native unit"),
    h(
      "p",
      { class: "measured" },
      "A bare binary under systemd, no docker layer: its service.yml and a generic unit file, both under a new <unit>/ directory.",
    ),
    h("div", { class: "edit-grid" }, unit.wrap, binary.wrap, envFile.wrap),
    dataDirs.wrap,
    h(
      "div",
      { class: "kp-field kp-field--check" },
      notifyChk,
      h(
        "label",
        { class: "kp-field__label", for: "add-native-notify" },
        "Type=notify (the chassis-rs kit's shape)",
      ),
    ),
    h("div", { class: "row-buttons" }, review),
  );
}

/**
 * @param {string} stack
 * @param {EditRead} e
 * @param {() => Promise<void>} reload
 */
function nativeCard(stack, e, reload) {
  return h(
    "div",
    { id: "native-services" },
    h("h2", null, "Native services"),
    h(
      "p",
      { class: "measured" },
      "Each unit's service.yml — its binary, its state and its own update policy.",
    ),
    e.natives.length
      ? nativeUnitForm(stack, e.natives, reload)
      : h(
          "p",
          { class: "measured" },
          "This stack has no native unit yet; add one below.",
        ),
    addNativeCard(stack, reload),
  );
}

// ── feat-firewall-1 ─────────────────────────────────────────────────────

/**
 * @param {HTMLElement} panel
 * @param {{name: string}} params
 * @returns {() => void}
 */
export function firewallTab(panel, params) {
  const stack = params.name;
  const abort = new AbortController();
  panel.replaceChildren(
    h("p", { class: "measured" }, "Reading the stack's firewall…"),
  );
  /** @type {() => void} */
  let detach = () => {};
  const load = async () => {
    const r = await readEdit(stack, abort.signal);
    if (!r.ok) {
      panel.replaceChildren(errorBox(r.error));
      return;
    }
    const e = /** @type {EditRead} */ (r.body);
    if (!e.manifest) {
      panel.replaceChildren(
        errorBox({
          what: "the firewall",
          why: e.manifest_error ?? "no stack file",
          fix: "fix the stack file in the Settings tab's raw editor",
        }),
      );
      return;
    }
    detach();
    detach = drawFirewall(panel, stack, e, load);
  };
  void load().catch(() => {});
  return () => {
    abort.abort();
    detach();
  };
}

/**
 * @param {HTMLElement} panel
 * @param {string} stack
 * @param {EditRead} e
 * @param {() => Promise<void>} reload
 * @returns {() => void}
 */
function drawFirewall(panel, stack, e, reload) {
  const m = /** @type {import("./editforms.js").ManifestView} */ (e.manifest);
  const original = m.firewall;
  let model = firewallModel(original);
  const state = h("span", { class: "state" });
  const dirty = h("span", {
    class: "measured state-word",
    "data-size": "Changed; not committed.",
    role: "status",
    id: "fw-dirty",
  });
  const enabled = h("input", {
    type: "checkbox",
    class: "kp-switch__input",
    role: "switch",
    id: "fw-enabled",
  });
  const policyIn = choice("fw-policy-in", ["DROP", "ACCEPT", "REJECT"]);
  const policyOut = choice("fw-policy-out", ["ACCEPT", "DROP", "REJECT"]);
  const mgmt = h("input", {
    class: "kp-field__input",
    type: "text",
    id: "fw-management-open",
    placeholder: "empty: the management guard applies",
  });
  const comment = h("textarea", {
    class: "kp-field__input",
    id: "fw-comment",
    rows: "3",
    spellcheck: "false",
  });
  const t = tableBlock({
    remember: "stack-firewall",
    caption: `Firewall rules of ${stack}, read top to bottom`,
    search: "Search rules",
    nothing: "No rules yet: the policies above decide everything.",
    columns: [
      { label: "#", sort: "number" },
      { label: "Direction", sort: "text", filter: "choice" },
      {
        label: "Action",
        sort: "text",
        order: "ACCEPT,REJECT,DROP",
        filter: "choice",
      },
      { label: "Other side", sort: "text" },
      { label: "Protocol", sort: "text" },
      { label: "Ports", sort: "text" },
      { label: "Note", sort: "text", cls: "wide" },
      { label: "Change", sort: "text" },
    ],
  });
  const add = h(
    "button",
    { type: "button", class: "kp-button", id: "fw-add" },
    "Add a rule…",
  );
  const review = h(
    "button",
    { type: "button", class: "kp-button kp-button--primary", id: "fw-review" },
    "Review and commit…",
  );
  const discard = h(
    "button",
    { type: "button", class: "kp-button kp-button--ghost", id: "fw-discard" },
    "Discard changes",
  );

  const paintState = () => {
    const on = original?.enabled;
    state.className = `state ${original ? (on ? "ok" : "warn") : "bad"}`;
    state.replaceChildren(
      h(
        "span",
        null,
        original ? (on ? "in force" : "declared, off") : "none declared",
      ),
    );
    const changed = firewallChanged(model, original);
    dirty.textContent = changed ? "Changed; not committed." : "";
    review.disabled = !changed;
    discard.disabled = !changed;
  };
  const paintOptions = () => {
    enabled.checked = model.enabled;
    policyIn.value = model.policy_in;
    policyOut.value = model.policy_out;
    mgmt.value = model.management_open;
    comment.value = model.comment;
  };
  const paintRules = () => {
    t.tbody.replaceChildren(
      ...model.rules.map((re, i) => {
        const r = re.rule;
        const btn = (
          /** @type {string} */ label,
          /** @type {string} */ act,
          disabled = false,
        ) => {
          const b = h(
            "button",
            {
              type: "button",
              class: "kp-button kp-button--sm kp-button--ghost",
              "data-act": act,
              "data-i": String(i),
              "aria-label": `${label} rule ${i + 1}: ${ruleSummary(r)}`,
            },
            label,
          );
          if (disabled) b.disabled = true;
          return b;
        };
        return h(
          "tr",
          {
            "data-kp-row-key": `${i}`,
            "data-rule": String(i),
            class: re.origin === null ? "rule-new" : "",
          },
          td(String(i + 1), "num"),
          td(r.dir),
          badgeCell({
            label: r.action,
            tone: r.action === "ACCEPT" ? "ok" : "bad",
          }),
          td((r.dir === "in" ? r.source : r.dest) ?? "anywhere", "mono"),
          td(r.proto ?? "any"),
          td(r.dport ?? (r.proto === "icmp" ? "—" : "any"), "mono"),
          td([r.note, r.comment?.split("\n")[0]].filter(Boolean).join(" · ")),
          h(
            "td",
            { class: "row-buttons" },
            btn("Edit", "edit"),
            btn("Up", "up", i === 0),
            btn("Down", "down", i === model.rules.length - 1),
            btn("Delete", "delete"),
          ),
        );
      }),
    );
    table?.refresh();
    paintState();
  };

  t.tbody.addEventListener("click", (ev) => {
    const b = /** @type {Element} */ (ev.target).closest("button[data-act]");
    if (!(b instanceof HTMLButtonElement)) return;
    const i = Number(b.dataset.i);
    switch (b.dataset.act) {
      case "up":
      case "down":
        model = moveRule(model, i, b.dataset.act === "up" ? -1 : 1);
        paintRules();
        break;
      case "delete":
        model = { ...model, rules: model.rules.filter((_, j) => j !== i) };
        paintRules();
        break;
      case "edit":
        openRuleDialog(model.rules[i].rule, (rule) => {
          model = {
            ...model,
            rules: model.rules.map((x, j) => (j === i ? { ...x, rule } : x)),
          };
          paintRules();
        });
        break;
    }
  });
  add.addEventListener("click", () =>
    openRuleDialog(null, (rule) => {
      model = { ...model, rules: [...model.rules, { origin: null, rule }] };
      paintRules();
    }),
  );
  enabled.addEventListener("change", () => {
    model = { ...model, enabled: enabled.checked };
    paintState();
  });
  policyIn.addEventListener("change", () => {
    model = { ...model, policy_in: policyIn.value };
    paintState();
  });
  policyOut.addEventListener("change", () => {
    model = { ...model, policy_out: policyOut.value };
    paintState();
  });
  mgmt.addEventListener("input", () => {
    model = { ...model, management_open: mgmt.value };
    paintState();
  });
  comment.addEventListener("input", () => {
    model = { ...model, comment: comment.value };
    paintState();
  });
  discard.addEventListener("click", () => {
    model = firewallModel(original);
    paintOptions();
    paintRules();
  });
  review.addEventListener("click", () => {
    const edit = firewallBody(model);
    openPlanDialog({
      id: `edit:firewall:${stack}`,
      title: `Firewall · ${stack}`,
      planUrl: `/data/stacks/${encodeURIComponent(stack)}/plan`,
      planBody: { edit },
      commitUrl: `/data/stacks/${encodeURIComponent(stack)}/commit`,
      commitBody: stackCommit(edit),
      onCommitted: () => void reload(),
    });
  });

  const field = (
    /** @type {string} */ label,
    /** @type {HTMLElement} */ control,
    /** @type {string} */ help,
  ) =>
    h(
      "div",
      { class: "kp-field" },
      h("label", { class: "kp-field__label", for: control.id }, label),
      control,
      h("span", { class: "kp-field__help" }, help),
    );
  panel.replaceChildren(
    ...headLine(e),
    h(
      "section",
      {
        class: "kp-card edit-card",
        "aria-label": "Firewall options",
        id: "fw-options",
      },
      h("div", { class: "title-row" }, h("h2", null, "Firewall"), state),
      h(
        "p",
        { class: "measured" },
        `Written by the deploy to /etc/pve/firewall/${m.vmid}.fw from stacks/${stack}/lxc-compose.yml. `,
        h("a", { href: "/app/firewall" }, "The fleet's matrix"),
      ),
      h(
        "label",
        { class: "kp-switch" },
        enabled,
        h("span", { class: "kp-switch__state", "aria-hidden": "true" }),
        h(
          "span",
          null,
          "In force (the deploy writes the file and switches firewall=1 on the network card)",
        ),
      ),
      h(
        "div",
        { class: "edit-grid" },
        field(
          "Inbound policy",
          policyIn,
          "What happens when no inbound rule matches.",
        ),
        field(
          "Outbound policy",
          policyOut,
          "What happens when no outbound rule matches.",
        ),
        field(
          "Management network open, because…",
          mgmt,
          "Empty: DNS to the router only, the rest of 10.10.5.0/24 dropped. A reason drops that guard.",
        ),
        field(
          "Comment at the top of the file",
          comment,
          "Lines written above the rules in pve's file.",
        ),
      ),
    ),
    h(
      "div",
      { class: "row-buttons" },
      add,
      " ",
      review,
      " ",
      discard,
      " ",
      dirty,
    ),
    t.wrap,
  );
  const detachTables = attachDataTables(panel);
  // The "In force" switch's On/Off words.
  const detachSwitches = attachSwitches(panel);
  const table = dataTable(t.wrap);
  paintOptions();
  paintRules();
  // feat-platform-10: the Live view replay works this very editor: its
  // model is the one Claude's steps built on the dashboard's server, and
  // the rule dialog is the one Add and Edit open.
  const unregister = register(`firewall:${stack}`, {
    model: () => model,
    setModel: (/** @type {import("./editforms.js").FirewallModel} */ m) => {
      model = m;
      paintOptions();
      paintRules();
    },
    openRule: (/** @type {number | null} */ i) =>
      openRuleDialog(
        i == null ? null : (model.rules[i]?.rule ?? null),
        () => {},
      ),
    review: () => review,
    add: () => add,
    rowButton: (/** @type {string} */ act, /** @type {number} */ i) =>
      /** @type {HTMLElement | null} */ (
        t.tbody.querySelector(`button[data-act="${act}"][data-i="${i}"]`)
      ),
  });
  return () => {
    unregister();
    detachTables();
    detachSwitches();
  };
}

/**
 * @param {string} id
 * @param {string[]} options
 */
function choice(id, options) {
  return h(
    "select",
    { class: "kp-field__input", id },
    ...options.map((o) => h("option", { value: o }, o)),
  );
}

/**
 * The rule dialog behind Add and Edit.
 * @param {import("./editforms.js").Rule | null} rule
 * @param {(r: import("./editforms.js").Rule) => void} done
 * @returns {{close: () => void}}
 */
function openRuleDialog(rule, done) {
  const fields = ruleFields(rule);
  const values = startValues({
    steps: [{ id: "rule", label: "Rule", fields }],
  });
  /** @type {Map<string, HTMLElement>} */
  const inputs = new Map();
  const errorBox = h("div");
  const wraps = fields.map((f) => {
    const x = editField(f, values[f.name], (v) => (values[f.name] = v));
    inputs.set(f.name, x.input);
    return x.wrap;
  });
  const save = h(
    "button",
    { type: "button", class: "kp-button kp-button--primary", id: "rule-save" },
    rule ? "Change the rule" : "Add the rule",
  );
  const cancel = h(
    "button",
    { type: "button", class: "kp-button", "data-kp-dialog-close": "" },
    "Cancel",
  );
  const d = openDialog({
    title: rule ? `Edit rule · ${ruleSummary(rule)}` : "Add a rule",
    description: "Nothing is written until the plan is reviewed and committed.",
    body: [
      h("div", { class: "edit-grid" }, ...wraps),
      errorBox,
      h("div", { class: "kp-dialog__actions" }, cancel, save),
    ],
    id: "rule-dialog",
  });
  const unregister = register("rule", {
    dialog: d.dialog,
    save: () => save,
    cancel: () => cancel,
    close: () => d.close(),
  });
  d.closed.then(unregister);
  cancel.addEventListener("click", () => d.close());
  save.addEventListener("click", () => {
    // Live view: the rule is kept on the dashboard's server.
    if (driven()) return;
    const errors = {
      ...checkFields(
        { steps: [{ id: "rule", label: "Rule", fields }] },
        values,
      ),
      ...ruleProblems(values),
    };
    if (!markErrors(inputs, errors)) {
      errorBox.replaceChildren(
        refusalCallout(
          { what: "the rule", why: Object.values(errors).join(" "), fix: "" },
          "warning",
          "Not yet",
        ),
      );
      return;
    }
    done(ruleFromValues(values));
    d.close();
  });
  return { close: () => d.close() };
}
