// feat-stacks-2: the plan before a commit, as the page draws it: each
// file's diff with its lines marked, what homelab would change on the
// machines, what the host last applied, and whether the commit may go.
// Pure.

/**
 * @typedef {{op: "=" | "+" | "-", old: number | null, new: number | null,
 *   text: string}} DiffLine
 * @typedef {{old_start: number, old_len: number, new_start: number,
 *   new_len: number, lines: DiffLine[]}} Hunk
 * @typedef {{path: string, status: "added" | "changed" | "removed",
 *   added: number, removed: number, hunks: Hunk[]}} FileDiff
 * @typedef {{tone: "info" | "warning", what: string, detail: string[],
 *   by: string | null}} Effect
 * @typedef {{stack: string, kind: string, head: string | null,
 *   sync_error: string | null, files: FileDiff[], effects: Effect[],
 *   follow_ups: string[], problems: string[], valid: boolean,
 *   unchanged: boolean, applied: {changes?: string[], never?: boolean,
 *   unavailable?: string} | null, subject: string,
 *   restarts_dashboard: boolean}} Plan
 */

/**
 * @param {Plan} p
 */
export function planView(p) {
  const files = p.files.map((f) => ({
    path: f.path,
    title: `${f.path} · ${f.status} · +${f.added} −${f.removed}`,
    badge: {
      label: f.status,
      tone:
        f.status === "removed" ? "bad" : f.status === "added" ? "ok" : "info",
    },
    hunks: f.hunks.map((h) => ({
      head: `@@ −${h.old_start},${h.old_len} +${h.new_start},${h.new_len} @@`,
      lines: h.lines.map((l) => ({
        cls:
          l.op === "+" ? "diff-add" : l.op === "-" ? "diff-del" : "diff-same",
        mark: l.op === "=" ? " " : l.op,
        no: l.op === "+" ? l.new : l.old,
        text: l.text,
      })),
    })),
  }));
  const effects = p.effects.map((e) => ({
    tone: e.tone,
    what: capital(e.what),
    detail: e.detail,
    by: e.by ? `by ${e.by}` : "nothing to run",
  }));
  let applied = "";
  if (!p.applied) applied = "";
  else if (p.applied.unavailable)
    applied = `What the host last applied could not be read: ${p.applied.unavailable}.`;
  else if (p.applied.never)
    applied =
      "The host has never applied this stack: the first deploy sends every file.";
  else if (!p.applied.changes?.length)
    applied =
      "A deploy would send the files the host already has, apart from the stack file itself.";
  else
    applied = `A deploy would change ${p.applied.changes.length} file(s) the host last applied (this edit and any earlier commit not yet deployed):`;
  return {
    files,
    effects,
    applied,
    appliedList: p.applied?.changes ?? [],
    problems: p.problems,
    blocked: !p.valid,
    blockedWhy: p.unchanged
      ? "Nothing changes: the edit is what the files already say."
      : p.problems.length
        ? "homelab refuses these files; nothing can be committed until they are corrected."
        : "",
    syncNote: p.sync_error
      ? `The working copy could not be brought up to date (${p.sync_error}); the plan is made on what it holds.`
      : "",
    restarts: p.restarts_dashboard && p.follow_ups.includes("deploy"),
  };
}

/** @param {string} s */
const capital = (s) => (s ? s[0].toUpperCase() + s.slice(1) : s);

/**
 * The commit request.
 * @param {Record<string, unknown>} edit the edit the plan was made for
 * @param {Record<string, string | boolean>} values subject, note, follow
 */
export function commitBody(edit, values) {
  const subject = String(values.subject ?? "").trim();
  const note = String(values.note ?? "").trim();
  const follow = String(values.follow ?? "none");
  return {
    edit,
    ...(subject ? { subject } : {}),
    ...(note ? { note } : {}),
    ...(follow && follow !== "none" ? { follow } : {}),
  };
}

/**
 * What the commit answered, in words.
 * @param {{committed: {commit: string, subject: string, pushed: boolean,
 *   landed_despite_error: boolean},
 *   follow: null | {job?: number, action?: string,
 *   refused?: {what: string, why: string, fix: string}}}} r
 */
export function committedText(r) {
  const c = r.committed;
  const short = c.commit.slice(0, 10);
  const parts = [`Committed ${short} and pushed: ${c.subject}.`];
  if (c.landed_despite_error)
    parts.push("The push reported an error, but the remote has the commit.");
  if (r.follow?.job != null)
    parts.push(
      `${r.follow.action === "resize" ? "Resize" : "Deploy"} queued as job ${r.follow.job}.`,
    );
  if (r.follow?.refused)
    parts.push(`The follow-up was refused: ${r.follow.refused.why}.`);
  return parts.join(" ");
}
