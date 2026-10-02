// The TUI parity round's pure view models (Kenny, 2026-09-28: "Wat nu in de
// TUI kan, moet nog altijd kunnen in ons systeem"): today's verdict, the
// flags a stack carries ([OFF] [CHANGED] [NOENV]), the host's settings as
// rows, the live log's filter, a transfer's bar, the version warnings, the
// apply plan in words, a manual check as a choice, and one shell line's
// result. No DOM, no fetch, no clock.

import { humanMb } from "./fleet.js";

/**
 * @typedef {import("./notices.js").Fix} Fix
 * @typedef {{level: "Broken" | "Attention", source: string, what: string,
 *   remedy: string, fix?: Fix}} TodayItem
 * @typedef {{today: {items: TodayItem[], unread: string[]}, verdict: string,
 *   needs_you: boolean, stack_files: number, skipped?: string | null}} TodayBody
 */

/**
 * fix-68's one answer: the verdict, its tone, and each item most severe
 * first.
 * @param {TodayBody} body
 */
export function todayView(body) {
  const items = [...(body.today.items ?? [])]
    .sort((a, b) => (a.level === b.level ? 0 : a.level === "Broken" ? -1 : 1))
    .map((i) => ({
      badge:
        i.level === "Broken"
          ? { label: "broken", tone: /** @type {const} */ ("bad") }
          : { label: "attention", tone: /** @type {const} */ ("warn") },
      source: i.source,
      what: i.what,
      remedy: i.remedy,
      fix: i.fix ?? null,
    }));
  const unread = body.today.unread ?? [];
  return {
    verdict: body.verdict,
    tone: !body.needs_you
      ? "success"
      : items.some((i) => i.badge.label === "broken")
        ? "destructive"
        : "warning",
    items,
    unread,
    note: body.skipped ?? "",
  };
}

/**
 * The fleet check's findings as rows, most severe first.
 * @param {{severity: string, subject: string, what: string, remedy: string,
 *   fix?: Fix, drift_diffable?: boolean, drift_stack?: string}[]} findings
 */
export function findingRows(findings) {
  const rank = { Broken: 0, Drift: 1, Noted: 2 };
  return [...findings]
    .sort(
      (a, b) =>
        (rank[/** @type {keyof typeof rank} */ (a.severity)] ?? 3) -
          (rank[/** @type {keyof typeof rank} */ (b.severity)] ?? 3) ||
        a.subject.localeCompare(b.subject),
    )
    .map((f) => ({
      badge: {
        label: f.severity.toLowerCase(),
        tone:
          f.severity === "Broken"
            ? /** @type {const} */ ("bad")
            : f.severity === "Drift"
              ? /** @type {const} */ ("warn")
              : /** @type {const} */ ("ok"),
      },
      subject: f.subject,
      what: f.what,
      remedy: f.remedy,
      fix: f.fix ?? null,
      // fix-219: a repo-drift finding (evaluate_repo_drift) can expand into
      // the actual per-file change; `driftStack` is the name the Health
      // page fetches `/data/fleet-check/{stack}/diff` with.
      diffable: f.drift_diffable ?? false,
      driftStack: f.drift_stack ?? null,
    }));
}

/**
 * @typedef {"changed" | "same" | "no_local_files" | "never_applied" |
 *   "not_compared"} DriftState
 */

/**
 * The flags the TUI shows beside a stack, in its words: [OFF] parked,
 * [CHANGED] its files differ from what the host applied, [NOENV] the host
 * holds no sealed env (a deploy fails closed).
 * @param {{enabled: boolean, env_sealed?: boolean}} s
 * @param {DriftState | null | undefined} drift
 * @returns {{label: string, tone: "warn" | "bad", title: string}[]}
 */
export function stackFlags(s, drift) {
  /** @type {{label: string, tone: "warn" | "bad", title: string}[]} */
  const out = [];
  if (!s.enabled)
    out.push({
      label: "OFF",
      tone: "warn",
      title: "parked: no nightly backup or update, not started on boot",
    });
  if (drift === "changed")
    out.push({
      label: "CHANGED",
      tone: "warn",
      title: "the files differ from what the host applied; a deploy is due",
    });
  if (s.env_sealed === false)
    out.push({
      label: "NOENV",
      tone: "bad",
      title:
        "the host holds no sealed env for this stack: a deploy fails closed",
    });
  return out;
}

/**
 * One stack's drift in a sentence (fix-107: only what was compared is
 * green).
 * @param {{state: DriftState, label: string, why?: string | null} | undefined} d
 * @returns {{label: string, tone: "ok" | "warn" | "info"}}
 */
export function driftFact(d) {
  if (!d) return { label: "not compared yet", tone: "info" };
  const tone =
    d.state === "same" ? "ok" : d.state === "changed" ? "warn" : "info";
  return { label: d.why ? `${d.label} (${d.why})` : d.label, tone };
}

/**
 * host.toml as read-only rows for the host page (feat-settings-1's page,
 * shown where the TUI shows its settings).
 * @param {{fields: {key: string, group: string, label: string,
 *   default: string, set: boolean, value: unknown, access: string}[]}} page
 */
export function hostSettingRows(page) {
  return page.fields.map((f) => ({
    group: f.group,
    key: f.key,
    label: f.label,
    value:
      f.access === "secret"
        ? f.set
          ? "set (not shown)"
          : "not set"
        : f.set
          ? valueText(f.value)
          : `default: ${f.default}`,
    source: f.set ? "host.toml" : "default",
  }));
}

/** @param {unknown} v */
function valueText(v) {
  if (v == null) return "—";
  if (typeof v === "string") return v;
  if (typeof v === "number" || typeof v === "boolean") return String(v);
  return JSON.stringify(v);
}

/**
 * @typedef {{seq: number, ts: number, level: string, source: string,
 *   msg: string, req: number | null, by: string | null}} HostLine
 */

/**
 * Whether a host line passes the log page's filter: a source (a stack's
 * name, HOST, …; empty for all), a level at least this severe, and text.
 * @param {HostLine} l
 * @param {{source: string, level: string, q: string}} f
 */
export function lineMatches(l, f) {
  if (f.source && l.source !== f.source) return false;
  const rank = { debug: 0, info: 1, warn: 2, error: 3 };
  const lv = rank[/** @type {keyof typeof rank} */ (l.level)] ?? 1;
  const min = rank[/** @type {keyof typeof rank} */ (f.level)] ?? 0;
  if (lv < min) return false;
  if (f.q && !l.msg.toLowerCase().includes(f.q.toLowerCase())) return false;
  return true;
}

/**
 * Who started the operation a line belongs to.
 * @param {HostLine} l
 */
export const whoText = (l) =>
  l.by ? (l.by === "admin" ? "this dashboard" : `session ${l.by}`) : "the host";

/**
 * One transfer's bar.
 * @param {{op: string, label: string, done: number, total: number | null}} t
 */
export function transferView(t) {
  const mb = (/** @type {number} */ b) => humanMb(b / (1024 * 1024));
  const pct =
    t.total && t.total > 0
      ? Math.min(100, Math.round((t.done / t.total) * 100))
      : null;
  return {
    label: `${t.op} · ${t.label}`,
    pct,
    text: t.total ? `${mb(t.done)} of ${mb(t.total)}` : `${mb(t.done)} so far`,
  };
}

/**
 * The version warnings every page shows: a newer host release, and a
 * dashboard older than the host it talks to.
 * @param {{latest: string | null, host: string | null, dashboard: string,
 *   update_available: boolean, dashboard_older: boolean} | null} v
 */
export function versionNotes(v) {
  if (!v) return { update: null, older: null };
  return {
    update: v.update_available
      ? `Host update available: ${v.latest} (the host runs ${v.host}).`
      : null,
    older: v.dashboard_older
      ? `This dashboard (${v.dashboard}) is older than the host (${v.host}): pages may miss what the host added; update the dashboard (install-native on the admin stack).`
      : null,
  };
}

/**
 * A tab that outlived its dashboard (Kenny, 2026-09-29: after 3.63.1 was
 * installed his open tab still drew 3.63.0's bar). The page is one
 * document that never reloads while he moves between pages, so a new
 * release of the dashboard reaches it only when the page notices the
 * server's version moved and loads again.
 * @param {string | null} loadedAs the dashboard version this page was
 *   served by (its first read)
 * @param {string | null | undefined} serving the version the dashboard
 *   answers with now
 * @returns {string | null} the words for the banner, or null when current
 */
export function outdatedPage(loadedAs, serving) {
  if (!loadedAs || !serving || loadedAs === serving) return null;
  return `The dashboard was updated to ${serving}; this page still runs ${loadedAs}.`;
}

/**
 * The apply plan in words, and why it cannot run when it cannot.
 * @param {{deploy: string[], new: string[], unchanged: string[],
 *   destroy: string[], ephemeral: string[], broken: [string, string][],
 *   reasons?: Record<string, string>}} p
 */
export function applySummary(p) {
  /** @type {string[]} */
  const lines = [];
  for (const n of p.deploy)
    lines.push(
      p.new.includes(n)
        ? `↑ ${n}: new, creates its container`
        : // fix-192: the plan says WHICH component differs (files, env,
          // secrets, or only the derived manifest) instead of the generic
          // "its files differ" that gave no reason to trust a redeploy.
          `↑ ${n}: deploy — ${p.reasons?.[n] ?? "its files differ from what the host applied"}`,
    );
  for (const n of p.destroy)
    lines.push(
      `✗ ${n}: gone from the files; destroyed only when its name is typed`,
    );
  for (const [n, why] of p.broken) lines.push(`! ${n}: does not build: ${why}`);
  if (p.unchanged.length)
    lines.push(`= ${p.unchanged.length} unchanged: ${p.unchanged.join(", ")}`);
  if (p.ephemeral.length)
    lines.push(
      `· ${p.ephemeral.join(", ")}: ephemeral, deployed by name only, never by apply`,
    );
  return {
    headline: `${p.deploy.length} to deploy · ${p.unchanged.length} unchanged · ${p.destroy.length} gone from the files`,
    lines,
    blocked: p.broken.length
      ? `${p.broken.length} stack(s) do not build; apply refuses until they do (nothing applied).`
      : "",
    pending: p.deploy.length + p.destroy.length > 0,
  };
}

/**
 * The manual checks as the answer form's choices, each with its whole
 * text: the open list wraps a long option (Kenny, 2026-09-29: cut at 120
 * characters, a check could not be told from its neighbour).
 * @param {{id: string, record: {stack: string, app: string, text: string}}[]} checks
 * @returns {{id: string, label: string}[]}
 */
export const checkChoices = (checks) =>
  checks.map((c) => ({
    id: c.id,
    label: `${c.record.stack}/${c.record.app}: ${c.record.text}`,
  }));

/**
 * One line of the shell page: what ran, and the host's answer ("exit N"
 * first, then the output).
 * @param {{state: string, message: string | null}} job
 */
export function shellResult(job) {
  const msg = job.message ?? "";
  const m = /^exit (-?\d+)\n?([\s\S]*)$/.exec(msg);
  if (m)
    return {
      done: true,
      exit: Number(m[1]),
      ok: m[1] === "0",
      output: m[2],
    };
  const done = job.state !== "queued" && job.state !== "running";
  return { done, exit: null, ok: job.state === "done", output: msg };
}

/**
 * The containers the shell can reach: the fleet's stacks, by vmid.
 * @param {{name: string, vmid: number}[]} stacks
 */
export const shellTargets = (stacks) =>
  [...stacks]
    .sort((a, b) => a.vmid - b.vmid)
    .map((s) => ({ value: String(s.vmid), label: `${s.vmid} · ${s.name}` }));
