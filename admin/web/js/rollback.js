// feat-stacks-6: what a stack can go back to, from
// GET /data/actions/{stack}/rollback-options. Pure.

/**
 * @typedef {{commit: string, at: number, subject: string, applied: boolean}} CommitRow
 * @typedef {{stack: string, applied_source: string | null,
 *   applied_commit: string | null, working_copy: boolean,
 *   commits: CommitRow[], native_units: string[], missing: string[]}} RollbackOptions
 */

/**
 * @param {RollbackOptions} o
 */
export function rollbackView(o) {
  const applied = o.applied_commit
    ? `The host last deployed commit ${o.applied_commit.slice(0, 10)}${o.applied_source ? ` (${o.applied_source})` : ""}.`
    : o.applied_source
      ? `The host last deployed from ${o.applied_source}; it recorded no commit.`
      : "The host has no record of where this stack was deployed from.";
  return {
    applied,
    workingCopy: o.working_copy,
    noWorkingCopy: o.working_copy
      ? null
      : "The dashboard has no working copy of the repository yet, so it cannot list or deploy earlier commits.",
    commits: (o.commits ?? []).map((c) => ({
      commit: c.commit,
      short: c.commit.slice(0, 10),
      at: c.at,
      subject: c.subject,
      applied: c.applied ? "deployed now" : "",
    })),
    units: o.native_units ?? [],
    missing: o.missing ?? [],
  };
}
