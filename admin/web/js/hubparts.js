// The stack hub's parts on kp-themes 11 (feat-kp11demo-13, the approved
// stacks demo): the health checks as a kp status list with the fix written
// out, what needs you as kp problems (the command with a Copy button), what
// the stack depends on and who uses it as a numbered kp deplist, and the
// Resources chart with a mark at every deploy. Pure helpers first (they
// are tested without a browser), the DOM builders after them.

import {
  attachStatusLists,
  buildStatusList,
  countStatus,
  setStatus,
} from "/static/kp/dist/kp-themes.min.js";

/**
 * @typedef {import("./stackhub.js").Health} Health
 * @typedef {import("./stackhub.js").Problem} Problem
 */

/** What to do about a check that is not fine, written out. */
const FIXES = {
  deployed: "Deploy creates the container from its files in the repository.",
  online:
    "Open Logs to see what happened last; Deploy brings the container back.",
  apps: "Open Logs filtered to the stopped app; a Deploy restarts it.",
  restarts:
    "Open Logs filtered to errors: a restart loop is a crash on start, and the first error says why.",
  web: "Check the stack's gateway route and the app's own logs.",
  disk: "Free space in the stack's volume, or give it more under Settings ▸ Size and network.",
  backup:
    "Back up now, then check that the backup target has room; the nightly round tries again tonight.",
  env: "Push the env… from the More menu so the host's vault holds a copy of each secret file.",
  drift: "Deploy makes the container match its files again.",
};

/**
 * The words of a fix for one check key.
 * @param {string} key
 */
export function fixFor(key) {
  if (key.startsWith("health:"))
    return "Open the app's logs for the last answer of its health check.";
  return /** @type {Record<string, string>} */ (FIXES)[key] ?? "";
}

/**
 * The hub's checks as kp status rows. Fine and "by design" rows are ok, a
 * reservation is a warn row, a failed check is failed; a check the host
 * could not measure is empty (never green).
 * @param {Health[]} rows
 * @returns {import("/static/kp/dist/kp-themes.min.js").StatusRow[]}
 */
export function statusRows(rows) {
  return rows.map((r) => {
    const state =
      r.tone === "bad"
        ? "failed"
        : r.tone === "warn"
          ? "warn"
          : r.tone === "unknown"
            ? "empty"
            : "ok";
    return {
      id: r.key,
      label: r.text,
      detail: r.verdict,
      state,
      ...(state === "failed" || state === "warn" ? { fix: fixFor(r.key) } : {}),
    };
  });
}

/**
 * "5 of 7 checks pass" from the rows' states.
 * @param {{ok: number, total: number, warn?: number}} c
 */
export const passText = (c) =>
  `${c.ok} of ${c.total} ${c.total === 1 ? "check passes" : "checks pass"}`;

/** The command that fixes a problem, when one does. @param {string} key @param {string} stack */
export function commandFor(key, stack) {
  if (key === "drift" || key === "not-deployed")
    return `homelab deploy ${stack}`;
  if (key === "backup-missed") return `homelab backup ${stack}`;
  if (key === "parked") return `homelab enable ${stack}`;
  if (key === "no-env") return `homelab seal-env ${stack}`;
  return undefined;
}

/**
 * The hub's problems as kp problems: the title is what is wrong, its text
 * why, the fix's own title the remedy, the fix button the action.
 * @param {Problem[]} items
 * @param {string} stack
 * @param {(p: Problem) => Node | null} actionOf
 * @returns {import("/static/kp/dist/kp-themes.min.js").Problem[]}
 */
export function problemItems(items, stack, actionOf) {
  return items.map((p) => {
    const command = commandFor(p.key, stack);
    return {
      key: p.key,
      what: p.title,
      why: p.text,
      remedy: p.act.title,
      ...(command ? { command } : {}),
      severity:
        p.tone === "bad" ? "critical" : p.tone === "warn" ? "warning" : "info",
      action: actionOf(p),
    };
  });
}

/**
 * Depends on / used by, from the topology's edges, as kp deplist rows. A
 * named address is no dependency; each pair once.
 * @param {{from: string, to: string, kind: string}[]} edges
 * @param {string} stack
 * @param {(name: string) => "running" | "stopped" | "unknown"} stateOf
 * @returns {import("/static/kp/dist/kp-themes.min.js").DepItem[]}
 */
export function depItems(edges, stack, stateOf) {
  /** @type {Map<string, import("/static/kp/dist/kp-themes.min.js").DepItem>} */
  const out = new Map();
  for (const e of edges) {
    if (e.kind === "named") continue;
    // A gateway route points from the gateway at its backend: the backend
    // is used by the gateway, so the dependency runs the other way.
    const [from, to] = e.kind === "route" ? [e.to, e.from] : [e.from, e.to];
    if (from !== stack && to !== stack) continue;
    const other = from === stack ? to : from;
    out.set(`${from}>${to}`, { from, to, state: stateOf(other) });
  }
  return [...out.values()];
}

/**
 * The Resources chart's spec from `/data/charts?stack=…&range=24h`: one
 * series per panel (CPU, memory, disk) in percent, and a mark for every
 * deploy of the stack in the window (charts.js `timeChart`, seconds).
 * @param {any} body the charts answer
 * @param {{at: number, label: string}[]} deploys unix seconds
 * @param {{from: number, to: number}} win unix seconds
 * @param {number | null} [ramMaxMb] the stack's memory limit, to show memory in percent
 * @returns {import("./charts.js").ChartSpec}
 */
export function resourcesSpec(body, deploys, win, ramMaxMb = null) {
  /** @type {import("./charts.js").Series[]} */
  const series = [];
  for (const p of body?.panels ?? []) {
    const title = String(p?.panel?.title ?? "");
    const label = /^CPU \(whole/i.test(title)
      ? "CPU"
      : /^Memory used \(whole/i.test(title)
        ? "Memory"
        : /^Disk used \(root/i.test(title)
          ? "Disk"
          : null;
    const first = p?.series?.[0];
    if (!label || !first?.points?.length) continue;
    // The container's memory is in bytes: shown as its share of the limit.
    if (label === "Memory") {
      if (!ramMaxMb) continue;
      const limit = ramMaxMb * 1024 * 1024;
      series.push({
        label,
        points: first.points.map(
          (/** @type {[number, number]} */ pt) =>
            /** @type {[number, number]} */ ([pt[0], (pt[1] / limit) * 100]),
        ),
      });
    } else series.push({ label, points: first.points });
  }
  return {
    label: "Resources of the stack, last 24 hours",
    key: "hub-resources",
    unit: "percent",
    yMax: 100,
    from: win.from,
    to: win.to,
    series,
    annotations: deploys.map((d) => ({
      at: d.at,
      label: d.label,
      tone: /** @type {const} */ ("info"),
    })),
  };
}

// ── the DOM half ─────────────────────────────────────────────────────────

/**
 * A kp status list in `host` that keeps its rows (and the focus on a Retry)
 * across the hub's frequent repaints: it is built once per set of checks and
 * every later paint changes the rows in place.
 * @param {HTMLElement} host
 * @param {{label: string, onRetry: (id: string) => void}} opts
 */
export function statusBlock(host, opts) {
  /** @type {HTMLElement | null} */
  let list = null;
  let keys = "";
  host.addEventListener("kp-status-retry", (e) =>
    opts.onRetry(/** @type {CustomEvent} */ (e).detail.id),
  );
  const detach = attachStatusLists(host);
  return {
    /**
     * @param {import("/static/kp/dist/kp-themes.min.js").StatusRow[]} rows
     * @param {(id: string) => Node | null} [extra] a node to put in a row's body (an Answer… button)
     * @returns {{ok: number, total: number}}
     */
    set(rows, extra) {
      const k = rows.map((r) => r.id).join("|");
      if (!list || k !== keys) {
        keys = k;
        list = buildStatusList(rows, {
          label: opts.label,
          retry: true,
          single: true,
        });
        host.replaceChildren(list);
        attachStatusLists(host);
        for (const r of rows) {
          const node = r.id ? extra?.(r.id) : null;
          if (!node || !r.id) continue;
          list
            .querySelector(
              `[data-kp-status-id="${CSS.escape(r.id)}"] .kp-statusrow__body`,
            )
            ?.append(node);
        }
      } else {
        for (const r of rows)
          setStatus(list, r.id ?? "", {
            state: r.state,
            label: r.label,
            detail: r.detail,
            fix: r.fix ?? "",
          });
      }
      const c = countStatus(list);
      return { ok: c.ok, total: c.total };
    },
    list: () => list,
    stop: detach,
  };
}
