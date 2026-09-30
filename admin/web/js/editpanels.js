// The stack page's two edit tabs. Settings (feat-stacks-2): the settings
// form, any file of the stack in the raw editor, and a preset's app added
// (feat-stacks-3). Firewall (feat-firewall-1): the stack's rules as a kp
// datatable with add, edit, move and delete, and its options. Every change
// ends in the plan dialog (editui.js): the plan first, then the commit,
// then optionally a deploy of exactly that commit.

import { notify, openDialog, refusalCallout } from "./actui.js";
import {
  changesSomething,
  checkFields,
  firewallBody,
  firewallChanged,
  firewallModel,
  moveRule,
  ruleFields,
  ruleFromValues,
  ruleProblems,
  ruleSummary,
  settingsBody,
  settingsForm,
  startValues,
  tileProblems,
} from "./editforms.js";
import {
  editField,
  markErrors,
  openPlanDialog,
  stackCommit,
} from "./editui.js";
import { badgeCell, errorBox, fetchJson, h, tableBlock, td } from "./dom.js";
import { driven, register } from "./drivehooks.js";
import { attachDataTables, dataTable } from "/static/kp/js/datatable.js";
import { attachSwitches } from "/static/kp/js/forms.js";

/**
 * @typedef {{stack: string, head: {commit: string, subject: string, at: number} | null,
 *   sync_error: string | null, texts: Record<string, string>,
 *   manifest: import("./editforms.js").ManifestView | null,
 *   manifest_error: string | null, images: Record<string, string>,
 *   self_stack: string, presets: {name: string, description: string, apps: string[]}[]}} EditRead
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
  const load = async () => {
    const r = await readEdit(stack, abort.signal);
    if (!r.ok) {
      panel.replaceChildren(errorBox(r.error));
      return;
    }
    const e = /** @type {EditRead} */ (r.body);
    // feat-platform-10: the Live view replay presses these very buttons.
    unregister();
    unregister = register(`stack-edit:${stack}`, {
      openRaw: () => {
        const d = panel.querySelector("#raw-editor");
        if (d instanceof HTMLDetailsElement) d.open = true;
      },
      review: (/** @type {string} */ family) =>
        /** @type {HTMLElement | null} */ (
          panel.querySelector(
            family === "raw"
              ? "#raw-review"
              : family === "add-app"
                ? "#add-app-review"
                : "#settings-review",
          )
        ),
    });
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
    parts.push(rawCard(stack, e, load));
    if (e.manifest && e.presets.length) parts.push(addAppCard(stack, e, load));
    panel.replaceChildren(...parts);
  };
  void load().catch(() => {});
  return () => {
    abort.abort();
    unregister();
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
 * @param {string} stack
 * @param {EditRead} e
 * @param {() => Promise<void>} reload
 */
function rawCard(stack, e, reload) {
  const files = Object.keys(e.texts).sort();
  const sel = h(
    "select",
    { class: "kp-field__input", id: "raw-file" },
    ...files.map((f) => h("option", { value: f }, f)),
  );
  const area = h("textarea", {
    class: "kp-field__input mono raw-text",
    id: "raw-text",
    rows: "18",
    spellcheck: "false",
    "aria-label": "The file's text",
  });
  const show = () => (area.value = e.texts[sel.value] ?? "");
  sel.value = files.includes("lxc-compose.yml")
    ? "lxc-compose.yml"
    : (files[0] ?? "");
  show();
  sel.addEventListener("change", show);
  const review = h(
    "button",
    { type: "button", class: "kp-button", id: "raw-review" },
    "Review and commit…",
  );
  review.addEventListener("click", () => {
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
    { class: "kp-card edit-card", id: "raw-editor" },
    h("summary", null, h("strong", null, "Edit a file of the stack")),
    h(
      "p",
      { class: "measured" },
      "Anything the form does not cover. Secrets (.env) are not here: they live in latch.",
    ),
    h(
      "div",
      { class: "kp-field" },
      h("label", { class: "kp-field__label", for: "raw-file" }, "File"),
      sel,
    ),
    area,
    h("div", { class: "row-buttons" }, review),
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
  const review = h(
    "button",
    { type: "button", class: "kp-button", id: "add-app-review" },
    "Review and commit…",
  );
  review.addEventListener("click", () => {
    const edit = { kind: "add_app", preset: sel.value };
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
    h("div", { class: "row-buttons" }, review),
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
