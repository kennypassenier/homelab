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

import { act, onAct, send } from "./act.js";
import { badge, openDialog, refusalCallout } from "./actui.js";
import { fetchJson, h } from "./dom.js";
import { diffBlocks } from "./editui.js";
import { mountJobPanel } from "./jobpanel.js";
import { planView } from "./plan.js";
import { majorJump, releaseUrl } from "./staleimages.js";
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
function jobEnd(job) {
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
 * The Update of one stale-image row: reads the new reference first (the
 * registry's digest for the newer tag), then shows what changes.
 * @param {StaleRow} row
 */
export async function openPinUpdate(row) {
  const status = h(
    "p",
    { class: "measured", role: "status" },
    "Reading the newer image's digest from its registry…",
  );
  const body = h("div", { class: "act-body pin-update" }, status);
  const d = openDialog({
    title: `Update ${row.stack}/${row.container} to ${row.latest}`,
    body: [body],
    id: "action-dialog",
  });
  const q = new URLSearchParams({ key: row.key, latest: row.latest });
  const r = await fetchJson(
    `/data/stacks/${encodeURIComponent(row.stack)}/pin-target?${q}`,
    "the newer image",
  );
  if (!d.dialog.open) return;
  if (!r.ok) {
    body.replaceChildren(refusalCallout(r.error), closeRow(d.close));
    return;
  }
  /** @type {Move} */
  const move = { stack: row.stack, ...r.body };
  drawMove(d, body, move, {
    major: majorJump(row.pinned, row.latest),
    notes: releaseUrl(row.upstream, row.latest),
    rollback: false,
  });
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
    void run(d, body, move, edit);
  });
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
