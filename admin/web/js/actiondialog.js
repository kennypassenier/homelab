// feat-stacks-4, feat-stacks-5, feat-stacks-7: the action dialog. A kp
// dialog holding a kp wizard, drawn step by step from the action's
// description in actionforms.js: the options, then the review with the
// exact CLI line (copyable), the deploy guard's refusal with its Force
// choice, "restarts the dashboard", and the typed name where the action
// asks for it. The press starts the job; the dialog then follows it
// (feat-ops-6).

import { act, actionLabel, catalogReady, onAct, send } from "./act.js";
import {
  REVIEW,
  actionForm,
  batchBody,
  batchConfirmErrors,
  batchConfirmText,
  batchForm,
  buildArgs,
  checkValues,
  fieldChoices,
  formFields,
  initialValues,
  lockedFields,
  nameTyped,
  pressGate,
  previewArgs,
  shownField,
} from "./actionforms.js";
import {
  badge,
  cliCommand,
  fieldEl,
  labeledCliCommand,
  notify,
  openDialog,
  refusalAlarm,
  refusalCallout,
} from "./actui.js";
import {
  fetchJson,
  fetchReport,
  h,
  progressBar,
  setLive,
  setProgress,
  statTile,
} from "./dom.js";
import { resolveSnapshotOwner, snapshotPickerRows } from "./snapshotpicker.js";
import { diffBlocks } from "./editui.js";
import { applySummary, checkChoices } from "./parity.js";
import { fileViews } from "./plan.js";
import { driven, register } from "./drivehooks.js";
import { stackDetail } from "./fleet.js";
import { batchView, jobBadge } from "./jobs.js";
import { mountJobPanel } from "./jobpanel.js";
import { batchOf, batchProcess } from "./batchprocess.js";
import { runSpec } from "./runspec.js";
import { buildRunView } from "./runview.js";
import { humanBytes } from "./backupsview.js";
import { openUpdateFlow } from "./pinupdate.js";
import { codeSpans, previewGate } from "./previewgate.js";
import { restoreHref } from "./restoreflow.js";
import { current } from "./store.js";
import { declare, dialogControl, drivable } from "./drivable.js";
import { countOf } from "./ui.js";
import { stage } from "./stage.js";
import {
  attachCardStates,
  setCardState,
  clearError,
  showError,
  DATED_EVENT,
  DATED_RETRY_EVENT,
  attachDatedOptions,
  setDatedOptions,
  setDatedState,
  BEFORE_STEP_EVENT,
  FINISH_EVENT,
  STEP_EVENT,
  attachWizards,
  wizard,
} from "/static/kp/dist/kp-themes.min.js";

/**
 * @typedef {{apps?: string[], units?: string[],
 *   commits?: {commit: string, subject: string}[],
 *   checks?: {id: string, label: string}[], templates?: string[],
 *   releases?: {value: string, label: string, disabled: boolean}[]}} Sources
 * @typedef {{preset?: import("./actionforms.js").Values, sources?: Sources,
 *   openRollback?: (stack: string) => void, driven?: boolean}} OpenOpts
 * @typedef {{form: import("./actionforms.js").ActionForm,
 *   dialog: HTMLDialogElement, closed: Promise<void>, close: () => void,
 *   input: (name: string, id?: string) => HTMLInputElement | HTMLSelectElement | null,
 *   set: (name: string, value: string | boolean, id?: string) => void,
 *   step: () => number, goTo: (index: number) => Promise<boolean>,
 *   button: (name: string) => HTMLElement | null,
 *   errors: (byName: Record<string, string>) => void,
 *   runError: (e: import("./doctor.js").RouteError | null) => void,
 *   showJob: (job: number) => void}} ActionController
 *   feat-platform-10: what the driven replay (drive.js) does to a dialog.
 */

/**
 * dashboard-latest: one repository's "release tag" dropdown — GitHub's
 * release list, newest first, "latest (now vX.Y.Z)" on top, an unsigned
 * release disabled. `unit` narrows a stack that holds several native
 * services (e.g. kyu) to the one whose repository is read.
 * @param {string} stack
 * @param {string} [unit]
 * @returns {Promise<{value: string, label: string, disabled: boolean}[]>}
 */
async function fetchReleases(stack, unit) {
  const q = unit ? `?unit=${encodeURIComponent(unit)}` : "";
  const r = await fetchJson(
    `/data/actions/${encodeURIComponent(stack)}/releases${q}`,
    "the release list",
  );
  return r.ok ? (r.body.releases ?? []) : [];
}

/**
 * The lists the form's choices come from: the fleet's apps, and the
 * stack's commits and native units when a field asks for them.
 * @param {import("./actionforms.js").ActionForm} form
 * @param {Sources} given
 * @returns {Promise<Sources>}
 */
async function readSources(form, given) {
  const out = { ...given };
  const fields = formFields(form);
  if (!out.apps && fields.some((f) => f.source === "apps"))
    out.apps =
      stackDetail(current().fleet, form.stack)?.apps.map((a) => a.name) ?? [];
  const wantsRepo = fields.some(
    (f) =>
      (f.source === "units" && !out.units) ||
      (f.source === "commits" && !out.commits),
  );
  // TUI parity: the manual checks and the host's OS templates.
  if (!out.checks && fields.some((f) => f.source === "checks")) {
    const r = await fetchReport("/data/manual-checks", "the manual checks");
    out.checks = r.ok ? checkChoices(r.report?.checks ?? []) : [];
  }
  if (!out.templates && fields.some((f) => f.source === "templates")) {
    const r = await fetchJson("/data/templates", "the templates");
    out.templates = r.ok ? (r.body.templates?.os ?? []) : [];
  }
  // dashboard-latest: the "release tag" dropdown. install-native's unit is
  // not chosen yet here (single-service stacks still resolve; a
  // multi-service one gets an empty list until the unit field's own
  // listener re-fetches it, wired in drawActionDialog).
  if (!out.releases && fields.some((f) => f.source === "releases"))
    out.releases = await fetchReleases(form.stack);
  if (wantsRepo) {
    const r = await fetchJson(
      `/data/actions/${encodeURIComponent(form.stack)}/rollback-options`,
      "the roll-back options",
    );
    if (r.ok) {
      out.units ??= r.body.native_units ?? [];
      out.commits ??= r.body.commits ?? [];
    } else {
      out.units ??= [];
      out.commits ??= [];
    }
  }
  return out;
}

/**
 * Whether the fleet knows this stack as a native service.
 * @param {string} stack
 */
const nativeStack = (stack) =>
  current().fleet?.stacks?.some(
    (/** @type {any} */ s) => s.name === stack && s.native === true,
  ) === true;

/**
 * Open one action's dialog on one stack (or on the host target).
 * `driven` (feat-platform-10): Claude drives it from the dashboard's
 * server; the dialog shows each step and never sends the press itself.
 * @param {string} stack
 * @param {string} action
 * @param {OpenOpts} [opts]
 * @returns {Promise<ActionController | null>}
 */
export async function openAction(stack, action, opts = {}) {
  // redesign-flows-1 (3.71.0, decision "one Update flow"): a person's
  // Update — the stack hub's button, a Stacks row, the palette — opens the
  // Update flow (see, impact, back up, update, verify, done). Live view
  // drives the action's own form (`driven`), which the server models.
  // redesign-openpoints-2: an app's Update (the hub's Apps row) opens the
  // flow with that app picked, not the whole stack.
  if (action === "update" && opts.driven !== true) {
    const app = opts.preset?.app;
    openUpdateFlow(stack, typeof app === "string" && app ? app : null);
    return null;
  }
  // redesign-final-h3: Restore… (Backups, the hub, the palette) opens the
  // Restore flow (FLOWS.md §5) with what the opener already chose; Live
  // view drives the action's own form.
  if (
    (action === "restore" || action === "restore-native") &&
    opts.driven !== true
  ) {
    const p = opts.preset ?? {};
    history.pushState(
      null,
      "",
      restoreHref(
        stack,
        typeof p.app === "string" ? p.app : null,
        typeof p.snapshot === "string" ? p.snapshot : null,
      ),
    );
    dispatchEvent(new PopStateEvent("popstate"));
    return null;
  }
  // fix-batch-native-1: a native service backs up the native way, whoever
  // asked (a Stacks row, a batch).
  if (action === "backup" && nativeStack(stack)) action = "backup-native";
  const catalog = await catalogReady();
  const entry = catalog?.actions.find((a) => a.action === action);
  if (!catalog || !entry) {
    notify(`The dashboard does not know the action "${action}".`, "error");
    return null;
  }
  // Deploying an earlier commit starts from the list of commits.
  if (action === "deploy-commit" && !opts.preset?.commit && opts.openRollback) {
    opts.openRollback(stack);
    return null;
  }
  const form = actionForm(entry, {
    stack,
    selfStack: catalog.self_stack,
    hostTarget: catalog.host_target,
  });
  if (form.refused) {
    await refusalAlarm({ what: form.title, why: form.refused, fix: "" }, 403);
    return null;
  }
  const sources = await readSources(form, opts.sources ?? {});
  const values = initialValues(form, opts.preset);
  // fix-216: a row or a notice that already picked an app, unit or commit
  // is not asked again — the field opens locked, read-only, with a
  // "Change" link, one shared rule for every action form rather than a
  // list each caller has to remember to pass.
  const locked = lockedFields(form, opts.preset);
  return drawActionDialog(form, values, sources, opts.driven === true, locked);
}

/**
 * `form.intro` as nodes: plain lines become paragraphs, consecutive lines
 * starting with "- " become one bullet list — the shape Adopt's step list
 * needs without teaching every other one-line description any markup.
 * @param {string} intro
 */
function introNodes(intro) {
  /** @type {Node[]} */
  const nodes = [];
  /** @type {string[]} */
  let items = [];
  const flushItems = () => {
    if (!items.length) return;
    // redesign-final M2: an intro's list is what will happen, in order (a
    // list, not an <ol>: the kp wizard reads an <ol> in its dialog as its
    // own steps).
    nodes.push(
      h(
        "ul",
        { class: "act-intro-steps" },
        ...items.map((i) => h("li", null, i)),
      ),
    );
    items = [];
  };
  for (const line of intro.split("\n")) {
    if (!line.trim()) continue;
    if (line.startsWith("- ")) items.push(line.slice(2));
    else {
      flushItems();
      // redesign-final-h2: `inline code` is code, never raw backticks.
      nodes.push(
        h(
          "p",
          { class: "measured act-intro" },
          ...codeSpans(line).map((x) =>
            typeof x === "string" ? x : h("code", null, x.code),
          ),
        ),
      );
    }
  }
  flushItems();
  return nodes;
}

/**
 * The dialog for one form.
 * @param {import("./actionforms.js").ActionForm} form
 * @param {import("./actionforms.js").Values} values
 * @param {Sources} sources
 * @param {boolean} driven
 * @param {string[]} [locked] fix-216: field names the opener already chose
 * @returns {ActionController}
 */
function drawActionDialog(form, values, sources, driven, locked = []) {
  const lockedSet = new Set(locked);
  /** @type {Map<string, HTMLInputElement | HTMLSelectElement>} */
  const inputs = new Map();
  /** @type {Map<string, HTMLElement>} */
  const wraps = new Map();
  /**
   * fix-216: the read-only summary a locked field shows instead of its
   * input — the chosen choice's own label (never a raw id), and a Change
   * link that reveals the real field for whoever does want to pick again.
   * @param {import("./actionforms.js").Field} f
   * @param {{input: HTMLInputElement | HTMLSelectElement, wrap: HTMLElement}} x
   */
  const lockedWrap = (f, x) => {
    const label = () => {
      if (f.kind === "choice") {
        const choices = fieldChoices(f, sources);
        const found = choices.find((c) => c.value === x.input.value);
        if (found) return found.label;
      }
      return x.input.value || "—";
    };
    const valueEl = h("strong", null, label());
    x.input.addEventListener("change", () => {
      valueEl.textContent = label();
    });
    const changeBtn = h(
      "button",
      {
        type: "button",
        class:
          "kp-button kp-button--sm kp-button--ghost act-field-locked__change",
        "data-act-unlock": f.name,
      },
      "Change",
    );
    const summary = h(
      "div",
      { class: "act-field-locked", "data-field": `${f.name}-locked` },
      h("span", { class: "kp-field__label" }, f.label),
      h(
        "span",
        { class: "act-field-locked__value" },
        valueEl,
        h("span", { class: "act-field-locked__hint" }, " — already chosen"),
      ),
      changeBtn,
    );
    x.wrap.hidden = true;
    changeBtn.addEventListener("click", () => {
      summary.hidden = true;
      x.wrap.hidden = false;
      x.input.focus();
    });
    return h(
      "div",
      { class: "act-field-locked-wrap", "data-field": f.name },
      summary,
      x.wrap,
    );
  };
  /** @param {import("./actionforms.js").Field} f */
  const field = (f) => {
    const x = fieldEl(
      f,
      values[f.name],
      f.kind === "choice" ? fieldChoices(f, sources) : [],
    );
    inputs.set(f.name, x.input);
    wraps.set(f.name, x.wrap);
    const read = () => {
      values[f.name] =
        x.input instanceof HTMLInputElement && x.input.type === "checkbox"
          ? x.input.checked
          : x.input.value;
      if (x.input.getAttribute("aria-invalid") === "true") clearError(x.input);
    };
    x.input.addEventListener("input", read);
    x.input.addEventListener("change", read);
    return lockedSet.has(f.name) ? lockedWrap(f, x) : x.wrap;
  };

  // The review step's parts: the preview, the guard, the typed name.
  const previewBox = h("div", { class: "act-preview", "aria-live": "polite" });
  const previewStage = stage(previewBox);
  setCardState(previewStage, "loading", {
    words: "Reading what this would do",
  });
  const guardBox = h("div", { class: "act-guard" });
  const restartsBox = h(
    "div",
    {
      class: "kp-alert kp-alert--warning act-restarts",
      role: "status",
      hidden: "",
      "data-kp-semantic": "",
    },
    h(
      "span",
      { class: "kp-alert__body" },
      h("span", { class: "kp-alert__label" }, "Restarts the dashboard: "),
      "this page loses its link for a moment; the job's end is read back after the restart.",
    ),
  );
  const runError = h("div", { class: "act-run-error" });
  const reviewStep = /** @type {import("./actionforms.js").Step} */ (
    form.steps.find((s) => s.id === REVIEW)
  );
  const forceField = reviewStep.fields.find((f) => f.when === "guard");
  const otherReview = reviewStep.fields.filter((f) => f.when !== "guard");
  const forceWrap = forceField ? field(forceField) : null;
  if (forceWrap) forceWrap.hidden = true;

  const labels = form.steps.map((s) =>
    h("li", { "data-kp-step-label": "", "data-step": s.id }, s.label),
  );
  const sections = form.steps.map((s, i) => {
    const sec = h("section", {
      "data-kp-step": "",
      "data-step": s.id,
      "aria-label": s.label,
    });
    if (i > 0) sec.hidden = true;
    if (s.id === REVIEW) {
      sec.append(
        // fix-249: a dialog with its own description (`form.intro`, at the
        // top) says what it does once; the short `what` line only stands
        // in for a form that has none.
        ...(form.intro
          ? []
          : [h("p", { class: "act-what" }, capital(form.what), ".")]),
        previewStage,
        guardBox,
        ...(forceWrap ? [forceWrap] : []),
        restartsBox,
        ...otherReview.map(field),
        runError,
      );
    } else sec.append(...s.fields.map(field));
    return sec;
  });
  // dashboard-latest: install-native's unit picks the repository the tag
  // dropdown reads; changing it re-fetches and redraws that dropdown, the
  // same re-fetch `ui pick act-unit` triggers when Claude drives it.
  const releaseField = formFields(form).find((f) => f.source === "releases");
  const unitInput = inputs.get("unit");
  const tagInput = inputs.get("tag");
  if (releaseField && unitInput && tagInput instanceof HTMLSelectElement) {
    unitInput.addEventListener("change", () => {
      const unit = /** @type {string} */ (unitInput.value);
      fetchReleases(form.stack, unit).then((list) => {
        const current = tagInput.value;
        tagInput.replaceChildren(
          ...list.map((c) =>
            h(
              "option",
              { value: c.value, ...(c.disabled ? { disabled: "" } : {}) },
              c.label,
            ),
          ),
        );
        tagInput.value = list.some((c) => c.value === current)
          ? current
          : "latest";
        values.tag = tagInput.value;
      });
    });
  }
  // fix-216: the snapshot field becomes a picker — every snapshot of the
  // app or unit already chosen, newest first, dated and aged, the newest
  // preselected and labelled "latest" — for any form that has one
  // (restore and restore-native today; one shared mechanism, not a
  // per-dialog patch). `snapshotWrap`/`snapshotInput` are the plain text
  // field `field()` already built; its value stays the single source of
  // truth (buildArgs, Live-view replay via `set()` all read it unchanged),
  // the picker only changes what is drawn around it.
  const snapshotInput = inputs.get("snapshot");
  const snapshotWrap = wraps.get("snapshot");
  /** @type {(() => Promise<void>) | null} */
  let startSnapshots = null;
  if (snapshotInput instanceof HTMLInputElement && snapshotWrap) {
    const help = snapshotWrap.querySelector(".kp-field__help");
    // kp's dated picker: one option per snapshot, newest first with its
    // "Latest" badge, the date and how long ago, the kind, size and short id
    // in a column. Its value is the snapshot's short id; the field keeps its
    // own contract (empty means the latest), so the newest writes "".
    const picker = h("div", {
      class: "kp-dated",
      "data-kp-dated": "",
      "data-kp-name": `act-snapshot-${form.id}`,
      "data-field": "snapshot-picker",
      "aria-label": "Snapshot to restore",
    });
    const gapNote = h(
      "p",
      { class: "measured act-snapshot-gap", hidden: "" },
      'Kind, size and file count are recorded for backups taken from 3.70.5 on; older snapshots say "not recorded".',
    );
    snapshotInput.hidden = true;
    snapshotWrap.insertBefore(picker, help);
    snapshotWrap.insertBefore(gapNote, help);
    /** @type {string} */
    let latestId = "";
    picker.addEventListener(DATED_EVENT, (e) => {
      const chosen = String(/** @type {CustomEvent} */ (e).detail.value ?? "");
      snapshotInput.value = chosen === latestId ? "" : chosen;
      snapshotInput.dispatchEvent(new Event("input", { bubbles: true }));
    });
    picker.addEventListener(DATED_RETRY_EVENT, () => void renderSnapshots());
    let seq = 0;
    const renderSnapshots = async () => {
      const mySeq = ++seq;
      gapNote.hidden = true;
      setDatedState(picker, "loading");
      const r = await fetchJson(
        `/data/backups/${encodeURIComponent(form.stack)}`,
        "this stack's backup snapshots",
      );
      if (mySeq !== seq) return;
      if (!r.ok) {
        setDatedState(picker, "error", {
          title: "The snapshot list did not read",
          text: r.error.fix ? `${r.error.why} — ${r.error.fix}` : r.error.why,
        });
        snapshotInput.hidden = false;
        return;
      }
      const repos = r.body?.repos ?? [];
      const appValue = typeof values.app === "string" ? values.app : "";
      const owner = resolveSnapshotOwner(repos, appValue);
      if (!owner) {
        setDatedState(picker, "empty", {
          reason: appValue
            ? `No repository named "${appValue}" was read`
            : "Choose an app above to see its snapshots, or leave Snapshot empty for the latest of every app",
          action: null,
        });
        snapshotInput.hidden = false;
        return;
      }
      const snaps = owner.snapshots ?? [];
      if (snaps.length === 0) {
        setDatedState(picker, "empty", {
          reason: `${owner.owner} has no snapshots yet`,
          action: null,
        });
        return;
      }
      const now = Math.floor(Date.now() / 1000);
      const rows = snapshotPickerRows(snaps, now, snapshotInput.value);
      latestId = rows.find((row) => row.latest)?.shortId ?? "";
      const timeOf = new Map(snaps.map((x) => [x.short_id, x.time]));
      setDatedOptions(
        picker,
        rows.map((row) => ({
          value: row.shortId,
          when: (timeOf.get(row.shortId) ?? 0) * 1000,
          // kp-themes 11.2.0: several meta texts sit side by side.
          meta: [
            row.kind ?? "kind not recorded",
            row.size ?? "size not recorded",
            ...(row.files ? [row.files] : []),
            row.shortId,
          ],
        })),
        rows.find((row) => row.selected)?.shortId ?? null,
      );
      gapNote.hidden = !rows.some(
        (row) => row.size == null || row.kind == null,
      );
    };
    startSnapshots = renderSnapshots;
    // A different app re-reads this app's own snapshots instead of the
    // one the dialog opened on (fix-216: "Change" on a locked app field
    // still lands here).
    inputs.get("app")?.addEventListener("change", () => void renderSnapshots());
  }
  const back = h(
    "button",
    { type: "button", class: "kp-button", "data-kp-wizard-back": "" },
    "Back",
  );
  const next = h(
    "button",
    {
      type: "button",
      class: `kp-button ${form.destructive ? "kp-button--destructive" : "kp-button--primary"}`,
      "data-kp-wizard-next": "",
      "data-kp-finish": form.submit,
      "data-kp-next": "Next",
      id: "act-run",
    },
    "Next",
  );
  const wiz = h(
    "div",
    {
      class: "kp-wizard act-wizard",
      "data-kp-wizard": "",
      "data-kp-validate": "false",
      "data-form": form.id,
    },
    ...(form.steps.length > 1
      ? [
          h(
            "ol",
            { class: "kp-wizard__steps", "data-kp-wizard-steps": "" },
            ...labels,
          ),
        ]
      : []),
    ...sections,
    h(
      "div",
      { class: "kp-wizard__actions" },
      ...(form.steps.length > 1 ? [back] : []),
      next,
    ),
  );
  // A field shown only while another has a value (the answer's days, with
  // accept), or worded differently then (the note becomes the required
  // reason): kept in step with the values as they change, by a click or a
  // driven step alike. Hidden fields are not checked or sent (buildArgs).
  const conditional = formFields(form).filter(
    (f) => f.show_when || f.change_when,
  );
  const applyConditions = () => {
    for (const f of conditional) {
      const wrap = wraps.get(f.name);
      const input = inputs.get(f.name);
      if (!wrap || !input) continue;
      const now = shownField(f, values);
      wrap.hidden = !now;
      if (!now) {
        if (input.getAttribute("aria-invalid") === "true") clearError(input);
        continue;
      }
      const label = wrap.querySelector(".kp-field__label");
      if (label) label.textContent = now.label;
      const help = wrap.querySelector(`[id="${f.id}-hint"]`);
      if (help) help.textContent = now.help;
      input.required = now.required;
    }
  };
  if (conditional.length) {
    wiz.addEventListener("input", applyConditions);
    wiz.addEventListener("change", applyConditions);
    applyConditions();
  }
  // fix-216 (rule 8: "every section and every action says what it does";
  // Kenny, 2026-10-02 on Adopt: "wat doet adopt exact?"): every action's
  // own plain-English description of what it does — and, for one whose
  // effects are not obvious from its name, a short step list — read from
  // `formspec.json`'s `intro` map so it is one place, not a string typed
  // again in each dialog that wants one.
  const body = h(
    "div",
    { class: "act-body" },
    ...(form.intro ? introNodes(form.intro) : []),
    wiz,
  );
  const d = openDialog({
    title: form.title,
    body: [body],
    id: "action-dialog",
  });
  d.dialog.dataset.form = form.id;
  if (driven) d.dialog.dataset.driven = "";
  const detachWizard = attachWizards(d.dialog, { focusStep: false });
  const detachDated = attachDatedOptions(d.dialog);
  const detachStates = attachCardStates(d.dialog);
  void startSnapshots?.();
  const handle = wizard(wiz);
  /** @type {() => void} */
  let stopPanel = () => {};
  d.closed.then(() => {
    detachWizard();
    detachDated();
    detachStates();
    stopPanel();
  });

  /** @type {import("./doctor.js").RouteError | null} */
  let guard = null;
  /**
   * redesign-final-h2: why this cannot run, in one plain sentence.
   * @param {string} reason
   */
  const blockedNote = (reason) =>
    h(
      "p",
      {
        class: "kp-alert kp-alert--warning act-blocked",
        role: "status",
        "data-kp-semantic": "",
      },
      reason || "Reading what this would do…",
    );
  /** @type {{ready: boolean, reason: string}} */
  let lastGate = { ready: true, reason: "" };
  let onReview = form.steps.length === 1 && form.steps[0].id === REVIEW;
  /**
   * The primary button follows the preview: held back, with the reason as
   * its description, while the preview says the action cannot run; on the
   * review step also until the typed name is right (fix-wipe-1).
   * @param {{ready: boolean, reason: string}} given
   */
  const setReady = (given) => {
    lastGate = given;
    const g = onReview ? pressGate(form, values, given) : given;
    const b = /** @type {HTMLButtonElement} */ (next);
    b.disabled = !g.ready;
    if (g.ready) b.removeAttribute("title");
    else b.title = g.reason;
  };
  let previewSeq = 0;
  const preview = async () => {
    const seq = ++previewSeq;
    previewBox.replaceChildren();
    setCardState(previewStage, "loading", {
      words: "Reading what this would do",
    });
    const r = await send(
      "POST",
      form.previewPath,
      previewArgs(form, values),
      `the preview of ${form.title}`,
    );
    if (seq !== previewSeq) return;
    setCardState(previewStage, "ready");
    if (!r.ok) {
      guard = null;
      // redesign-final-h2: one plain sentence, the primary held back.
      const g = previewGate(form, null, r.error);
      previewBox.replaceChildren(blockedNote(g.reason));
      setReady(g);
      return;
    }
    const p = r.body;
    const gate = previewGate(form, p, null);
    setReady(gate);
    previewBox.replaceChildren(
      ...(gate.ready ? [] : [blockedNote(gate.reason)]),
      // redesign-final M2: the command line is a quiet footnote for who
      // wants it, never the dialog's main content.
      ...(p.cli
        ? [
            h(
              "div",
              { class: "act-cli act-cli--quiet" },
              labeledCliCommand("The same from a workstation:", p.cli),
            ),
          ]
        : []),
      ...gate.notes.map((n) => h("p", { class: "measured act-cli-none" }, n)),
    );
    // TUI parity: what the press changes, before it is pressed (the Deploy
    // plan with its diff, the Apply plan).
    if (p.plan) {
      const files = fileViews(p.plan.files ?? []);
      previewBox.append(
        h(
          "details",
          {
            class: "act-plan kp-accordion__item kp-accordion__item--bare",
            open: "",
          },
          h(
            "summary",
            null,
            p.plan.new_stack
              ? "The plan: a new container, every file sent"
              : files.length
                ? `The plan: ${files.length} ${files.length === 1 ? "file changes" : "files change"}`
                : "The plan: no file changes",
          ),
          h("p", { class: "measured" }, p.plan.note),
          // fix-159: the running native units the deploy restarts, and why.
          ...(p.plan.restarts?.length
            ? [
                h(
                  "ul",
                  { class: "act-restarts" },
                  ...p.plan.restarts.map((/** @type {string} */ r) =>
                    h("li", null, r),
                  ),
                ),
              ]
            : []),
          ...diffBlocks(files),
        ),
      );
    } else if (p.wipe_plan) {
      // fix-wipe-1: what the press deletes, before the name is typed.
      const [head, ...rows] = String(p.wipe_plan).split("\n");
      previewBox.append(
        h(
          "div",
          {
            class: "act-plan kp-accordion__item kp-accordion__item--bare",
            "data-wipe-plan": "",
          },
          h("p", null, capital(head)),
          h(
            "ul",
            null,
            ...rows.map((l) => h("li", { class: "mono" }, l.trim())),
          ),
        ),
      );
    } else if (p.apply) {
      const s = applySummary(p.apply);
      previewBox.append(
        h(
          "div",
          {
            class: "act-plan kp-accordion__item kp-accordion__item--bare",
            "data-apply": "",
          },
          h("p", null, s.headline),
          h("ul", null, ...s.lines.map((l) => h("li", null, l))),
          ...(s.blocked
            ? [
                h(
                  "p",
                  { class: "kp-alert kp-alert--destructive", role: "alert" },
                  s.blocked,
                ),
              ]
            : []),
        ),
      );
    }
    restartsBox.hidden = !p.restarts_dashboard;
    guard = p.guard ?? null;
    guardBox.replaceChildren(
      ...(guard
        ? [refusalCallout(guard, "warning", "The deploy guard refuses")]
        : []),
    );
    // Force is only offered while the guard refuses; once ticked, the
    // preview asks again with force and the guard stands aside.
    if (forceWrap) forceWrap.hidden = !guard && values.force !== true;
  };
  inputs.get("force")?.addEventListener("change", () => void preview());
  // A field asked on the review step (exec's command) changes the line.
  for (const f of otherReview)
    if (f.name !== "confirm")
      inputs.get(f.name)?.addEventListener("change", () => void preview());
  // cli-yes: the line carries --yes once the name is typed right.
  let typedBefore = nameTyped(form, values);
  inputs.get("confirm")?.addEventListener("input", () => {
    setReady(lastGate);
    const now = nameTyped(form, values);
    if (now !== typedBefore) {
      typedBefore = now;
      void preview();
    }
  });

  /** @param {"options" | "review"} step */
  const stepErrors = (step) => {
    const errors = checkValues(form, values, step);
    for (const [name, input] of inputs) {
      if (errors[name]) showError(input, errors[name]);
      else if (input.getAttribute("aria-invalid") === "true") clearError(input);
    }
    const first = Object.keys(errors)[0];
    if (first) inputs.get(first)?.focus();
    return !first;
  };

  wiz.addEventListener(BEFORE_STEP_EVENT, (e) => {
    // Driven: the dashboard's server checked the step already.
    if (driven) return;
    const detail = /** @type {CustomEvent} */ (e).detail;
    const from = form.steps[detail.from]?.id;
    if (detail.direction === "forward" && from && !stepErrors(from))
      e.preventDefault();
  });
  wiz.addEventListener(STEP_EVENT, (e) => {
    const detail = /** @type {CustomEvent} */ (e).detail;
    onReview = form.steps[detail.step]?.id === REVIEW;
    if (onReview) {
      setReady({ ready: false, reason: "" });
      void preview();
    } else setReady({ ready: true, reason: "" });
  });
  if (form.steps.length === 1) {
    setReady({ ready: false, reason: "" });
    void preview();
  }

  let running = false;
  /** @param {number} job */
  const showJob = (job) => {
    if (body.dataset.job) return;
    const panel = mountJobPanel(job, { compact: true });
    stopPanel = panel.stop;
    handle?.element.remove();
    d.dialog.classList.add("act-dialog--run");
    body.replaceChildren(
      panel.element,
      h(
        "div",
        { class: "kp-dialog__actions" },
        h(
          "a",
          {
            class: "kp-button",
            href: `/activity?view=running&job=${job}`,
            "data-close-on-nav": "",
          },
          "Open in Jobs",
        ),
        h(
          "button",
          {
            type: "button",
            class: "kp-button kp-button--primary",
            "data-kp-dialog-close": "",
          },
          "Close",
        ),
      ),
    );
    body.dataset.job = String(job);
    body
      .querySelector("[data-close-on-nav]")
      ?.addEventListener("click", () => d.close());
    body
      .querySelector("[data-kp-dialog-close]")
      ?.addEventListener("click", () => d.close());
  };
  wiz.addEventListener(FINISH_EVENT, async () => {
    // Driven: the press runs on the dashboard's server, once; this tab
    // only shows the job it started (drive.js calls showJob).
    if (driven || running) return;
    if (!stepErrors(REVIEW)) return;
    if (guard && values.force !== true) {
      runError.replaceChildren(
        refusalCallout(
          {
            what: form.title,
            why: "the deploy guard refuses this deploy",
            fix: "pull the working copy first, or tick Force if undoing the host's last deploy is the point",
          },
          "destructive",
        ),
      );
      inputs.get("force")?.focus();
      return;
    }
    running = true;
    next.disabled = true;
    runError.replaceChildren(h("p", { class: "measured" }, "Sending…"));
    const r = await send(
      "POST",
      form.runPath,
      buildArgs(form, values),
      form.title,
    );
    running = false;
    next.disabled = false;
    if (!r.ok) {
      runError.replaceChildren(refusalCallout(r.error));
      await refusalAlarm(r.error, r.status);
      return;
    }
    showJob(/** @type {number} */ (r.body.job));
  });

  return {
    form,
    dialog: d.dialog,
    closed: d.closed,
    close: () => d.close(),
    input: (name) => inputs.get(name) ?? null,
    set: (name, value) => {
      const x = inputs.get(name);
      if (!x) return;
      const check = x instanceof HTMLInputElement && x.type === "checkbox";
      if (check) x.checked = value === true;
      else x.value = String(value);
      x.dispatchEvent(
        new Event(
          check || x instanceof HTMLSelectElement ? "change" : "input",
          {
            bubbles: true,
          },
        ),
      );
    },
    step: () => handle?.step() ?? 0,
    goTo: (i) => (handle ? handle.goTo(i) : Promise.resolve(false)),
    button: (name) =>
      name === "back"
        ? back
        : name === "close"
          ? /** @type {HTMLElement | null} */ (
              d.dialog.querySelector(".kp-dialog__close")
            )
          : next,
    errors: (byName) => {
      for (const [name, input] of inputs) {
        if (byName[name]) showError(input, byName[name]);
        else if (input.getAttribute("aria-invalid") === "true")
          clearError(input);
      }
    },
    runError: (e) =>
      runError.replaceChildren(
        ...(e ? [refusalCallout(e, "destructive")] : []),
      ),
    showJob,
  };
}

/** @param {string} s */
const capital = (s) => (s ? s[0].toUpperCase() + s.slice(1) : s);

/**
 * feat-stacks-5: one action on several stacks. The review lists every
 * stack with its CLI line and its guard, asks each typed name the action
 * needs, and follows the batch once it runs.
 * @param {string} action
 * @param {string[]} stacks
 */
export async function openBatch(action, stacks) {
  const catalog = await catalogReady();
  const entry = catalog?.actions.find((a) => a.action === action);
  if (!catalog || !entry || stacks.length === 0) return;
  const form = batchForm(entry, stacks, catalog.self_stack);
  if (form.refused) {
    await refusalAlarm({ what: form.title, why: form.refused, fix: "" }, 403);
    return;
  }
  /** @type {import("./actionforms.js").Values} */
  const values = {};
  /** @type {Map<string, HTMLInputElement | HTMLSelectElement>} */
  const inputs = new Map();
  const sharedFields = form.shared.map((f) => {
    const x = fieldEl(f, f.kind === "check" ? false : "");
    values[f.name] = f.kind === "check" ? false : "";
    inputs.set(f.name, x.input);
    x.input.addEventListener("change", () => {
      values[f.name] =
        x.input instanceof HTMLInputElement && x.input.type === "checkbox"
          ? x.input.checked
          : x.input.value;
    });
    return x.wrap;
  });
  const confirmFields = form.confirms.map((f) => {
    const x = fieldEl(f, "");
    const key = `confirm:${f.stack}`;
    values[key] = "";
    inputs.set(key, x.input);
    x.input.addEventListener("input", () => {
      values[key] = x.input.value;
      if (x.input.getAttribute("aria-invalid") === "true") clearError(x.input);
      syncRun();
    });
    return x.wrap;
  });
  // feat-batch-1: one grid, name | CLI line | Copy, every row under the
  // last; while a preview is read each row shows a bar shaped like it.
  const list = h("ul", {
    class: "batch-preview",
    "aria-busy": "true",
    "data-kp-state": "loading",
  });
  for (const s of stacks)
    list.append(
      h(
        "li",
        { "data-stack": s, "data-loading": "" },
        h("strong", { class: "batch-preview__name" }, s),
        h("span", {
          class: "kp-skeleton batch-preview__bar",
          role: "status",
          "aria-label": `Reading the preview of ${s}`,
        }),
        h("span", { class: "kp-skeleton batch-preview__copy" }),
      ),
    );
  const runError = h("div");
  const run = h(
    "button",
    {
      type: "button",
      class: `kp-button ${entry.destructive ? "kp-button--destructive" : "kp-button--primary"}`,
      id: "batch-run",
    },
    form.submit,
  );
  // fix-wipe-1: off until every stack's name is typed exactly.
  function syncRun() {
    const wrong = batchConfirmErrors(form, values);
    run.disabled = wrong.length > 0;
    if (wrong.length) run.title = batchConfirmText(wrong[0]);
    else run.removeAttribute("title");
  }
  syncRun();
  const cancel = h(
    "button",
    { type: "button", class: "kp-button", "data-kp-dialog-close": "" },
    "Cancel",
  );
  const body = h(
    "div",
    { class: "act-body", "data-form": form.id },
    h("p", { class: "act-what" }, capital(form.what), "."),
    ...(form.restartsDashboard
      ? [
          h(
            "div",
            { class: "kp-alert kp-alert--warning", role: "status" },
            `Restarts the dashboard: ${catalog.self_stack} is among the stacks.`,
          ),
        ]
      : []),
    h(
      "p",
      { class: "measured" },
      "Checked whole first, then run one by one in this order:",
    ),
    list,
    ...sharedFields,
    ...confirmFields,
    runError,
    h("div", { class: "kp-dialog__actions" }, cancel, run),
  );
  const d = openDialog({
    title: form.title,
    body: [body],
    id: "batch-dialog",
    wide: true,
  });
  d.dialog.dataset.form = form.id;
  // redesign-final (exact counts): "Back up · 5 stacks" is the stacks
  // the dialog lists.
  const titleEl = d.dialog.querySelector(".kp-dialog__title");
  if (titleEl instanceof HTMLElement)
    countOf(titleEl, list, ":scope > li[data-stack]");
  /** @type {() => void} */
  let stop = () => {};
  /** The batch runs: its progress in the dialog's place.
   * @param {{batch: number, jobs?: {job: number, stack: string}[]}} answer */
  const showBatch = (answer) => {
    if (body.dataset.batch) return;
    body.dataset.batch = String(answer.batch);
    const panel = mountBatchPanel(answer.batch, answer.jobs ?? []);
    stop = panel.stop;
    d.dialog.classList.add("act-dialog--run");
    body.replaceChildren(
      panel.element,
      h(
        "div",
        { class: "kp-dialog__actions" },
        h(
          "button",
          {
            type: "button",
            class: "kp-button kp-button--primary",
            "data-kp-dialog-close": "",
          },
          "Close",
        ),
      ),
    );
    body
      .querySelector("[data-kp-dialog-close]")
      ?.addEventListener("click", () => d.close());
  };
  // feat-platform-10: the Live view replay works this very dialog.
  const unregister = register("batch", {
    dialog: d.dialog,
    closed: d.closed,
    run: () => run,
    runError: (/** @type {import("./doctor.js").RouteError | null} */ e) =>
      runError.replaceChildren(...(e ? [refusalCallout(e)] : [])),
    showBatch,
    close: () => d.close(),
  });
  d.closed.then(() => {
    unregister();
    stop();
  });

  // Each stack's own preview: its CLI line and whether its guard refuses.
  let guarded = 0;
  await Promise.all(
    stacks.map(async (s) => {
      const r = await send(
        "POST",
        `/data/actions/${encodeURIComponent(s)}/${action}/preview`,
        {},
        `the preview of ${entry.label} on ${s}`,
      );
      const li = list.querySelector(`[data-stack="${CSS.escape(s)}"]`);
      if (!li) return;
      const parts = /** @type {Node[]} */ ([
        h("strong", { class: "batch-preview__name" }, s),
      ]);
      if (!r.ok)
        parts.push(
          h(
            "span",
            { class: "measured batch-preview__note" },
            `no preview: ${r.error.why}`,
          ),
        );
      else {
        if (r.body.cli) parts.push(cliCommand(r.body.cli, { wrap: "scroll" }));
        else
          parts.push(
            h(
              "span",
              { class: "measured batch-preview__note" },
              `no CLI line: ${r.body.cli_unavailable ?? "—"}`,
            ),
          );
        if (r.body.guard) {
          guarded += 1;
          parts.push(
            h(
              "div",
              { class: "batch-preview__guard" },
              refusalCallout(
                r.body.guard,
                "warning",
                "The deploy guard refuses",
              ),
            ),
          );
        }
      }
      li.removeAttribute("data-loading");
      li.replaceChildren(...parts);
    }),
  );
  list.removeAttribute("aria-busy");
  list.removeAttribute("data-kp-state");
  const force = inputs.get("force");
  if (force)
    /** @type {HTMLElement} */ (force.closest(".kp-field")).hidden =
      guarded === 0;

  run.addEventListener("click", async () => {
    // Live view: the batch runs on the dashboard's server, once.
    if (driven()) return;
    const wrong = batchConfirmErrors(form, values);
    for (const c of form.confirms) {
      const input = inputs.get(`confirm:${c.stack}`);
      if (!input) continue;
      if (wrong.includes(c.stack)) showError(input, batchConfirmText(c.stack));
      else clearError(input);
    }
    if (wrong.length) {
      inputs.get(`confirm:${wrong[0]}`)?.focus();
      return;
    }
    run.disabled = true;
    const r = await send(
      "POST",
      "/data/actions/batch",
      batchBody(form, values),
      form.title,
    );
    syncRun();
    if (!r.ok) {
      runError.replaceChildren(refusalCallout(r.error));
      await refusalAlarm(r.error, r.status);
      return;
    }
    showBatch(r.body);
  });
}

const CANCEL_JOB = declare({
  id: "batch-cancel-job",
  page: "activity",
  opens: "run",
  row: "<stack>",
  what: "take one queued job of a batch out of the queue (the batch panel's Cancel)",
  shows: "in a batch's panel, on a job that has not started",
});

/**
 * A batch's progress from `action_batch`, and the running job's own panel.
 * feat-batch-1: the overall progress with Stop batch and Cancel all queued
 * first, the job running now under it, the per-stack list compact last.
 * @param {number} batch
 * @param {{job: number, stack: string}[]} jobs
 */
export function mountBatchPanel(batch, jobs) {
  const bar = progressBar("Batch progress", 0);
  const text = h("span", { class: "kp-progress__value" });
  // Kenny, 2026-10-01: the totals used to be one sentence; stat tiles keep
  // Done/OK/Failed/Deferred in their own place instead.
  const totalTile = statTile("Total");
  const doneTile = statTile("Done");
  const okTile = statTile("OK");
  const failedTile = statTile("Failed");
  const deferredTile = statTile("Deferred");
  const totals = h(
    "div",
    { class: "kp-kpis batch-totals", "data-kp-kpis-columns": "all 3 2 1" },
    totalTile.el,
    doneTile.el,
    okTile.el,
    failedTile.el,
    deferredTile.el,
  );
  const rows = h("ul", { class: "batch-jobs" });
  // one-process-batch-1: the batch is ONE process, drawn once by the shared
  // progress view from the batch's whole step plan; it never switches to a
  // member job (only its log follows the stack that runs now).
  const view = buildRunView({
    label: `Batch ${batch}`,
    jobId: jobs[0]?.job ?? 0,
    compact: true,
    dataJob: String(batch),
  });
  const running = h("div", { class: "batch-running" }, view.element);
  /** The step names the members sent, by member and step number. @type {Map<string, string>} */
  const seen = new Map();
  /** How long each step took, by its title, from the progress we saw. @type {Map<string, number>} */
  const stepSeconds = new Map();
  let lastStep = { title: "", at: 0 };
  let boundJob = jobs[0]?.job ?? 0;
  /** The cancel routes answered 404: this host does not have them. */
  let cancelGone = false;
  // Kenny, 2026-09-30: a running batch could not be stopped halfway. The
  // job running now ends on its own; the queued ones never start.
  const stopBtn = dialogControl(
    h(
      "button",
      { type: "button", class: "kp-button kp-button--destructive batch-stop" },
      "Stop batch",
    ),
    "stop-batch",
  );
  stopBtn.title = "The job running now finishes; the queued ones never start";
  stopBtn.addEventListener("click", async () => {
    stopBtn.setAttribute("disabled", "");
    const r = await send(
      "POST",
      `/data/actions/batch/${batch}/stop`,
      {},
      "stop the batch",
    );
    if (!r.ok) {
      stopBtn.removeAttribute("disabled");
      notify(`${r.error.why}.`, "warning");
    }
  });
  const cancelAll = dialogControl(
    h(
      "button",
      { type: "button", class: "kp-button batch-cancel-all" },
      "Cancel all queued",
    ),
    "cancel-all-queued",
  );
  cancelAll.title = "Take every job that has not started out of the queue";
  cancelAll.addEventListener("click", async () => {
    cancelAll.setAttribute("disabled", "");
    const r = await send(
      "POST",
      `/data/actions/batch/${batch}/cancel`,
      {},
      "cancel the queued jobs",
    );
    cancelAll.removeAttribute("disabled");
    if (!r.ok) {
      if (r.status === 404) cancelGone = true;
      else notify(`${r.error.why}.`, "warning");
      paint();
    }
  });
  const element = h(
    "section",
    {
      class: "batch-panel",
      "aria-label": `Batch ${batch}`,
      "data-batch": String(batch),
    },
    h(
      "div",
      { class: "batch-head" },
      h(
        "div",
        { class: "kp-progress-group batch-head__bar" },
        h(
          "div",
          { class: "kp-progress__wrap" },
          h("span", { class: "kp-progress__label" }, `Batch ${batch}`),
          bar,
          text,
        ),
      ),
      h("div", { class: "batch-head__actions" }, cancelAll, stopBtn),
    ),
    running,
    totals,
    h("h3", { class: "batch-list-title" }, "Stacks in this batch"),
    rows,
  );
  /** @param {number} job */
  const cancelOne = async (job) => {
    const r = await send(
      "POST",
      `/data/actions/jobs/${job}/cancel`,
      {},
      "cancel the queued job",
    );
    if (!r.ok) {
      if (r.status === 404) cancelGone = true;
      else notify(`${r.error.why}.`, "warning");
      paint();
    }
  };
  function paint() {
    const b = act.batches.get(batch) ?? {
      batch,
      jobs: jobs.map((j) => ({
        job: j.job,
        stack: j.stack,
        state: act.jobs.find((x) => x.job === j.job)?.state ?? "queued",
        message: null,
      })),
      done: 0,
      ok: 0,
      failed: 0,
      deferred: 0,
    };
    const v = batchView(b);
    const queued = v.rows.filter(
      (r) =>
        (act.jobs.find((x) => x.job === r.job)?.state ?? "queued") ===
          "queued" && !b.done,
    ).length;
    stopBtn.hidden = !!b.done;
    cancelAll.hidden = cancelGone || queued === 0;
    setLive(totalTile.value, String(v.total));
    // The Done tile counts the finished stacks; `b.done` is whether the
    // whole batch is (it read "false" here before 3.74.0).
    setLive(
      doneTile.value,
      String((b.ok ?? 0) + (b.failed ?? 0) + (b.deferred ?? 0)),
    );
    setLive(okTile.value, String(b.ok));
    setLive(failedTile.value, String(b.failed));
    setLive(deferredTile.value, String(b.deferred));
    rows.replaceChildren(
      ...v.rows.map((r) => {
        const live = act.jobs.find((x) => x.job === r.job);
        const state = live?.state ?? "queued";
        const b2 = live ? jobBadge(live.state) : r.badge;
        const cancel =
          state === "queued" && !cancelGone
            ? drivable(
                h(
                  "button",
                  {
                    type: "button",
                    class: "kp-button kp-button--sm batch-jobs__cancel",
                    title: `Take ${r.stack} out of the queue`,
                  },
                  "Cancel",
                ),
                CANCEL_JOB,
                r.stack,
              )
            : h("span");
        if (cancel instanceof HTMLButtonElement)
          cancel.addEventListener("click", () => {
            cancel.setAttribute("disabled", "");
            void cancelOne(r.job);
          });
        return h(
          "li",
          { "data-job": String(r.job) },
          badge(b2),
          h("strong", { class: "batch-jobs__stack" }, r.stack),
          h(
            "a",
            { href: `/activity?view=running&job=${r.job}` },
            `job ${r.job}`,
          ),
          h("span", { class: "measured batch-jobs__note" }, r.message || ""),
          cancel,
        );
      }),
    );
    // The whole batch as one process.
    const now = Date.now() / 1000;
    const proc = batchProcess(/** @type {any} */ (b), act.jobs, now, seen);
    const pj = proc.job;
    const p = pj.progress;
    if (p && p.step) {
      const title = p.step;
      if (lastStep.title && lastStep.title !== title)
        stepSeconds.set(lastStep.title, Math.max(0, p.elapsed_s - lastStep.at));
      if (lastStep.title !== title) lastStep = { title, at: p.elapsed_s };
    }
    const spec = runSpec(pj, {
      rows: proc.rows,
      times: stepSeconds,
      now,
      progressAt: null,
      label: actionLabel,
      human: (n) => humanBytes(n),
    });
    if (spec.head && spec.figures) {
      const names = v.rows.map((r) => r.stack);
      spec.head.title = `${actionLabel(pj.action)} · ${names.length} ${names.length === 1 ? "stack" : "stacks"}`;
      spec.head.sub = `Batch ${batch}: ${names.join(", ")}`;
      spec.figures.of = batchOf(proc.total, proc.approx);
      spec.figures.note = proc.approx
        ? proc.because.length
          ? `About: ${proc.because.join("; ")}.`
          : "About: some stacks' step counts are known only when the host starts them."
        : "";
    }
    view.paint(spec);
    // The head bar counts steps across the members, like the figures do;
    // only before the plan exists does it fall back to the stacks done.
    const pct = b.done
      ? 100
      : p?.m
        ? Math.min(
            99,
            Math.max(0, Math.round(((p.n - (p.finished ? 0 : 1)) / p.m) * 100)),
          )
        : v.percent;
    setProgress(bar, pct);
    text.textContent = `${pct}%`;
    if (proc.current != null && proc.current !== boundJob) {
      boundJob = proc.current;
      view.log.bind(boundJob);
    }
  }
  const offB = onBatch(paint);
  const tick = setInterval(paint, 1000);
  paint();
  return {
    element,
    stop: () => {
      offB();
      clearInterval(tick);
      view.stop();
    },
  };
}

/**
 * feat-batch-1: open the panel of a batch that is already running or done,
 * from its id (Activity's batch rows). The host answers with the same
 * shape as the `action_batch` event.
 * @param {number} id
 * @returns {Promise<boolean>} whether the batch was found
 */
export async function reopenBatch(id) {
  const r = await fetchJson(`/data/actions/batch/${id}`, `batch ${id}`);
  if (!r.ok || !r.body) {
    notify(
      r.ok ? `The dashboard does not know batch ${id}.` : `${r.error.why}.`,
      "warning",
    );
    return false;
  }
  /** @type {import("./jobs.js").Batch} */
  const b = { ...r.body, batch: r.body.batch ?? id };
  act.batches.set(id, b);
  const panel = mountBatchPanel(id, b.jobs ?? []);
  const close = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--primary",
      "data-kp-dialog-close": "",
    },
    "Close",
  );
  const d = openDialog({
    title: `Batch ${id}`,
    body: [
      h(
        "div",
        { class: "act-body", "data-form": "batch-view" },
        panel.element,
        h("div", { class: "kp-dialog__actions" }, close),
      ),
    ],
    id: "batch-dialog",
    wide: true,
  });
  d.dialog.classList.add("act-dialog--run");
  d.closed.then(() => panel.stop());
  return true;
}

/** @param {() => void} f */
function onBatch(f) {
  const a = onAct("batch", f);
  const b = onAct("jobs", f);
  return () => {
    a();
    b();
  };
}
