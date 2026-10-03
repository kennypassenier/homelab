// Schedules (feat-stacks-8, arch-schedule): what the dashboard runs by
// itself and when. A kp datatable with the next run in the viewer's own
// time; create, edit and delete in a kp dialog; an on/off switch per row;
// live from the `schedules` event.

import {
  act,
  actionLabel,
  catalogReady,
  loadSchedules,
  onAct,
  send,
} from "../act.js";
import { scheduleArgFields, schedulableActions } from "../actionforms.js";
import {
  fieldEl,
  notify,
  openDialog,
  refusalAlarm,
  refusalCallout,
} from "../actui.js";
import { bindTableUrl, h, stateWord, tableBlock, td } from "../dom.js";
import { formatDateTime } from "../format.js";
import {
  DAYS,
  scheduleBody,
  scheduleRows,
  toggledBody,
  whenFromValues,
  whenValues,
} from "../schedules.js";
import { sortKeys } from "../sortkeys.js";
import { current } from "../store.js";
import {
  attachDataTables,
  compare,
  dataTable,
} from "/static/kp/js/datatable.js";
import { attachSwitches, clearError, showError } from "/static/kp/js/forms.js";
import { declare, dialogControl, drivable } from "../drivable.js";

// fix-239: Live view reaches every schedule control (`homelab ui click …`).
const NEW_SCHEDULE = declare({
  id: "new-schedule",
  page: "schedules",
  opens: "dialog",
  what: "open the New schedule dialog",
});
const EDIT_SCHEDULE = declare({
  id: "edit-schedule",
  page: "schedules",
  opens: "dialog",
  row: "<schedule id>",
  what: "open one schedule's Edit dialog",
});
const DELETE_SCHEDULE = declare({
  id: "delete-schedule",
  page: "schedules",
  opens: "dialog",
  row: "<schedule id>",
  what: "ask to delete one schedule (its dialog's Delete deletes it)",
});
const TOGGLE_SCHEDULE = declare({
  id: "toggle-schedule",
  page: "schedules",
  opens: "run",
  row: "<schedule id>",
  what: "turn one schedule on or off",
});

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const keys = sortKeys();
  const err = h("div");
  const zone = h("p", { class: "measured", id: "sched-zone" });
  const add = h(
    "button",
    { type: "button", class: "kp-button kp-button--primary", id: "sched-new" },
    "New schedule",
  );
  drivable(add, NEW_SCHEDULE);
  const t = tableBlock({
    remember: "schedules",
    caption: "Schedules",
    search: "Search schedules",
    state: "loading",
    nothing: "No schedules yet: New schedule adds one.",
    columns: [
      { label: "Stack", sort: "text", filter: "choice" },
      { label: "Action", sort: "text", filter: "choice" },
      { label: "When", sort: "text" },
      { label: "Next run (your time)", sort: "time" },
      { label: "Last run", sort: "text" },
      { label: "On", sort: "text", order: "off,on", filter: "choice" },
      { label: "Note", sort: "text" },
      { label: "Change", sort: "text" },
    ],
  });
  t.tbody.id = "schedules";
  root.replaceChildren(
    h("div", { class: "title-row" }, h("h1", null, "Schedules"), add),
    h(
      "p",
      { class: "measured" },
      "Runs while the dashboard is up. A slot missed while it was down is skipped and notified, never caught up.",
    ),
    zone,
    err,
    t.wrap,
  );
  const detach = attachDataTables(root, { compare: keys.compare(compare) });
  const table = dataTable(t.wrap);
  const unbind = bindTableUrl(table, "schedules");

  const render = () => {
    const list = act.schedules;
    if (!list) return;
    zone.textContent = `Times in a schedule are the host's clock (${list.zone}); the next run is shown in yours.`;
    const rows = scheduleRows(list.schedules, actionLabel);
    t.tbody.replaceChildren(
      ...rows.map((r) => {
        const sw = h("input", {
          class: "kp-switch__input",
          type: "checkbox",
          role: "switch",
          "aria-label": `Schedule ${r.stack} ${r.action} on`,
          "data-toggle": r.id,
        });
        sw.checked = r.enabled;
        drivable(sw, TOGGLE_SCHEDULE, r.id);
        const edit = h(
          "button",
          {
            type: "button",
            class: "kp-button kp-button--sm",
            "data-edit": r.id,
          },
          "Edit",
        );
        drivable(edit, EDIT_SCHEDULE, r.id);
        const del = h(
          "button",
          {
            type: "button",
            class: "kp-button kp-button--sm kp-button--destructive",
            "data-delete": r.id,
          },
          "Delete",
        );
        drivable(del, DELETE_SCHEDULE, r.id);
        return h(
          "tr",
          { "data-schedule": r.id, "data-kp-row-key": r.id },
          td(r.stack),
          td(r.action),
          td(r.when),
          // Toggling a schedule turns this cell into "off" and back; the
          // cell keeps the width of both, so the switch beside it does not
          // move under the pointer (Kenny, 2026-09-29).
          h(
            "td",
            null,
            stateWord(
              r.next == null || !r.enabled
                ? r.nextText
                : keys.note("time", r.nextText, r.next),
              [
                "off",
                "no further run",
                formatDateTime(r.next ?? Date.now() / 1000),
              ],
            ),
          ),
          td(r.last),
          h(
            "td",
            null,
            h(
              "label",
              { class: "kp-switch" },
              sw,
              stateWord(r.enabled ? "on" : "off", ["on", "off"]),
            ),
          ),
          td(r.note),
          h("td", { class: "row-buttons" }, edit, del),
        );
      }),
    );
    t.ready();
  };

  t.tbody.addEventListener("change", async (e) => {
    const input = /** @type {HTMLInputElement} */ (e.target);
    const id = input.dataset.toggle;
    if (!id) return;
    const s = act.schedules?.schedules.find(
      (x) => x.schedule.id === id,
    )?.schedule;
    if (!s) return;
    const r = await send(
      "PUT",
      `/data/schedules/${encodeURIComponent(id)}`,
      toggledBody(s, input.checked),
      "the schedule",
    );
    if (!r.ok) {
      input.checked = !input.checked;
      err.replaceChildren(refusalCallout(r.error, "destructive", "Not saved"));
    } else void loadSchedules();
  });
  t.tbody.addEventListener("click", (e) => {
    const b = /** @type {HTMLElement | null} */ (
      /** @type {Element} */ (e.target).closest("button")
    );
    if (!b) return;
    const id = b.dataset.edit ?? b.dataset.delete;
    const v = act.schedules?.schedules.find((x) => x.schedule.id === id);
    if (!v) return;
    if (b.dataset.edit) void openScheduleForm(v.schedule);
    else void confirmDelete(v.schedule);
  });
  add.addEventListener("click", () => void openScheduleForm(null));

  const off = onAct("schedules", render);
  const first = () => {
    if (!act.schedules)
      t.loading({ words: "Reading the schedules from the dashboard…" });
    void loadSchedules().then((e) => {
      if (e) t.failed(e);
    });
  };
  root.addEventListener("kp-datatable-retry", first);
  first();
  render();
  return () => {
    off();
    root.removeEventListener("kp-datatable-retry", first);
    unbind();
    detach();
  };
}

/**
 * Delete, after a kp dialog asks.
 * @param {import("../schedules.js").Schedule} s
 */
async function confirmDelete(s) {
  const yes = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--destructive",
      id: "sched-delete-yes",
    },
    "Delete",
  );
  dialogControl(yes, "delete");
  const d = openDialog({
    title: "Delete this schedule?",
    description: `${actionLabel(s.action)} on ${s.stack === "_host" ? "the host" : s.stack}. Jobs it already ran stay in the Jobs list.`,
    body: [
      h(
        "div",
        { class: "kp-dialog__actions" },
        h(
          "button",
          { type: "button", class: "kp-button", "data-kp-dialog-close": "" },
          "Cancel",
        ),
        yes,
      ),
    ],
    id: "sched-delete",
  });
  yes.addEventListener("click", async () => {
    const r = await send(
      "DELETE",
      `/data/schedules/${encodeURIComponent(s.id)}`,
      undefined,
      "the schedule",
    );
    if (!r.ok) {
      d.close();
      await refusalAlarm(r.error, r.status);
      return;
    }
    d.close();
    notify("Schedule deleted.", "success");
    void loadSchedules();
  });
}

/**
 * The new or edit form, drawn from the same field descriptions the action
 * dialog uses (actionforms.js).
 * @param {import("../schedules.js").Schedule | null} existing
 */
async function openScheduleForm(existing) {
  const catalog = await catalogReady();
  if (!catalog) return;
  const actions = schedulableActions(catalog);
  const stacks = (current().fleet?.stacks ?? []).map((s) => s.name);
  const stackSel = h(
    "select",
    { class: "kp-field__input", id: "sched-stack", required: "" },
    h("option", { value: catalog.host_target }, "The whole host"),
    ...stacks.map((s) => h("option", { value: s }, s)),
  );
  const actionSel = h("select", {
    class: "kp-field__input",
    id: "sched-action",
    required: "",
  });
  const argBox = h("div", { class: "sched-args" });
  const w = whenValues(existing?.when ?? null);
  const every = h(
    "select",
    { class: "kp-field__input", id: "sched-every" },
    h("option", { value: "day" }, "Every day"),
    h("option", { value: "week" }, "On chosen weekdays"),
    h("option", { value: "once" }, "Once"),
  );
  every.value = w.every;
  const at = h("input", {
    class: "kp-field__input",
    type: "time",
    id: "sched-at",
    required: "",
  });
  at.value = w.at;
  const date = h("input", {
    class: "kp-field__input",
    type: "date",
    id: "sched-date",
  });
  date.value = w.date;
  const dayBoxes = DAYS.map((d, i) => {
    const box = h("input", {
      class: "kp-field__check",
      type: "checkbox",
      id: `sched-day-${i}`,
      value: String(i),
    });
    box.checked = w.days.includes(i);
    return {
      box,
      wrap: h(
        "span",
        { class: "day-choice" },
        box,
        h("label", { for: `sched-day-${i}` }, d.slice(0, 3)),
      ),
    };
  });
  const daysField = h(
    "fieldset",
    { class: "kp-fieldset sched-days", id: "sched-days" },
    h("legend", null, "Days"),
    ...dayBoxes.map((d) => d.wrap),
  );
  const dateField = h(
    "div",
    { class: "kp-field" },
    h("label", { class: "kp-field__label", for: "sched-date" }, "Date"),
    date,
  );
  const enabled = h("input", {
    class: "kp-switch__input",
    type: "checkbox",
    role: "switch",
    id: "sched-enabled",
  });
  enabled.checked = existing?.enabled ?? true;
  const note = h("input", {
    class: "kp-field__input",
    type: "text",
    id: "sched-note",
    maxlength: "200",
  });
  note.value = existing?.note ?? "";
  const formErr = h("div");
  const save = h(
    "button",
    { type: "button", class: "kp-button kp-button--primary", id: "sched-save" },
    existing ? "Save" : "Create",
  );
  dialogControl(save, "save");
  /** @type {Map<string, HTMLInputElement | HTMLSelectElement>} */
  let argInputs = new Map();

  const fillActions = () => {
    const host = stackSel.value === catalog.host_target;
    const keep = actionSel.value || existing?.action || "";
    actionSel.replaceChildren(
      ...actions
        .filter((a) => (a.target === "host") === host)
        .map((a) => h("option", { value: a.action }, a.label)),
    );
    if ([...actionSel.options].some((o) => o.value === keep))
      actionSel.value = keep;
    fillArgs();
  };
  const fillArgs = () => {
    const entry = actions.find((a) => a.action === actionSel.value);
    argInputs = new Map();
    if (!entry) {
      argBox.replaceChildren();
      return;
    }
    const apps =
      current()
        .fleet?.stacks.find((s) => s.name === stackSel.value)
        ?.apps?.map((a) => a.name) ?? [];
    argBox.replaceChildren(
      ...scheduleArgFields(entry, stackSel.value).map((f) => {
        const prev =
          existing?.action === entry.action ? existing.args[f.name] : undefined;
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
  const showWhen = () => {
    daysField.hidden = every.value !== "week";
    dateField.hidden = every.value !== "once";
  };
  stackSel.addEventListener("change", fillActions);
  actionSel.addEventListener("change", fillArgs);
  every.addEventListener("change", showWhen);
  if (existing) stackSel.value = existing.stack;
  fillActions();
  showWhen();

  /** @param {string} label @param {HTMLElement} control @param {string} [help] */
  const field = (label, control, help) =>
    h(
      "div",
      { class: "kp-field" },
      h("label", { class: "kp-field__label", for: control.id }, label),
      control,
      ...(help ? [h("span", { class: "kp-field__help" }, help)] : []),
    );
  const d = openDialog({
    title: existing ? "Edit the schedule" : "New schedule",
    description: "Actions that need a typed name cannot be scheduled.",
    id: "sched-dialog",
    body: [
      h(
        "form",
        { class: "sched-form", "data-form": "schedule", novalidate: "" },
        field("Stack", stackSel),
        field("Action", actionSel),
        argBox,
        field("How often", every),
        daysField,
        dateField,
        field(
          "At",
          at,
          `The host's clock (${act.schedules?.zone ?? "Europe/Brussels"}).`,
        ),
        h(
          "label",
          { class: "kp-switch" },
          enabled,
          h("span", { class: "kp-switch__state", "aria-hidden": "true" }),
          h("span", null, "Enabled"),
        ),
        field("Note", note, "Optional; shows in the list."),
        formErr,
        h(
          "div",
          { class: "kp-dialog__actions" },
          h(
            "button",
            { type: "button", class: "kp-button", "data-kp-dialog-close": "" },
            "Cancel",
          ),
          save,
        ),
      ),
    ],
  });
  attachSwitches(d.dialog);

  save.addEventListener("click", async () => {
    for (const el of [at, date, actionSel]) clearError(el);
    formErr.replaceChildren();
    const wv = whenFromValues({
      every: every.value,
      at: at.value,
      date: date.value,
      days: dayBoxes
        .filter((x) => x.box.checked)
        .map((x) => Number(x.box.value)),
    });
    if (!wv.ok) {
      const target =
        wv.field === "date" ? date : wv.field === "days" ? dayBoxes[0].box : at;
      showError(target, wv.why);
      target.focus();
      return;
    }
    if (!actionSel.value) {
      showError(actionSel, "Pick an action.");
      return;
    }
    /** @type {Record<string, string | boolean>} */
    const args = {};
    for (const [name, input] of argInputs) {
      if (input instanceof HTMLInputElement && input.type === "checkbox") {
        if (input.checked) args[name] = true;
      } else if (input.value.trim()) args[name] = input.value.trim();
    }
    const body = scheduleBody({
      stack: stackSel.value,
      action: actionSel.value,
      args,
      when: wv.when,
      enabled: enabled.checked,
      note: note.value,
    });
    save.disabled = true;
    const r = existing
      ? await send(
          "PUT",
          `/data/schedules/${encodeURIComponent(existing.id)}`,
          body,
          "the schedule",
        )
      : await send("POST", "/data/schedules", body, "the schedule");
    save.disabled = false;
    if (!r.ok) {
      formErr.replaceChildren(
        refusalCallout(r.error, "destructive", "Not saved"),
      );
      return;
    }
    d.close();
    notify(existing ? "Schedule saved." : "Schedule created.", "success");
    void loadSchedules();
  });
}
