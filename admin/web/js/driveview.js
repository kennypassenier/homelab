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
 * @typedef {{active: boolean, by: string | null, seq: number, page: string,
 *   form: DriveForm | null, last_at: number, idle_s: number}} DriveState
 * @typedef {{do: string, path?: string, form?: string, target?: string,
 *   field?: string, text?: string, value?: string, on?: boolean,
 *   button?: string, op?: string}} DriveStep
 * @typedef {{seq: number, step: DriveStep, applied: boolean,
 *   refusal: DriveRefusal | null, state: DriveState}} DriveEvent
 * @typedef {{seq: number, page: string,
 *   form: {action: string, stack: string} | null}} Local
 * @typedef {{op: "goto", path: string} | {op: "open", action: string,
 *   stack: string, edit?: boolean} |
 *   {op: "type", name: string, id?: string, text: string} |
 *   {op: "set", name: string, id?: string, value: string | boolean} |
 *   {op: "row", row: string, target?: string} |
 *   {op: "press", button: string} | {op: "sync"} | {op: "close"} |
 *   {op: "note", refusal: DriveRefusal}} Op
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
  return `Claude is working on ${where}: ${action}${job}`;
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
      return [
        {
          op: "set",
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

/**
 * How long to wait before each letter: readable, and never more than about
 * two and a half seconds for the whole text.
 * @param {string} text
 * @returns {number} milliseconds per letter
 */
export const letterDelay = (text) =>
  Math.max(15, Math.min(70, Math.floor(2500 / Math.max(1, text.length))));
