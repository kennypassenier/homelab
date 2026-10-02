// feat-firewall-2: the fleet's firewall as the page draws it: every rule
// of every stack with its peers named, and the matrix of who may open what
// on whom (from the server's evaluation, homelab-core's first-match
// reading of both ends). Pure.

/**
 * @typedef {{stack: string, vmid: number, n: number, dir: "in" | "out",
 *   action: string, peer: string, peer_stacks: string[], proto: string,
 *   ports: string, note: string, enabled: boolean}} RuleRow
 * @typedef {{from: string, to: string, state: "open" | "some" | "none",
 *   allowed: string[], stopped_at_source: string[]}} Cell
 * @typedef {{stacks: string[], cells: Cell[], rules: RuleRow[],
 *   unguarded: string[]}} Matrix
 * @typedef {{stack: string, vmid: number, ip: string, declared: boolean,
 *   enabled: boolean, policy_in: string | null, policy_out: string | null,
 *   rules: number, management_open: string | null,
 *   live_enforced?: boolean, live_matches_repo?: boolean}} StackFw
 */

/**
 * fix-207 (Kenny: "Firewalls: 5 van de 11 staan aan … waarom zie ik dat
 * dan niet op de topology van fleet view?"): once the dashboard has an
 * answer from the host about what pve actually enforces, that answer
 * wins over the repository's own declaration — the repository is shown
 * only when the host did not say (`live_enforced` is `undefined`, an
 * older host or the link down).
 * @param {StackFw} s
 */
export function stackState(s) {
  if (s.live_enforced === true) {
    return s.live_matches_repo === false
      ? { label: "in force (repo differs)", tone: "warn" }
      : { label: "in force", tone: "ok" };
  }
  if (s.live_enforced === false && s.live_matches_repo === false) {
    return { label: "declared, not enforced", tone: "bad" };
  }
  if (!s.declared) return { label: "none declared", tone: "bad" };
  if (!s.enabled) return { label: "declared but off", tone: "warn" };
  return { label: "in force", tone: "ok" };
}

/**
 * The rules table's rows.
 * @param {RuleRow[]} rules
 */
export function ruleRows(rules) {
  return rules.map((r) => ({
    key: `${r.stack}:${r.n}`,
    stack: r.stack,
    vmid: r.vmid,
    n: r.n,
    dir: r.dir === "in" ? "in" : "out",
    action: r.action,
    tone: r.action === "ACCEPT" ? "ok" : "bad",
    peer:
      r.peer_stacks.length > 0
        ? `${r.peer} (${r.peer_stacks.join(", ")})`
        : r.peer,
    proto: r.proto,
    ports: r.ports || "any",
    note: r.note,
    inForce: r.enabled ? "yes" : "no",
  }));
}

/**
 * The matrix as rows: one per source stack, one cell per destination.
 * @param {Matrix} m
 */
export function matrixRows(m) {
  const by = new Map(m.cells.map((c) => [`${c.from}>${c.to}`, c]));
  return m.stacks.map((from) => ({
    from,
    cells: m.stacks.map((to) => {
      if (to === from) return { to, text: "—", tone: "", title: "itself" };
      const c = by.get(`${from}>${to}`);
      if (!c) return { to, text: "?", tone: "", title: "" };
      if (c.state === "open")
        return {
          to,
          text: "open",
          tone: "warn",
          title: `${to} has no firewall in force: everything from ${from} reaches it`,
        };
      const stopped = c.stopped_at_source.length
        ? ` (${from} stops: ${c.stopped_at_source.join(", ")})`
        : "";
      if (c.state === "none")
        return {
          to,
          text: c.stopped_at_source.length ? "stopped" : "none",
          tone: "",
          title: `nothing ${from} sends reaches ${to}${stopped}`,
        };
      return {
        to,
        text: c.allowed.join(", "),
        tone: "ok",
        title: `${from} may reach ${to} on ${c.allowed.join(", ")}${stopped}`,
      };
    }),
  }));
}
