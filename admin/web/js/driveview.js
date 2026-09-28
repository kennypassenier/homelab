// Milestone follow (feat-platform-10): the pure half of the driven replay.
// The dashboard's server keeps one "Claude is driving" state and pushes
// every step as a `drive` event with the whole state. A tab that follows
// turns each event into a short list of operations: the step itself,
// animated, when the tab saw the step before it; otherwise the operations
// that bring it to the state at once (catching up). A tab that does not
// follow gets no operation at all: it never changes page, opens a dialog
// or loses what its viewer typed; it only shows the badge.
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
 *   fields: DriveField[], buttons: string[]}} DriveForm
 * @typedef {{active: boolean, by: string | null, seq: number, page: string,
 *   form: DriveForm | null, last_at: number, idle_s: number}} DriveState
 * @typedef {{do: string, path?: string, form?: string, target?: string,
 *   field?: string, text?: string, value?: string, on?: boolean,
 *   button?: string}} DriveStep
 * @typedef {{seq: number, step: DriveStep, applied: boolean,
 *   refusal: DriveRefusal | null, state: DriveState}} DriveEvent
 * @typedef {{seq: number, page: string,
 *   form: {action: string, stack: string} | null}} Local
 * @typedef {{op: "goto", path: string} | {op: "open", action: string,
 *   stack: string} | {op: "type", name: string, text: string} |
 *   {op: "set", name: string, value: string | boolean} |
 *   {op: "press", button: string} | {op: "sync"} | {op: "close"} |
 *   {op: "note", refusal: DriveRefusal}} Op
 */

/** Where the toggle is remembered: per tab (sessionStorage). */
export const FOLLOW_KEY = "homelab.watch-claude";

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
  const where = s.form.stack === "_host" ? "the host" : s.form.stack;
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
  if (f && !same) ops.push({ op: "open", action: f.action, stack: f.stack });
  if (f) {
    for (const x of f.fields)
      ops.push({ op: "set", name: x.name, value: x.value });
    ops.push({ op: "sync" });
  }
  return ops;
}

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
      ops.push({ op: "open", action: s.form.action, stack: s.form.stack });
      ops.push({ op: "sync" });
      return ops;
    }
    case "type":
      return [
        { op: "type", name: nameOf(s, step.field), text: step.text ?? "" },
      ];
    case "pick":
      return [
        { op: "set", name: nameOf(s, step.field), value: step.value ?? "" },
      ];
    case "check":
      return [
        { op: "set", name: nameOf(s, step.field), value: step.on === true },
      ];
    case "press":
      return step.button === "close"
        ? [{ op: "press", button: "close" }, { op: "close" }]
        : [{ op: "press", button: step.button ?? "" }, { op: "sync" }];
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
