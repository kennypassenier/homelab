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
// version…") and the Map's stale-image rows. It lives at the Inbox's
// `?update=all` / `?update=<stack>[&app=<key>]`.
//
// The steps run on the existing machinery: the stack's Backup action, the
// stack editor's commit of `StackEdit::Settings { images }` with its
// deploy (fix-231's pin update), and for apps on a moving tag the stack's
// Update action (pull and recreate, rolled back when unhealthy). The run
// is held at module level, so leaving the page (inside this tab) keeps it
// going and coming back shows where it is.

import { act, onAct, send } from "../act.js";
import { refusalCallout } from "../actui.js";
import { errorBox, fetchJson, h, slowRead } from "../dom.js";
import { declare, drivable } from "../drivable.js";
import { diffBlocks } from "../editui.js";
import { formatDateTime, humanDuration } from "../format.js";
import { inboxNow } from "../inbox.js";
import { feedUpdates } from "../inboxsources.js";
import { jobEnd, openPinRollback } from "../pinupdate.js";
import { planView } from "../plan.js";
import { current } from "../store.js";
import { emptyState, pageHeader, section, skeletonLines } from "../ui.js";
import {
  commitSubject,
  doneWords,
  firstChosen,
  flowTitle,
  itemsFor,
  lastDeployS,
  logVerdict,
  restartWords,
  runRows,
  safetyNet,
  stacksOf,
  versionWords,
  whoNotices,
} from "../updateflow.js";
import { checkList, doneMark, ensureStyle, stepper } from "./flowskit.js";

const PICK = declare({
  id: "update-pick",
  page: "inbox",
  opens: "view",
  row: "<pin:stack:app/service | pull:stack>",
  what: "include or leave out one row of the Update flow's list (a plain click)",
});
const SEE = declare({
  id: "update-see-impact",
  page: "inbox",
  opens: "view",
  what: "the Update flow's step 1 → 2: see what the update changes",
});
const BACK = declare({
  id: "update-back",
  page: "inbox",
  opens: "view",
  what: "the Update flow's step 2 → 1: back to the list",
});
const MAJOR = declare({
  id: "update-major-read",
  page: "inbox",
  opens: "view",
  row: "<pin:stack:app/service>",
  what: "tick “I read the release notes” for a major version",
});
const GO = declare({
  id: "update-go",
  page: "inbox",
  opens: "run",
  what: "back up the chosen apps' stacks, update them and verify them",
});
const ROLL_BACK = declare({
  id: "update-roll-back",
  page: "inbox",
  opens: "dialog",
  row: "<pin:stack:app/service>",
  what: "the result's Roll back…: put the earlier version back the same way",
});

/** The six steps (flows/update.html). */
const STEPS = ["See", "Impact", "Back up", "Update", "Verify", "Done"];

/**
 * @typedef {import("../updateflow.js").Item} Item
 * @typedef {import("../updateflow.js").Scope} Scope
 * @typedef {import("../pinupdate.js").Move} Move
 * @typedef {import("../updateflow.js").RunRow & {
 *   state: "wait" | "run" | "ok" | "bad" | "skip", t0?: number,
 *   time?: string, note?: string}} LiveRow
 * @typedef {{key: string, scope: Scope, title: string, chosen: Item[],
 *   moves: Map<string, Move>, rows: LiveRow[], step: number,
 *   started: number, ended: number | null, failed: string | null,
 *   logsBad: boolean, jobs: number[], notes: {ts: number, msg: string}[],
 *   commits: string[], deployFrom: number, listeners: Set<() => void>}} Run
 */

/** @type {Run | null} the update running (or last run) in this tab */
let RUN = null;

/** @param {Scope} s */
const scopeKey = (s) => (s.all ? "all" : `stack:${s.stack}`);

const now = () => Date.now() / 1000;

/** @param {Run} run */
const changed = (run) => run.listeners.forEach((f) => f());

/**
 * @param {Run} run
 * @param {LiveRow["id"]} id
 * @param {LiveRow["state"]} state
 * @param {string} [note]
 */
function setRow(run, id, state, note) {
  const r = run.rows.find((x) => x.id === id);
  if (!r) return;
  if (state === "run") r.t0 = now();
  if ((state === "ok" || state === "bad") && r.t0 != null)
    r.time = humanDuration(now() - r.t0);
  r.state = state;
  if (note != null) r.note = note;
  run.step = Math.max(run.step, r.step);
  changed(run);
}

/** @param {Run} run @param {string} msg */
function note(run, msg) {
  run.notes.push({ ts: now(), msg });
  changed(run);
}

/**
 * Stop the run at a failed step: the rest is skipped, step 6 says why.
 * @param {Run} run
 * @param {LiveRow["id"]} id
 * @param {string} words
 */
function fail(run, id, words) {
  setRow(run, id, "bad", words);
  for (const r of run.rows) if (r.state === "wait") r.state = "skip";
  run.failed = words;
  run.ended = now();
  run.step = 6;
  changed(run);
}

/**
 * Follow one job to its end; its log lines show in the run's log.
 * @param {Run} run
 * @param {number} job
 */
async function follow(run, job) {
  run.jobs.push(job);
  changed(run);
  return jobEnd(job);
}

/** @param {number} ms */
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/**
 * Steps 3-5, in order; any failure stops the run where it is.
 * @param {Run} run
 */
async function runAll(run) {
  const stacks = stacksOf(run.chosen);
  const pins = run.chosen.filter((i) => i.kind === "pin");
  const pulls = run.chosen.filter((i) => i.kind === "pull");

  // 3 · Back up every stack the update touches.
  setRow(run, "backup", "run");
  for (const s of stacks) {
    const b = await send(
      "POST",
      `/data/actions/${encodeURIComponent(s)}/backup`,
      {},
      `the backup of ${s}`,
    );
    if (!b.ok) {
      fail(run, "backup", `The backup of ${s} was refused: ${b.error.why}.`);
      return;
    }
    const end = await follow(run, /** @type {number} */ (b.body.job));
    if (end !== "done") {
      fail(
        run,
        "backup",
        `The backup of ${s} ended ${end}, so nothing was changed.`,
      );
      return;
    }
  }
  setRow(run, "backup", "ok", `${stacks.length} backed up`);

  // 4 · Change the files (pinned images) and deploy; pull the rest.
  /** @type {{stack: string, job: number}[]} */
  const deploys = [];
  if (pins.length) {
    setRow(run, "commit", "run");
    for (const s of stacksOf(pins)) {
      const mine = pins.filter((i) => i.stack === s);
      /** @type {Record<string, string>} */
      const images = {};
      for (const i of mine) {
        const m = run.moves.get(i.id);
        if (m) images[/** @type {string} */ (i.key)] = m.to;
      }
      const c = await send(
        "POST",
        `/data/stacks/${encodeURIComponent(s)}/commit`,
        {
          edit: { kind: "settings", images },
          subject: commitSubject(mine),
          follow: "deploy",
        },
        `the commit of ${s}`,
      );
      if (!c.ok) {
        fail(run, "commit", `The commit of ${s} was refused: ${c.error.why}.`);
        return;
      }
      const sha = String(c.body.committed?.commit ?? "").slice(0, 7);
      if (sha) {
        run.commits.push(sha);
        note(run, `commit ${sha} ${commitSubject(mine)}`);
      }
      const f = c.body.follow;
      if (!f || f.refused || f.job == null) {
        fail(
          run,
          "commit",
          `The commit of ${s} queued no deploy: ${f?.refused?.why ?? "the host gave no job"}. Deploy ${s} from its page.`,
        );
        return;
      }
      deploys.push({ stack: s, job: f.job });
    }
    setRow(run, "commit", "ok", run.commits.join(", "));
  }
  setRow(run, "deploy", "run");
  run.deployFrom = now();
  for (const p of pulls) {
    const u = await send(
      "POST",
      `/data/actions/${encodeURIComponent(p.stack)}/update`,
      {},
      `the update of ${p.stack}`,
    );
    if (!u.ok) {
      fail(
        run,
        "deploy",
        `The update of ${p.stack} was refused: ${u.error.why}.`,
      );
      return;
    }
    deploys.push({ stack: p.stack, job: u.body.job });
  }
  for (const d of deploys) {
    const end = await follow(run, d.job);
    if (end !== "done") {
      fail(
        run,
        "deploy",
        `The deploy of ${d.stack} ended ${end}. ${pins.length ? "Roll back… puts the earlier version back." : "The Update action rolls back an app that is not healthy."}`,
      );
      return;
    }
  }
  setRow(run, "deploy", "ok");

  // 5 · Verify: the containers run (the deploy's health check answered),
  // and the logs since the restart look like the hour before.
  setRow(run, "health", "run");
  const fleet = current().fleet?.stacks ?? [];
  const words = stacks
    .map((s) => fleet.find((x) => x.name === s))
    .filter((s) => s != null)
    .map((s) => `${s.name}: ${s.apps_running} of ${s.apps_total} running`);
  setRow(
    run,
    "health",
    "ok",
    words.length ? words.join(" · ") : "the deploy's verify step passed",
  );
  setRow(run, "logs", "run");
  await sleep(8000);
  let after = 0;
  let before = 0;
  let bad = false;
  /** @type {string[]} */
  const unread = [];
  for (const s of stacks) {
    const since = Math.ceil(now() - run.deployFrom) + 3600;
    const r = await fetchJson(
      `/data/logs?${new URLSearchParams({ stack: s, since: String(since), limit: "5000" })}`,
      `the logs of ${s}`,
    );
    if (!r.ok) {
      unread.push(`${s}: ${r.error.why}`);
      continue;
    }
    const v = logVerdict(r.body.lines ?? [], run.deployFrom, now());
    after += v.after;
    before += v.before;
    if (!v.ok) bad = true;
  }
  if (unread.length === stacks.length)
    setRow(
      run,
      "logs",
      "skip",
      `the logs could not be read (${unread.join("; ")}); look at the stack's Logs tab`,
    );
  else {
    run.logsBad = bad;
    setRow(
      run,
      "logs",
      bad ? "bad" : "ok",
      `${after} error${after === 1 ? "" : "s"} since the restart, ${before} in the hour before${bad ? " — read them on the stack's Logs tab" : ""}`,
    );
  }
  run.ended = now();
  run.step = 6;
  changed(run);
}

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
    desc: "Moving an app to a newer version, in six visible steps. Nothing runs until step 2's button; after that you may leave this page — the job keeps going while this tab stays open, and the bar shows it.",
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
    h("summary", null, "The change to the stack files"),
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
    desc: "You can leave this page: the steps keep going while this dashboard tab stays open, and the bar shows the running job.",
    tools: [
      h(
        "a",
        {
          class: "kp-button kp-button--sm",
          href: back,
          title: "Go back; the update keeps running",
        },
        "Leave — keep it running",
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
  root.replaceChildren(head.el, steps.el, panels);

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
          action: h(
            "a",
            { class: "kp-button", href: back },
            scope.all ? "Back to the Inbox" : "Back to the stack",
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
      !ackOk ||
      (pins && (!plansReady || plansBlocked)) ||
      (RUN != null && RUN.ended == null);
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
        ? tile(
            "Undo later",
            "Roll back, 1 click",
            "from the result below: the same backup → files → deploy",
          )
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
                    h(
                      "a",
                      {
                        href: i.notes,
                        target: "_blank",
                        rel: "noopener noreferrer",
                      },
                      "Read the release notes",
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
      out.push(...diffBlocks(v.files));
    }
    diff.replaceChildren(
      ...(out.length
        ? out
        : [h("p", { class: "uf-hint" }, "No file changes.")]),
    );
    plansReady = true;
    paintGo();
  };

  // ---- 3-6 · the run ---------------------------------------------------
  const paintRun = () => {
    const run = RUN;
    if (!run || run.key !== key) return;
    step = run.step;
    show();
    head.title.textContent = run.title;
    runList.replaceChildren(checkList(run.rows));
    const lines = [
      ...run.jobs.flatMap((j) =>
        (act.logs.get(j) ?? []).map((l) => ({ ts: l.ts, msg: l.msg })),
      ),
      ...run.notes,
    ].sort((a, b) => a.ts - b.ts);
    log.textContent = lines
      .slice(-14)
      .map(
        (l) => `${new Date(l.ts * 1000).toTimeString().slice(0, 8)}  ${l.msg}`,
      )
      .join("\n");
    if (run.step === 6) paintDone(run);
  };

  /** @param {Run} run */
  const paintDone = (run) => {
    const ok = !run.failed;
    const pins = run.chosen.filter((i) => i.kind === "pin");
    const left = inboxNow().items.length;
    const rollbacks = run.commits.length
      ? pins
          .filter((i) => run.moves.has(i.id))
          .map((i) => {
            const b = drivable(
              h(
                "button",
                {
                  type: "button",
                  class: "kp-button",
                  title:
                    "Put the previous version back the same way: backup, files, deploy",
                },
                pins.length > 1 ? `Roll back ${i.container}…` : "Roll back…",
              ),
              ROLL_BACK,
              i.id,
            );
            b.addEventListener("click", () =>
              openPinRollback(/** @type {Move} */ (run.moves.get(i.id))),
            );
            return b;
          })
      : [];
    const backups = run.rows.find((r) => r.id === "backup");
    doneCard.replaceChildren(
      h(
        "div",
        { class: "uf-done__hero" },
        doneMark(ok ? "ok" : "bad"),
        h(
          "h2",
          { id: "uf-done-h" },
          !ok
            ? "The update stopped"
            : run.logsBad
              ? "Updated — new errors in the logs"
              : "Updated and healthy",
        ),
        h(
          "p",
          { class: "uf-hint uf-done__text" },
          doneWords(run.chosen, { failed: run.failed }),
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
            ? `${stacksOf(run.chosen).join(", ")} · a new snapshot, taken in ${backups.time ?? "—"}`
            : "not taken",
        ),
        h("dt", null, "Commit"),
        h("dd", { class: "mono" }, run.commits.join(", ") || "none"),
        h("dt", null, "Took"),
        h("dd", null, humanDuration((run.ended ?? now()) - run.started)),
        h("dt", null, "Started by"),
        h(
          "dd",
          null,
          run.scope.all
            ? "You, from the Inbox"
            : `You, from ${run.scope.stack}`,
        ),
      ),
      h(
        "div",
        { class: "uf-foot" },
        h("span", { class: "uf-foot__group" }, ...rollbacks),
        h(
          "span",
          { class: "uf-foot__group" },
          h(
            "a",
            {
              class: "kp-button",
              href: "/activity",
              title: "Every step of this update in the history",
            },
            "See it in Activity",
          ),
          h(
            "a",
            {
              class: "kp-button",
              href: `/stacks/${encodeURIComponent(stacksOf(run.chosen)[0] ?? "")}`,
              title: "The stack's hub: its state, logs and history",
            },
            "Open the stack",
          ),
          h(
            "a",
            {
              class: "kp-button kp-button--primary",
              href: "/inbox",
              title: "What else waits for you",
            },
            left ? `Back to the Inbox · ${left} left` : "Back to the Inbox",
          ),
        ),
      ),
    );
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
    const c = chosenItems();
    /** @type {Run} */
    const run = {
      key,
      scope,
      title: flowTitle(items, scope),
      chosen: c,
      moves,
      rows: runRows(c).map((r) => ({ ...r, state: "wait" })),
      step: 3,
      started: now(),
      ended: null,
      failed: null,
      logsBad: false,
      jobs: [],
      notes: [],
      commits: [],
      deployFrom: now(),
      listeners: new Set(),
    };
    RUN = run;
    run.listeners.add(paintRun);
    paintRun();
    void runAll(run).catch((e) =>
      fail(run, "deploy", `The update stopped: ${String(e)}`),
    );
  });

  offs.push(
    onAct("log", () => paintRun()),
    onAct("jobs", () => paintRun()),
  );

  if (RUN && RUN.key === key) {
    // Back on a running (or finished) update: show where it is.
    RUN.listeners.add(paintRun);
    paintRun();
  } else {
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
  }

  return () => {
    abort.abort();
    for (const f of offs) f();
    RUN?.listeners.delete(paintRun);
    see.stop();
    impact.stop();
    runCard.stop();
  };
}
