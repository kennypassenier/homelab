// feat-firewall-2: the fleet's firewall as the page draws it: every rule
// of every stack with its peers named, and the matrix of who may open what
// on whom (from the server's evaluation, homelab-core's first-match
// reading of both ends). Pure.

/**
 * @typedef {{stack: string, vmid: number, n: number, dir: "in" | "out",
 *   action: string, peer: string, peer_stacks: string[], proto: string,
 *   ports: string, note: string, enabled: boolean,
 *   disabled?: boolean}} RuleRow
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

// ── redesign-firewall (3.71.0, Kenny's approved demo firewall.html) ──────
// The page's words and numbers, pure so they are tested without a browser.

/**
 * @typedef {{head?: {commit: string, subject: string, at: number} | null,
 *   stacks: StackFw[], matrix?: Matrix | null}} FirewallRead
 * @typedef {"ok" | "warn" | "bad"} Tone
 */

/**
 * A stack's firewall in the demo's words: "in force", "live differs from
 * files", "no firewall" (and "declared, switched off" for an older host
 * that does not say what pve enforces). Built on `stackState`, so what the
 * host enforces still wins over what the repository declares (fix-207).
 * @param {StackFw} s
 * @returns {{tone: Tone, word: string}}
 */
export function fwState(s) {
  switch (stackState(s).label) {
    case "in force":
      return { tone: "ok", word: "in force" };
    case "in force (repo differs)":
    case "declared, not enforced":
      return { tone: "warn", word: "live differs from files" };
    case "declared but off":
      return { tone: "warn", word: "declared, switched off" };
    default:
      return { tone: "bad", word: "no firewall" };
  }
}

/**
 * The KPI strip: protected (in force) of all, unprotected (the file
 * declares none), open paths (every port gets through), rules by action.
 * @param {FirewallRead} d
 */
export function fwKpis(d) {
  const stacks = d.stacks ?? [];
  const rules = d.matrix?.rules ?? [];
  const cells = d.matrix?.cells ?? [];
  const drop = rules.filter((r) => r.action !== "ACCEPT").length;
  return {
    protected: stacks.filter((s) => fwState(s).tone === "ok").length,
    total: stacks.length,
    unprotected: stacks.filter((s) => !s.declared).length,
    open: cells.filter((c) => c.state === "open").length,
    rules: rules.length,
    drop,
    accept: rules.length - drop,
  };
}

/**
 * @typedef {{stack: string, tone: "warn" | "bad", title: string,
 *   text: string, fix: string, hint: string}} FwProblem
 */

/**
 * One attention item per stack whose firewall is not plainly in force,
 * worst first (no firewall before a mismatch), each with its fix's label.
 * @param {StackFw[]} stacks
 * @returns {FwProblem[]}
 */
export function fwAttention(stacks) {
  /** @type {FwProblem[]} */
  const out = [];
  for (const s of stacks) {
    if (fwState(s).tone === "ok") continue;
    if (s.live_enforced === true) {
      out.push({
        stack: s.stack,
        tone: "warn",
        title: `${s.stack}: a firewall runs on CT ${s.vmid}, but its stack file ${s.declared ? "says otherwise" : "declares none"}`,
        text: "The next apply would change what is in force. Compare the two, then declare the live rules or apply the file.",
        fix: "Compare…",
        hint: "Show the live-vs-file difference on the stack's Settings",
      });
    } else if (s.declared) {
      out.push({
        stack: s.stack,
        tone: "warn",
        title:
          s.live_enforced === false
            ? `${s.stack}: its stack file declares a firewall that is not in force on CT ${s.vmid}`
            : `${s.stack}: its firewall is declared but switched off`,
        text: "Every container may reach it on every port until the declaration is in force. Apply the stack, or switch the firewall on in its file.",
        fix: "Compare…",
        hint: "Show the stack's firewall on its Settings",
      });
    } else {
      out.push({
        stack: s.stack,
        tone: "bad",
        title: `${s.stack} (CT ${s.vmid}) has no firewall`,
        text: "Every container may reach it on every port. Declare one: start from the default (drop inbound, allow out) and add what it serves.",
        fix: "Declare…",
        hint: "Open this stack's firewall on its Settings to declare one",
      });
    }
  }
  return out.sort((a, b) =>
    a.tone === b.tone ? 0 : a.tone === "bad" ? -1 : 1,
  );
}

/**
 * The matrix cell from `from` to `to`.
 * @param {Matrix} m
 * @param {string} from
 * @param {string} to
 * @returns {Cell | null}
 */
export const cellOf = (m, from, to) =>
  m.cells.find((c) => c.from === from && c.to === to) ?? null;

/**
 * The square pinned before anyone clicks one: the path that lets the most
 * ports through (the most telling story), else the first square.
 * @param {Matrix} m
 * @returns {Cell | null}
 */
export function defaultCell(m) {
  /** @type {Cell | null} */
  let best = null;
  for (const c of m.cells)
    if (c.state === "some" && (!best || c.allowed.length > best.allowed.length))
      best = c;
  return best ?? m.cells[0] ?? null;
}

/**
 * Whether a rule decides the square from `from` to `to` (outlined while
 * the rule is hovered): `from`'s outbound rule naming `to`, or `to`'s
 * inbound rule naming `from`.
 * @param {RuleRow} r
 * @param {string} from
 * @param {string} to
 */
export const ruleHits = (r, from, to) =>
  (r.dir === "out" && r.stack === from && r.peer_stacks.includes(to)) ||
  (r.dir === "in" && r.stack === to && r.peer_stacks.includes(from));

/**
 * The rules that decide a cell.
 * @param {Matrix} m
 * @param {{from: string, to: string}} c
 */
export const rulesFor = (m, c) =>
  m.rules.filter((r) => ruleHits(r, c.from, c.to));

/**
 * The short text inside a matrix square: the ports ("8080, 8787"), "all"
 * for an unprotected target, nothing when blocked.
 * @param {Cell} c
 */
export const cellLabel = (c) =>
  c.state === "some"
    ? c.allowed.join(", ").replace(/tcp /g, "")
    : c.state === "open"
      ? "all"
      : "";

/**
 * A cell as one sentence: "gateway may open tcp 8080, tcp 8787."
 * @param {Cell} c
 */
export const cellWords = (c) =>
  c.state === "some"
    ? `${c.from} may open ${c.allowed.join(", ")}.`
    : c.state === "open"
      ? `${c.from} may open every port: the target has no firewall in force.`
      : `${c.from} is blocked on every port.`;

/**
 * Whether a rule matches the Rules card's filter: its action is on (none
 * on = all, DESIGN_LANGUAGE §10) and its text (address, ports, protocol,
 * why, peer stacks) contains the query.
 * @param {RuleRow} r
 * @param {Set<string>} actions
 * @param {string} q lower-case
 */
export const ruleMatches = (r, actions, q) =>
  (actions.size === 0 || actions.has(r.action)) &&
  (!q ||
    [r.peer, r.ports, r.proto, r.note, ...r.peer_stacks]
      .join(" ")
      .toLowerCase()
      .includes(q));

/**
 * The ports column: "tcp 8080, 8787", or the bare protocol, or "any".
 * @param {RuleRow} r
 */
export const portsText = (r) =>
  r.ports
    ? `${r.proto ? `${r.proto} ` : ""}${r.ports.replace(/,\s*/g, ", ")}`
    : r.proto || "any";

/**
 * One stack's firewall in a few facts, for the stack hub's Settings tab
 * (its Firewall summary): state, default policies, its rules by direction,
 * which stacks may reach it and which it may reach, and the link to this
 * page with the stack picked. `null` when the read does not know it.
 * @param {FirewallRead} d the `/data/firewall` read
 * @param {string} stack
 */
export function fwSummary(d, stack) {
  const s = (d.stacks ?? []).find((x) => x.stack === stack);
  if (!s) return null;
  const rules = (d.matrix?.rules ?? []).filter((r) => r.stack === stack);
  const cells = d.matrix?.cells ?? [];
  /** @param {Cell[]} cs @param {"from" | "to"} k */
  const reach = (cs, k) =>
    cs
      .filter((c) => c.state !== "none")
      .map((c) => ({ stack: c[k], label: cellLabel(c), state: c.state }));
  return {
    stack,
    vmid: s.vmid,
    ip: s.ip,
    ...fwState(s),
    policy_in: s.policy_in,
    policy_out: s.policy_out,
    inbound: rules.filter((r) => r.dir === "in").length,
    outbound: rules.filter((r) => r.dir === "out").length,
    reachedBy: reach(
      cells.filter((c) => c.to === stack),
      "from",
    ),
    reaches: reach(
      cells.filter((c) => c.from === stack),
      "to",
    ),
    href: `/firewall?stack=${encodeURIComponent(stack)}`,
  };
}

/**
 * redesign-config-2: where a rule stands in its stack's list. Proxmox
 * numbers a stack's rules across both directions, so "rule 17 of 4" (the
 * direction's count) read wrong; the stack's total comes first, the
 * direction's share after it.
 * @param {RuleRow} r
 * @param {RuleRow[]} rules every rule of the fleet
 */
export function ruleOrderText(r, rules) {
  const mine = rules.filter((x) => x.stack === r.stack);
  const way = mine.filter((x) => x.dir === r.dir).length;
  return `rule ${r.n} of ${mine.length} (${way} ${r.dir === "in" ? "inbound" : "outbound"}): first match wins`;
}

/**
 * redesign-config-7: the stack the Rules card opens on when nothing chose
 * one: the one with the most rules (the most telling), the first in the
 * page's order on a tie, "" when there is none. No stack name is known
 * to the code.
 * @param {string[]} names in the page's order
 * @param {RuleRow[]} rules
 */
export function busiestStack(names, rules) {
  let best = "";
  let most = 0;
  for (const n of names) {
    const c = rules.filter((r) => r.stack === n).length;
    if (c > most) {
      best = n;
      most = c;
    }
  }
  return best;
}
