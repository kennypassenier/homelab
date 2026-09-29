// Milestone follow (feat-platform-10): the pure half of the Live view replay.
// The dashboard's server keeps one "Claude is driving" state and pushes
// every step as a `drive` event with the whole state. A tab that follows
// turns each event into a short list of operations: the step itself,
// animated, when the tab saw the step before it; otherwise the operations
// that bring it to the state at once (catching up). A tab that does not
// follow gets no operation at all: it never changes page, opens a dialog
// or loses what its viewer typed; it only shows the badge. The edit forms
// (their state carries `edit`) are brought to the state by their own
// controller's `sync` (editdrive.js), so catching up is open and sync.
//
// Pure: no DOM, no fetch, no clock.

/**
 * @typedef {{id: string, name: string, kind: string, value: string | boolean,
 *   error: string | null, shown: boolean, step: string}} DriveField
 * @typedef {{job: number, state: string, message: string | null,
 *   progress: string | null}} DriveJob
 * @typedef {{what: string, why: string, fix: string}} DriveRefusal
 * @typedef {{id: string, action: string, stack: string, title: string,
 *   steps: string[], step: string, step_index: number,
 *   values: Record<string, string | boolean>, errors: Record<string, string>,
 *   run_error: DriveRefusal | null, job: DriveJob | null,
 *   fields: DriveField[], buttons: string[], edit?: DriveEdit}} DriveForm
 * @typedef {{family: string, model?: any, rows?: string[],
 *   sub?: {kind: string, title: string, target: number | string | null,
 *   values: Record<string, string | boolean>, errors: Record<string, string>},
 *   plan?: any, result?: any, staged?: Record<string, unknown>,
 *   confirmed?: string[], stacks?: string[], guarded: number}} DriveEdit
 * @typedef {{id: number, step: DriveStep, text: string, countdown: boolean,
 *   total_ms: number, left_ms: number}} DriveAnnounce
 * @typedef {{step: DriveStep, text: string, done: boolean}} DrivePlanStep
 * @typedef {{by: string, steps: DrivePlanStep[], next: number,
 *   changed: boolean}} DrivePlan
 * @typedef {{active: boolean, by: string | null, seq: number, page: string,
 *   form: DriveForm | null, last_at: number, idle_s: number,
 *   announce?: DriveAnnounce | null, paused_by?: string | null,
 *   stopped_by?: string | null, plan?: DrivePlan | null}} DriveState
 * @typedef {{do: string, path?: string, form?: string, target?: string,
 *   field?: string, text?: string, value?: string, on?: boolean,
 *   button?: string, op?: string}} DriveStep
 * @typedef {{kind?: "step" | "announce" | "control", seq: number,
 *   step: DriveStep, applied: boolean,
 *   refusal: DriveRefusal | null, state: DriveState}} DriveEvent
 * @typedef {{seq: number, page: string,
 *   form: {action: string, stack: string} | null}} Local
 * @typedef {{op: "goto", path: string} | {op: "open", action: string,
 *   stack: string, edit?: boolean} |
 *   {op: "type", name: string, id?: string, text: string} |
 *   {op: "set", name: string, id?: string, value: string | boolean} |
 *   {op: "pick", name: string, id?: string, value: string} |
 *   {op: "row", row: string, target?: string} |
 *   {op: "press", button: string} | {op: "sync"} | {op: "close"} |
 *   {op: "note", refusal: DriveRefusal} |
 *   {op: "highlight", step: DriveStep}} Op
 * @typedef {{kind: "link", path: string} | {kind: "action", action: string} |
 *   {kind: "field", id: string} | {kind: "button", button: string} |
 *   {kind: "row", op: string, target?: string} | {kind: "close"} |
 *   {kind: "none"}} Target
 */

/** Where the Live view switch is remembered: per tab (sessionStorage). */
export const FOLLOW_KEY = "homelab.live-view";

/**
 * Whether this tab follows, as remembered; off by default.
 * @param {{getItem: (k: string) => string | null} | null} storage
 */
export function readFollow(storage) {
  try {
    return storage?.getItem(FOLLOW_KEY) === "on";
  } catch {
    return false;
  }
}

/**
 * Is Claude driving now?
 * @param {DriveState | null} s
 * @param {number} now unix seconds
 */
export const isActive = (s, now) =>
  !!s && s.active && now - s.last_at < s.idle_s;

/**
 * The badge every tab shows while Claude drives, followed or not.
 * @param {DriveState | null} s
 * @param {number} now
 * @returns {string | null}
 */
export function badgeText(s, now) {
  if (!s || !isActive(s, now)) return null;
  if (!s.form) return `Claude is working on the dashboard: ${s.page}`;
  const where =
    s.form.stack === "_host"
      ? "the host"
      : s.form.stack.replace(/,/g, ", ") || "a new stack";
  const action = s.form.title.split(" · ")[0];
  const job = s.form.job ? ` · job ${s.form.job.job} ${s.form.job.state}` : "";
  const paused = s.paused_by ? ` · paused by ${s.paused_by}` : "";
  return `Claude is working on ${where}: ${action}${job}${paused}`;
}

/**
 * The tab's own record of where it is, after it caught up with `s`.
 * @param {DriveState} s
 * @returns {Local}
 */
export const localOf = (s) => ({
  seq: s.seq,
  page: s.page,
  form: s.form ? { action: s.form.action, stack: s.form.stack } : null,
});

/**
 * @param {DriveState} s
 * @param {string | undefined} id
 */
const nameOf = (s, id) =>
  s.form?.fields.find((f) => f.id === id)?.name ?? String(id);

/**
 * The operations that bring a tab from `local` to `s` at once.
 * @param {Local} local
 * @param {DriveState} s
 * @returns {Op[]}
 */
export function catchUp(local, s) {
  /** @type {Op[]} */
  const ops = [];
  const f = s.form;
  const same =
    !!local.form &&
    !!f &&
    local.form.action === f.action &&
    local.form.stack === f.stack;
  if (local.form && !same) ops.push({ op: "close" });
  if (local.page !== s.page) ops.push({ op: "goto", path: s.page });
  if (f && !same) ops.push(openOp(f));
  if (f) {
    // An edit form's controller fills its own fields in its sync, once
    // its dialogs are open (the commit step's fields exist only then).
    if (!f.edit)
      for (const x of f.fields)
        ops.push({ op: "set", name: x.name, id: x.id, value: x.value });
    ops.push({ op: "sync" });
  }
  return ops;
}

/**
 * @param {DriveForm} f
 * @returns {Op}
 */
const openOp = (f) =>
  f.edit
    ? { op: "open", action: f.action, stack: f.stack, edit: true }
    : { op: "open", action: f.action, stack: f.stack };

/**
 * The operations one applied step animates.
 * @param {Local} local
 * @param {DriveStep} step
 * @param {DriveState} s
 * @returns {Op[]}
 */
export function animate(local, step, s) {
  switch (step.do) {
    case "goto":
      return [{ op: "goto", path: s.page }];
    case "open": {
      if (!s.form) return catchUp(local, s);
      /** @type {Op[]} */
      const ops = [];
      if (local.page !== s.page) ops.push({ op: "goto", path: s.page });
      ops.push(openOp(s.form));
      ops.push({ op: "sync" });
      return ops;
    }
    case "type":
      return [
        {
          op: "type",
          name: nameOf(s, step.field),
          id: step.field,
          text: step.text ?? "",
        },
      ];
    case "pick":
      // Played slowly, as a person opens a list and picks (drivepace.js).
      return [
        {
          op: "pick",
          name: nameOf(s, step.field),
          id: step.field,
          value: step.value ?? "",
        },
        ...(s.form?.edit ? [/** @type {Op} */ ({ op: "sync" })] : []),
      ];
    case "check":
      return [
        {
          op: "set",
          name: nameOf(s, step.field),
          id: step.field,
          value: step.on === true,
        },
      ];
    case "edit":
      // A whole text at once (the raw editor's file).
      return [
        {
          op: "set",
          name: nameOf(s, step.field),
          id: step.field,
          value: step.text ?? "",
        },
      ];
    case "row":
      return [
        { op: "row", row: step.op ?? "", target: step.target },
        { op: "sync" },
      ];
    case "press": {
      if (step.button === "close")
        return [{ op: "press", button: "close" }, { op: "close" }];
      const pressed = /** @type {Op} */ ({
        op: "press",
        button: step.button ?? "",
      });
      // A press that hands over to another form (the roll back dialog's
      // row opens the action's dialog): press, then bring the tab there.
      const f = s.form;
      if (
        local.form &&
        f &&
        (local.form.action !== f.action || local.form.stack !== f.stack)
      )
        return [pressed, ...catchUp(local, s)];
      return [pressed, { op: "sync" }];
    }
    case "close":
    case "done":
      return local.form ? [{ op: "close" }] : [];
    default:
      return catchUp(local, s);
  }
}

/**
 * What a tab does with one `drive` event.
 * @param {Local} local where the tab is
 * @param {DriveEvent} ev
 * @param {boolean} following the tab's toggle
 * @returns {{ops: Op[], local: Local}}
 */
export function plan(local, ev, following) {
  if (!following) return { ops: [], local };
  // Live view: an announcement marks the step's target; a button pressed
  // in the bar only repaints. Neither is a step: `local` stays.
  if (ev.kind === "announce") {
    const a = ev.state.announce;
    return { ops: a ? [{ op: "highlight", step: a.step }] : [], local };
  }
  if (ev.kind === "control") return { ops: [], local };
  if (!ev.applied)
    return {
      ops: ev.refusal ? [{ op: "note", refusal: ev.refusal }] : [],
      local,
    };
  const ops =
    ev.seq === local.seq + 1
      ? animate(local, ev.step, ev.state)
      : catchUp(local, ev.state);
  return { ops, local: localOf(ev.state) };
}

// ── Live view: announce, plan and pause (Kenny, 2026-09-29) ────────────

/**
 * What is left of the announcement's countdown now: frozen while paused,
 * otherwise counted down on this tab's own clock from when the state came.
 * @param {DriveState | null} s
 * @param {number} since milliseconds since `s` arrived
 * @returns {number}
 */
export function leftNow(s, since) {
  const a = s?.announce;
  if (!a || !a.countdown) return 0;
  if (s?.paused_by) return a.left_ms;
  return Math.max(0, Math.min(a.total_ms, a.left_ms - Math.max(0, since)));
}

/**
 * "Step 2 of 5": the plan's next step, for the bar; empty without a plan.
 * @param {DriveState | null} s
 */
export function planCounter(s) {
  const p = s?.plan;
  if (!p || p.steps.length === 0) return "";
  const n = Math.min(p.next + 1, p.steps.length);
  return `Step ${n} of ${p.steps.length}`;
}

/**
 * The announcement bar of a tab in Live view: null when there is nothing to
 * announce and nothing is paused. Every string has a fixed place in the
 * bar, so a new value never moves the rest (the count is one digit wide,
 * the status and the counter keep their width when empty).
 * @param {DriveState | null} s
 * @param {number} left what `leftNow` says
 * @returns {{text: string, count: string, fraction: number,
 *   status: string, counter: string, paused: boolean,
 *   countdown: boolean} | null}
 */
export function announceView(s, left) {
  if (!s) return null;
  const a = s.announce ?? null;
  const paused = !!s.paused_by;
  if (!a && !paused) return null;
  const countdown = !!a && a.countdown;
  const ms = countdown ? Math.max(0, Math.min(a.total_ms, left)) : 0;
  return {
    text: a ? `Next: ${a.text}` : "Next: Claude's next step",
    count: countdown ? String(Math.ceil(ms / 1000)) : "",
    fraction: countdown && a.total_ms > 0 ? ms / a.total_ms : 0,
    status: paused ? `Paused by ${s.paused_by}` : "",
    counter: planCounter(s),
    paused,
    countdown,
  };
}

/**
 * The plan beside the page: each step with its mark, the current one being
 * the plan's next; null without a plan.
 * @param {DriveState | null} s
 * @returns {{changed: boolean, counter: string,
 *   items: {text: string, mark: "done" | "current" | "todo"}[]} | null}
 */
export function planList(s) {
  const p = s?.plan;
  if (!p) return null;
  return {
    changed: p.changed,
    counter: planCounter(s),
    items: p.steps.map((x, i) => ({
      text: x.text,
      mark: x.done ? "done" : i === p.next ? "current" : "todo",
    })),
  };
}

/** The edit forms whose page is a stack's tab. */
const EDIT_TABS = /** @type {Record<string, string>} */ ({
  settings: "settings",
  raw: "settings",
  "add-app": "settings",
  firewall: "firewall",
});

/**
 * The element a step will act on, as a description the page resolves: the
 * link of the page a `goto` shows, the action button an `open` presses, the
 * field, button or row of the open form.
 * @param {DriveStep} step
 * @returns {Target}
 */
export function targetOf(step) {
  switch (step.do) {
    case "goto":
      return step.path ? { kind: "link", path: step.path } : { kind: "none" };
    case "open": {
      const form = step.form ?? "";
      const tab = EDIT_TABS[form];
      if (tab && step.target)
        return { kind: "link", path: `/app/stacks/${step.target}/${tab}` };
      if (form === "host-settings")
        return { kind: "link", path: "/app/settings" };
      if (form.startsWith("batch") || form === "new-stack" || form === "import")
        return { kind: "none" };
      return { kind: "action", action: form };
    }
    case "type":
    case "edit":
    case "pick":
    case "check":
      return step.field ? { kind: "field", id: step.field } : { kind: "none" };
    case "press":
      return step.button === "close"
        ? { kind: "close" }
        : { kind: "button", button: step.button ?? "" };
    case "row":
      return { kind: "row", op: step.op ?? "", target: step.target };
    case "close":
      return { kind: "close" };
    default:
      return { kind: "none" };
  }
}

/**
 * A field's name in the open form, by its id.
 * @param {DriveState | null} s
 * @param {string} id
 */
export const fieldName = (s, id) =>
  s?.form?.fields.find((f) => f.id === id)?.name ?? id;
