// feat-stacks-4, feat-stacks-5, feat-stacks-7, feat-stacks-8: every action's
// form described once, as data. The dialog draws its wizard from this
// description, the batch and schedule forms reuse its fields, and milestone
// `follow` (feat-platform-10) can replay the same description step by step:
// every field has a stable id, every step a name, and `buildArgs` is the one
// place where the typed values become the request body the server takes.
//
// Pure: no DOM, no fetch, no clock.

import SPEC from "./formspec.json" with { type: "json" };

/**
 * The catalog row the server sends (GET /data/actions/catalog).
 * @typedef {"force" | "confirm" | "snapshot" | "app" | "unit" |
 *   "skip_backup" | "skip_safety_copy" | "commit" | "vmid" | "command" |
 *   "tag" | "version" | "privileged" | "base" | "check" | "verdict" |
 *   "days" | "note" | "destroy" | "leave_out" | "destroy_ids" |
 *   "destroy_ack"} ArgName
 * @typedef {{action: string, target: "stack" | "host", label: string,
 *   what: string, scope: string, needs: string, args: ArgName[],
 *   confirm: boolean, refused_for_self: boolean,
 *   destructive: boolean}} CatalogEntry
 *   destructive (fix-255): the one thing that paints an action red
 * @typedef {{actions: CatalogEntry[], host_target: string,
 *   self_stack: string}} Catalog
 */

/**
 * One field of a form. `source` names the list a choice is filled from
 * (the stack's apps, its native units, its commits); `when` says the field
 * only shows while the preview reports a deploy-guard refusal. `show_when`:
 * the field is on screen, checked and sent only while another field has a
 * value (the answer's days, with accept); `change_when`: its label, help
 * and required then (the note becomes the required reason).
 * @typedef {{field: string, value: string, says: string}} ShowWhen
 * @typedef {{field: string, value: string, label: string, help: string,
 *   required: boolean}} ChangeWhen
 * @typedef {{name: ArgName, id: string, kind: "check" | "text" | "choice" | "typed",
 *   label: string, help: string, required: boolean, pattern?: string,
 *   placeholder?: string,
 *   source?: "apps" | "units" | "commits" | "checks" | "templates" | "releases",
 *   empty?: string, danger?: boolean, when?: "guard", expect?: string,
 *   choices?: {value: string, label: string}[],
 *   show_when?: ShowWhen, change_when?: ChangeWhen}} Field
 * @typedef {{id: "options" | "review", label: string, fields: Field[]}} Step
 * @typedef {{id: string, action: string, target: "stack" | "host",
 *   stack: string, title: string, what: string, submit: string,
 *   steps: Step[], confirmName: string | null, refused: string | null,
 *   destructive: boolean, runPath: string, previewPath: string,
 *   intro: string | null}} ActionForm
 * @typedef {Record<string, string | boolean>} Values
 * @typedef {{force?: boolean, confirm?: string, snapshot?: string,
 *   app?: string, unit?: string, skip_backup?: boolean,
 *   skip_safety_copy?: boolean, commit?: string, vmid?: string,
 *   command?: string, tag?: string, version?: string, privileged?: boolean,
 *   base?: string, check?: string, verdict?: string, days?: string,
 *   note?: string, destroy?: string, leave_out?: string,
 *   destroy_ids?: string, destroy_ack?: boolean}} ActionArgs
 */

/** The step every form ends on. */
export const REVIEW = /** @type {const} */ ("review");

/** How the stack page groups its buttons, in order. */
export const GROUPS = /** @type {const} */ ([
  {
    group: "Deploy and update",
    actions: ["deploy", "update", "resize", "guards"],
  },
  {
    group: "Backups",
    actions: ["backup", "restore", "restore-native", "verify-restore"],
  },
  { group: "Secrets", actions: ["change-secret", "seal-env"] },
  { group: "Parking", actions: ["enable", "disable"] },
  {
    group: "Native services",
    actions: [
      "adopt",
      "backup-native",
      "update-native",
      "release-update-native",
      "install-native",
      "rollback-native",
    ],
  },
  {
    group: "Retire",
    actions: ["prune-orphans", "destroy", "forget", "wipe"],
  },
]);

/** Actions the roll back dialog starts, with its own list to pick from. */
export const ROLLBACK_ACTIONS = /** @type {const} */ ([
  "deploy-commit",
  "rollback-native",
]);

/**
 * One field's description in formspec.json, the file the dashboard's
 * server reads too (feat-platform-10): a step Claude sends is checked
 * against the same words a click is.
 * @typedef {{kind: Field["kind"], label: string, help: string,
 *   required: boolean, pattern?: string, placeholder?: string,
 *   source?: Field["source"], empty?: string, danger?: boolean,
 *   when?: "guard", expect?: string,
 *   list_only?: {label: string, help: string, required: boolean,
 *     placeholder: string},
 *   help_for?: Record<string, string>,
 *   label_for?: Record<string, string>,
 *   required_for?: Record<string, boolean>,
 *   choices?: {value: string, label: string}[],
 *   show_when?: ShowWhen, change_when?: ChangeWhen}} FieldDef
 */

/**
 * @param {string} template
 * @param {Record<string, string>} values
 */
export const fill = (template, values) =>
  template.replace(/\{(\w+)\}/g, (m, k) => values[k] ?? m);

/**
 * The field for one argument of one action.
 * @param {ArgName} arg
 * @param {CatalogEntry} entry
 * @param {string} stack
 * @returns {Field}
 */
export function argField(arg, entry, stack) {
  const def = /** @type {FieldDef} */ (SPEC.fields[arg]);
  const { list_only, help_for, label_for, required_for, pattern, ...base } =
    def;
  /** @type {Record<string, unknown>} */
  const merged = {
    ...base,
    ...(arg === "confirm" && !entry.confirm ? list_only : {}),
  };
  if (help_for && help_for[entry.action]) merged.help = help_for[entry.action];
  if (label_for && label_for[entry.action])
    merged.label = label_for[entry.action];
  if (required_for && entry.action in required_for)
    merged.required = required_for[entry.action];
  if (pattern)
    merged.pattern =
      SPEC.patterns[/** @type {keyof typeof SPEC.patterns} */ (pattern)];
  for (const [k, v] of Object.entries(merged))
    if (typeof v === "string") merged[k] = fill(v, { stack });
  return /** @type {Field} */ ({
    name: arg,
    id: `act-${arg.replace(/_/g, "-")}`,
    ...merged,
  });
}

/**
 * The field as the form stands with these values: null while its
 * `show_when` does not hold (hidden, not checked, not sent), its
 * `change_when` words laid on while that holds. The same as
 * core::drive::shown_field.
 * @template {{name: string, label: string, help: string, required: boolean,
 *   show_when?: ShowWhen, change_when?: ChangeWhen}} F
 * @param {F} f
 * @param {Record<string, unknown>} values
 * @returns {F | null}
 */
export function shownField(f, values) {
  const w = f.show_when;
  if (w && values[w.field] !== w.value) return null;
  const c = f.change_when;
  if (c && values[c.field] === c.value)
    return { ...f, label: c.label, help: c.help, required: c.required };
  return f;
}

/** Arguments asked on the review step, next to the preview. */
const REVIEW_ARGS = new Set(SPEC.review_args);

/**
 * Whether an action asks this argument on its review step: the typed name
 * and force always, and per action the ones a one-line form asks there
 * (exec's container and command).
 * @param {string} action
 * @param {string} arg
 */
export const onReview = (action, arg) =>
  REVIEW_ARGS.has(arg) || (REVIEW_FOR[action] ?? []).includes(arg);

/** Per action, the further arguments its review step asks. */
const REVIEW_FOR = /** @type {Record<string, string[]>} */ (SPEC.review_for);

/**
 * The whole form of one action on one target.
 * @param {CatalogEntry} entry
 * @param {{stack: string, selfStack: string, hostTarget?: string}} ctx
 *   stack: the stack, or the host target for a host-wide action
 * @returns {ActionForm}
 */
export function actionForm(entry, ctx) {
  const stack =
    entry.target === "host" ? (ctx.hostTarget ?? "_host") : ctx.stack;
  const fields = entry.args.map((a) => argField(a, entry, stack));
  const options = fields.filter((f) => !onReview(entry.action, f.name));
  const review = fields.filter((f) => onReview(entry.action, f.name));
  /** @type {Step[]} */
  const steps = [];
  if (options.length)
    steps.push({ id: "options", label: SPEC.steps.options, fields: options });
  steps.push({ id: REVIEW, label: SPEC.steps.review, fields: review });
  // redesign-final-h2: a host action that asks for a container's number
  // works on that one container, not on the whole host.
  const where =
    entry.target !== "host"
      ? stack
      : entry.args.includes("vmid")
        ? "one container"
        : "the whole host";
  const refused =
    stack === ctx.selfStack && entry.refused_for_self
      ? "The dashboard never does this to its own stack (arch-self); use the CLI from a workstation."
      : null;
  return {
    id: `action:${entry.action}`,
    action: entry.action,
    target: entry.target,
    stack,
    title: `${entry.label} · ${where}`,
    what: entry.what,
    submit: entry.label,
    steps,
    confirmName: entry.confirm ? stack : null,
    refused,
    destructive: entry.destructive === true,
    runPath: `/data/actions/${encodeURIComponent(stack)}/${entry.action}`,
    previewPath: `/data/actions/${encodeURIComponent(stack)}/${entry.action}/preview`,
    intro: /** @type {Record<string, string>} */ (SPEC.intro)[entry.action]
      ? fill(/** @type {Record<string, string>} */ (SPEC.intro)[entry.action], {
          stack,
        })
      : null,
  };
}

/**
 * The field names a preset value locks: the row or page that opened this
 * dialog already chose them, so the form shows them read-only with a
 * "Change" affordance instead of asking again (fix-216). `snapshot` is
 * never locked this way — it gets its own picker instead.
 */
export const LOCKABLE_FIELDS = /** @type {const} */ ([
  "app",
  "unit",
  "commit",
  "leave_out",
  "destroy",
  "destroy_ids",
]);

/**
 * Which of a form's fields should open locked: every `LOCKABLE_FIELDS`
 * name the form actually has, for which `preset` gives a non-empty value
 * (fix-216 — one shared mechanism, not a per-dialog list of what to lock).
 * @param {ActionForm} form
 * @param {Values} [preset]
 * @returns {string[]}
 */
export function lockedFields(form, preset = {}) {
  const names = new Set(formFields(form).map((f) => f.name));
  return LOCKABLE_FIELDS.filter(
    (n) => names.has(n) && typeof preset[n] === "string" && preset[n] !== "",
  );
}

/**
 * Every field of a form, in step order.
 * @param {ActionForm} form
 */
export const formFields = (form) => form.steps.flatMap((s) => s.fields);

/**
 * The values a fresh form starts with, `preset` laid over them (the roll
 * back dialog presets the commit or the unit).
 * @param {ActionForm} form
 * @param {Values} [preset]
 * @returns {Values}
 */
export function initialValues(form, preset = {}) {
  /** @type {Values} */
  const v = {};
  for (const f of formFields(form))
    v[f.name] =
      f.kind === "check" ? false : f.source === "releases" ? "latest" : "";
  for (const [k, x] of Object.entries(preset)) if (k in v) v[k] = x;
  return v;
}

/**
 * The request body: only the arguments that are set (the server refuses a
 * field the action does not take, and an empty one would be taken as set).
 * @param {ActionForm} form
 * @param {Values} values
 * @returns {ActionArgs}
 */
export function buildArgs(form, values) {
  /** @type {Record<string, string | boolean>} */
  const out = {};
  for (const f of formFields(form)) {
    if (!shownField(f, values)) continue;
    const v = values[f.name];
    if (f.kind === "check") {
      if (v === true) out[f.name] = true;
    } else if (typeof v === "string" && v.trim() !== "") out[f.name] = v.trim();
  }
  return /** @type {ActionArgs} */ (out);
}

/**
 * The body for the preview: the same as the run's, but a typed name that is
 * not (yet) right is left out; the server fills it in for the preview, so
 * the CLI line shows before the name is typed, and carries `--yes` only once
 * it is (cli-yes). A wipe keeps its list-only meaning unless the name was
 * typed right.
 * @param {ActionForm} form
 * @param {Values} values
 * @returns {ActionArgs}
 */
export function previewArgs(form, values) {
  const args = buildArgs(form, values);
  if (args.confirm !== undefined && args.confirm !== form.stack)
    delete args.confirm;
  return args;
}

/**
 * Whether the typed name matches (the preview's CLI line then carries
 * `--yes`), so the dialog knows when to read the preview again.
 * @param {ActionForm} form
 * @param {Values} values
 */
export const nameTyped = (form, values) =>
  typeof values.confirm === "string" && values.confirm.trim() === form.stack;

/**
 * What is wrong with the values of one step (or of the whole form), by
 * field name. A guard-only field is not checked here: it is a choice; a
 * field its `show_when` hides is not checked either.
 * @param {ActionForm} form
 * @param {Values} values
 * @param {"options" | "review"} [step]
 * @returns {Record<string, string>}
 */
export function checkValues(form, values, step) {
  /** @type {Record<string, string>} */
  const errors = {};
  const steps = step ? form.steps.filter((s) => s.id === step) : form.steps;
  for (const field of steps.flatMap((s) => s.fields)) {
    const f = shownField(field, values);
    if (!f) continue;
    const v = values[f.name];
    const text = typeof v === "string" ? v.trim() : "";
    if (f.kind === "check") continue;
    const words = {
      expect: f.expect ?? "",
      label: f.label.toLowerCase(),
      text,
    };
    if (f.required && text === "") {
      errors[f.name] = fill(
        f.kind === "typed" ? SPEC.messages.typed_missing : SPEC.messages.choose,
        words,
      );
      continue;
    }
    if (text === "") continue;
    if (f.kind === "typed" && text !== f.expect) {
      errors[f.name] = fill(SPEC.messages.typed_wrong, words);
      continue;
    }
    if (f.pattern && !new RegExp(`^(?:${f.pattern})$`).test(text))
      errors[f.name] = fill(SPEC.messages.pattern, words);
  }
  return errors;
}

/**
 * The choices of a `choice` field, from its own list or the lists the page
 * has read.
 * @param {Field} field
 * @param {{apps?: string[], units?: string[],
 *   commits?: {commit: string, subject: string}[],
 *   checks?: {id: string, label: string}[], templates?: string[],
 *   releases?: {value: string, label: string, disabled: boolean}[]}} sources
 * @returns {{value: string, label: string, disabled?: boolean}[]}
 */
export function fieldChoices(field, sources) {
  // dashboard-latest: the "release tag" dropdown is built server-side
  // ("latest (now vX.Y.Z)" first, unsigned releases disabled) — no
  // placeholder row, it is never empty while the repository has a release.
  if (field.source === "releases") return sources.releases ?? [];
  /** @type {{value: string, label: string}[]} */
  const out = [];
  if (!field.required) out.push({ value: "", label: field.empty ?? "None" });
  else out.push({ value: "", label: `Choose…` });
  if (field.choices) out.push(...field.choices);
  if (field.source === "checks")
    for (const c of sources.checks ?? [])
      out.push({ value: c.id, label: c.label });
  if (field.source === "templates")
    for (const t of sources.templates ?? []) out.push({ value: t, label: t });
  if (field.source === "apps")
    for (const a of sources.apps ?? []) out.push({ value: a, label: a });
  if (field.source === "units")
    for (const u of sources.units ?? []) out.push({ value: u, label: u });
  if (field.source === "commits")
    for (const c of sources.commits ?? [])
      out.push({
        value: c.commit,
        label: `${c.commit.slice(0, 10)} · ${c.subject}`,
      });
  return out;
}

/**
 * The stack page's buttons: every stack action in its group, with the
 * reason a button is off when it is.
 * @param {Catalog} catalog
 * @param {string} stack
 * @returns {{group: string, actions: {entry: CatalogEntry, refused: string | null}[]}[]}
 */
/**
 * fix-229: the actions that only mean something for a native service
 * (a systemd unit, no docker).
 * @param {string} name
 */
export const isNativeAction = (name) =>
  name === "adopt" || name.endsWith("-native");

/**
 * @param {Catalog} catalog
 * @param {string} stack
 * @param {boolean | null} [native] whether the stack runs native services;
 *   null when the host does not say
 */
export function stackActionGroups(catalog, stack, native = null) {
  const by = new Map(catalog.actions.map((a) => [a.action, a]));
  return GROUPS.map((g) => ({
    group: g.group,
    actions: g.actions.flatMap((name) => {
      const entry = by.get(name);
      if (!entry || entry.target !== "stack") return [];
      // fix-229: a native-service action is offered only on a stack that
      // runs native services. `null` (a host too old to say) offers all.
      if (native === false && isNativeAction(name)) return [];
      const refused =
        stack === catalog.self_stack && entry.refused_for_self
          ? "never on the dashboard's own stack"
          : null;
      return [{ entry, refused }];
    }),
  })).filter((g) => g.actions.length > 0);
}

/**
 * The host-wide actions, in catalog order.
 * @param {Catalog} catalog
 */
export const hostActions = (catalog) =>
  catalog.actions.filter((a) => a.target === "host");

/**
 * What several stacks can be given at once (feat-stacks-5): every stack
 * action that needs nothing picked per stack. A typed name is asked per
 * stack.
 * @param {Catalog} catalog
 */
export const batchActions = (catalog) =>
  catalog.actions.filter(
    (a) =>
      a.target === "stack" &&
      !a.args.some((x) => x === "unit" || x === "commit" || x === "tag"),
  );

/** Arguments a schedule cannot know in advance: a pick from a live list, a
 * container number, a command, an answer, the names apply destroys. */
const PER_RUN = new Set([
  "unit",
  "commit",
  "vmid",
  "command",
  "check",
  "verdict",
  "days",
  "note",
  "destroy",
  "leave_out",
  "destroy_ids",
  "destroy_ack",
]);

/**
 * The batch form: the action's own fields, minus the ones that differ per
 * stack (an app), plus one typed name per stack when the action asks it.
 * @param {CatalogEntry} entry
 * @param {string[]} stacks
 * @param {string} selfStack
 */
export function batchForm(entry, stacks, selfStack) {
  const shared = entry.args
    .filter((a) => a !== "app" && a !== "confirm")
    .map((a) => argField(a, entry, "each stack"));
  const confirms = entry.confirm
    ? stacks.map((s) => ({
        ...argField("confirm", entry, s),
        id: `act-confirm-${s}`,
        stack: s,
      }))
    : [];
  const refused =
    entry.refused_for_self && stacks.includes(selfStack)
      ? `The dashboard never does this to its own stack (${selfStack}); leave it out of the selection.`
      : null;
  return {
    id: `batch:${entry.action}`,
    action: entry.action,
    title: `${entry.label} · ${stacks.length} ${stacks.length === 1 ? "stack" : "stacks"}`,
    what: entry.what,
    submit: `${entry.label} ${stacks.length === 1 ? "1 stack" : `${stacks.length} stacks`}`,
    stacks,
    shared,
    confirms,
    refused,
    restartsDashboard: stacks.includes(selfStack),
  };
}

/**
 * The batch request body.
 * @param {ReturnType<typeof batchForm>} form
 * @param {Values} values shared field values, and `confirm:<stack>` per stack
 */
export function batchBody(form, values) {
  /** @type {Record<string, string | boolean>} */
  const args = {};
  for (const f of form.shared)
    if (f.kind === "check" && values[f.name] === true) args[f.name] = true;
    else if (
      typeof values[f.name] === "string" &&
      String(values[f.name]).trim()
    )
      args[f.name] = String(values[f.name]).trim();
  /** @type {Record<string, string>} */
  const confirms = {};
  for (const c of form.confirms) {
    const v = values[`confirm:${c.stack}`];
    if (typeof v === "string" && v.trim()) confirms[c.stack] = v.trim();
  }
  return {
    action: form.action,
    stacks: form.stacks,
    args,
    ...(form.confirms.length ? { confirms } : {}),
  };
}

/**
 * What a batch's typed name says when it is missing or wrong (the same
 * words the dashboard's server holds a driven batch with).
 * @param {string} stack
 */
export const batchConfirmText = (stack) =>
  fill(SPEC.edit.messages.batch_confirm, { stack });

/**
 * Which typed names in a batch are missing or wrong.
 * @param {ReturnType<typeof batchForm>} form
 * @param {Values} values
 * @returns {string[]} the stacks whose name is not typed right
 */
export function batchConfirmErrors(form, values) {
  return form.confirms
    .filter(
      (c) => String(values[`confirm:${c.stack}`] ?? "").trim() !== c.stack,
    )
    .map((c) => c.stack);
}

/**
 * What a schedule can run (arch-schedule): anything that needs no typed
 * name and no pick from a live list; a wipe only lists without its name,
 * so it is left out as well.
 * @param {Catalog} catalog
 */
export const schedulableActions = (catalog) =>
  catalog.actions.filter(
    (a) =>
      !a.confirm && a.action !== "wipe" && !a.args.some((x) => PER_RUN.has(x)),
  );

/**
 * The fields a schedule asks for an action: its option fields (an app, a
 * snapshot, a skip), never the review step's force or typed name.
 * @param {CatalogEntry} entry
 * @param {string} stack
 * @returns {Field[]}
 */
export const scheduleArgFields = (entry, stack) =>
  entry.args
    .filter((a) => !onReview(entry.action, a) || a === "tag")
    .map((a) => ({
      ...argField(a, entry, stack),
      id: `sched-${a.replace(/_/g, "-")}`,
    }));
