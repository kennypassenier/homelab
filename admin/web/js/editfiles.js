// The stack's Files card (feat-stacks-files): every text file of the stack,
// open, edited, created, deleted or renamed. kp's master-detail puts the list
// of files on the left and the editor on the right (the list folds to one
// pane on a phone); a JSON file is edited in kp's JSON field, any other text
// in a field that grows with it; a file edited and not yet committed is
// marked in the list and counted by the card's unsaved bar, whose Save
// reviews the open file (the plan with kp's diff). Editing an existing file
// still ends as a `raw` edit (what Live view drives with
// `homelab ui open raw <stack>`); create/delete/rename end as a `files` edit.

import { notify } from "./actui.js";
import { h } from "./dom.js";
import { buttonGroup, formGrid, gridRow } from "./gridrow.js";
import {
  editCard,
  markRow,
  openPlanDialog,
  stackCommit,
  wireBar,
} from "./editui.js";
import { attachAutosize } from "/static/kp/js/autosize.js";
import { showError } from "/static/kp/js/forms.js";
import {
  attachJsonFields,
  explainJson,
  jsonField,
} from "/static/kp/js/json-field.js";
import { selectPane } from "/static/kp/js/master-detail.js";

/** @param {string} text */
const lineCount = (text) => (text === "" ? 0 : text.split("\n").length);

/** @param {string} path */
const isJson = (path) => /\.json$/i.test(path);

/**
 * @param {string} stack
 * @param {{texts: Record<string, string>, file_templates?: Record<string, string>}} e
 * @param {() => Promise<void>} reload
 * @param {Map<string, string>} pending the texts edited and not committed, by path; they outlive a reload
 * @returns {{node: HTMLElement, bar: HTMLElement, bind: () => void, detach: () => void}}
 */
export function filesCard(stack, e, reload, pending) {
  const files = Object.keys(e.texts).sort();
  const templates = e.file_templates ?? {};
  /** @type {"edit" | "create"} */
  let mode = "edit";
  let current = files.includes("lxc-compose.yml")
    ? "lxc-compose.yml"
    : (files[0] ?? "");

  for (const [path, text] of [...pending])
    if (!(path in e.texts) || text === e.texts[path]) pending.delete(path);

  // Live view sets this select (and the text) by id as it always has; the
  // list is what a person sees, so the select only holds the choice.
  const sel = h(
    "select",
    {
      class: "kp-field__input kp-sr-only",
      id: "raw-file",
      tabindex: "-1",
      "data-kp-untracked": "",
      "aria-label": "The open file (the list beside it chooses it)",
    },
    ...files.map((f) => h("option", { value: f }, f)),
  );
  sel.value = current;

  const list = h("ul", {
    class: "kp-pane__list",
    role: "listbox",
    "aria-label": `Files of ${stack}`,
  });
  /** @type {Map<string, HTMLElement>} */
  const items = new Map();
  for (const f of files) {
    const slash = f.lastIndexOf("/");
    const li = h(
      "li",
      {
        role: "option",
        "data-kp-pane-key": f,
        "data-kp-name": f,
        "aria-selected": String(f === current),
        title: f,
      },
      slash >= 0
        ? h("span", { class: "edit-files__dir" }, f.slice(0, slash + 1))
        : "",
      h("b", { class: "mono" }, slash >= 0 ? f.slice(slash + 1) : f),
    );
    items.set(f, li);
    list.append(li);
  }
  const markItem = (/** @type {string} */ path) => {
    const li = items.get(path);
    if (!li) return;
    const text = pending.get(path);
    markRow(
      li,
      text === undefined
        ? null
        : {
            was: `${lineCount(e.texts[path] ?? "")} lines`,
            now: `${lineCount(text)} lines`,
          },
    );
  };
  for (const f of files) markItem(f);

  const newBtn = h(
    "button",
    { type: "button", class: "kp-button", id: "files-new" },
    "New file…",
  );

  // ── the detail: path/template (create), the text, rename ──
  const pathInput = h("input", {
    class: "kp-field__input mono",
    id: "files-new-path",
    type: "text",
    placeholder: "app/checks.yml",
    "data-kp-untracked": "",
  });
  const templateSel = h(
    "select",
    {
      class: "kp-field__input",
      id: "files-new-template",
      "data-kp-untracked": "",
    },
    h("option", { value: "" }, "Blank"),
    ...Object.keys(templates)
      .sort()
      .map((k) => h("option", { value: k }, k)),
  );
  const renameInput = h("input", {
    class: "kp-field__input mono",
    id: "files-rename-to",
    type: "text",
    "data-kp-untracked": "",
  });
  const renameGo = h(
    "button",
    { type: "button", class: "kp-button", id: "files-rename-go" },
    "Rename…",
  );
  const area = /** @type {HTMLTextAreaElement} */ (
    h("textarea", {
      class: "kp-field__input kp-field__input--multiline mono raw-text",
      id: "raw-text",
      rows: "10",
      spellcheck: "false",
      "aria-label": "The file's text",
      "data-kp-untracked": "",
    })
  );
  const fieldError = h("p", {
    class: "kp-field__error",
    "data-kp-field-error": "",
    hidden: "",
  });
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
  const addBtn = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--primary",
      id: "raw-review",
      hidden: "",
    },
    "Review and commit…",
  );

  const pathRow = gridRow({
    label: "Path",
    labelFor: "files-new-path",
    control: pathInput,
    value: "Where the new file goes, relative to the stack's folder.",
    hidden: true,
  });
  const templateRow = gridRow({
    label: "Start from",
    labelFor: "files-new-template",
    control: templateSel,
    value: "A template's text, or a blank file.",
    hidden: true,
  });
  const renameRow = gridRow({
    label: "New path",
    labelFor: "files-rename-to",
    control: renameInput,
    value:
      "The file moves in the working copy; nothing is written until you commit.",
    actions: renameGo,
    hidden: true,
  });
  const textRow = gridRow({
    label: "Text",
    labelFor: "raw-text",
    control: h("div", { class: "kp-field" }, area, fieldError),
    value: "The whole file as it should be.",
    span: "wide",
  });
  const buttonRow = gridRow({
    control: buttonGroup([renameBtn, deleteBtn, cancelBtn]),
    actions: addBtn,
    buttons: true,
  });
  const heading = h("h3", {
    "data-kp-pane-heading": "",
    tabindex: "-1",
    class: "mono",
  });
  heading.textContent = current;

  const detail = h(
    "section",
    {
      class: "kp-split__pane",
      "data-kp-pane": "detail",
      "aria-label": "The open file",
    },
    heading,
    h(
      "div",
      { class: "kp-pane__body" },
      formGrid(
        [pathRow, templateRow, textRow, renameRow, buttonRow],
        "edit-files__grid",
      ),
    ),
  );
  const masterDetail = h(
    "div",
    {
      class: "kp-split kp-split--panes edit-files",
      "data-kp-master-detail": "",
      "data-kp-pane-widths": "17rem 1fr",
      "data-kp-pane-history": "off",
      id: "files-panes",
    },
    h(
      "section",
      {
        class: "kp-split__pane",
        "data-kp-pane": "list",
        "aria-label": "Files",
      },
      sel,
      list,
      h("div", { class: "edit-files__tools" }, newBtn),
    ),
    detail,
  );

  const { node, bar } = editCard({
    id: "raw-editor",
    form: "raw",
    title: "Files",
    desc: "Every text file of the stack: open, edit, create, delete or rename. Secrets (.env) are not here: they live in latch.",
    blocks: [masterDetail],
  });

  // ── behaviour ──
  const showFile = () => {
    area.value = pending.get(current) ?? e.texts[current] ?? "";
    heading.textContent = current;
    renameRow.hidden = true;
    fieldError.hidden = true;
    area.removeAttribute("aria-invalid");
    applyMode();
  };

  /** @type {() => void} */
  let stopKind = () => {};
  /** @type {"json" | "text" | null} */
  let kind = null;
  /** The text box is kp's JSON field for a .json file, a growing box otherwise. */
  const applyMode = () => {
    if (!node.isConnected) return;
    const want = mode === "edit" && isJson(current) ? "json" : "text";
    if (want === kind) {
      if (kind === "json") jsonField(area)?.check();
      return;
    }
    stopKind();
    kind = want;
    if (want === "json") {
      area.removeAttribute("data-kp-autosize");
      area.removeAttribute("data-kp-counter");
      area.setAttribute("data-kp-json", "strict");
      stopKind = attachJsonFields(area.parentElement ?? node);
    } else {
      area.removeAttribute("data-kp-json");
      area.setAttribute("data-kp-autosize", "");
      area.setAttribute("data-kp-autosize-max", "30");
      area.setAttribute("data-kp-counter", "lines");
      stopKind = attachAutosize(area.parentElement ?? node);
    }
  };

  const choose = (/** @type {string} */ path, fromList = false) => {
    if (mode !== "edit" || !(path in e.texts) || path === current) return;
    current = path;
    if (sel.value !== path) sel.value = path;
    if (!fromList) selectPane(masterDetail, path);
    showFile();
  };
  masterDetail.addEventListener("kp-pane-select", (ev) => {
    const key = /** @type {CustomEvent} */ (ev).detail?.key;
    if (typeof key === "string") choose(key, true);
  });
  sel.addEventListener("change", () => choose(sel.value));

  area.addEventListener("input", () => {
    if (mode !== "edit") return;
    fieldError.hidden = true;
    area.removeAttribute("aria-invalid");
    if (area.value === e.texts[current]) pending.delete(current);
    else pending.set(current, area.value);
    markItem(current);
  });

  const setMode = (/** @type {"edit" | "create"} */ m) => {
    mode = m;
    const creating = m === "create";
    pathRow.hidden = !creating;
    templateRow.hidden = !creating;
    renameRow.hidden = true;
    renameBtn.hidden = creating;
    deleteBtn.hidden = creating;
    newBtn.hidden = creating;
    cancelBtn.hidden = !creating;
    addBtn.hidden = !creating;
    list.toggleAttribute("inert", creating);
    if (creating) {
      pathInput.value = "";
      templateSel.value = "";
      area.value = "";
      heading.textContent = "New file";
      applyMode();
      pathInput.focus();
    } else {
      area.value = "";
      showFile();
    }
  };
  newBtn.addEventListener("click", () => setMode("create"));
  cancelBtn.addEventListener("click", () => setMode("edit"));
  templateSel.addEventListener("change", () => {
    area.value = templates[templateSel.value] ?? "";
  });

  /** @param {string} id @param {string} title @param {Record<string, unknown>} edit @param {() => void} [after] */
  const plan = (id, title, edit, after) =>
    openPlanDialog({
      id,
      title,
      planUrl: `/data/stacks/${encodeURIComponent(stack)}/plan`,
      planBody: { edit },
      commitUrl: `/data/stacks/${encodeURIComponent(stack)}/commit`,
      commitBody: stackCommit(edit),
      onCommitted: () => {
        after?.();
        void reload();
      },
    });

  renameBtn.addEventListener("click", () => {
    renameRow.hidden = !renameRow.hidden;
    if (!renameRow.hidden) {
      renameInput.value = current;
      renameInput.focus();
      renameInput.select();
    }
  });
  renameGo.addEventListener("click", () => {
    const to = renameInput.value.trim();
    if (!to || to === current) {
      notify("Give the file another path first.", "info");
      return;
    }
    plan(
      `edit:files:rename:${stack}`,
      `Rename ${current} · ${stack}`,
      { kind: "files", op: "rename", from: current, to },
      () => pending.delete(current),
    );
  });
  deleteBtn.addEventListener("click", () =>
    plan(
      `edit:files:delete:${stack}`,
      `Delete ${current} · ${stack}`,
      { kind: "files", op: "delete", path: current },
      () => pending.delete(current),
    ),
  );
  addBtn.addEventListener("click", () => {
    const path = pathInput.value.trim();
    if (!path) {
      notify("Give the new file a path first.", "info");
      pathInput.focus();
      return;
    }
    plan(`edit:files:create:${stack}`, `${path} · ${stack}`, {
      kind: "files",
      op: "create",
      path,
      content: area.value,
    });
  });

  // The bar's Save reviews the open file when it is the one changed, else the
  // first changed file; the others stay marked and are reviewed next.
  wireBar(
    bar,
    () => {
      const path = pending.has(current) ? current : [...pending.keys()][0];
      if (path === undefined) {
        notify("The file is as it was.", "info");
        return null;
      }
      const content = pending.get(path) ?? "";
      if (isJson(path)) {
        const r = explainJson(content);
        if (!r.ok) {
          if (path !== current) choose(path);
          showError(
            area,
            r.line != null
              ? `Line ${r.line}, column ${r.column}: ${r.why}.`
              : `That is not valid JSON: ${r.why}.`,
          );
          area.focus();
          return null;
        }
      }
      return plan(
        `edit:raw:${stack}`,
        `${path} · ${stack}`,
        { kind: "raw", path, content },
        () => pending.delete(path),
      );
    },
    {
      // Revert one file: put its committed text back.
      revert: (key) => {
        if (!pending.has(key)) return false;
        pending.delete(key);
        markItem(key);
        if (key === current) area.value = e.texts[key] ?? "";
        return true;
      },
      // Discard every file (the bar gives no Undo here: a file is long).
      discard: () => {
        for (const k of [...pending.keys()]) {
          pending.delete(k);
          markItem(k);
        }
        if (mode === "edit") area.value = e.texts[current] ?? "";
      },
    },
  );

  showFile();
  return {
    node,
    bar,
    bind: () => applyMode(),
    detach: () => stopKind(),
  };
}
