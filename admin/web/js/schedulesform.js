// The schedule form (feat-kp11demo-9.1/9.4, the demo Kenny approved on
// 2026-10-19): one sentence ("Run [Back up] on [vaultwarden] [every week] on
// [Mon Tue] at [03:30]."), a note, a warning when another schedule starts in
// the same minute, the next five runs while you type, and kp's unsaved bar
// counting what you changed. It is drawn into the page's detail pane (the
// surface Kenny chose: beside the list). The module owns the form's Live view
// fields; the page owns what saving does.

import { fieldEl } from "./actui.js";
import { scheduleArgFields } from "./actionforms.js";
import { h } from "./dom.js";
import { declare, declareField, drivable } from "./drivable.js";
import {
  EVERY_WORDS,
  draftBody,
  isoDate,
  runsWithClash,
  scheduleTitle,
} from "./schedules.js";
import { openConfirmation } from "/static/kp/dist/kp-themes.min.js";
import { attachRuns, setRuns } from "/static/kp/dist/kp-themes.min.js";
import {
  attachSentences,
  sentence,
  setSentenceFieldState,
  setSentenceWhy,
} from "/static/kp/dist/kp-themes.min.js";
import {
  attachTimes,
  setTime,
  timeValue,
} from "/static/kp/dist/kp-themes.min.js";
import {
  attachUnsaved,
  markUnsavedClean,
  unsaved,
  unsavedSaved,
  discardUnsaved,
  setUnsavedSaving,
} from "/static/kp/dist/kp-themes.min.js";
import { attachWeekdays, weekdaysOf } from "/static/kp/dist/kp-themes.min.js";

// Live view reaches every field and button of the form (invariant 39).
export const SCHED_ACTION = declareField({
  id: "sched-action",
  page: "schedules",
  what: "what the schedule runs (schedule form)",
});
export const SCHED_STACK = declareField({
  id: "sched-stack",
  page: "schedules",
  what: "the stack the schedule runs on (schedule form)",
});
export const SCHED_EVERY = declareField({
  id: "sched-every",
  page: "schedules",
  what: "how often the schedule runs (schedule form)",
});
export const SCHED_DATE = declareField({
  id: "sched-date",
  page: "schedules",
  what: "the date of a one-time run, dd/mm/yyyy (schedule form)",
});
export const SCHED_AT = declareField({
  id: "sched-at",
  page: "schedules",
  what: "the time of day the schedule runs, 24 hours (schedule form)",
});
export const SCHED_NOTE = declareField({
  id: "sched-note",
  page: "schedules",
  what: "the schedule's note (schedule form)",
});

const WEEKDAY = declare({
  id: "schedule-weekday",
  page: "schedules",
  opens: "view",
  row: "<mon|tue|wed|thu|fri|sat|sun>",
  what: "turn one weekday of a weekly schedule on or off in the form",
  shows: "in the schedule form, once its sentence says every week",
  reach: [{ do: "click", control: "new-schedule" }],
});
const SAVE = declare({
  id: "save-schedule",
  page: "schedules",
  opens: "run",
  what: "save the changes of the schedule form (kp's unsaved bar)",
  shows: "in the schedule form, once something is changed",
  reach: [{ do: "click", control: "new-schedule" }],
});
const DISCARD = declare({
  id: "discard-schedule-changes",
  page: "schedules",
  opens: "view",
  what: "put every value of the schedule form back (Undo for 6 s)",
  shows: "in the schedule form, once something is changed",
  reach: [{ do: "click", control: "new-schedule" }],
});
const ADD = declare({
  id: "add-schedule",
  page: "schedules",
  opens: "run",
  what: "add the new schedule as the form reads now",
  shows: "in the form of a new schedule",
  reach: [{ do: "click", control: "new-schedule" }],
});
const CLOSE = declare({
  id: "close-schedule-form",
  page: "schedules",
  opens: "view",
  what: "close the schedule form (asks first when something is changed)",
  shows: "in the schedule form",
  reach: [{ do: "click", control: "new-schedule" }],
});

const DAY_NAMES = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

let uid = 0;

/**
 * @typedef {import("./schedules.js").Draft} Draft
 * @typedef {import("./schedules.js").ScheduleView} ScheduleView
 * @typedef {{action: string, label: string, target: "stack" | "host"}} ActionChoice
 * @typedef {{
 *   title: string,
 *   draft: Draft,
 *   id: string | null,
 *   actions: import("./actionforms.js").CatalogEntry[],
 *   stacks: () => string[] | null,
 *   others: () => ScheduleView[],
 *   hostTarget: string,
 *   zone: string,
 *   nightly: number | null | undefined,
 *   label: (action: string) => string,
 *   now: () => number,
 *   appsOf: (stack: string) => string[],
 *   save: (body: ReturnType<typeof draftBody> & {ok: true}) => Promise<
 *     {ok: true} | {ok: false, why: string}>,
 *   onClose: () => void,
 *   onSaved: () => void,
 * }} FormOptions
 */

/**
 * Draw the form into `host` and return its controller.
 * @param {HTMLElement} host
 * @param {FormOptions} o
 */
export function buildScheduleForm(host, o) {
  const n = ++uid;
  const isNew = o.id == null;
  const opt = (/** @type {string} */ v, /** @type {string} */ label) =>
    `<option value="${v}">${label}</option>`;
  const near =
    typeof o.nightly === "number"
      ? ` data-kp-time-near="${String(o.nightly).padStart(2, "0")}:00" data-kp-time-near-width="20"`
      : "";
  const presets = [
    ...(typeof o.nightly === "number"
      ? [`Nightly + 1 h=${String((o.nightly + 1) % 24).padStart(2, "0")}:00`]
      : []),
    "Lunch=12:00",
    "Evening=22:00",
  ].join(",");
  host.innerHTML = `
    <header class="kp-drawer__head">
        <h3 class="kp-dialog__title" id="sch-ft-${n}"></h3>
        <p class="kp-drawer__desc">Complete the sentence. The host plans in its own clock (${o.zone}).</p>
        <div class="sch-unsavedslot"><div class="kp-unsaved" data-kp-unsaved data-kp-unsaved-for="#sch-f-${n}" data-kp-name="sch-f-${n}" data-kp-placement="header" data-kp-keep="off" data-kp-warn-leave="off" id="sch-u-${n}"></div></div>
    </header>
    <div class="kp-drawer__body sch-formbody">
        <form class="sch-form kp-formgrid" id="sch-f-${n}" action="#" novalidate>
            <fieldset class="kp-sentence" data-kp-sentence data-kp-formgrid-span="all">
                <legend class="kp-sr-only">Schedule</legend>
                <p class="kp-sentence__text">
                    Run
                    <span class="kp-sentence__field"><select name="action" id="${SCHED_ACTION}" aria-label="What to run"></select></span>
                    on
                    <span class="kp-sentence__field" data-sch-stackfield><select name="stack" id="${SCHED_STACK}" aria-label="Which stack"></select></span>
                    <span class="kp-sentence__field"><select name="every" id="${SCHED_EVERY}" aria-label="How often">${Object.entries(
                      EVERY_WORDS,
                    )
                      .map(([v, w]) => opt(v, w))
                      .join(
                        "",
                      )}</select></span><span class="kp-sentence__when" data-kp-show-when="every=week">
                        on
                        <span class="kp-sentence__field" data-kp-block><span class="kp-weekdays" data-kp-weekdays data-kp-week-starts-on="1" data-kp-presets="false" data-kp-summary="false" aria-label="Run on"></span></span></span><span class="kp-sentence__when" data-kp-show-when="every=once" hidden>
                        on
                        <span class="kp-sentence__field"><input name="date" id="${SCHED_DATE}" size="10" placeholder="dd/mm/yyyy" aria-label="Date (dd/mm/yyyy)" /></span></span>
                    at
                    <span class="kp-sentence__field"><span class="kp-time" data-kp-time data-kp-time-look="inline" data-kp-time-step="15" data-kp-name="at" data-kp-time-id="${SCHED_AT}" data-kp-label="At (24 hours)" data-kp-time-presets="${presets}"${near}></span></span>.
                </p>
            </fieldset>
            <div class="kp-formgrid__row" data-kp-row="options" data-sch-options hidden>
                <span class="kp-formgrid__label">Options</span>
                <div class="kp-formgrid__control sch-options" data-sch-optionbox></div>
                <p class="kp-formgrid__value">Handed to the action each time it runs</p>
            </div>
            <div class="kp-formgrid__row" data-kp-row="note">
                <label class="kp-formgrid__label" for="${SCHED_NOTE}">Note</label>
                <div class="kp-formgrid__control"><input class="kp-field__input" name="note" id="${SCHED_NOTE}" maxlength="200" /></div>
                <p class="kp-formgrid__value">Shown in the table, never used by the host</p>
            </div>
            <div class="sch-clash" data-sch-clash data-kp-formgrid-span="all" hidden>
                <div class="kp-alert kp-alert--warning" role="status" data-kp-semantic><span><span class="kp-alert__label">Warning: </span><span data-sch-clash-text></span></span></div>
                <button type="button" class="kp-button" data-sch-clash-move>Move 15 minutes later</button>
            </div>
            <div class="sch-preview" data-kp-formgrid-span="all">
                <div class="kp-runs" data-kp-runs data-kp-runs-count="5" data-kp-time-zone="${o.zone}" data-sch-preview></div>
            </div>
        </form>
    </div>
    <footer class="kp-drawer__foot sch-formfoot">
        ${isNew ? '<button type="button" class="kp-button kp-button--primary" data-sch-add>Add schedule</button>' : ""}
        <button type="button" class="kp-button" data-sch-close>Close</button>
    </footer>`;

  const q = (/** @type {string} */ sel) => host.querySelector(sel);
  const title = /** @type {HTMLElement} */ (q(".kp-dialog__title"));
  title.textContent = o.title;
  const form = /** @type {HTMLFormElement} */ (q("form"));
  const field = (/** @type {string} */ name) =>
    /** @type {HTMLInputElement & HTMLSelectElement} */ (
      form.elements.namedItem(name)
    );
  const sel = {
    action: field("action"),
    stack: field("stack"),
    every: field("every"),
    date: field("date"),
    note: field("note"),
  };
  const weekdays = /** @type {HTMLElement} */ (q("[data-kp-weekdays]"));
  const timeEl = /** @type {HTMLElement} */ (q("[data-kp-time]"));
  const stackField = /** @type {HTMLElement} */ (q("[data-sch-stackfield]"));
  const sentenceEl = /** @type {HTMLElement} */ (q("[data-kp-sentence]"));
  const optionsRow = /** @type {HTMLElement} */ (q("[data-sch-options]"));
  const optionBox = /** @type {HTMLElement} */ (q("[data-sch-optionbox]"));
  const bar = /** @type {HTMLElement} */ (q("[data-kp-unsaved]"));
  const preview = /** @type {HTMLElement} */ (q("[data-sch-preview]"));
  const clash = /** @type {HTMLElement} */ (q("[data-sch-clash]"));

  const d = o.draft;
  const schedulable = o.actions;
  const stackOf = (/** @type {string} */ action) =>
    schedulable.find((a) => a.action === action);
  // Stack-level actions first, then the host's own, as the catalog lists.
  sel.action.innerHTML = [
    ...schedulable.filter((a) => a.target === "stack"),
    ...schedulable.filter((a) => a.target === "host"),
  ]
    .map((a) => opt(a.action, a.label))
    .join("");
  sel.action.value = d.action;
  sel.every.value = d.every;
  sel.date.value = d.date ? d.date.split("-").reverse().join("/") : "";
  sel.note.value = d.note;
  weekdays.setAttribute(
    "data-kp-value",
    (d.days.length ? d.days : [1, 4]).map((x) => DAY_NAMES[x]).join(","),
  );
  timeEl.setAttribute("data-kp-value", d.at);

  /** @type {Map<string, HTMLInputElement | HTMLSelectElement>} */
  let argInputs = new Map();

  const fillStacks = () => {
    const entry = stackOf(sel.action.value);
    const host_ = entry?.target === "host";
    const fleet = o.stacks() ?? [];
    const names = host_
      ? [o.hostTarget]
      : [
          ...new Set([
            ...fleet,
            ...(d.stack && d.stack !== o.hostTarget ? [d.stack] : []),
          ]),
        ];
    const keep = sel.stack.value || d.stack;
    sel.stack.innerHTML = names
      .map((s) => opt(s, s === o.hostTarget ? "the whole host" : s))
      .join("");
    if (names.includes(keep)) sel.stack.value = keep;
  };
  const fillArgs = () => {
    const entry = stackOf(sel.action.value);
    argInputs = new Map();
    const fields = entry ? scheduleArgFields(entry, sel.stack.value) : [];
    optionsRow.hidden = fields.length === 0;
    optionBox.replaceChildren(
      ...fields.map((f) => {
        const prev =
          entry?.action === d.action
            ? (d.args[f.name] ?? undefined)
            : undefined;
        const apps = o.appsOf(sel.stack.value);
        const x = fieldEl(
          f,
          f.kind === "check"
            ? prev === true
            : typeof prev === "string"
              ? prev
              : "",
          f.source === "apps"
            ? [
                { value: "", label: f.empty ?? "Every app" },
                ...apps.map((a) => ({ value: a, label: a })),
              ]
            : [],
        );
        argInputs.set(f.name, x.input);
        return x.wrap;
      }),
    );
  };
  fillStacks();

  attachSentences(host);
  attachWeekdays(host);
  attachTimes(host);
  attachRuns(host);
  attachUnsaved(host);
  fillArgs();
  if (!o.stacks()) setSentenceFieldState(stackField, "loading");

  // Live view: the weekday chips, the bar's buttons, the foot's buttons.
  host
    .querySelectorAll(".kp-weekdays__chips button")
    .forEach((b, i) =>
      drivable(/** @type {HTMLElement} */ (b), WEEKDAY, DAY_NAMES[i]),
    );
  const markBar = () => {
    const save = bar.querySelector(".kp-unsaved__save");
    if (save && !(/** @type {HTMLElement} */ (save).dataset.drive))
      drivable(/** @type {HTMLElement} */ (save), SAVE);
    const discard = bar.querySelector(".kp-unsaved__discard");
    if (discard && !(/** @type {HTMLElement} */ (discard).dataset.drive))
      drivable(/** @type {HTMLElement} */ (discard), DISCARD);
  };
  markBar();
  const markWatch = new MutationObserver(markBar);
  markWatch.observe(bar, { childList: true, subtree: true });
  const closeBtn = /** @type {HTMLElement} */ (q("[data-sch-close]"));
  drivable(closeBtn, CLOSE);
  const addBtn = /** @type {HTMLElement | null} */ (q("[data-sch-add]"));
  if (addBtn) drivable(addBtn, ADD);

  /** @returns {Draft} */
  const draft = () => {
    /** @type {Record<string, unknown>} */
    const args = {};
    for (const [name, input] of argInputs) {
      if (input instanceof HTMLInputElement && input.type === "checkbox") {
        if (input.checked) args[name] = true;
      } else if (input.value.trim()) args[name] = input.value.trim();
    }
    return {
      action: sel.action.value,
      stack: sel.stack.value,
      every: sel.every.value,
      days: weekdaysOf(weekdays)
        .filter((x) => typeof x === "number")
        .map((x) => (Number(x) + 6) % 7)
        .sort(),
      at: timeValue(timeEl),
      date: isoDate(sel.date.value),
      note: sel.note.value,
      enabled: d.enabled,
      args,
    };
  };

  const name = (/** @type {import("./schedules.js").Schedule} */ s) =>
    scheduleTitle(o.label(s.action), s.stack, o.hostTarget);

  const refresh = () => {
    const body = draftBody(draft());
    if (!body.ok) {
      setRuns(preview, [], {
        reason: "Pick the days, a date and a time to see the next runs.",
      });
      clash.hidden = true;
      return;
    }
    const others = o.others().filter((v) => v.schedule.id !== o.id);
    const times = runsWithClash(others, body.body.when, o.now(), 5, {
      zone: o.zone,
      nightly: o.nightly,
      name,
    });
    setRuns(preview, times, { reason: "No run is left: the date has passed." });
    const first = times.find((t) => t.conflict);
    const text = clash.querySelector("[data-sch-clash-text]");
    clash.hidden = !first;
    if (text && first)
      text.textContent = `${first.conflict?.replace("Same minute as", "A run starts in the same minute as")}.`;
  };
  form.addEventListener("input", refresh);
  form.addEventListener("change", (e) => {
    if (e.target === sel.action) {
      fillStacks();
      fillArgs();
    } else if (e.target === sel.stack) fillArgs();
    refresh();
  });
  form.addEventListener("kp-weekdays-change", refresh);
  form.addEventListener("kp-time-change", refresh);
  form.addEventListener("submit", (e) => e.preventDefault());

  q("[data-sch-clash-move]")?.addEventListener("click", () => {
    const at = timeValue(timeEl).split(":").map(Number);
    const t = (at[0] * 60 + at[1] + 15) % 1440;
    const pad = (/** @type {number} */ x) => String(x).padStart(2, "0");
    setTime(timeEl, `${pad(Math.floor(t / 60))}:${pad(t % 60)}`, {
      fire: true,
    });
    refresh();
  });

  /** Save: what the bar's Save and the new form's Add both do. */
  const submit = async () => {
    const body = draftBody(draft());
    setSentenceWhy(sentenceEl, null);
    if (!body.ok) {
      setSentenceWhy(sentenceEl, body.why);
      setUnsavedSaving(bar, false);
      return;
    }
    setUnsavedSaving(bar, true);
    const r = await o.save(body);
    if (r.ok) {
      unsavedSaved(bar);
      o.onSaved();
      return;
    }
    const changes = unsaved(bar)?.changes() ?? [];
    const key =
      changes.find((c) => /action|stack/.test(c.key))?.key ??
      changes[changes.length - 1]?.key;
    if (key) unsavedSaved(bar, { refused: { [key]: r.why } });
    else {
      setUnsavedSaving(bar, false);
      setSentenceWhy(sentenceEl, r.why);
    }
  };
  bar.addEventListener("kp-unsaved-save", (e) => {
    e.preventDefault();
    void submit();
  });
  addBtn?.addEventListener("click", () => void submit());

  const dirty = () => unsaved(bar)?.changes().length ?? 0;
  /** Close asks first when something is changed: a click outside never loses it. */
  const close = () => {
    const n_ = dirty();
    if (!n_) return o.onClose();
    openConfirmation(closeBtn, {
      phrase: `Close and lose ${n_} ${n_ === 1 ? "change" : "changes"}?`,
      onAccept: () => {
        discardUnsaved(bar);
        o.onClose();
      },
      initialFocus: "cancel",
    });
  };
  closeBtn.addEventListener("click", close);
  // The first look is the saved look: nothing counts as changed yet.
  window.setTimeout(() => markUnsavedClean(bar), 150);
  refresh();

  return {
    close,
    dirty,
    refresh,
    /** The fleet's stacks arrived (or changed). */
    stacksChanged: () => {
      const keep = sel.stack.value;
      fillStacks();
      if (keep) sel.stack.value = keep;
      setSentenceFieldState(stackField, o.stacks() ? "ready" : "loading");
      fillArgs();
    },
    focus: () => sel.action.focus({ preventScroll: true }),
    destroy: () => markWatch.disconnect(),
    element: host,
    sentence: () => sentence(sentenceEl),
  };
}
