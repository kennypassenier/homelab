// redesign-flows-5 (the senior review of Deploy all changes, item 2, and
// the coordinator's destroy rule of 2026-10-03): what one press of Deploy
// all changes sends, worked out from the plan and the page's ticks. Pure:
// no DOM, no fetch.
//
// Apply deploys the ticked stacks; the unticked ones and the stacks that
// cannot be planned are named in `leave_out`, so the host's plan runs for
// the rest instead of refusing the whole. A destroy never rides along: it
// is its own red step after the deploys, armed by typing the stack's name,
// confirmed by its own tick, and sent with the CT number the plan showed.

/**
 * @typedef {{deploy: string[], new: string[], destroy: string[],
 *   broken: [string, string][], unchanged: string[], ephemeral: string[],
 *   reasons?: Record<string, string>}} Plan
 */

/** @param {number} n @param {string} one @param {string} [many] */
const count = (n, one, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/**
 * The go bar of the deploys: its words, and the apply form's preset.
 * @param {Plan} p
 * @param {Set<string>} ticked
 */
export function goPlan(p, ticked) {
  const deploy = p.deploy.filter((s) => ticked.has(s));
  const unticked = p.deploy.filter((s) => !ticked.has(s));
  const broken = p.broken.map(([s]) => s);
  const leaveOut = [...unticked, ...broken];
  const left = [
    ...(unticked.length ? [`${unticked.join(", ")} left out`] : []),
    ...(broken.length
      ? [
          `${broken.join(", ")} cannot be planned and ${broken.length === 1 ? "is" : "are"} left alone`,
        ]
      : []),
  ];
  return {
    n: deploy.length,
    deploy,
    leaveOut,
    title: deploy.length
      ? `Apply ${count(deploy.length, "deploy")}`
      : "Nothing ticked to deploy",
    note: deploy.length
      ? `${left.length ? `${left.join("; ")}. ` : ""}One confirmed batch; each stack is backed up first and shows its own progress in the stack list. Nothing is destroyed here.`
      : "Tick a stack under Will deploy to deploy it.",
    label: deploy.length
      ? `Apply ${count(deploy.length, "deploy")}…`
      : "Apply…",
    /** The apply form's preset: the whole plan sends nothing extra. */
    preset: /** @type {Record<string, string>} */ (
      leaveOut.length ? { leave_out: leaveOut.join(", ") } : {}
    ),
  };
}

/**
 * The destroy step: what is armed, and why its button is off.
 * @param {Plan} p
 * @param {Set<string>} typed the gone stacks whose name was typed
 * @param {(s: string) => number | null} vmidOf the CT number the host records
 * @param {{ack: boolean, deploysPending: boolean}} o
 */
export function destroyStep(p, typed, vmidOf, o) {
  const armed = p.destroy.filter((s) => typed.has(s));
  const ids = armed.map(vmidOf);
  const known = ids.every((x) => x != null);
  const why = !armed.length
    ? "Type a stack's name under Will be destroyed to arm it."
    : !known
      ? "The host's fleet does not say the CT number of every armed stack yet; plan again."
      : o.deploysPending
        ? "Runs after the deploys: apply the ticked deploys first, or untick them."
        : !o.ack
          ? "Tick the confirmation to destroy."
          : null;
  return {
    armed,
    ids,
    ready: why == null,
    why,
    title: armed.length
      ? `Destroy ${count(armed.length, "stack")}: ${armed.map((s, i) => `${s}${ids[i] != null ? ` (CT ${ids[i]})` : ""}`).join(", ")}`
      : `Nothing armed: ${count(p.destroy.length, "stack")} gone from the files`,
    ackLabel: `Yes, destroy ${armed.length ? armed.join(", ") : "the armed stacks"}. Each is backed up and its backup restored as a check first; one that does not restore is left untouched.`,
    label: armed.length ? `Destroy ${armed.length}…` : "Destroy…",
    /** The apply form's preset: every deploy left out, the destroys typed. */
    preset: {
      leave_out: [...p.deploy, ...p.broken.map(([s]) => s)].join(", "),
      destroy: armed.join(", "),
      destroy_ids: ids.map(String).join(", "),
      destroy_ack: true,
    },
  };
}
