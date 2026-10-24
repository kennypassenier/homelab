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
  keepHidden,
  offReasonField,
  offWords,
  switchedRows,
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
  moveRuleTo,
  ruleIntent,
  toggleRuleOff,
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
  settingsExtGroups,
  settingsForm,
  settingsGroups,
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
  editBlock,
  editCard,
  editField,
  editGroup,
  editRow,
  markErrors,
  markRow,
  markUnsavedClean,
  openPlanDialog,
  stackCommit,
  wireBar,
} from "./editui.js";
import { filesCard } from "./editfiles.js";
import { stage } from "./stage.js";
import { badgeCell, errorBox, fetchJson, h, tableBlock, td } from "./dom.js";
import { driven, handle, register } from "./drivehooks.js";
import { attachDataTables, dataTable } from "/static/kp/js/datatable.js";
import { attachSwitches } from "/static/kp/js/forms.js";
import { attachAll } from "/static/kp/js/auto.js";
import {
  CARD_RETRY_EVENT,
  attachCardStates,
  setCardState,
} from "/static/kp/js/card-state.js";
import { declare, drivable, viaForm } from "./drivable.js";
import { buttonGroup, gridRow } from "./gridrow.js";

// check-turn-off-1: a check or probe row of the stack's checks list is turned
// off (with a reason) or on again; both change the draft the list's Review
// and commit… writes to checks.yml.
const CHECK_OFF = declare({
  id: "checks-turn-off",
  page: "stack",
  opens: "dialog",
  row: "<list>:<row number>",
  what: "turn a check or a probe of the stack's checks list off: asks a short reason, writes off: <reason> on it when the change is committed",
});
const CHECK_ON = declare({
  id: "checks-turn-on",
  page: "stack",
  opens: "view",
  row: "<list>:<row number>",
  what: "turn a check or a probe of the stack's checks list on again (removes its off: reason when the change is committed)",
});

/**
 * fix-239: every card of a stack's settings tab is one edit form, which
 * Live view opens as `homelab ui open <form> <stack>`; the card says which,
 * so each of its buttons is reachable through it.
 * @template {Node} N
 * @param {N} node
 * @param {string} form
 * @returns {N}
 */
const card = (node, form) => {
  if (node instanceof HTMLElement) viaForm(node, form);
  return node;
};

/**
 * @typedef {{unit: string, path: string, unit_file_path: string,
 *   manifest: NativeManifestView | null}} NativeView
 * @typedef {{stack_name: string, vmid: number, hostname: string, unit: string,
 *   binary: string, env_file: string | null, data_dirs: string[],
 *   update_cmd: string | null, stateless: boolean, restore_note: string | null,
 *   release_repo: string | null, release_asset: string | null,
 *   backup_from_newest: string | null, after_restore: string | null,
 *   backup_pause: boolean | "chassis",
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
 * Each app's checks.yml as the edit read carries it: under `manifest`
 * (shell/edit.rs), where the stack's own parts are.
 * @param {EditRead} e
 * @returns {Record<string, import("./editforms.js").ChecksView>}
 */
const checksOf = (e) =>
  e.checks ??
  /** @type {any} */ (e.manifest)?.checks ??
  /** @type {Record<string, import("./editforms.js").ChecksView>} */ ({});

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
  // The tab's own footprint while it is read: kp's busy card, and the failed
  // plate with its Try again in the same box.
  const reading = stage(h("div", { class: "edit-stage__body" }));
  reading.classList.add("edit-stage");
  panel.replaceChildren(reading);
  setCardState(reading, "loading", { words: "Reading the stack's files" });
  const stopStates = attachCardStates(reading);
  /** @type {() => void} */
  let unregister = () => {};
  /** @type {() => void} */
  let detachTables = () => {};
  /** @type {() => void} */
  let detachRows = () => {};
  /** @type {() => void} */
  let detachKp = () => {};
  /** The files edited and not committed: they outlive a reload of the tab. @type {Map<string, string>} */
  const pending = new Map();
  const load = async () => {
    const r = await readEdit(stack, abort.signal);
    if (!r.ok) {
      panel.replaceChildren(reading);
      setCardState(reading, "error", {
        title: `Could not read ${r.error.what}`,
        text: [
          `Why: ${r.error.why}`,
          ...(r.error.fix ? [`What to do: ${r.error.fix}`] : []),
        ].join("\n"),
      });
      return;
    }
    const e = /** @type {EditRead} */ (r.body);
    const parts = /** @type {Node[]} */ ([...headLine(e)]);
    if (e.manifest) {
      const s = settingsCard(stack, e, load);
      parts.push(card(s.node, "settings"));
    } else
      parts.push(
        h(
          "div",
          { class: "kp-alert kp-alert--warning", role: "status" },
          `The settings form needs a readable lxc-compose.yml: ${e.manifest_error ?? "none"}. The raw editor below still works.`,
        ),
      );
    const files = filesCard(stack, e, load, pending);
    parts.push(card(files.node, "raw"));
    if (e.manifest && e.presets.length)
      parts.push(card(addAppCard(stack, e, load), "add-app"));
    let settingsExt = null;
    let apps = null;
    let latch = null;
    let tiles = null;
    if (e.manifest) {
      settingsExt = settingsExtCard(stack, e, load);
      parts.push(card(settingsExt.node, "settings-ext"));
      apps = appsEditCard(stack, e, load);
      parts.push(card(apps.node, "apps"));
      latch = latchEditCard(stack, e, load);
      parts.push(card(latch.node, "latch"));
      tiles = tilesEditCard(stack, e, load);
      parts.push(card(tiles.node, "tiles"));
    }
    if (e.manifest && (e.manifest.native_only || e.natives.length))
      parts.push(card(nativeCard(stack, e, load).node, "native"));
    setCardState(reading, "ready");
    panel.replaceChildren(...parts);
    // feat-platform-10: the row tables' kp datatables need their wraps in
    // the live document first (drawFirewall's own order, at the panel
    // level here since several tables share this one tab).
    detachTables();
    detachTables = attachDataTables(panel);
    // A table's own search box and sort controls are not edits of the form.
    for (const c of panel.querySelectorAll(
      ".edit-block :is(input, select, textarea)",
    ))
      c.setAttribute("data-kp-untracked", "");
    settingsExt?.bind();
    apps?.bind();
    latch?.bind();
    tiles?.bind();
    files.bind();
    detachRows();
    detachRows = () => {
      settingsExt?.detach();
      apps?.detach();
      latch?.detach();
      tiles?.detach();
      files.detach();
    };
    // The unsaved bars, the quantities and the other kp parts the cards
    // carry; Live view finds the bars' Save once they stand.
    detachKp();
    const kp = attachAll(panel);
    detachKp = kp;
    await kp.ready;
    if (abort.signal.aborted) return;
    // feat-platform-10: the Live view replay presses these very buttons,
    // and opens/acts on the row tables' rows.
    unregister();
    unregister = register(`stack-edit:${stack}`, {
      // The Files card is a plain section (kp's master-detail), always open:
      // there is nothing to unfold, so the Live view step only brings it
      // into view.
      openRaw: () => {
        panel
          .querySelector("#raw-editor")
          ?.scrollIntoView({ block: "start", behavior: "instant" });
      },
      // A card's review is its unsaved bar's Save; the two add forms keep
      // their own button.
      review: (/** @type {string} */ family) =>
        /** @type {HTMLElement | null} */ (
          panel.querySelector(
            {
              raw: "#raw-editor .kp-unsaved__save",
              "add-app": "#add-app-review",
              "settings-ext": "#settings-ext-form .kp-unsaved__save",
              apps: "#apps-form .kp-unsaved__save",
              latch: "#latch-form .kp-unsaved__save",
              tiles: "#tiles-form .kp-unsaved__save",
              native: "#native-form .kp-unsaved__save",
              "add-native": "#add-native-review",
            }[family] ?? "#settings-form .kp-unsaved__save",
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
  reading.addEventListener(CARD_RETRY_EVENT, () => {
    setCardState(reading, "loading", { words: "Reading the stack's files" });
    void load().catch(() => {});
  });
  void load().catch(() => {});
  return () => {
    abort.abort();
    unregister();
    detachTables();
    detachRows();
    detachKp();
    stopStates();
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
  const groups = fieldGroups(
    settingsGroups(form.steps[0].fields),
    values,
    inputs,
  );
  const { node, bar } = editCard({
    id: "settings-form",
    form: form.id,
    title: "Settings",
    desc: `CT ${m.vmid} · ${m.hostname} · ${m.ip}. Written into stacks/${stack}/lxc-compose.yml and the apps' compose files, with every comment kept.`,
    groups,
  });
  wireBar(bar, () => {
    const errors = { ...checkFields(form, values), ...tileProblems(values) };
    if (!markErrors(inputs, errors)) return null;
    const edit = settingsBody(form, values);
    if (!changesSomething(edit)) {
      notify("Nothing is changed yet.", "info");
      return null;
    }
    return openPlanDialog({
      id: form.id,
      title: `Settings · ${stack}`,
      planUrl: `/data/stacks/${encodeURIComponent(stack)}/plan`,
      planBody: { edit },
      commitUrl: `/data/stacks/${encodeURIComponent(stack)}/commit`,
      commitBody: stackCommit(edit),
      onCommitted: () => void reload(),
    });
  });
  return { node, bar };
}

/**
 * A form's field groups as grid groups: each field a row, its input kept in
 * `inputs` (for the errors) and its typed value in `values`.
 * @param {import("./editforms.js").FieldGroup[]} groups
 * @param {import("./editforms.js").Values} values
 * @param {Map<string, HTMLElement>} inputs
 * @param {() => void} [changed]
 * @returns {HTMLElement[]}
 */
function fieldGroups(groups, values, inputs, changed) {
  return groups.map((g) =>
    editGroup({
      id: `group-${g.id}`,
      title: g.title,
      desc: g.desc,
      rows: g.fields.map((f) => {
        const x = editRow(f, values[f.name], (v) => {
          values[f.name] = v;
          changed?.();
        });
        inputs.set(f.name, x.input);
        return x.row;
      }),
    }),
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
  const tileGroup = h("div", {
    class: "kp-formgrid__group edit-group",
    id: "add-app-tiles",
  });
  /** @type {Map<string, HTMLInputElement>} */
  const tileInputs = new Map();
  const renderTiles = () => {
    const preset = e.presets.find((p) => p.name === sel.value);
    tileInputs.clear();
    if (!preset || !preset.apps.length) {
      tileGroup.hidden = true;
      tileGroup.replaceChildren();
      return;
    }
    tileGroup.hidden = false;
    tileGroup.replaceChildren(
      h("h3", { class: "kp-formgrid__heading" }, "Tiles (optional)"),
      h(
        "p",
        {
          class: "measured edit-group__desc",
          "data-kp-formgrid-span": "all",
        },
        "A hostname gives the app a tile on the start page; blank means no tile for that app.",
      ),
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
        return gridRow({
          label: `${app} hostname`,
          labelFor: id,
          control: input,
          value: `Where ${app} opens.`,
        });
      }),
    );
  };
  sel.addEventListener("change", renderTiles);
  renderTiles();
  const review = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--primary",
      id: "add-app-review",
    },
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
  const { node } = editCard({
    id: "add-app",
    form: "add-app",
    bar: false,
    title: "Add an app",
    desc: "A preset's app joins this stack: its files, its entry in apps and its /appdata folder.",
    groups: [
      editGroup({
        id: "group-add-app",
        title: "Preset",
        desc: "The preset whose app is added; nothing is written until the plan is reviewed and committed.",
        rows: [
          gridRow({
            label: "Preset",
            labelFor: "add-app-preset",
            control: sel,
            value: "The preset's apps and their folders.",
          }),
        ],
      }),
      tileGroup,
      gridRow({
        control: buttonGroup([review]),
        buttons: true,
      }),
    ],
  });
  return node;
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
  const groups = fieldGroups(
    settingsExtGroups(form.steps[0].fields),
    values,
    inputs,
  );
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
    onChange: () => tg.sync(),
  });
  const tg = tableGroup({
    id: "group-retention",
    title: "Retention",
    desc: "How many of this stack's own snapshots are kept, per period; with no tier of its own the fleet default applies.",
    table: retention,
    start: startRetention,
  });
  const { node, bar } = editCard({
    id: "settings-ext-form",
    form: form.id,
    title: "Network & hardware",
    desc: "The address, the lxc flags and the storage a rebuild would use, and this stack's own snapshot retention. Address and vmid clashes with another stack are caught at Review.",
    groups,
    blocks: [tg.block],
  });
  wireBar(
    bar,
    () => {
      const errors = checkFields(form, values);
      if (!markErrors(inputs, errors)) return null;
      const edit = settingsExtBody(
        form,
        values,
        rowsChanged(retention.getRows(), startRetention)
          ? retention.getRows()
          : null,
      );
      if (!changesSomething(edit)) {
        notify("Nothing is changed yet.", "info");
        return null;
      }
      return openPlanDialog({
        id: form.id,
        title: `Network & hardware · ${stack}`,
        planUrl: `/data/stacks/${encodeURIComponent(stack)}/plan`,
        planBody: { edit },
        commitUrl: `/data/stacks/${encodeURIComponent(stack)}/commit`,
        commitBody: stackCommit(edit),
        onCommitted: () => void reload(),
      });
    },
    tableHooks([tg]),
  );
  return {
    node,
    bar,
    bind: () => retention.bind(),
    detach: () => retention.detach(),
  };
}

/**
 * A row table as a titled block of its card: its Add on the block's end
 * edge, and the block marked changed for kp's unsaved bar while its rows
 * differ from the committed ones.
 * @param {{id: string, title: string, desc: string, table: RowTable,
 *   start: import("./editforms.js").RowEdit[]}} t
 */
function tableGroup({ id, title, desc, table, start }) {
  const block = editBlock({
    id,
    title,
    desc,
    body: table.body,
    action: table.add,
  });
  block.dataset.kpName = title;
  const sync = () => {
    const rows = table.getRows();
    markRow(
      block,
      rowsChanged(rows, start)
        ? {
            was: `${start.length} ${start.length === 1 ? "row" : "rows"}`,
            now: `${rows.length} ${rows.length === 1 ? "row" : "rows"}, edited`,
          }
        : null,
    );
  };
  return { block, title, sync, reset: () => table.setRows(start) };
}

/**
 * The bar's revert and discard for the row tables of a card.
 * @param {ReturnType<typeof tableGroup>[]} groups
 */
function tableHooks(groups) {
  return {
    revert: (/** @type {string} */ key) => {
      const g = groups.find((x) => x.title === key);
      g?.reset();
      return g !== undefined;
    },
    discard: () => groups.forEach((g) => g.reset()),
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
 * @typedef {{wrap: Node[], body: Node[], add: HTMLElement,
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
 *   done: (row: Record<string, unknown>) => void,
 *   cancelled?: () => void}} opts
 */
function openRowDialog({
  title,
  fields,
  saveLabel,
  fromValues,
  problems,
  datalists,
  done,
  cancelled,
}) {
  const steps = [{ id: "row", label: "Row", fields }];
  let saved = false;
  const values = startValues({ steps });
  /** @type {Map<string, HTMLElement>} */
  const inputs = new Map();
  const errBox = h("div");
  const wraps = fields.map((f) => {
    const x = editRow(f, values[f.name], (v) => (values[f.name] = v));
    inputs.set(f.name, x.input);
    // A picker that also takes free text (e.g. a tile's group): a
    // <datalist> of what is already used, offered alongside typing.
    const options = datalists?.[f.name];
    if (options?.length && x.input instanceof HTMLInputElement) {
      const listId = `${f.id}-list`;
      x.input.setAttribute("list", listId);
      (x.row.querySelector(".kp-field") ?? x.row).append(
        h(
          "datalist",
          { id: listId },
          ...options.map((/** @type {string} */ o) =>
            h("option", { value: o }),
          ),
        ),
      );
    }
    return x.row;
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
      h("div", { class: "kp-formgrid" }, ...wraps),
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
  void d.closed.then(() => {
    if (!saved) cancelled?.();
  });
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
    saved = true;
    done(fromValues(values));
    d.close();
  });
}

/**
 * Asks for the short reason a check is turned off (the row dialog, one field).
 * @param {string} title
 * @returns {Promise<string | null>} the reason, or null when cancelled
 */
function askOffReason(title) {
  return new Promise((resolve) => {
    openRowDialog({
      title,
      fields: [offReasonField()],
      saveLabel: "Turn it off",
      fromValues: (v) => ({ off: String(v.off ?? "").trim() }),
      done: (row) => resolve(String(row.off)),
      cancelled: () => resolve(null),
    });
  });
}

/**
 * check-turn-off-1: turn one check or probe of an app's checks.yml off, or on
 * again, from outside the stack's own list (an Inbox row, the fleet check's
 * findings): reads the stack's files, changes that one row and opens the plan
 * dialog, so the change is a commit like any other, visible and undoable.
 * @param {{stack: string, app: string, name: string, on?: boolean,
 *   onCommitted?: () => void}} o
 * @returns {Promise<void>}
 */
export async function switchCheck(o) {
  const reason = o.on
    ? null
    : await askOffReason(`Turn off · ${o.name} · ${o.app} · ${o.stack}`);
  if (!o.on && reason == null) return;
  const r = await readEdit(o.stack, new AbortController().signal);
  if (!r.ok) {
    notify(`${r.error.what}: ${r.error.why}`, "warning");
    return;
  }
  const e = /** @type {EditRead} */ (r.body);
  const view = checksOf(e)[o.app];
  if (!view || "error" in view) {
    notify(`${o.app}'s checks.yml could not be read here.`, "warning");
    return;
  }
  const hit = switchedRows(view, o.name, reason);
  if (!hit) {
    notify(
      `${o.stack}/${o.app} has no check or probe named ${o.name} in its checks.yml.`,
      "warning",
    );
    return;
  }
  const edit = checksBody({
    app: o.app,
    checks: hit.list === "checks" ? hit.rows : null,
    manual: null,
    probes: hit.list === "probes" ? hit.rows : null,
    busyCheck: view.busy_check?.command ?? "",
    url: view.url ?? "",
  });
  openPlanDialog({
    id: `edit:checks-off:${o.stack}:${o.app}`,
    title: `${o.on ? "Turn on" : "Turn off"} · ${o.name} · ${o.app} · ${o.stack}`,
    planUrl: `/data/stacks/${encodeURIComponent(o.stack)}/plan`,
    planBody: { edit },
    commitUrl: `/data/stacks/${encodeURIComponent(o.stack)}/commit`,
    commitBody: stackCommit(edit),
    onCommitted: () => o.onCommitted?.(),
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
 *   addLabel: string, driveKey: string, offable?: string,
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
  offable,
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
    columns: [
      ...columns,
      ...(offable ? [{ label: "Status", sort: "text" }] : []),
      { label: "Change", sort: "text" },
    ],
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
          ...(offable
            ? [td(offWords(r.row), r.row.off ? "off-reason" : "")]
            : []),
          h(
            "td",
            { class: "row-buttons" },
            btn("Edit", "edit"),
            ...(offable
              ? [
                  drivable(
                    r.row.off ? btn("Turn on", "on") : btn("Turn off…", "off"),
                    r.row.off ? CHECK_ON : CHECK_OFF,
                    `${offable}:${i + 1}`,
                  ),
                ]
              : []),
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
        else
          setRows(
            rows.map((r, j) =>
              j === i ? { ...r, row: keepHidden(r.row, newRow) } : r,
            ),
          );
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
      case "off": {
        const row = rows[i].row;
        void askOffReason(`Turn off · ${summary(row)}`).then((why) => {
          if (why == null) return;
          setRows(
            rows.map((r, j) =>
              j === i ? { ...r, row: { ...r.row, off: why } } : r,
            ),
          );
        });
        break;
      }
      case "on":
        setRows(
          rows.map((r, j) => {
            if (j !== i) return r;
            const { off, ...rest } = r.row;
            void off;
            return { ...r, row: rest };
          }),
        );
        break;
    }
  });
  // kp-themes 11.2.0's row reorder, beside the Up and Down buttons Live view
  // presses: a handle (or Alt+Up / Alt+Down) moves a row.
  t.wrap.setAttribute("data-kp-rows-reorder", "");
  t.wrap.addEventListener("kp-datatable-reorder", (ev) => {
    const { from, to } = /** @type {CustomEvent} */ (ev).detail;
    if (!rows[from] || to < 0 || to >= rows.length) return;
    const next = [...rows];
    const [moved] = next.splice(from, 1);
    next.splice(to, 0, moved);
    setRows(next);
    /** @type {HTMLElement | null} */ (
      t.tbody.querySelector(
        `tr[data-kp-row-key="${to}"] [data-kp-reorder-handle]`,
      )
    )?.focus({ preventScroll: true });
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
    body: [t.wrap],
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
  const removeRows = apps.map((a) => {
    const x = editRow(appsRemoveField(a), false, () => {});
    removeBoxes.set(a, /** @type {HTMLInputElement} */ (x.input));
    return x.row;
  });
  let addBlankText = "";
  const addBlankX = editRow(appsAddBlankField(), "", (v) => {
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
    onChange: () => storageG.sync(),
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
    onChange: () => mountsG.sync(),
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
    onChange: () => logsG.sync(),
  });
  const storageG = tableGroup({
    id: "group-storage",
    title: "Storage",
    desc: "Folders on the host bind-mounted into the container; each is backed up nightly unless marked no_backup.",
    table: storage,
    start: startStorage,
  });
  const mountsG = tableGroup({
    id: "group-data-mounts",
    title: "Data mounts",
    desc: "Large host datasets mounted read-write into the container (a film library, a download folder); never backed up by this stack.",
    table: dataMounts,
    start: startDataMounts,
  });
  const logsG = tableGroup({
    id: "group-log-files",
    title: "Log files",
    desc: "Log files inside the container that the dashboard rotates and caps, so they never fill the disk.",
    table: logFiles,
    start: startLogFiles,
  });

  const { node, bar } = editCard({
    id: "apps-form",
    form: "apps",
    title: "Apps & storage",
    desc: "Remove or add the stack's apps, and the folders and logs they use. Removing an app drops it from apps:, its directory and its storage entries; data already on the container is untouched.",
    groups: [
      editGroup({
        id: "group-apps-remove",
        title: "Remove apps",
        desc: apps.length
          ? "A ticked app is dropped from the stack when the change is committed."
          : "This stack has no apps yet, so there is nothing to remove.",
        rows: removeRows,
      }),
      editGroup({
        id: "group-apps-add",
        title: "Add blank apps",
        desc: "A blank app gets a directory and a minimal docker-compose.yml to fill in; an app from a preset is added in the card above.",
        rows: [addBlankX.row],
      }),
    ],
    blocks: [storageG.block, mountsG.block, logsG.block],
  });
  wireBar(
    bar,
    () => {
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
        return null;
      }
      return openPlanDialog({
        id: `edit:apps:${stack}`,
        title: `Apps & storage · ${stack}`,
        planUrl: `/data/stacks/${encodeURIComponent(stack)}/plan`,
        planBody: { edit },
        commitUrl: `/data/stacks/${encodeURIComponent(stack)}/commit`,
        commitBody: stackCommit(edit),
        onCommitted: () => void reload(),
      });
    },
    tableHooks([storageG, mountsG, logsG]),
  );
  return {
    node,
    bar,
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

const RESTART_MOVE = declare({
  id: "stack-restart-order-move",
  page: "stack",
  opens: "view",
  row: "<stack>/<unit>/<earlier|later>",
  at: (row) =>
    row ? `/stacks/${encodeURIComponent(row.split("/")[0])}/settings` : null,
  what: "move a native unit earlier or later in the stack's restart order (saved with the Latch card)",
  shows: "on the Settings tab of a stack with two or more native units",
});

/**
 * The restart order (gap 74, feat 6.5): the stack's native units in the
 * order a deploy goes through them, and so the order a changed secret file
 * restarts them in, on kp's reorder list (a handle, arrow keys and drag)
 * with Earlier / Later buttons beside each unit for Live view and for a
 * hand that does not drag. The order is the `natives:` list of the stack
 * file; the card's Save writes it.
 * @param {string} stack
 * @param {string[]} natives
 * @param {() => void} onChange
 */
function restartOrderGroup(stack, natives, onChange) {
  const start = [...natives];
  const list = h("ul", {
    class: "kp-reorder rs-order",
    "data-kp-reorder": "",
    "aria-label": `Restart order of ${stack}`,
  });
  /** @param {string} unit */
  const item = (unit) => {
    /** @param {"earlier" | "later"} way */
    const mover = (way) => {
      const b = h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--sm",
          title: `Restart ${unit} ${way}: ${way === "earlier" ? "before" : "after"} the unit ${way === "earlier" ? "above" : "below"} it`,
        },
        way === "earlier" ? "Earlier" : "Later",
      );
      b.addEventListener("click", () => {
        const li = b.closest("li");
        if (!li) return;
        if (way === "earlier" && li.previousElementSibling)
          list.insertBefore(li, li.previousElementSibling);
        else if (way === "later" && li.nextElementSibling)
          list.insertBefore(li.nextElementSibling, li);
        else return;
        changed();
      });
      return drivable(b, RESTART_MOVE, `${stack}/${unit}/${way}`);
    };
    return h(
      "li",
      { "data-kp-item": unit, "data-kp-label": unit },
      h(
        "button",
        {
          type: "button",
          "data-kp-handle": "",
          "aria-label": `Move ${unit}`,
        },
        "⠿",
      ),
      h("span", { class: "rs-order__n" }),
      h("b", { class: "rs-order__name" }, unit),
      h("span", { class: "rs-order__moves" }, mover("earlier"), mover("later")),
    );
  };
  list.append(...start.map(item));
  const order = () =>
    [...list.children].map(
      (c) => /** @type {HTMLElement} */ (c).dataset.kpItem ?? "",
    );
  const block = editBlock({
    id: "group-restart-order",
    title: "Restart order",
    desc: "The order a deploy goes through this stack's native units, and so the order a changed secret file restarts them in: the first listed restarts first. Drag a handle, press an arrow key on it, or use Earlier and Later.",
    body: [list],
  });
  block.dataset.kpName = "Restart order";
  const changed = () => {
    [...list.children].forEach((li, i) => {
      const n = li.querySelector(".rs-order__n");
      if (n) n.textContent = `${i + 1}`;
    });
    const now = order();
    markRow(
      block,
      now.join() === start.join()
        ? null
        : { was: start.join(", then "), now: now.join(", then ") },
    );
    onChange();
  };
  list.addEventListener("kp-reorder", changed);
  changed();
  return {
    block,
    title: "Restart order",
    /** The new order, or null while it is as the file has it. */
    value: () => (order().join() === start.join() ? null : order()),
    sync: changed,
    reset: () => {
      list.replaceChildren(...start.map(item));
      changed();
    },
  };
}

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
  const secretRows = apps.map((a) => {
    const x = editRow(
      latchSecretField(a),
      latch.latch_secrets.includes(a),
      () => {},
    );
    secretBoxes.set(a, /** @type {HTMLInputElement} */ (x.input));
    return x.row;
  });
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
    onChange: () => filesG.sync(),
  });
  const filesG = tableGroup({
    id: "group-latch-files",
    title: "Secret files",
    desc: "Secret files this stack receives from latch at deploy time, and where they land in the container.",
    table: files,
    start: startFiles,
  });
  const natives = m.natives ?? [];
  const orderG =
    natives.length > 1 ? restartOrderGroup(stack, natives, () => {}) : null;
  const { node, bar } = editCard({
    id: "latch-form",
    form: "latch",
    title: "Latch",
    desc: "Which apps' .env comes from latch, and which secret files this stack gets from latch. A value must never contain '${': latch --expand parses every file it is given, so one unresolvable placeholder would break every stack's secrets.",
    groups: [
      editGroup({
        id: "group-latch-secrets",
        title: "Apps whose .env comes from latch",
        desc: apps.length
          ? "A ticked app reads its .env from latch instead of a plaintext file."
          : "This stack has no apps yet, so there is no .env to move to latch.",
        rows: secretRows,
      }),
    ],
    blocks: [filesG.block, ...(orderG ? [orderG.block] : [])],
  });
  wireBar(
    bar,
    () => {
      const wantSecrets = apps.filter((a) => secretBoxes.get(a)?.checked);
      const secretErrors = latchSecretsProblems(wantSecrets);
      if (Object.keys(secretErrors).length) {
        notify(Object.values(secretErrors)[0], "warning");
        return null;
      }
      const secretsChanged =
        JSON.stringify([...wantSecrets].sort()) !==
        JSON.stringify([...latch.latch_secrets].sort());
      const edit = latchBody({
        secrets: secretsChanged ? wantSecrets : null,
        files: rowsChanged(files.getRows(), startFiles)
          ? files.getRows()
          : null,
        order: orderG?.value() ?? null,
      });
      if (!changesSomething(edit)) {
        notify("Nothing is changed yet.", "info");
        return null;
      }
      return openPlanDialog({
        id: `edit:latch:${stack}`,
        title: `Latch · ${stack}`,
        planUrl: `/data/stacks/${encodeURIComponent(stack)}/plan`,
        planBody: { edit },
        commitUrl: `/data/stacks/${encodeURIComponent(stack)}/commit`,
        commitBody: stackCommit(edit),
        onCommitted: () => void reload(),
      });
    },
    tableHooks(orderG ? [filesG, orderG] : [filesG]),
  );
  return {
    node,
    bar,
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
    onChange: () => {
      const rows = table.getRows();
      markRow(
        tg.block,
        tilesEditBody(rows, originals).tiles.length > 0
          ? {
              was: `${startRows.length} ${startRows.length === 1 ? "tile" : "tiles"}`,
              now: `${rows.length} ${rows.length === 1 ? "tile" : "tiles"}, edited`,
            }
          : null,
      );
    },
  });
  const tg = tableGroup({
    id: "group-tiles",
    title: "Tiles",
    desc: "This stack's tiles on the dashboard's start page, keyed by the hostname each one opens. Renaming a tile's hostname keeps it; \"probe\" is never set here, the client fills it in at deploy time.",
    table,
    start: startRows,
  });
  const { node, bar } = editCard({
    id: "tiles-form",
    form: "tiles",
    title: "Tiles",
    desc: "The tiles this stack puts on the start page: their hostname, name, group and order.",
    blocks: [tg.block],
  });
  wireBar(
    bar,
    () => {
      const edit = tilesEditBody(table.getRows(), originals);
      if (edit.tiles.length === 0) {
        notify("Nothing is changed yet.", "info");
        return null;
      }
      return openPlanDialog({
        id: `edit:tiles:${stack}`,
        title: `Tiles · ${stack}`,
        planUrl: `/data/stacks/${encodeURIComponent(stack)}/plan`,
        planBody: { edit },
        commitUrl: `/data/stacks/${encodeURIComponent(stack)}/commit`,
        commitBody: stackCommit(edit),
        onCommitted: () => void reload(),
      });
    },
    {
      revert: () => {
        table.setRows(startRows);
        return true;
      },
      discard: () => table.setRows(startRows),
    },
  );
  return {
    node,
    bar,
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
  // fix-239: `homelab ui open checks <stack>/<app>` reaches this tab's rows.
  viaForm(panel, "checks");
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
  const checksByApp = checksOf(e);
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
    class: "measured kp-state-word",
    "data-kp-words": "Changed; not committed.",
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
      offable: "checks",
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
      offable: "probes",
      rows: startProbes,
      onChange: () => repaintStatus(),
    });
    busyField.value = view.busy_check?.command ?? "";
    urlField.value = view.url ?? "";
    body.replaceChildren(
      h("h3", null, "checks:"),
      h(
        "p",
        { class: "measured" },
        "Measured checks: a command or URL the dashboard runs to decide this app is healthy.",
      ),
      ...checksTable.wrap,
      h("h3", null, "manual:"),
      h(
        "p",
        { class: "measured" },
        "Questions only a person can answer (for example: does the audio stay in sync); asked again when the app's files change.",
      ),
      ...manualTable.wrap,
      h("h3", null, "probes:"),
      h(
        "p",
        { class: "measured" },
        "Nightly probes: a deeper test run once a night, whose result shows on Health.",
      ),
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
 * Give a control its value and make that its saved value too, which is what
 * kp's unsaved bar compares an edit against.
 * @param {HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement} el
 * @param {string | boolean} value
 */
function setBase(el, value) {
  if (el instanceof HTMLInputElement && el.type === "checkbox") {
    el.defaultChecked = value === true;
    el.checked = value === true;
  } else if (el instanceof HTMLSelectElement) {
    el.value = String(value);
    for (const o of el.options) o.defaultSelected = o.value === el.value;
  } else {
    el.defaultValue = String(value);
    el.value = String(value);
  }
}

/**
 * A labelled text field as a row of the grid, for the native form's plain
 * (non-`editforms.js`) fields — one `service.yml` key each, its own stable
 * id for Live view.
 * @param {string} id
 * @param {string} label
 * @param {string} value
 * @param {{textarea?: boolean, help?: string, placeholder?: string}} [opts]
 */
function nativeField(id, label, value, opts = {}) {
  const input = opts.textarea
    ? h("textarea", {
        class: "kp-field__input kp-field__input--multiline mono",
        id,
        rows: "3",
        spellcheck: "false",
        "data-kp-autosize": "",
        "data-kp-autosize-max": "12",
      })
    : h("input", {
        class: "kp-field__input mono",
        id,
        type: "text",
        placeholder: opts.placeholder ?? "",
      });
  setBase(input, value);
  return {
    input,
    row: gridRow({
      label,
      labelFor: id,
      control: h(
        "div",
        { class: "kp-field" },
        input,
        h("p", {
          class: "kp-field__error",
          "data-kp-field-error": "",
          hidden: "",
        }),
      ),
      value: opts.help,
    }),
  };
}

/**
 * A `<select>` field for a fixed picker (`update_policy`, `metrics`).
 * @param {string} id
 * @param {string} label
 * @param {{value: string, label: string}[]} options
 * @param {string} current
 * @param {string} [help]
 */
function nativeChoice(id, label, options, current, help) {
  const input = h(
    "select",
    { class: "kp-field__input", id },
    ...options.map((o) => h("option", { value: o.value }, o.label)),
  );
  setBase(input, current);
  return {
    input,
    row: gridRow({ label, labelFor: id, control: input, value: help }),
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
    { class: "kp-field__input", id: "native-unit", "data-kp-untracked": "" },
    ...natives.map((n) => h("option", { value: n.unit }, n.unit)),
  );
  const warn = h("div", {
    class: "kp-alert kp-alert--warning",
    role: "status",
    hidden: "",
  });
  const binary = nativeField("native-binary", "Binary", "", {
    help: "The executable systemd starts.",
  });
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
  const statelessRow = gridRow({
    label: "Stateless",
    labelFor: "native-stateless",
    control: stateless,
    value: "No data_dirs, by design.",
  });
  const restoreNote = nativeField("native-restore-note", "Restore note", "", {
    textarea: true,
    help: "Printed in the DR runbook under this unit.",
  });
  const releaseRepo = nativeField("native-release-repo", "Release repo", "", {
    placeholder: "owner/name",
    help: "The GitHub repository whose releases update the unit.",
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
  const afterRestore = nativeField(
    "native-after-restore",
    "After restore",
    "",
    {
      textarea: true,
      help: "A shell one-liner run inside the container after a snapshot is unpacked and before the unit starts. Empty: nothing runs.",
    },
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
    "Whether the unit is quiet while its data is archived.",
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
    "Who updates the unit, and when.",
  );
  const metrics = nativeChoice(
    "native-metrics",
    "Metrics",
    [
      { value: "measured", label: "Measured (default)" },
      { value: "not_measured", label: "Not measured (noted, not drift)" },
    ],
    "measured",
    "Whether a missing metrics endpoint counts as drift.",
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
  const pathLine = h("p", { class: "measured", id: "native-path" });
  const unitRow = gridRow({
    label: "Unit",
    labelFor: "native-unit",
    control: sel,
    value: pathLine,
  });
  const show = () => {
    const n = natives.find((x) => x.unit === sel.value) ?? natives[0];
    pathLine.textContent = n
      ? `${n.path} · unit file ${n.unit_file_path} (edited in the Files card).`
      : "";
    const m = n?.manifest;
    warn.hidden = Boolean(m);
    warn.textContent = m
      ? ""
      : `${n?.path ?? "this unit's service.yml"} does not read as a native service: use the Files card.`;
    setBase(binary.input, m?.binary ?? "");
    setBase(envFile.input, m?.env_file ?? "");
    setBase(dataDirs.input, (m?.data_dirs ?? []).join("\n"));
    setBase(updateCmd.input, m?.update_cmd ?? "");
    setBase(stateless, m?.stateless ?? false);
    setBase(restoreNote.input, m?.restore_note ?? "");
    setBase(releaseRepo.input, m?.release_repo ?? "");
    setBase(releaseAsset.input, m?.release_asset ?? "");
    setBase(backupNewest.input, m?.backup_from_newest ?? "");
    setBase(afterRestore.input, m?.after_restore ?? "");
    setBase(
      backupPause.input,
      m?.backup_pause === "chassis"
        ? "chassis"
        : m?.backup_pause
          ? "true"
          : "false",
    );
    setBase(updatePolicy.input, m?.update_policy ?? "manual");
    setBase(metrics.input, m?.metrics === false ? "not_measured" : "measured");
  };
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
    after_restore: afterRestore.input.value,
    backup_pause: /** @type {"false" | "true" | "chassis"} */ (
      backupPause.input.value
    ),
    update_policy: /** @type {"manual" | "auto" | "self"} */ (
      updatePolicy.input.value
    ),
    metrics: /** @type {"measured" | "not_measured"} */ (metrics.input.value),
  });
  const { node, bar } = editCard({
    id: "native-form",
    form: "native",
    title: "Edit a native unit",
    desc: "Change one unit's service.yml: its binary, state directories and update policy.",
    groups: [
      editGroup({
        id: "group-native-unit",
        title: "Unit",
        desc: "The systemd unit whose service.yml is edited; the fields below follow the unit you pick.",
        rows: [unitRow],
      }),
      editGroup({
        id: "group-native-program",
        title: "Program",
        desc: "What systemd starts, the environment it reads and the release repository that updates it.",
        rows: [binary.row, envFile.row, releaseRepo.row, releaseAsset.row],
      }),
      editGroup({
        id: "group-native-update",
        title: "Update and metrics",
        desc: "Who updates the unit, how, and whether a missing metrics endpoint counts as drift.",
        rows: [updatePolicy.row, updateCmd.row, metrics.row],
      }),
      editGroup({
        id: "group-native-backup",
        title: "Data, backup and restore",
        desc: "What the nightly backup archives, how the unit is paused for it and how a restore puts the data back.",
        rows: [
          statelessRow,
          dataDirs.row,
          backupNewest.row,
          backupPause.row,
          afterRestore.row,
          restoreNote.row,
        ],
      }),
      gridRow({
        control: buttonGroup([removeBtn]),
        buttons: true,
      }),
    ],
    blocks: [warn],
  });
  // Another unit is another saved state, not an edit.
  sel.addEventListener("change", () => {
    show();
    markUnsavedClean(bar);
  });
  wireBar(bar, () =>
    openPlanDialog({
      id: `edit:native:${stack}`,
      title: `Native services · ${stack}`,
      planUrl: `/data/stacks/${encodeURIComponent(stack)}/plan`,
      planBody: { edit: edit() },
      commitUrl: `/data/stacks/${encodeURIComponent(stack)}/commit`,
      commitBody: stackCommit(edit()),
      onCommitted: () => void reload(),
    }),
  );
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
  return { node, bar };
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
    help: "The executable systemd starts.",
  });
  const envFile = nativeField("add-native-env-file", "Environment file", "", {
    placeholder: "/appdata/…/….env",
    help: "Empty: the unit has no EnvironmentFile.",
  });
  const dataDirs = nativeField(
    "add-native-data-dirs",
    "Data directories (one per line)",
    "",
    { textarea: true, help: "What the nightly backup archives." },
  );
  const notifyChk = h("input", {
    type: "checkbox",
    id: "add-native-notify",
    class: "kp-field__check",
  });
  notifyChk.checked = true;
  const review = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--primary",
      id: "add-native-review",
    },
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
  const { node } = editCard({
    id: "add-native",
    form: "add-native",
    bar: false,
    title: "Add a native unit",
    desc: "A bare binary under systemd, no docker layer: its service.yml and a generic unit file, both under a new <unit>/ directory.",
    groups: [
      editGroup({
        id: "group-add-native",
        title: "The new unit",
        desc: "Its name, the binary systemd starts and what the nightly backup archives; nothing is written until the plan is committed.",
        rows: [
          unit.row,
          binary.row,
          envFile.row,
          dataDirs.row,
          gridRow({
            label: "Type=notify",
            labelFor: "add-native-notify",
            control: notifyChk,
            value: "The chassis-rs kit's shape.",
          }),
        ],
      }),
      gridRow({ control: buttonGroup([review]), buttons: true }),
    ],
  });
  return node;
}

/**
 * @param {string} stack
 * @param {EditRead} e
 * @param {() => Promise<void>} reload
 * @returns {{node: HTMLElement, bar: HTMLElement | null}}
 */
function nativeCard(stack, e, reload) {
  const unitForm = e.natives.length
    ? nativeUnitForm(stack, e.natives, reload)
    : null;
  const node = h(
    "div",
    { id: "native-services", class: "edit-native" },
    unitForm
      ? unitForm.node
      : h(
          "section",
          { class: "kp-card edit-card", "aria-label": "Native services" },
          h("h2", { class: "edit-card__title" }, "Native services"),
          h(
            "p",
            { class: "measured" },
            "This stack has no native unit yet; add one below. Each unit's service.yml holds its binary, its state and its own update policy.",
          ),
        ),
    addNativeCard(stack, reload),
  );
  return { node, bar: unitForm?.bar ?? null };
}

// ── feat-firewall-1 ─────────────────────────────────────────────────────

/**
 * @param {HTMLElement} panel
 * @param {{name: string}} params
 * @returns {() => void}
 */
export function firewallTab(panel, params) {
  const stack = params.name;
  // fix-239: `homelab ui open firewall <stack>` reaches this tab's rows.
  viaForm(panel, "firewall");
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
  // redesign-config-8: the Firewall page's "Move up" / "Disable" arrive as
  // ?rule=<n>&do=up|disable|enable: staged here like any edit, reviewed
  // and written through this editor's own Review and commit. The address
  // forgets them at once, so a reload or the read after a commit does
  // not stage them twice.
  const q = new URLSearchParams(location.search);
  /** @type {number | null} the staged rule's row, to point at */
  let staged = null;
  const act = q.get("do");
  if (act && q.get("rule")) {
    const n = Number(q.get("rule"));
    const next = ruleIntent(model, n, act);
    if (next !== model) staged = act === "up" ? n - 2 : n - 1;
    model = next;
    q.delete("do");
    q.delete("rule");
    const rest = q.toString();
    history.replaceState(
      history.state,
      "",
      `${location.pathname}${rest ? `?${rest}` : ""}`,
    );
  }
  const state = h("span", { class: "state" });
  const dirty = h("span", {
    class: "measured kp-state-word",
    "data-kp-words": "Changed; not committed.",
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
  t.wrap.setAttribute("data-kp-rows-reorder", "");
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
            class: [
              re.origin === null ? "rule-new" : "",
              r.disabled ? "rule-off" : "",
              i === staged ? "rule-staged" : "",
            ]
              .filter(Boolean)
              .join(" "),
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
          td(
            [
              r.disabled ? "off: Proxmox skips it" : "",
              r.note,
              r.comment?.split("\n")[0],
            ]
              .filter(Boolean)
              .join(" · "),
          ),
          h(
            "td",
            { class: "row-buttons" },
            btn("Edit", "edit"),
            btn("Up", "up", i === 0),
            btn("Down", "down", i === model.rules.length - 1),
            btn(r.disabled ? "Enable" : "Disable", "toggle"),
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
      case "toggle":
        model = toggleRuleOff(model, i);
        paintRules();
        break;
      case "delete":
        model = { ...model, rules: model.rules.filter((_, j) => j !== i) };
        paintRules();
        break;
      case "edit":
        openRuleDialog(model.rules[i].rule, (rule) => {
          // The dialog edits the rule's fields; whether it is off stays.
          if (model.rules[i].rule.disabled) rule = { ...rule, disabled: true };
          model = {
            ...model,
            rules: model.rules.map((x, j) => (j === i ? { ...x, rule } : x)),
          };
          paintRules();
        });
        break;
    }
  });
  // kp-themes 11.2.0's row reorder: the handle (and Alt+Up / Alt+Down on it)
  // moves a rule; the table says where it went, the model follows and the
  // rows are drawn again with their numbers.
  t.wrap.addEventListener("kp-datatable-reorder", (ev) => {
    const { from, to } = /** @type {CustomEvent} */ (ev).detail;
    model = moveRuleTo(model, from, to);
    paintRules();
    /** @type {HTMLElement | null} */ (
      t.tbody.querySelector(`tr[data-rule="${to}"] [data-kp-reorder-handle]`)
    )?.focus({ preventScroll: true });
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
        h("a", { href: "/firewall" }, "The fleet's matrix"),
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
  if (staged !== null)
    t.tbody
      .querySelector(`tr[data-rule="${staged}"]`)
      ?.scrollIntoView({ block: "center" });
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
