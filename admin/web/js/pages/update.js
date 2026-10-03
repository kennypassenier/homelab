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
import { ensureStyle, errorBox, fetchJson, h, slowRead } from "../dom.js";
import { declare, drivable } from "../drivable.js";
import { diffBlocks } from "../editui.js";
import { formatDateTime, humanDuration } from "../format.js";
import { inboxNow } from "../inbox.js";
import { feedUpdates } from "../inboxsources.js";
import { openPinRollback } from "../pinupdate.js";
import { planView } from "../plan.js";
import { current } from "../store.js";
import { emptyState, pageHeader, section, skeletonLines } from "../ui.js";
import {
  firstChosen,
  flowRows,
  flowTitle,
  itemsFor,
  lastDeployS,
  logVerdict,
  movesOf,
  restartWords,
  safetyNet,
  scopeMatches,
  stacksOf,
  undoWords,
  updatesBody,
  whoNotices,
} from "../updateflow.js";
import { checkList, doneMark, stepper } from "./flowskit.js";

const PICK = declare({
  id: "update-pick",
  page: "update",
  opens: "view",
  row: "<pin:stack:app/service | pull:stack>",
  what: "include or leave out one row of the Update flow's list (a plain click)",
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
});
const MAJOR = declare({
  id: "update-major-read",
  page: "update",
  opens: "view",
  row: "<pin:stack:app/service>",
  what: "tick “I read the release notes” for a major version",
});
const GO = declare({
  id: "update-go",
  page: "update",
  opens: "run",
  what: "back up the chosen apps' stacks, update them and verify them",
});
const ROLL_BACK = declare({
  id: "update-roll-back",
  page: "update",
  opens: "dialog",
  row: "<pin:stack:app/service>",
  what: "the result's Roll back…: put the earlier version back the same way",
});

const NOTES = declare({
  id: "update-release-notes",
  page: "update",
  opens: "view",
  row: "<pin:stack:app/service>",
  what: "a row's release notes, in a new tab",
});
const MAJOR_NOTES = declare({
  id: "update-major-notes",
  page: "update",
  opens: "view",
  row: "<pin:stack:app/service>",
  what: "a major version's release notes, in a new tab",
});
const DIFF = declare({
  id: "update-diff",
  page: "update",
  opens: "view",
  what: "fold or unfold the change to the stack files",
});
const LEAVE = declare({
  id: "update-leave",
  page: "update",
  opens: "view",
  what: "leave the running update (the job keeps going on the server)",
});
const AFTER = declare({
  id: "update-after",
  page: "update",
  opens: "view",
  row: "activity|stack|inbox|back|again",
  what: "the result's links: Activity, the stack, back to the Inbox, or the list again; or Back from an empty list",
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
const scopeKey = (s) => (s.all ? "all" : `stack:${s.stack}`);

const now = () => Date.now() / 1000;

/** How long a finished update stays on its flow's page after a reload. */
const SHOW_DONE_S = 30 * 60;

/** @param {Job} j */
const finishedJob = (j) => j.state !== "queued" && j.state !== "running";

/**
 * @param {HTMLElement} root
 * @param {Scope} scope
 * @returns {() => void}
 */
export function mount(root, scope) {
  ensureStyle("/css/pages/flows.css");
  ensureStyle("/css/pages/update.css");
  const key = scopeKey(scope);
  const crumbs = scope.all
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
  const steps = stepper(STEPS.map((s) => s));
  const back = scope.all
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
  const list = h("div", {
    class: "uf-list",
    role: "group",
    "aria-label": "Apps to update",
  });
  const chosenN = h("span", { class: "uf-hint", "aria-live": "polite" });
  const next = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--primary",
        title:
          "Show downtime, what restarts, who notices and the change to the files; nothing runs yet",
        disabled: "",
      },
      "See what this changes →",
    ),
    SEE,
  );
  see.body.append(list, h("div", { class: "uf-foot" }, chosenN, next));

  // ---- 2 · Impact ----------------------------------------------------
  const impact = section({
    title: "2 · What this changes",
    desc: "Read before you start. Everything below is worked out from the stack files and the fleet map — nothing to know by heart.",
  });
  const tiles = h("div", { class: "uf-impact" });
  const deps = h(
    "div",
    { class: "uf-deps" },
    skeletonLines(2, "Reading who reaches these apps"),
  );
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
  const diffFold = h(
    "details",
    { class: "uf-fold" },
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
    diff,
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
  const runList = h("div");
  const log = h("pre", {
    class: "uf-log",
    "aria-live": "polite",
    "aria-label": "What the host did",
  });
  runCard.body.append(runList, log);

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

  /** @type {Item[]} */
  let items = [];
  /** @type {Set<string>} */
  let chosen = new Set();
  let picked = false;
  let step = 1;
  /** @type {Set<string>} */
  const acked = new Set();
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
  const paintList = () => {
    head.title.textContent = flowTitle(items, scope);
    if (!items.length) {
      list.replaceChildren(
        emptyState({
          title: "Nothing has a newer version",
          text: "The fleet check found no pinned app with a newer release. When one comes out it shows here and in the Inbox.",
          action: drivable(
            h(
              "a",
              {
                class: "kp-button",
                href: back,
                title: "Nothing to update: go back",
              },
              scope.all ? "Back to the Inbox" : "Back to the stack",
            ),
            AFTER,
            "back",
          ),
        }),
      );
    } else
      list.replaceChildren(
        ...items.map((i) => {
          const box = drivable(
            h("input", {
              type: "checkbox",
              "aria-label": `Include ${i.stack}/${i.container}`,
            }),
            PICK,
            i.id,
          );
          /** @type {HTMLInputElement} */ (box).checked = chosen.has(i.id);
          box.addEventListener("change", () => {
            if (/** @type {HTMLInputElement} */ (box).checked) chosen.add(i.id);
            else chosen.delete(i.id);
            paintChosen();
          });
          const kind =
            i.kind === "pull"
              ? h("span", { class: "kp-badge" }, "moving tag")
              : i.major
                ? h(
                    "span",
                    { class: "kp-badge kp-badge--warning" },
                    "major version",
                  )
                : h("span", { class: "kp-badge" }, "minor version");
          const what =
            i.kind === "pull"
              ? "apps that follow a tag such as :latest get whatever it points at now; one that is not healthy within 2 min is rolled back by itself"
              : i.major
                ? "may change settings or data — read the notes"
                : "fixes and small features";
          return h(
            "label",
            { class: "uf-item", "data-item": i.id },
            box,
            h("strong", null, `${i.stack} / ${i.container}`),
            h(
              "span",
              { class: "uf-ver" },
              ...(i.kind === "pin"
                ? [h("s", null, i.from), ` → ${i.to}`]
                : ["newest"]),
            ),
            h(
              "small",
              null,
              kind,
              ` ${what}`,
              ...(i.notes
                ? [
                    " · ",
                    drivable(
                      h(
                        "a",
                        {
                          href: i.notes,
                          target: "_blank",
                          rel: "noopener noreferrer",
                          title: `The ${i.to} release notes, in a new tab`,
                        },
                        "release notes",
                      ),
                      NOTES,
                      i.id,
                    ),
                  ]
                : []),
            ),
          );
        }),
      );
    paintChosen();
  };
  const paintChosen = () => {
    const n = chosenItems().length;
    chosenN.textContent = `${n} of ${items.length} chosen · click a row to include or leave it out`;
    /** @type {HTMLButtonElement} */ (next).disabled = n === 0;
  };

  /** @param {any} body */
  const got = (body) => {
    feedUpdates(body);
    items = itemsFor(body, scope);
    if (!picked) {
      chosen = firstChosen(items, scope);
      picked = true;
    } else
      chosen = new Set(
        [...chosen].filter((id) => items.some((i) => i.id === id)),
      );
    const at = Number(body?.measured_at) || 0;
    seeDesc.textContent = `${at ? `Found by the fleet check at ${formatDateTime(at)}, which` : "The fleet check"} compares each pinned image with its upstream releases. Click a row to include or leave it out.`;
    if (step === 1) paintList();
  };

  // ---- step 2 ----------------------------------------------------------
  const paintGo = () => {
    const c = chosenItems();
    const majors = c.filter((i) => i.major);
    const ackOk = majors.every((i) => acked.has(i.id));
    const pins = c.some((i) => i.kind === "pin");
    go.textContent =
      c.length > 1
        ? `Back up and update ${c.length} apps`
        : "Back up and update";
    /** @type {HTMLButtonElement} */ (go).disabled =
      !ackOk || (pins && (!plansReady || plansBlocked)) || running();
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
    // A major version needs the release notes read (fix-231's rule).
    const majors = c.filter((i) => i.major);
    majorBox.replaceChildren(
      ...majors.map((i) => {
        const tick = drivable(
          h("input", {
            type: "checkbox",
            id: `uf-ack-${i.id.replace(/[^a-z0-9]/gi, "-")}`,
          }),
          MAJOR,
          i.id,
        );
        /** @type {HTMLInputElement} */ (tick).checked = acked.has(i.id);
        tick.addEventListener("change", () => {
          if (/** @type {HTMLInputElement} */ (tick).checked) acked.add(i.id);
          else acked.delete(i.id);
          paintGo();
        });
        return h(
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
              "A major release can change its settings or move its data in a way the old version cannot read back. Restoring the backup from step 3 is then the way back.",
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
          h(
            "label",
            { class: "uf-major__ack", for: tick.id },
            tick,
            " I read the release notes",
          ),
        );
      }),
    );
    paintGo();
    // Who notices, from the fleet map's dependencies.
    void (async () => {
      const r = await fetchJson(
        "/data/dependencies",
        "who reaches these apps",
        abort.signal,
      );
      if (abort.signal.aborted) return;
      if (!r.ok) {
        deps.replaceChildren(errorBox(r.error));
        return;
      }
      const rows = whoNotices(c, r.body.dependencies ?? []);
      deps.replaceChildren(
        ...(rows.length
          ? rows.map((d) =>
              h(
                "div",
                { class: "uf-dep" },
                h("span", { class: "kp-badge" }, `${d.who} (reaches it)`),
                h(
                  "span",
                  { class: "uf-dep__arrow", "aria-hidden": "true" },
                  "→",
                ),
                h("strong", null, d.what),
              ),
            )
          : [
              h(
                "span",
                { class: "uf-hint" },
                "No other stack reaches these apps.",
              ),
            ]),
      );
    })().catch(() => {});
    // The change to the files: each pin's newer reference (its registry's
    // digest), then each stack's plan with all its moves.
    plansReady = false;
    plansBlocked = false;
    moves = new Map();
    blocked.replaceChildren();
    diff.replaceChildren(
      skeletonLines(3, "Reading the change to the stack files"),
    );
    paintGo();
    if (!pins.length) {
      diff.replaceChildren(
        h(
          "p",
          { class: "uf-hint" },
          "No file changes: apps on a moving tag keep their image line and pull what it points at now.",
        ),
      );
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
    runList.replaceChildren(
      checkList(
        flowRows(
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
        ),
      ),
    );
    log.textContent = (act.logs.get(j.job) ?? [])
      .slice(-14)
      .map(
        (l) => `${new Date(l.ts * 1000).toTimeString().slice(0, 8)}  ${l.msg}`,
      )
      .join("\n");
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
            "inbox",
          ),
        ),
      ),
    );
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
  next.addEventListener("click", () => {
    if (!chosenItems().length) return;
    void toImpact().catch(() => {});
  });
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
  list.replaceChildren(
    skeletonLines(3, "Reading which apps have a newer version"),
  );
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
      const err = errorBox(r.error);
      if (scope.all) list.replaceChildren(err);
      else {
        items = itemsFor(null, scope);
        chosen = firstChosen(items, scope);
        picked = true;
        paintList();
        list.prepend(err);
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
  };
}
