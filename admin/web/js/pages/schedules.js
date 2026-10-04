// Schedules (feat-stacks-8, arch-schedule; redesigned for release 3.71.0 as
// the demo Kenny approved on 2026-10-03): what the dashboard runs on its
// own and when. A "Next run" hero (Run now, Skip this run), the next seven
// days as a calendar with the host's nightly round for reference, every
// schedule in a table with its switch first, and a New/Edit drawer that
// reads as a sentence and previews the next three runs. Every time is the
// host's wall clock. A page module of its own, so Activity can host it as a
// tab: `mount(root)` draws into any container and returns its cleanup.

import {
  act,
  actionLabel,
  catalogReady,
  loadSchedules,
  onAct,
  send,
} from "../act.js";
import { scheduleArgFields, schedulableActions } from "../actionforms.js";
import { fieldEl, refusalCallout } from "../actui.js";
import { agoEl, setAgo } from "../ago.js";
import { fetchJson, h } from "../dom.js";
import { declare, declareField, dialogControl, drivable } from "../drivable.js";
import {
  EVERY_WORDS,
  TEMPLATES,
  WEEKDAYS,
  agenda,
  cadenceText,
  countText,
  dateTimeText,
  dayText,
  draftBody,
  draftFor,
  lastRunView,
  nearNightly,
  nextRuns,
  nextUp,
  scheduleTitle,
  stackTops,
  targetText,
  timeText,
  untilText,
  weekPlan,
} from "../schedules.js";
import { current } from "../store.js";
import {
  TOAST_MS,
  keyRow,
  pageHeader,
  rowMenu,
  section,
  toast,
} from "../ui.js";

/** How long a change can be undone: as long as its toast shows. */
const UNDO_MS = TOAST_MS;

/**
 * One of the page's cards (ui.js `section`), its foot one line of text.
 * @param {{title?: string, desc?: string, body: import("../dom.js").Child,
 *   foot?: string, id?: string, label?: string}} spec
 */
const card = (spec) =>
  section({
    ...spec,
    plain: true,
    foot: spec.foot ? [spec.foot] : undefined,
  }).el;

/**
 * @typedef {import("../schedules.js").ScheduleView} ScheduleView
 * @typedef {import("../schedules.js").Schedule} Schedule
 * @typedef {import("../schedules.js").Draft} Draft
 * @typedef {import("../schedules.js").Template} Template
 */

// fix-239, invariant 39: Live view reaches every control on this page.
// review M5: every page field Live view may set is declared (drivable.js
// `declareField`); the client and the dashboard refuse any other.
const SCHED_ACTION = declareField({
  id: "sched-action",
  page: "schedules",
  what: "what the schedule runs (New schedule drawer)",
});
const SCHED_STACK = declareField({
  id: "sched-stack",
  page: "schedules",
  what: "the stack the schedule runs on (New schedule drawer)",
});
const SCHED_EVERY = declareField({
  id: "sched-every",
  page: "schedules",
  what: "how often the schedule runs (New schedule drawer)",
});
const SCHED_DATE = declareField({
  id: "sched-date",
  page: "schedules",
  what: "the date of a one-time run (New schedule drawer)",
});
const SCHED_AT = declareField({
  id: "sched-at",
  page: "schedules",
  what: "the time of day the schedule runs (New schedule drawer)",
});
const SCHED_NOTE = declareField({
  id: "sched-note",
  page: "schedules",
  what: "the schedule's note (New schedule drawer)",
});

const NEW_SCHEDULE = declare({
  id: "new-schedule",
  page: "schedules",
  opens: "dialog",
  what: "open the New schedule drawer",
});
const SCHEDULE_TEMPLATE = declare({
  id: "schedule-template",
  page: "schedules",
  opens: "dialog",
  row: "<template id>",
  what: "open the New schedule drawer filled in from a template (empty page only)",
  shows: "on an empty Schedules page",
});
const TOGGLE_SCHEDULE = declare({
  id: "toggle-schedule",
  page: "schedules",
  opens: "run",
  row: "<schedule id>",
  what: "turn one schedule on or off (Undo for 6 s)",
});
const SCHEDULE_MENU = declare({
  id: "schedule-menu",
  page: "schedules",
  opens: "dialog",
  row: "<schedule id>",
  what: "open one schedule's menu: press edit, run-now or delete",
  // drive-reach: 3.71.0 moved a row's Edit and Delete into this menu.
  was: [
    { id: "edit-schedule", press: "edit" },
    { id: "delete-schedule", press: "delete" },
  ],
});
const RUN_NEXT = declare({
  id: "run-next-now",
  page: "schedules",
  opens: "run",
  what: "start the next scheduled action now, as a job; its times stay",
});
const SKIP_NEXT = declare({
  id: "skip-next-run",
  page: "schedules",
  opens: "run",
  what: "skip only the next run; the schedule stays on",
});
const UNDO_CHANGE = declare({
  id: "undo-schedule-change",
  page: "schedules",
  opens: "run",
  what: "undo the last switch or delete while its toast shows (6 s)",
  shows: "for 6 s after a switch or a delete",
  reach: [
    { do: "click", control: "schedule-menu", row: "*" },
    { do: "press", button: "delete" },
  ],
});
const FIND_SCHEDULE = declare({
  id: "find-schedule",
  page: "schedules",
  opens: "run",
  row: "<schedule id>",
  what: "click a run on the calendar to find its schedule in the list",
});
const SORT_SCHEDULES = declare({
  id: "sort-schedules",
  page: "schedules",
  opens: "run",
  row: "<column: on, what, when, next, last, note>",
  what: "sort the schedules by a column (again: the other way)",
});

/** The zone a host answers in before its list has come. */
const DEFAULT_ZONE = "Europe/Brussels";
/** Calendar geometry: a 24-hour column this tall, pills at least this far apart. */
const COL_PX = 240;
const PILL_GAP = 20;
const COLOURS = ["", "c2", "c3", "c4", "c5"];

/** The host's nightly round hour, read once per page visit. */
async function readNightly() {
  const r = await fetchJson("/data/host-settings", "the host settings");
  if (!r.ok) return undefined;
  /** @type {{key: string, value: unknown}[]} */
  const fields = r.body?.page?.fields ?? [];
  const v = fields.find((f) => f.key === "backup_hour")?.value;
  return typeof v === "number" && v >= 0 && v <= 23 ? v : null;
}

const now = () => Date.now() / 1000;
const hostTarget = () => act.catalog?.host_target ?? "_host";
/** @param {Schedule} s */
const titleOf = (s) =>
  scheduleTitle(actionLabel(s.action), s.stack, hostTarget());

/**
 * @param {HTMLElement} root
 * @param {{level?: "h1" | "h2"}} [opts] `level`: "h2" when another page
 *   (Activity's "Planned" view) carries the page's h1
 * @returns {() => void}
 */
export function mount(root, opts = {}) {
  // `nx-ops`: the shared blocks in the ops-kit look the Schedules demo uses.
  root.classList.add("sch-root", "nx-ops");
  /** @type {number | null | undefined} undefined: not read (yet) */
  let nightly;
  /** Deletes waiting out their Undo: id → timer. */
  /** @type {Map<string, ReturnType<typeof setTimeout>>} */
  const pendingDeletes = new Map();
  /** @type {{key: string, dir: 1 | -1}} */
  let sort = readSort();
  let alive = true;
  /** A toast on this page; its Undo is a control Live view can press. */
  /** @param {string} text @param {() => void} [undo] */
  const say = (text, undo) =>
    toast(text, {
      host: root,
      replace: "all",
      action: undo
        ? { label: "Undo", run: undo, drive: { id: UNDO_CHANGE } }
        : undefined,
    });

  const zone = () => act.schedules?.zone ?? DEFAULT_ZONE;
  const list = () =>
    (act.schedules?.schedules ?? []).filter(
      (v) => !pendingDeletes.has(v.schedule.id),
    );

  const count = h("span", { id: "sched-count", class: "sch-num" });
  const zoneChip = h("span", { class: "sch-chip", id: "sched-zone" });
  const read = agoEl("read");
  const add = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--primary",
      id: "sched-new",
      title: "Pick an action, a target and when it runs (N)",
      onclick: () => void openDrawer(null, null),
    },
    // redesign-final-h5: no key badge on this one button; N is in its
    // description, as every other button's key is.
    "New schedule",
  );
  drivable(add, NEW_SCHEDULE);
  const header = pageHeader({
    level: opts.level,
    title: "Schedules",
    desc: "What the dashboard runs on its own, and when. A slot missed while the dashboard was down is skipped and you get a notice; it never runs late.",
    meta: [zoneChip, count, read],
    actions: [add],
  }).el;
  // redesign-final-h5: hosted in Activity's Planned view this is a section
  // of that page (an h2 at section size), never a second page title.
  if (opts.level === "h2") header.classList.add("sch-head--hosted");
  const body = h("div", { class: "sch-body" });
  // The page's top-level pieces sit straight in `root`, so the shell's one
  // section gap (fix-206) spaces them.
  /** @type {HTMLElement[]} */
  let pieces = [];

  const paintZone = () => {
    const z = zone();
    let mine = "";
    try {
      mine = Intl.DateTimeFormat().resolvedOptions().timeZone;
    } catch {
      mine = "";
    }
    zoneChip.replaceChildren(
      h("span", { class: "sch-dot sch-dot--info" }),
      mine === z
        ? `Host clock: ${z}, the same as yours`
        : `Host clock: ${z}; yours is ${mine || "unknown"}, times here are the host's`,
    );
  };

  const show = (/** @type {HTMLElement[]} */ next) => {
    for (const p of pieces) p.remove();
    pieces = next;
    root.append(...next);
  };

  const loading = () => {
    count.textContent = "";
    show([
      card({
        label: "Reading the schedules",
        body: h(
          "div",
          {
            "data-kp-state": "loading",
            class: "sch-hero",
            "aria-busy": "true",
          },
          h(
            "div",
            null,
            h("div", { class: "sch-label" }, "Next run"),
            h("span", {
              class: "sch-sk",
              style: "width:180px;height:32px;margin-top:4px",
            }),
          ),
          h(
            "div",
            { class: "sch-hero__what" },
            "Reading the schedules from the dashboard…",
          ),
          h("span"),
        ),
      }),
      card({
        title: "The next 7 days",
        desc: "Every run on the calendar, read from the schedules below.",
        body: h("span", {
          class: "sch-sk",
          style: `height:${COL_PX + 24}px`,
        }),
      }),
      card({
        title: "All schedules",
        desc: "Turn one off with its switch; its slots are kept but nothing runs.",
        body: h(
          "div",
          { style: "display:grid;gap:10px" },
          [0, 1, 2].map(() =>
            h("span", { class: "sch-sk", style: "height:36px" }),
          ),
        ),
      }),
    ]);
  };

  const failed = (/** @type {import("../doctor.js").RouteError} */ e) => {
    const retry = h(
      "button",
      { type: "button", class: "kp-button", onclick: () => void first() },
      "Read again",
    );
    show([
      card({
        title: "The schedules could not be read",
        desc: "Nothing changed; the dashboard's answer is below.",
        body: h(
          "div",
          { style: "display:grid;gap:12px;justify-items:start" },
          refusalCallout(e, "destructive", "Could not read"),
          retry,
        ),
      }),
    ]);
  };

  const render = () => {
    if (!alive || !act.schedules) return;
    paintZone();
    const all = list();
    // The empty page says it in its own card; the demo shows no "0 · 0".
    count.textContent = all.length ? countText(all) : "";
    const focused = /** @type {HTMLElement | null} */ (
      document.activeElement?.closest?.("tr[data-id]") ?? null
    )?.dataset.id;
    show(all.length ? filled(all) : empty());
    if (focused)
      /** @type {HTMLElement | null} */ (
        root.querySelector(`tr[data-id="${CSS.escape(focused)}"]`)
      )?.focus({ preventScroll: true });
  };

  // ── the empty page ──
  const empty = () => {
    const round =
      typeof nightly === "number"
        ? `Nothing runs on its own apart from the host's nightly round at ${pad(nightly)}:00.`
        : "Nothing runs on its own yet.";
    return [
      card({
        title: "No schedules yet",
        desc: `${round} Start from one of these, or make your own with New schedule.`,
        body: h(
          "div",
          { class: "sch-templates" },
          TEMPLATES.map((t) =>
            drivable(
              h(
                "button",
                {
                  type: "button",
                  class: "sch-tpl",
                  "data-template": t.id,
                  onclick: () => void openDrawer(null, t),
                },
                h("b", null, t.title),
                h("span", null, t.says),
              ),
              SCHEDULE_TEMPLATE,
              t.id,
            ),
          ),
        ),
      }),
    ];
  };

  // ── the filled page ──
  const filled = (/** @type {ScheduleView[]} */ all) => {
    const z = zone();
    const t = now();
    const next = nextUp(all);
    /** @type {Map<string, string>} */
    const colour = new Map(
      all.map((v, i) => [v.schedule.id, COLOURS[i % COLOURS.length]]),
    );
    return [
      hero(next, z, t),
      card({
        title: "The next 7 days",
        desc:
          typeof nightly === "number"
            ? "Every run on the calendar; the dashed blocks are the host's own nightly round, for reference."
            : nightly === null
              ? "Every run on the calendar; the host has no nightly round."
              : "Every run on the calendar; the host's nightly round is not shown, its hour could not be read.",
        body: [week(all, z, t, colour), agendaList(all, z, t)],
        id: "sched-week",
      }),
      card({
        title: "All schedules",
        desc: "Turn one off with its switch; its slots are kept but nothing runs.",
        body: h("div", { class: "sch-scroll" }, table(all, z, t)),
        foot: "Times are the host's clock; a run that starts becomes a job on Activity.",
        id: "sched-all",
      }),
      keyRow([
        ["N", "new schedule"],
        ["Space", "turn the focused schedule on or off"],
        ["E", "edit it"],
        ["Esc", "close the drawer or menu"],
        ["click a header", "sort"],
      ]),
    ];
  };

  const hero = (
    /** @type {ScheduleView | null} */ next,
    /** @type {string} */ z,
    /** @type {number} */ t,
  ) => {
    if (!next || next.next_run == null)
      return card({
        label: "Next run",
        id: "sched-hero",
        body: h(
          "div",
          { class: "sch-hero" },
          h(
            "div",
            null,
            h("div", { class: "sch-label" }, "Next run"),
            h("div", { class: "sch-hero__when" }, "None"),
          ),
          h(
            "div",
            { class: "sch-hero__what" },
            "Every schedule is off or has no further run.",
          ),
          h("span"),
        ),
      });
    const s = next.schedule;
    const at = next.next_run;
    const runNow = h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm",
        id: "sched-run-now",
        title:
          "Start this action now as a job; the schedule keeps its next slot",
        onclick: () => void runNowFor(s),
      },
      "Run now",
    );
    drivable(runNow, RUN_NEXT);
    const skip = h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm",
        id: "sched-skip",
        title: "Skip only this one run; the schedule stays on",
        onclick: () => void skipRun(s, at),
      },
      "Skip this run",
    );
    drivable(skip, SKIP_NEXT);
    return card({
      label: "Next run",
      id: "sched-hero",
      body: h(
        "div",
        { class: "sch-hero" },
        h(
          "div",
          null,
          h("div", { class: "sch-label" }, "Next run"),
          h("div", { class: "sch-hero__when" }, untilText(at, t)),
        ),
        h(
          "div",
          { class: "sch-hero__what" },
          h("b", null, titleOf(s)),
          `${dateTimeText(at, z)} · ${lowerFirst(cadenceText(s.when))}`,
        ),
        h("div", { class: "sch-btn-row" }, runNow, skip),
      ),
    });
  };

  const week = (
    /** @type {ScheduleView[]} */ all,
    /** @type {string} */ z,
    /** @type {number} */ t,
    /** @type {Map<string, string>} */ colour,
  ) => {
    const plan = weekPlan(
      all,
      t,
      z,
      typeof nightly === "number" ? nightly : null,
      actionLabel,
      hostTarget(),
    );
    const y = (/** @type {number} */ h) => (h / 24) * COL_PX;
    const wrap = h("div", {
      class: "sch-week",
      role: "group",
      "aria-label": "Runs in the next 7 days",
    });
    wrap.append(
      h(
        "div",
        { class: "sch-week__head" },
        h("div"),
        plan.days.map((d) =>
          h(
            "div",
            { class: `sch-week__day${d.today ? " today" : ""}` },
            d.head,
          ),
        ),
      ),
      h(
        "div",
        { class: "sch-week__axis", "aria-hidden": "true" },
        [0, 6, 12, 18, 24].map((hr) =>
          h("span", { style: `top:${y(hr)}px` }, `${pad(hr)}:00`),
        ),
      ),
    );
    for (const d of plan.days) {
      const col = h("div", {
        class: `sch-week__col${d.today ? " today" : ""}`,
      });
      const tops = stackTops(
        d.pills.map((p) => Math.max(10, y(p.hour))),
        PILL_GAP,
      );
      d.pills.forEach((p, i) => {
        const top = `top:${Math.min(tops[i], COL_PX - 10)}px`;
        if (p.host) {
          col.append(
            h(
              "div",
              {
                class: "sch-pill sch-pill--host",
                style: top,
                title: `The host's own nightly round at ${p.at} (backups, updates), for reference`,
              },
              h("b", null, p.at),
              p.text,
            ),
          );
          return;
        }
        const id = /** @type {string} */ (p.id);
        const cls = [
          "sch-pill",
          colour.get(id) ?? "",
          p.next ? "next" : "",
          p.off ? "off" : "",
          p.skipped ? "skipped" : "",
        ]
          .filter(Boolean)
          .join(" ");
        const state = p.off
          ? " (off)"
          : p.skipped
            ? " (skipped)"
            : p.next
              ? " (next run)"
              : "";
        col.append(
          drivable(
            h(
              "button",
              {
                type: "button",
                class: cls,
                style: top,
                "data-id": id,
                title: `${p.text} · ${dateTimeText(p.t, z)}${state}: click to find it in the list`,
                onclick: () => flash(id),
              },
              h("b", null, p.at),
              p.text,
            ),
            FIND_SCHEDULE,
            id,
          ),
        );
      });
      if (d.today)
        col.append(
          h("div", {
            class: "sch-nowline",
            style: `top:${y(plan.nowHour)}px`,
            title: `now, ${timeText(t, z)}`,
          }),
        );
      wrap.append(col);
    }
    return wrap;
  };

  const agendaList = (
    /** @type {ScheduleView[]} */ all,
    /** @type {string} */ z,
    /** @type {number} */ t,
  ) => {
    const rows = agenda(all, t, z, actionLabel, 8, hostTarget());
    return h(
      "div",
      { class: "sch-agenda", "aria-label": "The next runs" },
      rows.length
        ? rows.map((r) =>
            h("div", { "data-id": r.id }, h("span", null, r.when), r.text),
          )
        : h("div", null, h("span", null, "—"), "No run in the next 7 days."),
    );
  };

  /** @type {[string, string, (v: ScheduleView) => string | number][]} */
  const COLUMNS = [
    ["on", "On", (v) => (v.schedule.enabled ? 0 : 1)],
    ["what", "What", (v) => titleOf(v.schedule).toLowerCase()],
    ["when", "When", (v) => cadenceText(v.schedule.when)],
    ["next", "Next run", (v) => v.next_run ?? Number.MAX_SAFE_INTEGER],
    [
      "last",
      "Last run",
      (v) =>
        Math.max(
          v.schedule.last_run?.slot ?? 0,
          v.schedule.last_missed?.slot ?? 0,
        ),
    ],
    ["note", "Note", (v) => v.schedule.note.toLowerCase()],
  ];

  const table = (
    /** @type {ScheduleView[]} */ all,
    /** @type {string} */ z,
    /** @type {number} */ t,
  ) => {
    const col = COLUMNS.find((c) => c[0] === sort.key);
    const rows = col
      ? [...all].sort((a, b) => {
          const x = col[2](a);
          const y = col[2](b);
          return (x < y ? -1 : x > y ? 1 : 0) * sort.dir;
        })
      : all;
    const head = h(
      "tr",
      null,
      COLUMNS.map(([key, label]) => {
        const b = drivable(
          h(
            "button",
            {
              type: "button",
              class: "sch-sort",
              title: `Sort by ${label.toLowerCase()}`,
              onclick: () => {
                sort =
                  sort.key === key
                    ? { key, dir: sort.dir === 1 ? -1 : 1 }
                    : { key, dir: 1 };
                saveSort(sort);
                render();
              },
            },
            label,
          ),
          SORT_SCHEDULES,
          key,
        );
        return h(
          "th",
          {
            class: key === "last" || key === "note" ? "sch-hide-phone" : null,
            "aria-sort":
              sort.key === key
                ? sort.dir === 1
                  ? "ascending"
                  : "descending"
                : null,
          },
          b,
        );
      }),
      h("th", null, h("span", { class: "nx-vh" }, "Actions")),
    );
    return h(
      "table",
      { class: "sch-table" },
      h("thead", null, head),
      h(
        "tbody",
        null,
        rows.map((v) => row(v, z, t)),
      ),
    );
  };

  const row = (
    /** @type {ScheduleView} */ v,
    /** @type {string} */ z,
    /** @type {number} */ t,
  ) => {
    const s = v.schedule;
    const sw = h("input", {
      class: "kp-switch__input",
      type: "checkbox",
      role: "switch",
      "aria-label": `${titleOf(s)} on`,
      title: "On: runs at its times. Off: keeps the schedule, runs nothing.",
      onchange: (/** @type {Event} */ e) =>
        void setOn(s, /** @type {HTMLInputElement} */ (e.target).checked),
    });
    sw.checked = s.enabled;
    drivable(sw, TOGGLE_SCHEDULE, s.id);
    const menuBtn = h(
      "button",
      {
        type: "button",
        class: "nx-icon-btn",
        "aria-label": `Edit, run now or delete ${titleOf(s)}`,
        "aria-haspopup": "menu",
        title: "Edit · Run now · Delete",
        onclick: (/** @type {MouseEvent} */ e) =>
          openMenu(/** @type {HTMLElement} */ (e.currentTarget), s),
      },
      "···",
    );
    drivable(menuBtn, SCHEDULE_MENU, s.id);
    const last = lastRunView(v, z);
    const arg = argSummary(s.args);
    return h(
      "tr",
      { class: s.enabled ? null : "off", "data-id": s.id, tabindex: 0 },
      h("td", null, h("label", { class: "kp-switch" }, sw)),
      h(
        "td",
        { class: "sch-what" },
        h("b", null, actionLabel(s.action)),
        h(
          "small",
          null,
          targetText(s.stack, hostTarget()),
          arg ? h("span", { class: "mono" }, ` · ${arg}`) : null,
        ),
      ),
      h(
        "td",
        null,
        h(
          "div",
          { class: "sch-cadence" },
          cadenceText(s.when),
          s.when.every === "week"
            ? h(
                "span",
                { class: "sch-days", "aria-hidden": "true" },
                WEEKDAYS.map((d, i) =>
                  h(
                    "i",
                    {
                      class:
                        s.when.every === "week" && s.when.days.includes(i)
                          ? "on"
                          : null,
                    },
                    d[0],
                  ),
                ),
              )
            : null,
        ),
      ),
      h(
        "td",
        null,
        s.enabled && v.next_run != null
          ? h(
              "div",
              { class: "sch-cadence" },
              h("span", { class: "sch-num" }, untilText(v.next_run, t)),
              h("small", null, dateTimeText(v.next_run, z)),
            )
          : h(
              "span",
              { class: "sch-muted" },
              s.enabled ? "no further run" : "off",
            ),
      ),
      h(
        "td",
        { class: "sch-hide-phone" },
        last.tone === "none" && last.job == null
          ? h("span", { class: "sch-muted" }, last.text)
          : h(
              "span",
              { class: "sch-status" },
              h("span", {
                class: `sch-dot${last.tone === "none" ? "" : ` sch-dot--${last.tone}`}`,
              }),
              last.job == null ? last.text : `${last.text} · `,
              last.job == null
                ? null
                : h(
                    "a",
                    { class: "sch-link", href: `/jobs?job=${last.job}` },
                    `job ${last.job}`,
                  ),
            ),
      ),
      h("td", { class: "sch-hide-phone sch-muted" }, s.note || "—"),
      h("td", null, menuBtn),
    );
  };

  // ── actions ──
  /** @param {Schedule} s @param {boolean} on @param {boolean} [quiet] */
  const setOn = async (s, on, quiet = false) => {
    paintOn(s.id, on);
    const r = await send(
      "PUT",
      `/data/schedules/${encodeURIComponent(s.id)}`,
      { ...bodyOf(s), enabled: on },
      "the schedule",
    );
    if (!r.ok) {
      paintOn(s.id, !on);
      say(`Not saved: ${r.error.why}`);
      return;
    }
    const view = /** @type {ScheduleView} */ (r.body);
    if (!quiet)
      say(
        on
          ? `${titleOf(s)} is on again: next run ${view.next_run == null ? "none" : dateTimeText(view.next_run, zone())}.`
          : `${titleOf(s)} is off. Its slots are kept; nothing runs until you turn it on.`,
        () => void setOn({ ...s, enabled: on }, !on, true),
      );
    void loadSchedules();
  };

  /** The row and its pills show `on` at once, before the host answers. */
  const paintOn = (/** @type {string} */ id, /** @type {boolean} */ on) => {
    const tr = root.querySelector(`tr[data-id="${CSS.escape(id)}"]`);
    tr?.classList.toggle("off", !on);
    const input = /** @type {HTMLInputElement | null} */ (
      tr?.querySelector('input[role="switch"]') ?? null
    );
    if (input) input.checked = on;
    root
      .querySelectorAll(`.sch-pill[data-id="${CSS.escape(id)}"]`)
      .forEach((p) => p.classList.toggle("off", !on));
    const all = list().map((v) =>
      v.schedule.id === id
        ? { ...v, schedule: { ...v.schedule, enabled: on } }
        : v,
    );
    count.textContent = countText(all);
  };

  /** @param {Schedule} s */
  const runNowFor = async (s) => {
    const r = await send(
      "POST",
      `/data/actions/${encodeURIComponent(s.stack)}/${encodeURIComponent(s.action)}`,
      s.args ?? {},
      `${actionLabel(s.action)} ${s.stack}`,
    );
    if (!r.ok) {
      say(`Not started: ${r.error.why}`);
      return;
    }
    say(`${titleOf(s)} started as job ${r.body?.job}. Follow it on Activity.`);
  };

  /** @param {Schedule} s @param {number} slot */
  const skipRun = async (s, slot) => {
    const r = await send(
      "POST",
      `/data/schedules/${encodeURIComponent(s.id)}/skip`,
      { slot },
      "skipping a run",
    );
    if (!r.ok) {
      say(`Not skipped: ${r.error.why}`);
      void loadSchedules();
      return;
    }
    const z = zone();
    const next = /** @type {ScheduleView} */ (r.body).next_run;
    say(
      `The ${timeText(slot, z)} run is skipped; ${next == null ? "there is no next one" : `the next one is ${dayWord(next, z)}`}.`,
    );
    void loadSchedules();
  };

  /** @param {Schedule} s */
  const remove = (s) => {
    const prior = pendingDeletes.get(s.id);
    if (prior) clearTimeout(prior);
    pendingDeletes.set(
      s.id,
      setTimeout(() => void commitDelete(s.id), UNDO_MS),
    );
    render();
    say(`${titleOf(s)} deleted.`, () => {
      const timer = pendingDeletes.get(s.id);
      if (timer) clearTimeout(timer);
      pendingDeletes.delete(s.id);
      render();
    });
  };

  const commitDelete = async (/** @type {string} */ id) => {
    const timer = pendingDeletes.get(id);
    if (timer) clearTimeout(timer);
    const r = await send(
      "DELETE",
      `/data/schedules/${encodeURIComponent(id)}`,
      undefined,
      "the schedule",
    );
    pendingDeletes.delete(id);
    if (!r.ok && alive) say(`Not deleted: ${r.error.why}`);
    void loadSchedules();
  };

  const flash = (/** @type {string} */ id) => {
    const tr = /** @type {HTMLElement | null} */ (
      root.querySelector(`tr[data-id="${CSS.escape(id)}"]`)
    );
    if (!tr) return;
    tr.scrollIntoView({ block: "center", behavior: "smooth" });
    tr.classList.add("sch-flash");
    tr.focus({ preventScroll: true });
    setTimeout(() => tr.classList.remove("sch-flash"), 1400);
  };

  // ── the row menu: a non-modal dialog, so Live view can press its items ──
  /** @type {ReturnType<typeof rowMenu> | null} */
  let menu = null;
  const closeMenu = () => {
    menu?.close();
    menu = null;
  };
  /** @param {HTMLElement} btn @param {Schedule} s */
  const openMenu = (btn, s) => {
    menu = rowMenu({
      anchor: btn,
      label: `${titleOf(s)}: edit, run now or delete`,
      title: titleOf(s),
      items: [
        {
          name: "edit",
          label: "Edit…",
          hint: "change what, where or when",
          run: () => void openDrawer(s, null),
        },
        {
          name: "run-now",
          label: "Run now",
          hint: "start it once as a job; its times stay",
          run: () => void runNowFor(s),
        },
        {
          name: "delete",
          label: "Delete",
          hint: "remove it; jobs it ran stay on Activity; Undo for 6 s",
          run: () => remove(s),
          danger: true,
        },
      ],
    });
  };

  // ── the New / Edit drawer ──
  const ctx = () => ({
    stacks: (current().fleet?.stacks ?? []).map((x) => x.name),
    hostTarget: hostTarget(),
    now: now(),
    zone: zone(),
  });

  /** @param {Schedule | null} existing @param {Template | null} template */
  const openDrawer = async (existing, template) => {
    document
      .querySelectorAll("dialog.sch-drawer")
      .forEach((d) => /** @type {HTMLDialogElement} */ (d).close());
    closeMenu();
    const catalog = await catalogReady();
    if (!catalog || !alive) return;
    const c = ctx();
    const d = draftFor(existing, template, c);
    const actions = schedulableActions(catalog);
    const stackActions = actions.filter((a) => a.target === "stack");
    const hostActions = actions.filter((a) => a.target === "host");
    if (!actions.some((a) => a.action === d.action) && actions[0]) {
      d.action = actions[0].action;
      d.stack = actions[0].target === "host" ? c.hostTarget : d.stack;
    }

    const actionSel = h(
      "select",
      { id: SCHED_ACTION, "aria-label": "Action" },
      [...stackActions, ...hostActions].map((a) =>
        h("option", { value: a.action }, a.label),
      ),
    );
    actionSel.value = d.action;
    const stackSel = h("select", { id: SCHED_STACK, "aria-label": "On" });
    const everySel = h(
      "select",
      { id: SCHED_EVERY, "aria-label": "How often" },
      Object.entries(EVERY_WORDS).map(([v, w]) => h("option", { value: v }, w)),
    );
    everySel.value = d.every;
    const date = h("input", {
      type: "date",
      id: SCHED_DATE,
      "aria-label": "On the date",
    });
    date.value = d.date;
    const at = h("input", {
      type: "time",
      id: SCHED_AT,
      "aria-label": "At",
    });
    at.value = d.at;
    const dateBit = h("span", null, " on ", date);
    const sentence = h(
      "p",
      { class: "sch-sentence" },
      "Run ",
      actionSel,
      " on ",
      stackSel,
      " ",
      everySel,
      dateBit,
      " at ",
      at,
    );
    const days = new Set(d.days.length ? d.days : [1, 4]);
    const dayBox = h("span", { class: "sch-daypick" });
    const daysWhy = h("span", { class: "sch-field__why", role: "alert" });
    const daysField = h(
      "div",
      { class: "sch-field" },
      h("span", { class: "sch-field__label" }, "Weekdays"),
      dayBox,
      daysWhy,
    );
    const argBox = h("div", { class: "sch-field" });
    /** @type {Map<string, HTMLInputElement | HTMLSelectElement>} */
    let argInputs = new Map();
    const hour = typeof nightly === "number" ? nightly : null;
    const near = h(
      "div",
      { class: "sch-alert", id: "sched-near", role: "status" },
      h("span", { class: "sch-alert__icon", "aria-hidden": "true" }, "!"),
      h(
        "div",
        null,
        h("div", { class: "sch-alert__t" }, "Close to the nightly round"),
        h(
          "div",
          { class: "sch-alert__d" },
          hour == null
            ? ""
            : `This is within an hour of the host's nightly round at ${pad(hour)}:00; a long run may still be busy when the round starts, and the round then waits. ${pad((hour + 1) % 24)}:00 or later avoids that.`,
        ),
      ),
    );
    const preview = h("div", {
      class: "sch-preview",
      "aria-live": "polite",
    });
    const note = h("input", {
      id: SCHED_NOTE,
      type: "text",
      maxlength: 200,
      placeholder: "Why this runs; shown in the list",
    });
    note.value = d.note;
    const formErr = h("div");

    const fillStacks = () => {
      const entry = actions.find((a) => a.action === actionSel.value);
      const host = entry?.target === "host";
      const names = host
        ? [c.hostTarget]
        : [
            ...new Set([
              ...c.stacks,
              ...(existing && existing.stack !== c.hostTarget
                ? [existing.stack]
                : []),
            ]),
          ];
      const keep = stackSel.value || d.stack;
      stackSel.replaceChildren(
        ...names.map((n) =>
          h("option", { value: n }, n === c.hostTarget ? "the whole host" : n),
        ),
      );
      if (names.includes(keep)) stackSel.value = keep;
      fillArgs();
    };
    const fillArgs = () => {
      const entry = actions.find((a) => a.action === actionSel.value);
      argInputs = new Map();
      const fields = entry ? scheduleArgFields(entry, stackSel.value) : [];
      const apps =
        current()
          .fleet?.stacks.find((x) => x.name === stackSel.value)
          ?.apps?.map((a) => a.name) ?? [];
      argBox.hidden = fields.length === 0;
      argBox.replaceChildren(
        h("span", { class: "sch-field__label" }, "Options"),
        ...fields.map((f) => {
          const prev =
            existing?.action === entry?.action ? d.args[f.name] : undefined;
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
    const repaint = () => {
      const every = everySel.value;
      daysField.hidden = every !== "week";
      dateBit.hidden = every !== "once";
      dayBox.replaceChildren(
        ...WEEKDAYS.map((name, i) =>
          dialogControl(
            h(
              "button",
              {
                type: "button",
                "aria-pressed": String(days.has(i)),
                title: `Run on ${name}`,
                onclick: () => {
                  if (days.has(i)) days.delete(i);
                  else days.add(i);
                  repaint();
                },
              },
              name,
            ),
            `day-${name.toLowerCase()}`,
          ),
        ),
      );
      daysWhy.textContent = "";
      near.hidden = !nearNightly(at.value, hour);
      const when = whenOf();
      const z = zone();
      const runs = when ? nextRuns(when, now(), 3, z) : [];
      preview.replaceChildren(
        h("h3", null, "It will run"),
        ...(runs.length
          ? runs.map((t) =>
              h(
                "div",
                { "data-slot": t },
                dateTimeText(t, z),
                h("span", null, untilText(t, now())),
              ),
            )
          : [
              h(
                "p",
                null,
                every === "week" && days.size === 0
                  ? "Pick at least one weekday."
                  : every === "once"
                    ? "Pick a date and time still to come."
                    : "Pick a time.",
              ),
            ]),
      );
    };
    /** @returns {import("../schedules.js").When | null} */
    const whenOf = () => {
      const b = draftBody(draft());
      return b.ok ? b.body.when : null;
    };
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
        action: actionSel.value,
        stack: stackSel.value,
        every: everySel.value,
        days: [...days].sort(),
        at: at.value,
        date: date.value,
        note: note.value,
        enabled: d.enabled,
        args,
      };
    };

    actionSel.addEventListener("change", () => {
      fillStacks();
      repaint();
    });
    stackSel.addEventListener("change", fillArgs);
    everySel.addEventListener("change", repaint);
    at.addEventListener("input", repaint);
    at.addEventListener("change", repaint);
    date.addEventListener("input", repaint);
    date.addEventListener("change", repaint);
    fillStacks();
    repaint();

    const cancel = dialogControl(
      h(
        "button",
        {
          type: "button",
          class: "kp-button",
          onclick: () => dlg.close(),
        },
        "Cancel",
      ),
      "cancel",
    );
    const save = dialogControl(
      h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--primary",
          title: existing
            ? "Save the changes; a changed time starts from now"
            : "Add the schedule; it runs at the times previewed above",
        },
        existing ? "Save" : "Add schedule",
      ),
      "save",
    );
    const title = existing ? "Edit schedule" : "New schedule";
    const dlg = h(
      "dialog",
      { class: "sch-drawer", "aria-labelledby": "sched-drawer-title" },
      h(
        "div",
        { class: "sch-drawer__h" },
        h("h2", { id: "sched-drawer-title", class: "kp-dialog__title" }, title),
        h(
          "p",
          null,
          "Read it as a sentence; the preview shows exactly when it will run.",
        ),
      ),
      h(
        "div",
        { class: "sch-drawer__b" },
        sentence,
        daysField,
        argBox,
        near,
        preview,
        h(
          "div",
          { class: "sch-field" },
          h(
            "label",
            { for: "sched-note" },
            "Note ",
            h(
              "span",
              { class: "sch-muted", style: "font-weight:400" },
              "(optional)",
            ),
          ),
          note,
        ),
        formErr,
      ),
      h("div", { class: "sch-drawer__f" }, cancel, save),
    );
    dlg.addEventListener("close", () => dlg.remove());
    // A click on the backdrop (the dialog itself, outside its box) closes it.
    dlg.addEventListener("click", (e) => {
      if (e.target !== dlg) return;
      const b = dlg.getBoundingClientRect();
      if (
        e.clientX < b.left ||
        e.clientX > b.right ||
        e.clientY < b.top ||
        e.clientY > b.bottom
      )
        dlg.close();
    });
    save.addEventListener("click", async () => {
      formErr.replaceChildren();
      const v = draft();
      const b = draftBody(v);
      if (!b.ok) {
        if (b.field === "days") daysWhy.textContent = b.why;
        else
          formErr.replaceChildren(
            refusalCallout(
              { what: "the schedule", why: b.why, fix: "" },
              "warning",
              "Not yet",
            ),
          );
        return;
      }
      save.disabled = true;
      const r = existing
        ? await send(
            "PUT",
            `/data/schedules/${encodeURIComponent(existing.id)}`,
            b.body,
            "the schedule",
          )
        : await send("POST", "/data/schedules", b.body, "the schedule");
      save.disabled = false;
      if (!r.ok) {
        formErr.replaceChildren(
          refusalCallout(r.error, "destructive", "Not saved"),
        );
        return;
      }
      dlg.close();
      say(existing ? "Schedule saved." : "Schedule added.");
      void loadSchedules();
    });
    document.body.append(dlg);
    dlg.showModal();
    actionSel.focus();
  };

  // ── keys: N new, Space toggles the focused row, E edits it ──
  let lastG = 0;
  /** @param {KeyboardEvent} e */
  const onKey = (e) => {
    if (e.ctrlKey || e.metaKey || e.altKey || e.defaultPrevented) return;
    const t = /** @type {HTMLElement | null} */ (e.target);
    if (
      t?.closest?.("input, select, textarea, [contenteditable]") ||
      document.querySelector("dialog[open]")
    )
      return;
    if (e.key === "g") {
      lastG = Date.now();
      return;
    }
    // "g n" is the shortcut to Notifications, not a new schedule.
    if (Date.now() - lastG < 1500) return;
    if (e.key === "n" || e.key === "N") {
      e.preventDefault();
      void openDrawer(null, null);
      return;
    }
    const tr = /** @type {HTMLElement | null} */ (
      t?.closest?.("tr[data-id]") ?? null
    );
    if (!tr || !root.contains(tr)) return;
    const v = list().find((x) => x.schedule.id === tr.dataset.id);
    if (!v) return;
    if (e.key === " ") {
      e.preventDefault();
      void setOn(v.schedule, !v.schedule.enabled);
    } else if (e.key === "e" || e.key === "E") {
      e.preventDefault();
      void openDrawer(v.schedule, null);
    }
  };
  document.addEventListener("keydown", onKey);

  root.replaceChildren(header);
  paintZone();
  // "read … ago" counts from the moment the list last arrived.
  if (act.schedules) setAgo(read, now());
  const off = onAct("schedules", () => {
    setAgo(read, now());
    render();
  });
  const first = () => {
    if (!act.schedules) loading();
    void loadSchedules().then((e) => {
      if (e && !act.schedules) failed(e);
    });
  };
  void readNightly().then((h) => {
    nightly = h;
    render();
  });
  // The relative times ("in 2 h 14 min") move on.
  const tick = setInterval(() => {
    if (!document.querySelector("dialog.nx-rowmenu[open]")) render();
  }, 30_000);
  first();
  render();
  return () => {
    alive = false;
    off();
    clearInterval(tick);
    document.removeEventListener("keydown", onKey);
    closeMenu();
    document
      .querySelectorAll("dialog.sch-drawer")
      .forEach((d) => /** @type {HTMLDialogElement} */ (d).close());
    // A delete still waiting out its Undo goes through now.
    for (const id of [...pendingDeletes.keys()]) void commitDelete(id);
    root.classList.remove("sch-root", "nx-ops");
  };
}

/** "Every day at 10:00" → "every day at 10:00"; a weekday keeps its capital. */
const lowerFirst = (/** @type {string} */ s) =>
  /^(Every|Once)\b/.test(s) ? s[0].toLowerCase() + s.slice(1) : s;

/** @param {number} n */
const pad = (n) => String(n).padStart(2, "0");

/** "tomorrow at 10:00", "today at 22:00" or "05/10/2026 10:00". */
function dayWord(/** @type {number} */ t, /** @type {string} */ z) {
  const today = dayText(now(), z);
  const tomorrow = dayText(now() + 86_400, z);
  const day = dayText(t, z);
  if (day === today) return `today at ${timeText(t, z)}`;
  if (day === tomorrow) return `tomorrow at ${timeText(t, z)}`;
  return dateTimeText(t, z);
}

/** The arguments worth showing beside the target: an app, a commit. */
function argSummary(/** @type {Record<string, unknown>} */ args) {
  const v = args.commit ?? args.app ?? args.unit ?? args.tag;
  return typeof v === "string" ? v : "";
}

/** The body PUT wants for a schedule as it is. @param {Schedule} s */
function bodyOf(s) {
  return {
    stack: s.stack,
    action: s.action,
    args: s.args,
    when: s.when,
    enabled: s.enabled,
    note: (s.note ?? "").trim(),
  };
}

const SORT_KEY = "sched-sort";
/** @returns {{key: string, dir: 1 | -1}} */
function readSort() {
  try {
    const v = JSON.parse(localStorage.getItem(SORT_KEY) ?? "null");
    if (v && typeof v.key === "string" && (v.dir === 1 || v.dir === -1))
      return v;
  } catch {
    /* a private window: the default order */
  }
  return { key: "", dir: 1 };
}
/** @param {{key: string, dir: 1 | -1}} s */
function saveSort(s) {
  try {
    localStorage.setItem(SORT_KEY, JSON.stringify(s));
  } catch {
    /* not kept; the page still sorts */
  }
}
