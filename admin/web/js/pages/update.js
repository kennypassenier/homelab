// redesign-flows-1 (redesign 3.71.0, FLOWS.md §3.1, demo flows/update.html;
// Kenny approved 2026-10-03, decision "one Update flow"): updating an app
// in six visible steps a layman follows — 1 See (what has a newer version,
// a plain click includes or leaves out a row), 2 Impact (downtime, what
// restarts, the safety net, the undo, who notices, a major version's
// required tick, the change to the files), then 3 Back up, 4 Update,
// 5 Verify, 6 Done (a summary with Roll back…).
//
// Reached from the Inbox's "N apps have a newer version" row, a stack's
// Update (hub header, Stacks row: actiondialog.js sends a person's Update
// here), the palette ("update kp-soft", "Update apps with a newer
// version…") and the Map's stale-image rows. It lives at its own address
// since the review (redesign-flows-11): `/update?all=1` or
// `/update?stack=<stack>[&app=<key>]`; the old `/inbox?update=…` is sent on.
//
// redesign-flows-6 (review items 3 and 4): steps 3-5 are ONE job on the
// dashboard's server (`update-apps`, admin/src/shell/actions_flow.rs) —
// back up, commit the new image line, deploy, verify healthy at the new
// version within 2 min, and roll a pinned app back by itself when it is
// not. This page only starts it and follows it (its rows are the job's
// `flow`), so a closed tab stops nothing, and the job is in Activity like
// every other. The log comparison is read here from the job's deploy time.

import { act, onAct, send } from "../act.js";
import { refusalCallout } from "../actui.js";
import { stage } from "../stage.js";
import { attachCardStates, setCardState } from "/static/kp/js/card-state.js";
import { attachDeplists, deplist } from "/static/kp/js/deplist.js";
import {
  ensureStyle,
  errorBox,
  fetchJson,
  h,
  labelCells,
  selectCell,
  slowRead,
  tableBlock,
} from "../dom.js";
import { declare, drivable } from "../drivable.js";
import { diffBlocks } from "../editui.js";
import { longCountdown } from "../longcountdown.js";
import { formatDateTime, humanDuration } from "../format.js";
import { inboxNow } from "../inboxlist.js";
import { jobLogViewer } from "../joblog.js";
import { feedUpdates } from "../inboxsources.js";
import { openPinRollback } from "../pinupdate.js";
import { planView } from "../plan.js";
import { current } from "../store.js";
import { doneMark, emptyState, pageHeader, section, segSwitch } from "../ui.js";
import {
  firstChosen,
  flowRows,
  flowTitle,
  itemsFor,
  lastDeployS,
  logVerdict,
  movesOf,
  notesQuery,
  restartWords,
  safetyNet,
  scopeMatches,
  stacksOf,
  undoWords,
  updatesBody,
  whoNotices,
} from "../updateflow.js";
import { attachAckGates } from "/static/kp/js/ack-gate.js";
import { attachCodes, setCode } from "/static/kp/js/code.js";
import { attachDataTables } from "/static/kp/js/datatable.js";
import { attachIdmarks } from "/static/kp/js/idmark.js";
import { setPipeline, attachPipelines } from "/static/kp/js/pipeline.js";
import { bindSelection } from "../selectbind.js";
import { attachDisabledReasons } from "/static/kp/js/reason.js";
import {
  attachSteplists,
  buildSteplist,
  setStep,
} from "/static/kp/js/steplist.js";
import {
  attachVersionDeltas,
  VERSION_SORT,
  summarizeDeltas,
  versionLevel,
} from "/static/kp/js/version-delta.js";

const PICK = declare({
  id: "update-pick",
  page: "update",
  opens: "view",
  row: "<pin:stack:app/service | pull:stack>",
  what: "include or leave out one row of the Update flow's list (a plain click)",
  selects: true,
});
const SEE = declare({
  id: "update-see-impact",
  page: "update",
  opens: "view",
  what: "the Update flow's step 1 → 2: see what the update changes",
});
const BACK = declare({
  id: "update-back",
  page: "update",
  opens: "view",
  what: "the Update flow's step 2 → 1: back to the list",
  shows: "on step 2 (what changes)",
  reach: [{ do: "click", control: "update-see-impact" }],
});
const MAJOR_GATE = declare({
  id: "update-major-read",
  page: "update",
  opens: "view",
  row: "<pin:stack:app/service>",
  what: "tick “I read the release notes of …” for a major version (it can be ticked once the notes link was opened; a version with no link, after 4 seconds)",
  shows: "under the list, once a major version is ticked",
  reach: [{ do: "click", control: "update-pick", row: "*" }],
});
const UNTICK = declare({
  id: "update-untick",
  page: "update",
  opens: "view",
  what: "untick every ticked app of the Update flow's list",
  shows: "while an app is ticked",
  reach: [{ do: "click", control: "update-pick", row: "*" }],
});
const MAJORS = declare({
  id: "update-majors",
  page: "update",
  opens: "view",
  row: "alone|together",
  what: "allow one major version at a time (the default), or several together, each with its own read",
});
const LAYOUT = declare({
  id: "update-run-layout",
  page: "update",
  opens: "view",
  row: "board|steps",
  what: "show the running update as a board with a lane per step or as a list of steps",
  shows: "while the update runs or after it",
});
const GO = declare({
  id: "update-go",
  page: "update",
  opens: "run",
  what: "back up the chosen apps' stacks, update them and verify them",
  shows: "on step 2, once every major version's notes are ticked as read",
  reach: [{ do: "click", control: "update-see-impact" }],
});
const ROLL_BACK = declare({
  id: "update-roll-back",
  page: "update",
  opens: "dialog",
  row: "<pin:stack:app/service>",
  what: "the result's Roll back…: put the earlier version back the same way",
  shows: "in the result of an update that moved a pinned version",
});

const NOTES = declare({
  id: "update-release-notes",
  page: "update",
  opens: "view",
  row: "<pin:stack:app/service>",
  what: "a row's release notes, in a new tab",
});
const GATE_NOTES = declare({
  id: "update-major-gate-notes",
  page: "update",
  opens: "view",
  row: "<pin:stack:app/service>",
  what: "open a major version's release notes from its acknowledge gate, in a new tab (the tick waits for it)",
  shows: "under the list, once a major version is ticked",
  reach: [{ do: "click", control: "update-pick", row: "*" }],
});
const MAJOR_NOTES = declare({
  id: "update-major-notes",
  page: "update",
  opens: "view",
  row: "<pin:stack:app/service>",
  what: "a major version's release notes, in a new tab",
  shows: "on step 2 (what changes)",
  reach: [{ do: "click", control: "update-see-impact" }],
});
const DIFF = declare({
  id: "update-diff",
  page: "update",
  opens: "view",
  what: "fold or unfold the change to the stack files",
  shows: "on step 2 (what changes)",
  reach: [{ do: "click", control: "update-see-impact" }],
});
const LEAVE = declare({
  id: "update-leave",
  page: "update",
  opens: "view",
  what: "leave the running update (the job keeps going on the server)",
  shows: "while the flow's update runs",
});
const AFTER = declare({
  id: "update-after",
  page: "update",
  opens: "view",
  row: "activity|stack|needs-you|back|again",
  what: "the result's links: Activity, the stack, back to the Inbox, or the list again; or Back from an empty list",
  shows: "in the result of an update, or on an empty list (Back)",
});

/** The six steps (flows/update.html). */
const STEPS = ["See", "Impact", "Back up", "Update", "Verify", "Done"];

/**
 * @typedef {import("../updateflow.js").Item} Item
 * @typedef {import("../updateflow.js").Scope} Scope
 * @typedef {import("../pinupdate.js").Move} Move
 * @typedef {import("../jobs.js").Job} Job
 */

/**
 * The update job this browser started, per scope: a reload or a tab
 * opened again shows it, running or finished, until "Update more" (a
 * per-viewer convenience; the job itself lives on the server).
 */
const STARTED = "homelab-update-jobs";
const TAB_JOB = {
  /** @param {string} k @returns {number | undefined} */
  get(k) {
    try {
      const v = JSON.parse(localStorage.getItem(STARTED) ?? "{}")[k];
      return typeof v === "number" ? v : undefined;
    } catch {
      return undefined;
    }
  },
  /** @param {string} k @param {number | null} job */
  set(k, job) {
    try {
      const all = JSON.parse(localStorage.getItem(STARTED) ?? "{}");
      if (job == null) delete all[k];
      else all[k] = job;
      localStorage.setItem(STARTED, JSON.stringify(all));
    } catch {
      // No storage (a private window): the running job is still found.
    }
  },
};

/** @param {Scope} s */
const scopeKey = (s) =>
  s.all ? (s.only ? `only:${s.only.join(",")}` : "all") : `stack:${s.stack}`;

const now = () => Date.now() / 1000;

/** How long a finished update stays on its flow's page after a reload. */
const SHOW_DONE_S = 30 * 60;
/** How long a finished update can be rolled back (the backup of step 3 stays for 7 days). */
const UNDO_WINDOW_S = 7 * 24 * 3600;

/** @param {Job} j */
const finishedJob = (j) => j.state !== "queued" && j.state !== "running";

/**
 * @param {HTMLElement} root
 * @param {Scope} scope
 * @returns {() => void}
 */
export function mount(root, scope) {
  ensureStyle("/css/pages/update.css");
  const key = scopeKey(scope);
  // A Stacks batch (`only`) belongs to Stacks, every app to the Inbox.
  const fromStacks = scope.all && !!scope.only;
  const crumbs = fromStacks
    ? [{ label: "Stacks", href: "/stacks" }, { label: "Update apps" }]
    : scope.all
      ? [{ label: "Inbox", href: "/inbox" }, { label: "Update apps" }]
      : [
          { label: "Stacks", href: "/stacks" },
          {
            label: scope.stack,
            href: `/stacks/${encodeURIComponent(scope.stack)}`,
          },
          { label: "Update" },
        ];
  const head = pageHeader({
    title: scope.all ? "Update apps" : `Update ${scope.stack}`,
    desc: "Moving an app to a newer version, in six visible steps. Nothing runs until step 2's button; after that you may leave this page — the job keeps going and the bar shows it.",
    crumbs,
  });
  // kp's step list in its journey look: the steps reached are done, the one
  // the flow stands at is current.
  const steps = {
    el: buildSteplist(
      STEPS.map((title, i) => ({ id: String(i + 1), title })),
      { label: "Steps" },
    ),
    /** @param {number} n */
    set: (n) => {
      STEPS.forEach((_, i) =>
        setStep(steps.el, String(i + 1), {
          state: i + 1 < n ? "done" : i + 1 === n ? "current" : "pending",
        }),
      );
    },
  };
  steps.el.classList.add("kp-steplist--journey", "uf-steps");
  attachSteplists(steps.el);
  const back = fromStacks
    ? "/stacks"
    : scope.all
      ? "/inbox"
      : `/stacks/${encodeURIComponent(scope.stack)}`;

  // ---- 1 · See -------------------------------------------------------
  const see = section({
    title: "1 · Which apps have a newer version",
    desc: "Reading the fleet check, which compares each pinned image with its upstream releases…",
  });
  const seeDesc = /** @type {HTMLElement} */ (
    see.el.querySelector(".nx-card__head p")
  );
  // The list is one kp datatable with a selection bar over it; a major
  // version holds the next step off until its release notes were read (kp
  // acknowledge gate); the table loads, empties and fails the table's own way.
  const table = tableBlock({
    remember: "update-list",
    caption: "Apps with a newer version; tick the ones to update",
    captionHidden: true,
    search: null,
    nothing: "Everything is up to date.",
    select: { label: "Select every update", actions: [] },
    columns: [
      { label: "App", sort: "text" },
      { label: "Jump", sort: "number" },
      { label: "Waiting", sort: "number", cls: "kp-text-end" },
      { label: "Note", sort: null },
    ],
  });
  table.wrap.querySelector("[data-kp-datatable-actions]")?.remove();
  table.wrap.id = "uf-table";
  const summary = h("p", { class: "uf-summary", role: "status" });
  const gates = h("div", { class: "uf-gates", "aria-live": "polite" });
  const next = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--primary",
        id: "uf-see",
        "data-kp-selection-action": "see",
        title:
          "Show downtime, what restarts, who notices and the change to the files; nothing runs yet",
      },
      "See what this changes",
    ),
    SEE,
  );
  const bar = h(
    "div",
    {
      class: "kp-selection uf-bar",
      "data-kp-selection": "",
      "data-kp-selection-for": "uf-table",
      "data-kp-placement": "sticky-top",
      id: "uf-bar",
      "aria-label": "Apps to update",
    },
    h("div", { class: "kp-selection__actions" }, next),
  );
  // One major at a time is the default; several together, each read, is the
  // other option (kept in this browser).
  let majorsTogether = false;
  try {
    majorsTogether =
      localStorage.getItem("homelab.update.majors") === "together";
  } catch {
    // the choice lasts for this visit
  }
  const majorsSeg = segSwitch({
    label: "Major versions",
    value: majorsTogether ? "together" : "alone",
    items: [
      {
        value: "alone",
        label: "One major at a time",
        hint: "A major version is updated on its own, so its notes get a read of their own",
      },
      {
        value: "together",
        label: "Allowed together, each read",
        hint: "Several majors in one run, each with its own tick",
      },
    ],
    onChange: (v) => {
      majorsTogether = v === "together";
      try {
        localStorage.setItem("homelab.update.majors", v);
      } catch {
        // kept for this visit only
      }
      // The rule is re-read from the ticks as they stand.
      paintChosen();
      sel.refresh();
    },
    mark: (b, v) => void drivable(b, MAJORS, v),
  });
  const majorsRow = h(
    "div",
    { class: "uf-ctl" },
    h("b", { class: "uf-ctl__label" }, "Major versions"),
    majorsSeg.el,
  );
  const list = h(
    "div",
    { class: "uf-list" },
    summary,
    majorsRow,
    bar,
    table.wrap,
    gates,
  );
  see.body.append(list);
  const detachTables = attachDataTables(list);

  const detachMarks = attachIdmarks(list);
  const detachGates = attachAckGates(list);
  const detachDeltas = attachVersionDeltas(list);
  /** @type {Set<string>} */
  const acked = new Set();
  const rules = () => ({
    see: (/** @type {{key: string, label: string}[]} */ sel) => {
      const majors = sel.filter(
        (x) => items.find((i) => i.id === x.key)?.major,
      );
      if (!majorsTogether && majors.length && sel.length > 1)
        return `A major update needs its own read: update ${majors[0].label} on its own`;
      // The acknowledge gate and this bar both write the button's
      // aria-disabled; the bar is the one that redraws on every tick, so the
      // unread notes are a reason here too.
      const unread = majors.filter((x) => !acked.has(x.key));
      return unread.length
        ? `Confirm the release notes of ${unread.map((x) => x.label).join(", ")} first`
        : "";
    },
  });
  const sel = bindSelection(list, bar, rules, undefined, (c) =>
    drivable(c, UNTICK),
  );

  // ---- 2 · Impact ----------------------------------------------------
  const impact = section({
    title: "2 · What this changes",
    desc: "Read before you start. Everything below is worked out from the stack files and the fleet map — nothing to know by heart.",
  });
  const tiles = h("div", { class: "uf-impact" });
  const deps = h("div", {
    class: "kp-deplist",
    "data-kp-deplist": "",
    "data-kp-dep-order": "none",
    "data-kp-dep-direction": "from-reaches-to",
    "data-kp-dep-heading": "{n} reaches these apps",
    "data-kp-dep-empty": "No other stack reaches these apps.",
  });
  const detachDeps = attachDeplists(deps);
  deplist(deps)?.setState("loading");
  const depsCard = h(
    "section",
    { class: "uf-sub", "aria-labelledby": "uf-deps-h" },
    h(
      "div",
      { class: "nx-card__head" },
      h("h3", { id: "uf-deps-h" }, "Who notices"),
      h(
        "p",
        { class: "section-head__desc" },
        "Stacks that reach these apps (from the fleet map). They keep running; their requests fail for the downtime above.",
      ),
    ),
    deps,
  );
  const majorBox = h("div", { class: "uf-major" });
  const diff = h("div", { class: "uf-diff" });
  const diffStage = stage(diff);
  const diffFold = h(
    "details",
    { class: "uf-fold kp-accordion__item kp-accordion__item--bare" },
    drivable(
      h(
        "summary",
        {
          title:
            "Fold or unfold the diff of the stack files this update changes",
        },
        "The change to the stack files",
      ),
      DIFF,
    ),
    diffStage,
  );
  const blocked = h("div");
  const backBtn = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button",
        title: "Back to the list; nothing ran",
      },
      "← Back",
    ),
    BACK,
  );
  const go = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--primary",
        disabled: "",
        "data-kp-disabled-reason": "Choose what to update first",
        title:
          "Back up each stack, then change the files, deploy and verify; you can leave the page once it runs",
      },
      "Back up and update",
    ),
    GO,
  );
  impact.body.append(
    tiles,
    depsCard,
    majorBox,
    diffFold,
    blocked,
    h("div", { class: "uf-foot" }, backBtn, go),
  );
  const detachGoReason = attachDisabledReasons(impact.body);

  // ---- 3-5 · Running ---------------------------------------------------
  const runCard = section({
    title: "Running",
    desc: "You can leave this page: the job runs on the host and the bar shows it until it ends.",
    tools: [
      drivable(
        h(
          "a",
          {
            class: "kp-button kp-button--sm",
            href: back,
            title: "Go back; the update keeps running",
          },
          "Leave — keep it running",
        ),
        LEAVE,
      ),
    ],
  });
  // The run is a kp pipeline board (a lane per step, a card per app) or a
  // kp step list; the job reports its steps for the whole batch, so every
  // card stands in the lane of the step the job is at, and one that failed
  // stays in the lane it failed in.
  const board = h("section", {
    class: "kp-pipeline uf-board",
    "data-kp-pipeline": "",
    "data-kp-pipeline-share": "false",
    "aria-label": "Updates in progress",
  });
  const stepsBox = h("div", { class: "uf-steps", hidden: "" });
  /** @type {"board" | "steps"} */
  let layout = "board";
  try {
    if (localStorage.getItem("homelab.update.layout") === "steps")
      layout = "steps";
  } catch {
    // the choice lasts for this visit
  }
  const layoutSeg = segSwitch({
    label: "Layout",
    value: layout,
    items: [
      {
        value: "board",
        label: "A board with a lane per step",
        hint: "Each app is a card that moves through Notes, Back up, Update, Verify and Done",
      },
      {
        value: "steps",
        label: "A step list",
        hint: "The steps of the update with their times",
      },
    ],
    onChange: (v) => {
      layout = v === "steps" ? "steps" : "board";
      try {
        localStorage.setItem("homelab.update.layout", layout);
      } catch {
        // kept for this visit only
      }
      paintRun();
    },
    mark: (b, v) => void drivable(b, LAYOUT, v),
  });
  const runList = h(
    "div",
    { class: "uf-run" },
    h(
      "div",
      { class: "uf-ctl" },
      h("b", { class: "uf-ctl__label" }, "Layout"),
      layoutSeg.el,
    ),
    board,
    stepsBox,
  );
  const detachRun = [attachPipelines(runList), attachSteplists(runList)];
  // kp's log viewer, the same builder as the job panel's (feat-kpadopt-2).
  const jobLog = jobLogViewer(0, {
    whole: false,
    tools: "search,wrap,time,level,copy",
    label: "What the host did",
    height: "12rem",
    hint: "No lines yet. Lines appear here as the host works.",
  });
  jobLog.element.classList.add("uf-log");
  runCard.body.append(runList, jobLog.element);

  // ---- 6 · Done --------------------------------------------------------
  const doneCard = h("section", {
    class: "kp-card nx-card uf-done",
    "aria-labelledby": "uf-done-h",
  });

  const panels = h(
    "div",
    { class: "uf-panels" },
    see.el,
    impact.el,
    runCard.el,
    doneCard,
  );
  // Review item 7: the page at the demo's width.
  root.replaceChildren(
    h("div", { class: "uf-page" }, head.el, steps.el, panels),
  );
  const detachStages = attachCardStates(root);

  /** @type {Item[]} */
  let items = [];
  /** @type {Set<string>} */
  let chosen = new Set();
  let picked = false;
  let step = 1;
  /** @type {Map<string, Move>} */
  let moves = new Map();
  let plansReady = false;
  let plansBlocked = false;
  const abort = new AbortController();
  /** @type {(() => void)[]} */
  const offs = [];

  const show = () => {
    steps.set(step);
    see.el.hidden = step !== 1;
    impact.el.hidden = step !== 2;
    runCard.el.hidden = !(step >= 3 && step <= 5);
    doneCard.hidden = step !== 6;
  };
  const chosenItems = () => items.filter((i) => chosen.has(i.id));

  // ---- step 1 ----------------------------------------------------------
  /** @type {Map<string, string>} when each pin's newer version was released */
  let releasedOf = new Map();
  /** Whole days since a release date ("2026-09-30"), or null. @param {string | undefined} iso */
  const daysSince = (iso) => {
    const m = iso ? /^(\d{4})-(\d{2})-(\d{2})/.exec(iso) : null;
    return m
      ? Math.max(
          0,
          Math.floor(
            (now() - Date.UTC(+m[1], +m[2] - 1, +m[3]) / 1000) / 86400,
          ),
        )
      : null;
  };
  const paintList = () => {
    head.title.textContent = flowTitle(items, scope);
    const sum = summarizeDeltas(
      items
        .filter((i) => i.kind === "pin")
        .map((i) => ({ from: i.from, to: i.to })),
      {},
    );
    summary.textContent = items.length
      ? `${items.length} ${items.length === 1 ? "update" : "updates"} waiting: ${sum.text}`
      : "Everything is up to date.";
    if (!items.length) {
      table.tbody.replaceChildren();
      table.ready();
      paintChosen();
      return;
    }
    table.tbody.replaceChildren(
      ...items.map((i) => {
        const rel = i.kind === "release";
        const tick = selectCell(i.id, `Include ${i.stack}/${i.container}`);
        const box = /** @type {HTMLInputElement} */ (
          tick.querySelector("input")
        );
        drivable(box, PICK, i.id);
        if (rel) {
          box.disabled = true;
          box.setAttribute(
            "data-kp-disabled-reason",
            "Part of the homelab itself: it moves when the host is updated",
          );
        }
        const level =
          i.kind === "pin" ? versionLevel(i.from, i.to, {}) : "other";
        const url = i.notes;
        const jump =
          i.kind === "pin"
            ? h("span", {
                class: "kp-badge kp-badge--version",
                "data-kp-version-from": i.from,
                "data-kp-version-to": i.to,
                "data-kp-version-app": i.container,
                ...(url ? { "data-kp-version-href": url } : {}),
              })
            : h(
                "span",
                { class: "kp-badge" },
                rel ? "with a homelab release" : "moving tag",
              );
        const waited = daysSince(releasedOf.get(i.id));
        return h(
          "tr",
          {
            "data-kp-row-key": i.id,
            "data-kp-row-label": `${i.stack}/${i.container}`,
          },
          tick,
          h(
            "td",
            { "data-kp-sort-value": `${i.stack}/${i.container}` },
            h(
              "span",
              { class: "uf-app" },
              h("span", {
                class: "kp-idmark kp-idmark--sm",
                "data-kp-idmark": i.container,
                "aria-hidden": "true",
              }),
              h("b", null, i.container),
              h("span", { class: "kp-timestamp" }, i.stack),
            ),
          ),
          h(
            "td",
            {
              "data-kp-sort-value": String(VERSION_SORT[level]),
            },
            jump,
          ),
          h(
            "td",
            {
              class: "kp-text-end",
              "data-kp-sort-value": String(waited ?? -1),
            },
            waited == null ? "—" : `${waited} ${waited === 1 ? "day" : "days"}`,
          ),
          h(
            "td",
            null,
            i.kind === "pull"
              ? "A moving tag: it runs what the tag points at now; an app that is not healthy within 2 min is rolled back by itself."
              : rel
                ? "Part of the homelab itself: it moves when the host is updated (System › Host › Update the host…), not from here."
                : i.major
                  ? "Major: read the release notes first; a major release can change settings or move data."
                  : "Fixes and small features.",
          ),
        );
      }),
    );
    labelCells(table.tbody, () => ["", "App", "Jump", "Waiting", "Note"]);
    attachVersionDeltas(table.tbody);
    table.ready();
    attachDisabledReasons(table.tbody);
    for (const b of /** @type {NodeListOf<HTMLInputElement>} */ (
      table.tbody.querySelectorAll("input[data-kp-select-row]")
    )) {
      const want = chosen.has(b.value) && !b.disabled;
      if (b.checked !== want) {
        b.checked = want;
        b.dispatchEvent(new Event("change", { bubbles: true }));
      }
    }
    paintChosen();
  };
  bar.addEventListener("kp-selection-change", (e) => {
    const ks = new Set(/** @type {CustomEvent} */ (e).detail.keys);
    chosen = ks;
    syncGates();
    paintChosen();
  });
  bar.addEventListener("kp-selection-action", () => {
    if (!chosenItems().length) return;
    void toImpact().catch(() => {});
  });
  /**
   * ask-kp11be-6: a major version's release notes, fetched and cleaned by
   * the dashboard and drawn as plain text in kp's code block (there is no
   * markdown view in kp, gap line 113). The link above stays: the tick still
   * waits for it to be opened.
   * @param {import("../updateflow.js").Item} m
   */
  const noteBlock = (m) => {
    const el = h("figure", { class: "kp-code uf-notes", "data-kp-code": "" });
    const title = `Release notes of ${m.to}`;
    setCode(el, {
      title,
      language: "Markdown",
      state: "loading",
      loadingWords: `Reading the release notes of ${m.to}…`,
    });
    void (async () => {
      const r = await fetchJson(
        `/data/release-notes?${notesQuery(m.notes)}`,
        "the release notes",
        abort.signal,
      );
      if (abort.signal.aborted) return;
      if (!r.ok) {
        setCode(el, {
          title,
          state: "error",
          message: `${r.error.what}: ${r.error.why}`,
        });
        return;
      }
      const n = r.body;
      if (!n.body) {
        setCode(el, {
          title,
          state: "empty",
          message: `${n.name || m.to} has no release notes written. Open the release page from the link.`,
        });
        return;
      }
      setCode(el, {
        title: n.name ? `${title}: ${n.name}` : title,
        language: "Markdown",
        text: n.body,
        wrap: true,
        maxLines: 14,
        copy: true,
        open: "dialog",
        ...(n.truncated
          ? {
              truncated: {
                shown: String(n.body.length),
                total: String(n.total),
              },
            }
          : {}),
      });
    })().catch(() => {});
    return el;
  };
  /** One kp acknowledge gate per ticked major version, kept in step with the ticks. */
  const syncGates = () => {
    const majors = chosenItems().filter((i) => i.major);
    for (const g of /** @type {NodeListOf<HTMLElement>} */ (
      gates.querySelectorAll("[data-kp-ack]")
    ))
      if (!majors.some((m) => m.id === g.dataset.kpAckKey)) {
        acked.delete(g.dataset.kpAckKey ?? "");
        g.remove();
      }
    for (const m of majors) {
      if (gates.querySelector(`[data-kp-ack-key="${CSS.escape(m.id)}"]`))
        continue;
      gates.append(
        h(
          "div",
          {
            class: "kp-alert kp-alert--warning",
            "data-kp-ack": "",
            "data-kp-ack-for": "#uf-see",
            "data-kp-ack-key": m.id,
            "data-kp-ack-mode": "all",
            "data-kp-ack-label": `I read the release notes of ${m.to}`,
            "data-kp-ack-link": m.notes ?? "",
            // The tick waits for the notes link to be opened; a version with
            // no link can only wait a few seconds.
            ...(m.notes
              ? { "data-kp-ack-require-link": "" }
              : { "data-kp-ack-min-ms": "4000" }),
          },
          h(
            "div",
            { class: "kp-alert__body" },
            h(
              "strong",
              null,
              `${m.stack}/${m.container} ${m.from} → ${m.to} is a major version. `,
            ),
            "It can change settings or move data.",
          ),
          ...(notesQuery(m.notes) ? [noteBlock(m)] : []),
        ),
      );
    }
    attachCodes(gates);
    attachAckGates(gates);
    for (const box of /** @type {NodeListOf<HTMLInputElement>} */ (
      gates.querySelectorAll("[data-kp-ack] input[type=checkbox]")
    )) {
      const key =
        box.closest("[data-kp-ack]")?.getAttribute("data-kp-ack-key") ?? "";
      if (!box.dataset.drive) drivable(box, MAJOR_GATE, key);
    }
    for (const a of /** @type {NodeListOf<HTMLElement>} */ (
      gates.querySelectorAll("[data-kp-ack] .kp-ack__link")
    )) {
      const key =
        a.closest("[data-kp-ack]")?.getAttribute("data-kp-ack-key") ?? "";
      if (!a.dataset.drive) drivable(a, GATE_NOTES, key);
    }
  };
  gates.addEventListener("kp-ack-change", (e) => {
    const d = /** @type {CustomEvent} */ (e).detail;
    if (d.key) {
      if (d.checked) acked.add(d.key);
      else acked.delete(d.key);
      paintChosen();
    }
  });
  const paintChosen = () => {
    sel.refresh();
  };

  /** @param {any} body */
  const got = (body) => {
    feedUpdates(body);
    items = itemsFor(body, scope);
    releasedOf = new Map(
      (body?.images ?? [])
        .filter((/** @type {any} */ x) => x.key && x.released)
        .map((/** @type {any} */ x) => [
          `pin:${String(x.where_).split("/")[0]}:${x.key}`,
          String(x.released),
        ]),
    );
    if (!picked) {
      chosen = firstChosen(items, scope);
      // One major at a time: a list that opens with a major among others
      // starts with the others; the major is one tick away.
      if (!majorsTogether && chosen.size > 1)
        chosen = new Set(
          [...chosen].filter((id) => !items.find((i) => i.id === id)?.major),
        );
      picked = true;
    } else
      chosen = new Set(
        [...chosen].filter((id) => items.some((i) => i.id === id)),
      );
    const at = Number(body?.measured_at) || 0;
    seeDesc.textContent = `${at ? `Found by the fleet check at ${formatDateTime(at)}, which` : "The fleet check"} compares each pinned image with its upstream releases. Tick the apps to update, read the notes of a major version, then see what it changes.`;
    if (step === 1) paintList();
  };

  // ---- step 2 ----------------------------------------------------------
  /** Stops the undo window's clock when the done card is drawn again or left. */
  let stopUndoClock = () => {};
  const paintGo = () => {
    const c = chosenItems();
    const majors = c.filter((i) => i.major);
    const ackOk = majors.every((i) => acked.has(i.id));
    const pins = c.some((i) => i.kind === "pin");
    go.textContent =
      c.length > 1
        ? `Back up and update ${c.length} apps`
        : "Back up and update";
    const why = running()
      ? "An update is already running"
      : !ackOk
        ? "Confirm the release notes of each major update first"
        : pins && plansBlocked
          ? "A plan is blocked: the notice above says what stops it"
          : pins && !plansReady
            ? "The plan of each pinned update is still being worked out"
            : "";
    /** @type {HTMLButtonElement} */ (go).disabled = why !== "";
    if (why) go.setAttribute("data-kp-disabled-reason", why);
    else go.removeAttribute("data-kp-disabled-reason");
  };

  const toImpact = async () => {
    step = 2;
    show();
    const c = chosenItems();
    const net = safetyNet(c);
    const took = lastDeployS(c, act.jobs);
    const tile = (
      /** @type {string} */ label,
      /** @type {string} */ value,
      /** @type {string} */ ctx,
    ) =>
      h(
        "div",
        { class: "uf-tile" },
        h("b", null, label),
        h("strong", null, value),
        h("span", { class: "uf-hint" }, ctx),
      );
    const pins = c.filter((i) => i.kind === "pin");
    tiles.replaceChildren(
      tile(
        "Downtime",
        took != null
          ? `under ${humanDuration(took)} per app`
          : "a few seconds per app",
        took != null
          ? `while the new container starts (the last deploy took ${humanDuration(took)})`
          : "while the new container starts",
      ),
      tile("Restarts", restartWords(c), "nothing else in the stack restarts"),
      tile("Safety net", net.value, net.ctx),
      pins.length
        ? tile("Undo later", undoWords().value, undoWords().ctx)
        : tile(
            "Undo later",
            "Restore the backup",
            "the snapshot from step 3 holds the data as it was",
          ),
    );
    // A major version's notes were read in step 1 (its gate); here it is
    // named again with the way back.
    const majors = c.filter((i) => i.major);
    majorBox.replaceChildren(
      ...majors.map((i) =>
        h(
          "section",
          {
            class: "kp-alert kp-alert--warning uf-major__item",
            role: "note",
            "data-major": i.id,
          },
          h("span", { class: "uf-major__icon", "aria-hidden": "true" }, "!"),
          h(
            "p",
            { class: "uf-major__text" },
            h(
              "strong",
              null,
              `${i.stack}/${i.container} ${i.from} → ${i.to} is a major version`,
            ),
            h(
              "span",
              null,
              `Its release notes were read in step 1${acked.has(i.id) ? "" : " (not yet: go back and tick them)"}. A major release can change its settings or move its data in a way the old version cannot read back; restoring the backup from step 3 is then the way back.`,
              ...(i.notes
                ? [
                    " ",
                    drivable(
                      h(
                        "a",
                        {
                          href: i.notes,
                          target: "_blank",
                          rel: "noopener noreferrer",
                          title: `The ${i.to} release notes, in a new tab`,
                        },
                        "Read the release notes",
                      ),
                      MAJOR_NOTES,
                      i.id,
                    ),
                  ]
                : []),
            ),
          ),
        ),
      ),
    );
    paintGo();
    // Who notices, from the fleet map's dependencies.
    deplist(deps)?.setState("loading");
    void (async () => {
      const r = await fetchJson(
        "/data/dependencies",
        "who reaches these apps",
        abort.signal,
      );
      if (abort.signal.aborted) return;
      if (!r.ok) {
        deplist(deps)?.setState("error", {
          message: `Who reaches these apps did not read: ${r.error.why}`,
        });
        return;
      }
      const rows = whoNotices(c, r.body.dependencies ?? []);
      deplist(deps)?.set(
        rows.map((d) => ({ from: d.who, to: d.what, label: "reaches it" })),
      );
    })().catch(() => {});
    // The change to the files: each pin's newer reference (its registry's
    // digest), then each stack's plan with all its moves.
    plansReady = false;
    plansBlocked = false;
    moves = new Map();
    blocked.replaceChildren();
    diff.replaceChildren();
    setCardState(diffStage, "loading", {
      words: "Reading the change to the stack files",
    });
    paintGo();
    if (!pins.length) {
      diff.replaceChildren(
        h(
          "p",
          { class: "uf-hint" },
          "No file changes: apps on a moving tag keep their image line and pull what it points at now.",
        ),
      );
      setCardState(diffStage, "ready");
      plansReady = true;
      paintGo();
      return;
    }
    /** @type {Node[]} */
    const out = [];
    for (const s of stacksOf(pins)) {
      const mine = pins.filter((i) => i.stack === s);
      /** @type {Record<string, string>} */
      const images = {};
      for (const i of mine) {
        const q = new URLSearchParams({
          key: /** @type {string} */ (i.key),
          latest: i.to,
        });
        const t = await fetchJson(
          `/data/stacks/${encodeURIComponent(s)}/pin-target?${q}`,
          `the newer image of ${i.stack}/${i.container}`,
          abort.signal,
        );
        if (abort.signal.aborted) return;
        if (!t.ok) {
          blocked.append(refusalCallout(t.error));
          plansBlocked = true;
          continue;
        }
        moves.set(i.id, { stack: s, ...t.body });
        images[/** @type {string} */ (i.key)] = t.body.to;
      }
      if (!Object.keys(images).length) continue;
      const p = await send(
        "POST",
        `/data/stacks/${encodeURIComponent(s)}/plan`,
        { edit: { kind: "settings", images } },
        `the plan of ${s}`,
      );
      if (abort.signal.aborted) return;
      if (!p.ok) {
        blocked.append(refusalCallout(p.error));
        plansBlocked = true;
        continue;
      }
      const v = planView(p.body);
      if (v.blocked) {
        plansBlocked = true;
        blocked.append(
          h(
            "div",
            { class: "kp-alert kp-alert--destructive", role: "alert" },
            h("strong", null, `${s}: ${v.blockedWhy}`),
            ...(v.problems.length
              ? [h("ul", null, ...v.problems.map((x) => h("li", null, x)))]
              : []),
          ),
        );
      }
      out.push(...diffBlocks(v.files, { open: false }));
    }
    diff.replaceChildren(
      ...(out.length
        ? out
        : [h("p", { class: "uf-hint" }, "No file changes.")]),
    );
    setCardState(diffStage, "ready");
    plansReady = true;
    paintGo();
  };

  // ---- 3-6 · the job ---------------------------------------------------
  /** @type {number | null} the update job this page follows */
  let jobId = null;
  /** @type {{state: string, note: string} | null} */
  let logs = null;
  /** @type {number | null} the job whose logs were read */
  let logsFor = null;
  const jobOf = () =>
    jobId == null ? null : (act.jobs.find((j) => j.job === jobId) ?? null);
  const running = () => {
    const j = jobOf();
    return j != null && !finishedJob(j);
  };

  /**
   * The log comparison, read here once the job has deployed and ended:
   * errors in the first 2 minutes after the restart against the hour
   * before (it reads the past, so a reload reads the same).
   * @param {Job} j
   */
  const readLogs = async (j) => {
    const flow = j.flow;
    const at = Number(flow?.deploy_at) || 0;
    const healthOk = (flow?.rows ?? []).some(
      (/** @type {any} */ r) => r.id === "health" && r.state !== "wait",
    );
    if (!at || !healthOk) {
      logs = { state: "skip", note: "nothing was deployed" };
      paintRun();
      return;
    }
    logs = { state: "run", note: "reading the logs" };
    paintRun();
    const stacks = [
      ...new Set((flow.items ?? []).map((/** @type {any} */ i) => i.stack)),
    ];
    let after = 0;
    let before = 0;
    let bad = false;
    /** @type {string[]} */
    const unread = [];
    for (const s of stacks) {
      const since = Math.ceil(now() - at) + 3600;
      const r = await fetchJson(
        `/data/logs?${new URLSearchParams({ stack: s, since: String(since), limit: "5000" })}`,
        `the logs of ${s}`,
        abort.signal,
      );
      if (abort.signal.aborted) return;
      if (!r.ok) {
        unread.push(`${s}: ${r.error.why}`);
        continue;
      }
      const v = logVerdict(r.body.lines ?? [], at, Math.min(now(), at + 120));
      after += v.after;
      before += v.before;
      if (!v.ok) bad = true;
    }
    logs =
      unread.length === stacks.length
        ? {
            state: "skip",
            note: `the logs could not be read (${unread.join("; ")}); look at the stack's Logs tab`,
          }
        : {
            state: bad ? "bad" : "ok",
            note: `${after} error${after === 1 ? "" : "s"} since the restart, ${before} in the hour before${bad ? " — read them on the stack's Logs tab" : ""}`,
          };
    paintRun();
  };

  const paintRun = () => {
    const j = jobOf();
    if (!j) return;
    const flow = j.flow ?? null;
    const done = finishedJob(j);
    step = done ? 6 : Math.min(5, Math.max(3, Number(flow?.step) || 3));
    show();
    head.title.textContent = flowTitle(items, scope);
    const rows = flowRows(
      flow ?? {
        rows: [
          {
            id: "backup",
            step: 3,
            title: "Queued",
            desc: "the job starts as soon as the one before it ends",
            state: "wait",
          },
        ],
      },
      done ? logs : null,
    );
    layoutSeg.set(layout);
    board.hidden = layout !== "board";
    stepsBox.hidden = layout !== "steps";
    const failedRow = rows.find((r) => r.state === "bad");
    const lane = failedRow
      ? failedRow.step >= 5
        ? "verify"
        : failedRow.step === 4
          ? "update"
          : "backup"
      : done
        ? "done"
        : step >= 5
          ? "verify"
          : step === 4
            ? "update"
            : "backup";
    const jobItems = /** @type {any[]} */ (flow?.items ?? chosenItems());
    setPipeline(board, {
      lanes: [
        { key: "notes", label: "Notes", tone: 1 },
        { key: "backup", label: "Back up", tone: 2 },
        { key: "update", label: "Update", tone: 3 },
        { key: "verify", label: "Verify", tone: 4 },
        { key: "done", label: "Done", tone: 5 },
      ],
      cards: jobItems.map((i) => ({
        id: String(i.id ?? `${i.stack}/${i.container ?? i.key}`),
        lane,
        title: String(i.container ?? i.app ?? i.key ?? i.stack),
        sub: String(i.stack),
        meta: failedRow
          ? `failed: ${failedRow.note ?? failedRow.title}`
          : i.to
            ? `to ${i.to}`
            : "moving tag",
        mark: String(i.container ?? i.stack)
          .slice(0, 2)
          .toUpperCase(),
      })),
    });
    if (layout === "steps") {
      stepsBox.replaceChildren(
        buildSteplist(
          rows.map((r) => ({
            id: r.id,
            title: r.title,
            note: r.note ? `${r.desc} · ${r.note}` : r.desc,
            state:
              r.state === "ok"
                ? "done"
                : r.state === "run"
                  ? "running"
                  : r.state === "bad"
                    ? "failed"
                    : r.state === "skip"
                      ? "skipped"
                      : "pending",
            time: r.time,
          })),
          { label: "Steps of the update" },
        ),
      );
      attachSteplists(stepsBox);
    }
    jobLog.bind(j.job);
    if (done) {
      if (logsFor !== j.job) {
        logsFor = j.job;
        void readLogs(j).catch(() => {});
      }
      paintDone(j);
    }
    paintGo();
  };

  /** @param {Job} j */
  const paintDone = (j) => {
    const flow = j.flow ?? { items: [], rows: [], commits: [] };
    const ok = j.state === "done";
    const jobItems = /** @type {any[]} */ (flow.items ?? []);
    const jobStacks = [...new Set(jobItems.map((i) => i.stack))];
    const runMoves = movesOf(jobItems);
    const left = inboxNow().items.length;
    const rollbacks =
      ok && (flow.commits ?? []).length
        ? [...runMoves.entries()].map(([id, m]) => {
            const b = drivable(
              h(
                "button",
                {
                  type: "button",
                  class: "kp-button",
                  title:
                    "Put the previous version back the same way: backup, files, deploy",
                },
                runMoves.size > 1 ? `Roll back ${m.key}…` : "Roll back…",
              ),
              ROLL_BACK,
              id,
            );
            b.addEventListener("click", () => openPinRollback(m));
            return b;
          })
        : [];
    const backups = (flow.rows ?? []).find(
      (/** @type {any} */ r) => r.id === "backup",
    );
    const took =
      j.started_at != null && j.finished_at != null
        ? humanDuration(j.finished_at - j.started_at)
        : "—";
    const by =
      j.origin?.from === "claude"
        ? `Claude (Live view, ${j.origin.by})`
        : j.origin?.from === "schedule"
          ? "A schedule"
          : fromStacks
            ? "You, from Stacks"
            : scope.all
              ? "You, from the Inbox"
              : `You, from ${scope.stack}`;
    const words = jobItems
      .map((i) =>
        i.kind === "pin"
          ? `${i.stack}/${i.app ?? i.key} runs ${runMoves.get(`pin:${i.stack}:${i.key}`)?.to_version ?? "the new version"}`
          : `${i.stack}'s apps on a moving tag run what their tags point at now`,
      )
      .join("; ");
    // gap 124: a 7-day window reads in days (js/longcountdown.js).
    stopUndoClock();
    /** @type {(() => void)[]} */
    const clocks = [];
    /** @param {number} until @param {string} name */
    const undoClock = (until, name) => {
      const c = longCountdown(until, name);
      clocks.push(c.stop);
      return c.el;
    };
    doneCard.replaceChildren(
      h(
        "div",
        { class: "uf-done__hero" },
        doneMark(ok ? "ok" : "bad"),
        h(
          "h2",
          { id: "uf-done-h" },
          !ok
            ? flow.rolled_back
              ? "Not healthy — the earlier version is back"
              : "The update stopped"
            : logs?.state === "bad"
              ? "Updated — new errors in the logs"
              : "Updated and healthy",
        ),
        h(
          "p",
          { class: "uf-hint uf-done__text" },
          ok
            ? `${words}. It answered its health check, logs look like before, and the backup from step 3 stays for 7 days.`
            : (j.message ?? flow.failed ?? "The job ended without a word."),
        ),
      ),
      h(
        "dl",
        { class: "uf-kv" },
        h("dt", null, "Backup"),
        h(
          "dd",
          null,
          backups?.state === "ok"
            ? `${jobStacks.join(", ")} · a new snapshot, taken in ${backups.took_s != null ? humanDuration(backups.took_s) : "—"}`
            : "not taken",
        ),
        h("dt", null, "Commit"),
        h("dd", { class: "mono" }, (flow.commits ?? []).join(", ") || "none"),
        h("dt", null, "Took"),
        h("dd", null, took),
        h("dt", null, "Started by"),
        h("dd", null, by),
        ...(ok && runMoves.size
          ? [
              h("dt", null, "Undo"),
              h(
                "dd",
                null,
                "Roll back, 1 click, for 7 days: here, or from ",
                ...jobStacks.flatMap((s, i) => [
                  ...(i ? [", "] : []),
                  h(
                    "a",
                    { href: `/stacks/${encodeURIComponent(s)}/history` },
                    `${s}'s History`,
                  ),
                ]),
                ".",
              ),
              ...(j.finished_at != null
                ? [
                    h("dt", null, "Undo window"),
                    h(
                      "dd",
                      null,
                      undoClock(
                        (j.finished_at + UNDO_WINDOW_S) * 1000,
                        "The undo window closes in",
                      ),
                    ),
                  ]
                : []),
            ]
          : []),
      ),
      h(
        "div",
        { class: "uf-foot" },
        h("span", { class: "uf-foot__group" }, ...rollbacks),
        h(
          "span",
          { class: "uf-foot__group" },
          drivable(
            h(
              "a",
              {
                class: "kp-button",
                href: `/activity?view=running&job=${j.job}`,
                title: "This update's job, with every step, in Activity",
              },
              "See it in Activity",
            ),
            AFTER,
            "activity",
          ),
          drivable(
            h(
              "a",
              {
                class: "kp-button",
                href: `/stacks/${encodeURIComponent(jobStacks[0] ?? "")}`,
                title: "The stack's hub: its state, logs and history",
              },
              "Open the stack",
            ),
            AFTER,
            "stack",
          ),
          drivable(
            h(
              "button",
              {
                type: "button",
                class: "kp-button",
                title: "Show the list of apps with a newer version again",
              },
              "Update more",
            ),
            AFTER,
            "again",
          ),
          drivable(
            h(
              "a",
              {
                class: "kp-button kp-button--primary",
                href: "/inbox",
                title: "What else waits for you",
              },
              left ? `Back to the Inbox · ${left} left` : "Back to the Inbox",
            ),
            AFTER,
            "needs-you",
          ),
        ),
      ),
    );
    stopUndoClock = () => clocks.forEach((stop) => stop());
    doneCard
      .querySelector("[data-drive-row=again]")
      ?.addEventListener("click", () => {
        jobId = null;
        TAB_JOB.set(key, null);
        step = 1;
        show();
        paintList();
      });
  };

  // ---- wiring ----------------------------------------------------------
  backBtn.addEventListener("click", () => {
    step = 1;
    show();
    paintList();
  });
  go.addEventListener("click", () => {
    if (/** @type {HTMLButtonElement} */ (go).disabled) return;
    /** @type {HTMLButtonElement} */ (go).disabled = true;
    void (async () => {
      const r = await send(
        "POST",
        "/data/actions/_host/update-apps",
        updatesBody(chosenItems(), moves),
        "the update",
      );
      if (!r.ok) {
        blocked.replaceChildren(refusalCallout(r.error));
        paintGo();
        return;
      }
      jobId = /** @type {number} */ (r.body.job);
      TAB_JOB.set(key, jobId);
      logs = null;
      logsFor = null;
      step = 3;
      show();
      paintRun();
    })().catch(() => paintGo());
  });

  /**
   * An update job of this flow's scope the server runs now (any tab's), or
   * the one this browser started and has not left with "Update more" yet,
   * finished in the last half hour: a reload or a closed tab finds it.
   */
  const adopt = () => {
    if (jobId != null && jobOf()) return;
    const mine = TAB_JOB.get(key);
    const j = act.jobs.find(
      (x) =>
        x.action === "update-apps" &&
        scopeMatches(x.flow?.items ?? [], scope) &&
        (!finishedJob(x) ||
          (x.job === mine && now() - (x.finished_at ?? 0) < SHOW_DONE_S)),
    );
    if (!j) return;
    jobId = j.job;
    TAB_JOB.set(key, jobId);
    paintRun();
  };

  offs.push(
    onAct("log", () => paintRun()),
    onAct("jobs", () => {
      adopt();
      paintRun();
    }),
  );

  show();
  table.loading({ words: "Reading which apps have a newer version" });
  void (async () => {
    const r = await slowRead(
      "/data/stale-images",
      "the apps with a newer version",
      abort.signal,
      got,
    );
    if (abort.signal.aborted) return;
    if (r.ok) got(r.body);
    else if (!items.length) {
      // The fleet check did not answer: one stack can still pull what
      // its moving tags point at; the reason stays on top.
      if (scope.all) table.failed(r.error);
      else {
        items = itemsFor(null, scope);
        chosen = firstChosen(items, scope);
        picked = true;
        paintList();
        list.prepend(errorBox(r.error));
      }
    }
  })().catch(() => {});
  adopt();
  paintRun();

  return () => {
    abort.abort();
    for (const f of offs) f();
    see.stop();
    impact.stop();
    runCard.stop();
    detachTables();
    sel.stop();
    detachMarks();
    detachGoReason();
    stopUndoClock();
    detachGates();
    detachDeltas();
    detachDeps();
    jobLog.stop();
    detachStages();
    for (const f of detachRun) f();
  };
}
