// redesign-flows-2 (redesign 3.71.0, demos flows/inbox.html, health.html
// and notifications.html; Kenny approved 2026-10-03): the Inbox's sources
// the foundation did not wire yet, as plain data — the apps with a newer
// version (the fleet check's stale images), the stacks whose secrets have
// no copy in the host's vault (setup), the manual checks a person answers,
// and Today's broken / attention items — plus the "worth a look" rows that
// are not counted. Every row says what, why, and carries what its fix
// button needs; the Inbox page draws them and inboxsources.js feeds them in
// with `setInboxSource`. No DOM, no fetch, no clock: node tests hold it.

import { askView } from "./asks.js";
import { checkAnswer } from "./checks.js";
import { drillState } from "./backupsview.js";
import { versionNotes } from "./parity.js";
import { majorJump } from "./staleimages.js";
import { formatDateTime } from "./format.js";

/**
 * Which filter chip a row answers to (flows/inbox.html "Show").
 * @typedef {"ask" | "fail" | "update" | "setup" | "check"} Kind
 * @typedef {{label: string, href?: string}} SrcChip
 * @typedef {{stack: string, container: string, key: string, from: string,
 *   to: string, upstream: string, major: boolean}} StaleApp
 *   one app the Update flow can move (`key`: `<app>/<service>` of its file)
 * @typedef {Omit<StaleApp, "key"> & {key: string | null, release: boolean}}
 *   NewerApp one app with a newer version (redesign-final-h1): `release`
 *   when it comes with a homelab release (no key)
 * @typedef {import("./inboxlist.js").InboxItem & {kind?: Kind,
 *   chips?: SrcChip[], stacks?: string[], apps?: NewerApp[],
 *   fix?: import("./notices.js").Fix | null, check?: string,
 *   checkState?: "open" | "not ok", checkText?: string,
 *   previous?: "ok" | "fail" | null, job?: number, remedy?: string}} Row
 */

/** The chips' order and words (flows/inbox.html). */
export const KINDS = /** @type {const} */ ([
  { value: "ask", label: "Questions" },
  { value: "fail", label: "Failures" },
  { value: "update", label: "Updates" },
  { value: "setup", label: "Setup" },
  { value: "check", label: "Checks" },
]);

/**
 * A row's kind: what its source says, or for the foundation's own rows
 * (the host's questions, the notices) what they are.
 * @param {Row} r
 * @returns {Kind}
 */
export function kindOf(r) {
  if (r.kind) return r.kind;
  if (r.source === "asks") return "ask";
  return "fail";
}

/**
 * redesign-final-h1: THE apps with a newer version, every place's one
 * source (Stacks' flag, the hub, the Inbox's Updates row, the Update
 * flow): each stale-image row, a pin a stack file holds (`key`, the Update
 * flow moves it) or one that comes with a homelab release (`release`: no
 * key, it lives in the homelab binary).
 * @param {{images?: {where_: string, pinned: string, latest: string,
 *   upstream: string, key: string | null}[]} | null | undefined} body
 * @returns {NewerApp[]}
 */
export function newerApps(body) {
  return (body?.images ?? [])
    .map((x) => {
      const [stack, ...rest] = String(x.where_).split("/");
      return {
        stack,
        container: rest.join("/"),
        key: x.key ?? null,
        release: !x.key,
        from: x.pinned,
        to: x.latest,
        upstream: x.upstream,
        major: majorJump(x.pinned, x.latest),
      };
    })
    .sort(
      (a, b) =>
        a.stack.localeCompare(b.stack) ||
        a.container.localeCompare(b.container),
    );
}

/**
 * The stale-image rows the Update flow can move: a pin a stack file holds
 * (`key`), with its version jump (`newerApps` without the ones that come
 * with a homelab release).
 * @param {{images?: {where_: string, pinned: string, latest: string,
 *   upstream: string, key: string | null}[]} | null | undefined} body
 * @returns {StaleApp[]}
 */
export function staleApps(body) {
  return (body?.images ?? [])
    .filter((x) => x.key)
    .map((x) => {
      const [stack, ...rest] = String(x.where_).split("/");
      return {
        stack,
        container: rest.join("/"),
        key: /** @type {string} */ (x.key),
        from: x.pinned,
        to: x.latest,
        upstream: x.upstream,
        major: majorJump(x.pinned, x.latest),
      };
    })
    .sort(
      (a, b) =>
        a.stack.localeCompare(b.stack) ||
        a.container.localeCompare(b.container),
    );
}

/** @param {number} n @param {string} one @param {string} [many] */
export const plural = (n, one, many = `${one}s`) =>
  `${n} ${n === 1 ? one : many}`;

/**
 * The Updates row: ONE row however many apps (flows/inbox.html "3 apps
 * have a newer version"), or none.
 * @param {{images?: any[], measured_at?: number | null} | null | undefined} body
 * @returns {Row[]}
 */
export function updateRows(body) {
  // redesign-final-h1: every app with a newer version, the ones that come
  // with a homelab release included (said so), as Stacks counts them.
  const apps = newerApps(body);
  if (!apps.length) return [];
  const at = Number(body?.measured_at) || 0;
  const stacks = [...new Set(apps.map((a) => a.stack))];
  return [
    {
      key: "updates:stale-images",
      severity: "warn",
      kind: "update",
      title: `${plural(apps.length, "app has", "apps have")} a newer version`,
      why: `${apps
        .map(
          (a) =>
            `${a.stack}/${a.container} ${a.from} → ${a.to}${a.major ? " (major)" : ""}${a.release ? " (with a homelab release)" : ""}`,
        )
        .join(
          " · ",
        )}. Each is backed up first; Roll back puts the old version back.`,
      href: "/inbox?update=all",
      stack: stacks.length === 1 ? stacks[0] : null,
      at,
      source: "updates",
      chips: [{ label: "update" }, { label: "from the fleet check" }],
      stacks,
      apps,
    },
  ];
}

/**
 * The Setup row: the stacks whose secret files have no copy in the host's
 * vault ([NOENV]); a deploy of each gives the vault its copy (the doctor's
 * own remedy). One row for all of them, or none.
 * @param {{stacks?: {name: string, env_sealed?: boolean | null}[]} | null | undefined} fleet
 * @returns {Row[]}
 */
export function setupRows(fleet) {
  const names = (fleet?.stacks ?? [])
    .filter((s) => s.env_sealed === false)
    .map((s) => s.name);
  if (!names.length) return [];
  return [
    {
      key: "setup:noenv",
      severity: "warn",
      kind: "setup",
      title: `${plural(names.length, "stack has", "stacks have")} no sealed env on the host`,
      why: "Running containers are fine, but the host's vault holds no copy of their secrets, so a lost container could not get them back. A deploy of each stack gives the vault its copy.",
      href: "/inbox#setup-noenv",
      stack: names.length === 1 ? names[0] : null,
      at: 0,
      source: "setup",
      chips: [{ label: "setup" }, { label: names.join(", ") }],
      stacks: names,
    },
  ];
}

/**
 * One row per manual check that waits for a person: never answered
 * ("open") or answered "fails" (and not accepted for now).
 * @param {{checks?: {id: string, record: any}[], now?: number} | null | undefined} report
 * @returns {Row[]}
 */
export function checkRowsOf(report) {
  const now = report?.now ?? 0;
  /** @type {Row[]} */
  const out = [];
  for (const c of report?.checks ?? []) {
    const r = c.record ?? {};
    const a = checkAnswer(r, now).label;
    if (a !== "open" && a !== "not ok") continue;
    out.push({
      key: `check:${c.id}`,
      severity: a === "not ok" ? "bad" : "warn",
      kind: "check",
      title: a === "not ok" ? `A check fails: ${r.text}` : `Check: ${r.text}`,
      why:
        a === "not ok"
          ? "Answered “it fails”. Fix the cause, then answer it again; “Not now” asks again in 7 days."
          : "Only a person can confirm this one. Answer it here; “Not now” asks again in 7 days.",
      href: `/inbox#check-${c.id}`,
      stack: r.stack ?? null,
      at: Number(r.answered_at ?? r.registered_at) || 0,
      source: "checks",
      chips: [
        { label: "check" },
        ...(r.stack
          ? [{ label: r.stack, href: `/stacks/${encodeURIComponent(r.stack)}` }]
          : []),
      ],
      check: c.id,
      checkState: a,
      checkText: String(r.text ?? ""),
      // kp11: "Same as last time" is offered only for a check that was
      // answered "fails" before; a never-answered one has no last time.
      previous: a === "not ok" ? "fail" : null,
    });
  }
  return out;
}

/**
 * Today's broken and attention items (the doctor, the fleet check's
 * findings, the open incidents), each a row with its remedy and, when the
 * dashboard runs it, its Fix. A doctor's "stack X env" item is left out
 * for a stack the Setup row already names.
 * @param {{today?: {items?: {level: string, source: string, what: string,
 *   remedy: string, fix?: import("./notices.js").Fix}[]},
 *   measured_at?: number} | null | undefined} body
 * @param {Iterable<string>} [setupStacks]
 * @returns {Row[]}
 */
export function todayRows(body, setupStacks = []) {
  const skip = new Set(setupStacks);
  const at = Number(body?.measured_at) || 0;
  /** @type {Row[]} */
  const out = [];
  for (const i of body?.today?.items ?? []) {
    const env = /^stack (\S+) env:/.exec(i.what);
    if (env && skip.has(env[1])) continue;
    out.push({
      key: `today:${i.source}:${i.what}`,
      severity: i.level === "Broken" ? "bad" : "warn",
      kind: i.source === "incident" ? "fail" : "check",
      title: i.what,
      why: `What to do: ${i.remedy}`,
      href: "/inbox#today",
      stack: i.fix?.stack ?? null,
      at,
      source: "today",
      chips: [
        { label: i.source === "incident" ? "failure" : "check" },
        {
          label:
            i.source === "doctor"
              ? "from the host's doctor"
              : i.source === "incident"
                ? "an open incident"
                : i.source === "check"
                  ? "from the fleet check"
                  : `from ${i.source}`,
        },
        ...(i.fix?.stack && i.fix.stack !== "_host"
          ? [
              {
                label: i.fix.stack,
                href: `/stacks/${encodeURIComponent(i.fix.stack)}`,
              },
            ]
          : []),
      ],
      fix: i.fix ?? null,
      remedy: i.remedy,
    });
  }
  return out;
}

/**
 * The source chips of a foundation row (a host question, a notice).
 * @param {Row} r
 * @returns {SrcChip[]}
 */
export function chipsOf(r) {
  // fix-inbox-group-1 (Kenny, 2026-10-09: "er staat geen datum van de
  // 'storing' bij"): every row says when, and how often when it stands for
  // several notices of the same thing.
  /** @type {SrcChip[]} */
  const when = [];
  if (r.at > 0) {
    const first = r.first_at ?? r.at;
    when.push({
      label:
        (r.count ?? 1) > 1
          ? `${r.count}× since ${formatDateTime(first)}, last ${formatDateTime(r.at)}`
          : formatDateTime(r.at),
    });
  }
  if (r.chips) return [...r.chips, ...when];
  /** @type {SrcChip[]} */
  const out = [{ label: r.source === "asks" ? "question" : "failure" }];
  if (r.stack)
    out.push({
      label: r.stack,
      href: `/stacks/${encodeURIComponent(r.stack)}`,
    });
  if (r.source === "notices")
    out.push({ label: "in Activity", href: "/activity" });
  return [...out, ...when];
}

/**
 * One host question as an Inbox row's words: what it asks, and what
 * happens when nobody answers in time (flows/inbox.html).
 * @param {import("./asks.js").Ask} a
 * @param {number} now
 */
export function askWords(a, now) {
  const v = askView(a, now);
  return {
    title: `The host asks: ${a.what}`,
    why: `${a.op} is waiting at “${a.step}”. If nobody answers in ${v.left.replace(/ left to answer$/, "")}, the host ${a.if_stopped || "stops there"} (the safe choice).`,
    left: v.left,
    urgent: v.urgent,
  };
}

/**
 * The rows that are not counted in the badge (flows/inbox.html "Worth a
 * look"): repositories never restore-drilled, and a newer host release.
 * @param {{repos?: any[]}[]} backups every stack's `/data/backups/<stack>`
 * @param {any} versions `/data/versions`
 * @returns {Row[]}
 */
export function worthRows(backups, versions) {
  /** @type {Row[]} */
  const out = [];
  const repos = backups.flatMap((b) => b?.repos ?? []);
  const never = repos.filter((r) => drillState(r) === "never").length;
  if (repos.length && never)
    out.push({
      key: "worth:drills",
      severity: "info",
      title: `${never} of ${plural(repos.length, "backup repository", "backup repositories")} ${never === 1 ? "was" : "were"} never restore-drilled`,
      why: "A drill restores the newest snapshot into a scratch folder and compares it; nothing live is touched.",
      href: "/backups?undrilled=1",
      stack: null,
      at: 0,
      source: "worth",
    });
  const n = versionNotes(versions);
  if (n.update)
    out.push({
      key: "worth:host-release",
      severity: "info",
      title: `Host ${versions.latest} is available`,
      why: `You run ${versions.host}. The host updates itself and rolls back if it does not come up.`,
      href: "/host",
      stack: null,
      at: 0,
      source: "worth",
    });
  return out;
}

// ---- check-row-answer-1: every row points at its fix --------------------

/**
 * One way a row leads to where its fix is given.
 * @typedef {{type: "answer-ask"} |
 *   {type: "update-flow", stacks: string[], n: number} |
 *   {type: "push-envs", stacks: string[]} |
 *   {type: "answer-check", check: string, verdict: "ok" | "nok",
 *     label: string} |
 *   {type: "open-check", check: string} |
 *   {type: "open-job", job: number} |
 *   {type: "run-fix", fix: import("./notices.js").Fix, retry: boolean} |
 *   {type: "link", href: string, label: string, why: "happened" | "remedy"} |
 *   {type: "none", note: string}} RowFix
 */

/** The pages a remedy's words can name, and where they live. */
const REMEDY_PAGES = /** @type {const} */ ([
  [/\bbackups?\b/i, "/backups", "Open Backups"],
  [/\bfirewall\b/i, "/firewall", "Open the Firewall"],
  [/\bstorage\b/i, "/storage", "Open Storage"],
  [/\bactivity\b/i, "/activity", "Open Activity"],
  [/\bsettings\b/i, "/settings", "Open Settings"],
  [/\bhost page\b|\bsystem\b/i, "/system", "Open System"],
  [/\bmap\b/i, "/map", "Open the Map"],
  [/\bmetrics\b|\bcharts?\b/i, "/charts", "Open Metrics"],
]);

/** @param {string} s */
const sentenceCase = (s) => (s ? s[0].toUpperCase() + s.slice(1) : s);

/**
 * What a remedy's words point at: `homelab <action> <stack>` becomes that
 * action's own dialog when the dashboard has the action; "…the Backups
 * page" and the like become a link to the page. Null when it names neither.
 * @param {string} remedy
 * @param {(action: string) => boolean} known whether the dashboard has this action
 * @param {string | null} [stack] the row's own stack, for a command that names none
 * @returns {RowFix | null}
 */
export function remedyTarget(remedy, known, stack = null) {
  const cmd = /homelab\s+([a-z][a-z-]*)(?:\s+([a-z0-9][a-z0-9_-]*))?/i.exec(
    remedy,
  );
  if (cmd && known(cmd[1].toLowerCase())) {
    const target = cmd[2] && !cmd[2].startsWith("-") ? cmd[2] : (stack ?? "");
    if (target)
      return {
        type: "run-fix",
        retry: false,
        fix: {
          action: cmd[1].toLowerCase(),
          stack: target,
          label: sentenceCase(cmd[1].toLowerCase().replace(/-/g, " ")),
        },
      };
  }
  const page = /\b([a-z]+)\s+(?:page|tab)\b/i.exec(remedy);
  const word = page ? page[0] : remedy;
  for (const [re, href, label] of REMEDY_PAGES)
    if (re.test(word) && (page || /open|go to|see|look at/i.test(remedy)))
      return { type: "link", href, label, why: "remedy" };
  return null;
}

/**
 * Every Inbox row's way to its fix (Kenny, 2026-10-23: "ik moet nooit
 * raden naar iets als we zelf kunnen wijzen naar de oplossing"): a host
 * question to its card, an update to the Update flow, a manual check to
 * It works / It fails on the row itself with a link into the Manual checks
 * card, a failed job to its job and the action that runs it again, a
 * finding to the action or page its remedy names. The first entry is the
 * row's main button. A row with no known fix says so (`none`), never offers
 * only Snooze.
 * @param {Row} r
 * @param {{notice?: import("./notices.js").Notice | null,
 *   job?: {action: string, stack: string, state: string,
 *     args?: Record<string, unknown>} | null,
 *   known?: (action: string) => boolean}} [ctx]
 * @returns {RowFix[]}
 */
export function rowFixes(r, ctx = {}) {
  const known = ctx.known ?? (() => true);
  const kind = kindOf(r);
  if (r.source === "asks") return [{ type: "answer-ask" }];
  if (kind === "update")
    return [
      {
        type: "update-flow",
        stacks: r.stacks ?? [],
        n: r.apps?.length ?? 0,
      },
    ];
  if (kind === "setup") return [{ type: "push-envs", stacks: r.stacks ?? [] }];
  if (r.check)
    return [
      {
        type: "answer-check",
        check: r.check,
        verdict: "ok",
        label: "It works",
      },
      {
        type: "answer-check",
        check: r.check,
        verdict: "nok",
        label: "It fails",
      },
      { type: "open-check", check: r.check },
    ];
  /** @type {RowFix[]} */
  const out = [];
  const job = r.job ?? ctx.notice?.job;
  if (job != null) out.push({ type: "open-job", job });
  const given = r.fix ?? ctx.notice?.fixes?.[0] ?? null;
  if (given) out.push({ type: "run-fix", fix: given, retry: false });
  else if (
    job != null &&
    ctx.job &&
    known(ctx.job.action) &&
    ctx.job.stack &&
    !["done", "queued", "running"].includes(ctx.job.state)
  )
    // A failed job's own action, with its own arguments, run again.
    out.push({
      type: "run-fix",
      retry: true,
      fix: {
        action: ctx.job.action,
        stack: ctx.job.stack,
        args: Object.fromEntries(
          Object.entries(ctx.job.args ?? {})
            .filter(([, v]) => v != null && v !== "" && v !== false)
            .map(([k, v]) => [k, String(v)]),
        ),
        label: "Run it again",
      },
    });
  else {
    const remedy = r.remedy ?? r.problem?.remedy ?? "";
    const t = remedy ? remedyTarget(remedy, known, r.stack) : null;
    if (t) out.push(t);
  }
  if (r.source === "notices" && r.href && job == null)
    out.push({
      type: "link",
      href: r.href,
      label: "See what happened",
      why: "happened",
    });
  if (!out.length)
    out.push({
      type: "none",
      note: "No fix to point to yet: what to do is written under “What went wrong”, or snooze it.",
    });
  return out;
}

/**
 * inbox-resolved-clears-1: the one count of the Inbox with its split shown:
 * everything waiting (the number on the bar and on the Stacks tile), and
 * how much of it is something to deal with and how much is an update.
 * @param {Row[]} rows
 * @returns {{total: number, act: number, updates: number, text: string}}
 */
export function countSplit(rows) {
  const updates = rows.filter((r) => kindOf(r) === "update").length;
  const act = rows.length - updates;
  const parts = [];
  if (act) parts.push(`${act} to deal with`);
  if (updates) parts.push(plural(updates, "update"));
  return { total: rows.length, act, updates, text: parts.join(" · ") };
}

/**
 * The rows a set of chips shows: none on means all; the order is
 * inboxlist.js's (worst first, newest first within a weight).
 * @param {Row[]} rows
 * @param {Set<string>} kinds
 */
export function shownRows(rows, kinds) {
  return rows.filter((r) => kinds.size === 0 || kinds.has(kindOf(r)));
}

/** The list's group headings, in the order they stand (the approved demo). */
export const GROUPS = /** @type {const} */ ([
  "Urgent",
  "Updates",
  "Setup",
  "Checks",
]);

/**
 * The heading a row stands under: the host's questions and the failures
 * are Urgent, the rest by what they are.
 * @param {Row} r
 * @returns {(typeof GROUPS)[number]}
 */
export function groupOf(r) {
  const k = kindOf(r);
  return k === "update"
    ? "Updates"
    : k === "setup"
      ? "Setup"
      : k === "check"
        ? "Checks"
        : "Urgent";
}

/**
 * The rows under their headings, headings in GROUPS' order, a heading with
 * no row left out; a row keeps its place within its heading.
 * @param {Row[]} rows
 * @returns {{name: (typeof GROUPS)[number], rows: Row[]}[]}
 */
export function groupRows(rows) {
  return GROUPS.map((name) => ({
    name,
    rows: rows.filter((r) => groupOf(r) === name),
  })).filter((g) => g.rows.length > 0);
}

/**
 * kp11: a failure's "What went wrong" fold, drawn from what the notice
 * carries: what happened, why it matters and what to do. Null when the
 * notice has no remedy to show.
 * @param {Row} r
 * @returns {{key: string, what: string, why?: string, remedy: string, severity: "critical" | "warning"} | null}
 */
export function problemOf(r) {
  const p = r.problem;
  if (!p?.remedy) return null;
  return {
    key: r.key,
    what: p.what,
    ...(p.why ? { why: p.why } : {}),
    remedy: p.remedy,
    severity: r.severity === "bad" ? "critical" : "warning",
  };
}

/**
 * The exact count the card's timestamp shows.
 * @param {number} shown
 * @param {number} all
 */
export function countWords(shown, all) {
  return shown === all
    ? `${plural(all, "item")} waiting`
    : `${shown} of ${plural(all, "item")} shown`;
}

// ---- kp11: the host's questions as question cards -----------------------

/**
 * The key a question has in the Inbox (`ask:` + this is its row's key).
 * @param {import("./asks.js").Ask} a
 */
export const askKey = (a) => `${a.boot ?? ""}:${a.id}`;

/**
 * The id a question card gets: a CSS-safe word, one per question.
 * @param {import("./asks.js").Ask} a
 */
export const askId = (a) =>
  `${a.boot || "boot"}-${a.id}`.replace(/[^a-z0-9-]/gi, "-");

/**
 * One sentence from a clause the host sends ("the deploy goes on"): a
 * capital letter and a full stop.
 * @param {string} clause
 * @param {string} fallback
 */
function sentence(clause, fallback) {
  const t = (clause || fallback).trim();
  const up = t.charAt(0).toUpperCase() + t.slice(1);
  return /[.!?]$/.test(up) ? up : `${up}.`;
}

/**
 * A host question as kp-themes' question card reads it: what it asks, what
 * each answer sets in motion, and the safe answer when nobody answers.
 * @param {import("./asks.js").Ask} a
 * @returns {import("/static/kp/js/question-card.js").QuestionSpec}
 */
export function askSpec(a) {
  const allow = sentence(a.if_allowed, "the operation goes on");
  const stop = sentence(a.if_stopped, "the operation stops here");
  return {
    id: askId(a),
    question: `${a.op} is waiting at “${a.step}”`,
    text: a.what,
    deadline: a.deadline * 1000,
    asked: a.asked_at * 1000,
    countdown: "ring",
    defaultAnswer: "stop",
    answers: [
      {
        value: "allow",
        label: "Allow",
        tone: "primary",
        consequence: allow,
        outcome: `Allowed. ${allow}`,
      },
      {
        value: "stop",
        label: "Leave it stopped",
        consequence: stop,
        outcome: `Stopped. ${stop}`,
      },
    ],
  };
}

// ---- kp11: the notices of the last day, to tick and act on together -----

/** How many notices the bulk card lists; the rest are offered as "match". */
export const BULK_SHOWN = 10;

/**
 * The unread notices of the last 24 hours, newest first: the first
 * `BULK_SHOWN` to list, how many match in all, and every id.
 * @param {import("./notices.js").Notice[]} notices
 * @param {number} now unix seconds
 */
export function bulkRows(notices, now) {
  const all = notices
    .filter((n) => !n.read && n.at >= now - 86400)
    .sort((a, b) => b.at - a.at || b.id - a.id);
  return {
    rows: all.slice(0, BULK_SHOWN),
    matching: all.length,
    ids: all.map((n) => n.id),
  };
}

/**
 * Why "Fix all" is off for these notices, or "" when it is on: one batch
 * runs one action on several stacks, so every picked notice needs a fix,
 * the same action, no choices of its own, and a stack.
 * @param {number[]} ids
 * @param {Map<number, import("./notices.js").Notice>} byId
 */
export function fixAllRule(ids, byId) {
  const picked = ids.flatMap((id) => byId.get(id) ?? []);
  const fixes = picked.flatMap((n) => n.fixes?.[0] ?? []);
  if (fixes.length === 0) return "None of these can be fixed";
  if (fixes.length < picked.length)
    return `${fixes.length} of ${picked.length} can be fixed`;
  if (fixes.some((f) => f.action !== fixes[0].action))
    return "These need different fixes: fix them one by one";
  if (
    fixes.some((f) => f.stack === "_host" || Object.keys(f.args ?? {}).length)
  )
    return "Their fixes carry their own choices: fix them from the list";
  return "";
}

/**
 * The action and the stacks a "Fix all" runs as one batch (call after
 * `fixAllRule` said "").
 * @param {number[]} ids
 * @param {Map<number, import("./notices.js").Notice>} byId
 * @returns {{action: string, stacks: string[]} | null}
 */
export function fixAllBatch(ids, byId) {
  const fixes = ids.flatMap((id) => byId.get(id)?.fixes?.[0] ?? []);
  if (!fixes.length) return null;
  return {
    action: fixes[0].action,
    stacks: [...new Set(fixes.map((f) => f.stack))],
  };
}
