// fix-231 (Kenny, 2026-10-02, Dutch: "ik zie die tabel wel, maar er zijn
// geen actions aan verbonden?"): the Fleet view's stale-image row's
// Update. Moving a pinned app is an edit of its `image:` line followed by a
// deploy of its stack (docs/deployment/UPDATE_POLICY.md); this dialog runs
// that through the machinery that already exists, in order:
//
// 1. the stack's Backup action (the act queue's own job, followed in the
//    same job panel every action dialog shows) — if it does not end done,
//    nothing else happens;
// 2. the stack editor's own commit of `StackEdit::Settings { images }`
//    (the new tag and the digest its registry gave, from
//    `/data/stacks/{stack}/pin-target`), pushed;
// 3. that commit's deploy, queued by the commit itself (`follow: deploy`).
//
// A MAJOR version jump (the first number differs) is marked and needs an
// explicit "I read the release notes for this major version" before
// Confirm. Afterwards Roll back puts the old reference back the same way.
//
// redesign-flows-1 (3.71.0): the row's Update opens the one Update flow
// (pages/update.js) with that app picked; this dialog stays as its Roll
// back… (`openPinRollback`).

import { act, onAct, send } from "./act.js";
import { badge, openDialog, refusalCallout } from "./actui.js";
import { h } from "./dom.js";
import { diffBlocks } from "./editui.js";
import { mountJobPanel } from "./jobpanel.js";
import { planView } from "./plan.js";
import { updateHref } from "./updateflow.js";
import { dialogControl } from "./drivable.js";

/**
 * One stale-image row, as the Fleet view has it.
 * @typedef {{stack: string, container: string, key: string,
 *   pinned: string, latest: string, upstream: string}} StaleRow
 * What the dialog moves: from one `image:` line to another.
 * @typedef {{stack: string, key: string, file: string,
 *   from: string, to: string, from_version: string, to_version: string}} Move
 */

const TICK_LABEL = "I read the release notes for this major version";

/**
 * A job's end, once it has one: "done", "failed", "deferred", "refused"
 * or "unknown".
 * @param {number} job
 * @returns {Promise<string>}
 */
export function jobEnd(job) {
  return new Promise((resolve) => {
    /** @type {() => void} */
    let stop = () => {};
    const look = () => {
      const j = act.jobs.find((x) => x.job === job);
      if (j && j.state !== "queued" && j.state !== "running") {
        stop();
        resolve(j.state);
      }
    };
    stop = onAct("jobs", look);
    look();
  });
}

/**
 * redesign-flows-1 (3.71.0, decision "one Update flow"): open the Update
 * flow — the Inbox page's `?update=` view — from anywhere, through the
 * app's own router (a new address plus the popstate it listens to).
 * @param {string | null} [stack] null: every app with a newer version
 * @param {string | null} [app] the stale row's `<app>/<service>` key
 */
export function openUpdateFlow(stack = null, app = null) {
  history.pushState(null, "", updateHref(stack, app));
  dispatchEvent(new PopStateEvent("popstate"));
}

/**
 * The Update of one stale-image row (the Map's table): since 3.71.0 the
 * one Update flow, with that app picked.
 * @param {StaleRow} row
 */
export function openPinUpdate(row) {
  openUpdateFlow(row.stack, row.key);
}

/**
 * redesign-flows-1: the Update flow's Roll back… — the dialog that puts
 * `move`'s earlier image back the same way (backup, commit, deploy).
 * @param {Move} move the move as it ran (from → to)
 */
export function openPinRollback(move) {
  const d = openDialog({
    title: `Roll back ${move.stack}/${move.key} to ${move.from_version}`,
    body: [h("div", { class: "act-body pin-update" })],
    id: "action-dialog",
  });
  const body = /** @type {HTMLElement} */ (
    d.dialog.querySelector(".pin-update")
  );
  void drawMove(
    d,
    body,
    {
      ...move,
      from: move.to,
      to: move.from,
      from_version: move.to_version,
      to_version: move.from_version,
    },
    { major: false, notes: null, rollback: true },
  );
}

/** @param {() => void} close */
function closeRow(close) {
  const btn = h(
    "button",
    { type: "button", class: "kp-button", "data-kp-dialog-close": "" },
    "Close",
  );
  btn.addEventListener("click", close);
  return h("div", { class: "kp-dialog__actions" }, btn);
}

/**
 * The review and, once confirmed, the run.
 * @param {{dialog: HTMLDialogElement, close: () => void}} d
 * @param {HTMLElement} body
 * @param {Move} move
 * @param {{major: boolean, notes: string | null, rollback: boolean}} o
 */
async function drawMove(d, body, move, o) {
  const service = move.key.split("/")[1] ?? move.key;
  const edit = { kind: "settings", images: { [move.key]: move.to } };
  body.replaceChildren(
    h("p", { class: "measured", role: "status" }, "Reading the plan…"),
  );
  const plan = await send(
    "POST",
    `/data/stacks/${encodeURIComponent(move.stack)}/plan`,
    { edit },
    "the plan of this update",
  );
  if (!d.dialog.open) return;
  if (!plan.ok) {
    body.replaceChildren(refusalCallout(plan.error), closeRow(d.close));
    return;
  }
  const v = planView(plan.body);

  const facts = h(
    "dl",
    { class: "facts pin-update__facts" },
    h("dt", null, "Stack"),
    h("dd", null, move.stack),
    h("dt", null, "App / service"),
    h("dd", null, move.key),
    h("dt", null, "From"),
    h(
      "dd",
      { "data-pin-from": "" },
      h("strong", null, move.from_version),
      h("span", { class: "mono pin-update__ref" }, move.from),
    ),
    h("dt", null, "To"),
    h(
      "dd",
      { "data-pin-to": "" },
      h("strong", null, move.to_version),
      h("span", { class: "mono pin-update__ref" }, move.to),
    ),
    h("dt", null, "File"),
    h("dd", { class: "mono" }, move.file),
    ...(o.notes
      ? [
          h("dt", null, "Release notes"),
          h(
            "dd",
            null,
            h(
              "a",
              { href: o.notes, target: "_blank", rel: "noopener noreferrer" },
              `${move.to_version} on GitHub`,
            ),
          ),
        ]
      : []),
  );

  // The same kp check field every action dialog draws (actui.js
  // `fieldEl`'s check shape), written out: it is no action argument.
  /** @type {{wrap: HTMLElement, input: HTMLInputElement} | null} */
  let tick = null;
  if (o.major) {
    const input = h("input", {
      class: "kp-field__check",
      type: "checkbox",
      id: "pin-major-read",
      name: "major_read",
      "aria-describedby": "pin-major-read-hint",
    });
    tick = {
      input,
      wrap: h(
        "div",
        { class: "kp-field kp-field--check", "data-field": "major_read" },
        input,
        h(
          "label",
          { class: "kp-field__label", for: "pin-major-read" },
          TICK_LABEL,
        ),
        h(
          "span",
          { class: "kp-field__help", id: "pin-major-read-hint" },
          "A major release can change its configuration or migrate its data in a way the old version cannot read back.",
        ),
      ),
    };
  }
  const majorBox = o.major
    ? h(
        "div",
        {
          class: "kp-alert kp-alert--warning pin-update__major",
          role: "note",
          "data-pin-major": "",
        },
        h(
          "span",
          { class: "kp-alert__body" },
          h("span", { class: "kp-alert__label" }, "Major version: "),
          `${move.from_version} → ${move.to_version}. Read the release notes for breaking changes before this runs.`,
        ),
      )
    : null;

  const steps = h(
    "ol",
    { class: "act-intro-steps pin-update__steps" },
    h(
      "li",
      null,
      `Back up ${move.stack} (its Backup action). If the backup fails or stands aside, nothing else happens.`,
    ),
    h(
      "li",
      null,
      `Rewrite the image line in ${move.file} from ${move.from_version} to ${move.to_version}, then commit and push it.`,
    ),
    h(
      "li",
      null,
      `Deploy ${move.stack} at that commit: the deploy pulls the new image and restarts ${service}.`,
    ),
  );
  const after = h(
    "p",
    { class: "measured" },
    o.rollback
      ? "This puts the earlier image back; the backup taken before the update still holds the data as it was then."
      : `Afterwards, Roll back puts ${move.from_version} back the same way; the backup from step 1 holds the data as it was.`,
  );
  const diff = v.files.length
    ? h(
        "details",
        { class: "act-plan" },
        h("summary", null, "The change to the stack file"),
        ...diffBlocks(v.files),
      )
    : null;
  const blocked = v.blocked
    ? h(
        "div",
        { class: "kp-alert kp-alert--destructive error", role: "alert" },
        h("strong", null, v.blockedWhy),
        ...(v.problems.length
          ? [h("ul", null, ...v.problems.map((p) => h("li", null, p)))]
          : []),
      )
    : null;

  const confirm = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--primary",
      "data-pin-confirm": "",
    },
    o.rollback
      ? `Back up and roll back to ${move.to_version}`
      : `Back up and update to ${move.to_version}`,
  );
  const cancel = h(
    "button",
    { type: "button", class: "kp-button", "data-kp-dialog-close": "" },
    "Cancel",
  );
  cancel.addEventListener("click", () => d.close());
  // fix-239: `homelab ui press confirm` (or `click confirm`) in Live view;
  // the major-version tick is `homelab ui check pin-major-read on`.
  dialogControl(confirm, "confirm");
  const ready = () => {
    const ok = !v.blocked && (!tick || tick.input.checked);
    /** @type {HTMLButtonElement} */ (confirm).disabled = !ok;
  };
  tick?.input.addEventListener("change", ready);
  ready();

  body.replaceChildren(
    h(
      "p",
      { class: "measured act-intro" },
      o.rollback
        ? `Moves ${move.key} back to the image it ran before the update.`
        : `Moves ${move.key}'s pinned image to the upstream's newer release.`,
    ),
    facts,
    ...(majorBox ? [majorBox] : []),
    h("h3", { class: "pin-update__h" }, "What happens, in order"),
    steps,
    after,
    ...(diff ? [diff] : []),
    ...(blocked ? [blocked] : []),
    ...(tick ? [tick.wrap] : []),
    h("div", { class: "kp-dialog__actions" }, cancel, confirm),
  );

  confirm.addEventListener("click", () => {
    if (/** @type {HTMLButtonElement} */ (confirm).disabled) return;
    // redesign-integrate-4: a roll back runs on the dashboard's server as
    // the Update flow's own job, so a closed tab never stops it half-way.
    void (o.rollback ? runOnServer(d, body, move) : run(d, body, move, edit));
  });
}

/**
 * The body of the server's one update job (`POST /data/actions/_host/
 * update-apps`, admin/src/shell/actions_flow.rs) for one pinned app moving
 * from `move.from` to `move.to`: back up, commit the image line, deploy,
 * verify, and on a failed verify put the image it ran back by itself.
 * @param {Move} move
 */
export function pinJobBody(move) {
  return {
    updates: JSON.stringify([
      {
        stack: move.stack,
        kind: "pin",
        key: move.key,
        app: move.key.split("/")[0],
        from: move.from,
        to: move.to,
      },
    ]),
  };
}

/**
 * A roll back as one server job, followed in the shared job panel; the
 * job goes on when this dialog or its tab is closed (Activity has it).
 * @param {{dialog: HTMLDialogElement, close: () => void}} d
 * @param {HTMLElement} body
 * @param {Move} move the roll back: from the image it runs to the earlier one
 */
async function runOnServer(d, body, move) {
  const panelSlot = h("div", { class: "pin-update__panel" });
  const end = h("div", { class: "pin-update__end" });
  body.replaceChildren(
    h(
      "p",
      { class: "measured", role: "status" },
      `Backing up ${move.stack}, then committing and deploying ${move.key} at ${move.to_version}. This runs on the dashboard: closing this dialog or the tab does not stop it; Activity shows it.`,
    ),
    panelSlot,
    end,
  );
  const r = await send(
    "POST",
    "/data/actions/_host/update-apps",
    pinJobBody(move),
    `the roll back of ${move.key}`,
  );
  if (!r.ok) {
    end.replaceChildren(refusalCallout(r.error), closeRow(d.close));
    return;
  }
  const job = /** @type {number} */ (r.body.job);
  const p = mountJobPanel(job, { compact: true });
  d.dialog.addEventListener("close", () => p.stop(), { once: true });
  panelSlot.replaceChildren(p.element);
  const state = await jobEnd(job);
  if (!d.dialog.open) return;
  end.replaceChildren(
    h(
      "p",
      {
        class:
          state === "done"
            ? "measured"
            : "kp-alert kp-alert--destructive error",
      },
      state === "done"
        ? `${move.key} runs ${move.to_version} again.`
        : `The roll back ended ${state}; the job above says why, and the backup it took holds the data as it was.`,
    ),
    closeRow(d.close),
  );
}

/**
 * The three steps, each one's job followed in the shared job panel.
 * @param {{dialog: HTMLDialogElement, close: () => void}} d
 * @param {HTMLElement} body
 * @param {Move} move
 * @param {Record<string, unknown>} edit
 */
async function run(d, body, move, edit) {
  const stepState = [0, 1, 2].map(() => h("span", null));
  const set = (
    /** @type {number} */ i,
    /** @type {string} */ label,
    /** @type {string} */ tone,
  ) => stepState[i].replaceChildren(badge({ label, tone }));
  const progress = h(
    "ol",
    { class: "pin-update__progress", "aria-label": "Progress" },
    ...[
      `Back up ${move.stack}`,
      `Commit ${move.key} at ${move.to_version}`,
      `Deploy ${move.stack}`,
    ].map((label, i) =>
      h(
        "li",
        null,
        h("span", { class: "pin-update__n" }, `${i + 1}.`),
        h("span", null, label),
        stepState[i],
      ),
    ),
  );
  const panelSlot = h("div", { class: "pin-update__panel" });
  const end = h("div", { class: "pin-update__end" });
  body.replaceChildren(progress, panelSlot, end);
  set(0, "running", "warn");
  set(1, "waiting", "info");
  set(2, "waiting", "info");

  /** @type {() => void} */
  let stopPanel = () => {};
  d.dialog.addEventListener("close", () => stopPanel(), { once: true });
  const follow = async (/** @type {number} */ job) => {
    stopPanel();
    const p = mountJobPanel(job, { compact: true });
    stopPanel = p.stop;
    panelSlot.replaceChildren(p.element);
    return jobEnd(job);
  };
  const stopWith = (/** @type {Node[]} */ ...nodes) => {
    end.replaceChildren(...nodes, closeRow(d.close));
  };

  // 1. The backup.
  const b = await send(
    "POST",
    `/data/actions/${encodeURIComponent(move.stack)}/backup`,
    {},
    `the backup of ${move.stack}`,
  );
  if (!b.ok) {
    set(0, "refused", "bad");
    stopWith(refusalCallout(b.error));
    return;
  }
  const backed = await follow(/** @type {number} */ (b.body.job));
  if (backed !== "done") {
    set(0, backed, "bad");
    stopWith(
      refusalCallout({
        what: `the update of ${move.key}`,
        why: `the backup ended ${backed}`,
        fix: "nothing was changed; the job above says why",
      }),
    );
    return;
  }
  set(0, "done", "ok");

  // 2. The commit, which queues 3. its deploy.
  set(1, "running", "warn");
  const c = await send(
    "POST",
    `/data/stacks/${encodeURIComponent(move.stack)}/commit`,
    {
      edit,
      subject: `Move ${move.key} from ${move.from_version} to ${move.to_version} (stale images)`,
      follow: "deploy",
    },
    `the commit of ${move.key}`,
  );
  if (!c.ok) {
    set(1, "refused", "bad");
    stopWith(refusalCallout(c.error));
    return;
  }
  set(1, "done", "ok");
  const f = c.body.follow;
  if (!f || f.refused || f.job == null) {
    set(2, "refused", "bad");
    stopWith(
      refusalCallout(
        f?.refused ?? {
          what: `the deploy of ${move.stack}`,
          why: "the commit queued no deploy",
          fix: "deploy the stack from its page",
        },
      ),
    );
    return;
  }
  set(2, "running", "warn");
  const deployed = await follow(/** @type {number} */ (f.job));
  set(2, deployed, deployed === "done" ? "ok" : "bad");

  // The way back, either way.
  const back = h(
    "button",
    { type: "button", class: "kp-button", "data-pin-rollback": "" },
    `Roll back to ${move.from_version}`,
  );
  dialogControl(back, "roll-back");
  back.addEventListener("click", () => {
    d.close();
    const dd = openDialog({
      title: `Roll back ${move.stack}/${move.key} to ${move.from_version}`,
      body: [h("div", { class: "act-body pin-update" })],
      id: "action-dialog",
    });
    const bb = /** @type {HTMLElement} */ (
      dd.dialog.querySelector(".pin-update")
    );
    void drawMove(
      dd,
      bb,
      {
        ...move,
        from: move.to,
        to: move.from,
        from_version: move.to_version,
        to_version: move.from_version,
      },
      { major: false, notes: null, rollback: true },
    );
  });
  const close = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--primary",
      "data-kp-dialog-close": "",
    },
    "Close",
  );
  close.addEventListener("click", () => d.close());
  end.replaceChildren(
    h(
      "p",
      {
        class:
          deployed === "done"
            ? "measured"
            : "kp-alert kp-alert--destructive error",
      },
      deployed === "done"
        ? `${move.key} now runs ${move.to_version}. If it misbehaves, roll back.`
        : `The deploy ended ${deployed}. Roll back puts ${move.from_version} back.`,
    ),
    h("div", { class: "kp-dialog__actions" }, back, close),
  );
}
