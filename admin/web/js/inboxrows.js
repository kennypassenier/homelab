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
 * @typedef {import("./inbox.js").InboxItem & {kind?: Kind,
 *   chips?: SrcChip[], stacks?: string[], apps?: NewerApp[],
 *   fix?: import("./notices.js").Fix | null, check?: string,
 *   checkState?: "open" | "not ok"}} Row
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
  if (r.chips) return r.chips;
  /** @type {SrcChip[]} */
  const out = [{ label: r.source === "asks" ? "question" : "failure" }];
  if (r.stack)
    out.push({
      label: r.stack,
      href: `/stacks/${encodeURIComponent(r.stack)}`,
    });
  if (r.source === "notices")
    out.push({ label: "in Activity", href: "/activity" });
  return out;
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

/**
 * The rows a set of chips shows: none on means all; "worst first" keeps
 * inbox.js's order, "newest" sorts by time.
 * @param {Row[]} rows
 * @param {Set<string>} kinds
 * @param {"worst" | "newest"} order
 */
export function shownRows(rows, kinds, order) {
  const out = rows.filter((r) => kinds.size === 0 || kinds.has(kindOf(r)));
  return order === "newest" ? [...out].sort((a, b) => b.at - a.at) : out;
}

/**
 * The card's heading: "5 things need you · 1 is urgent".
 * @param {Row[]} rows
 */
export function headline(rows) {
  const urgent = rows.filter((r) => r.severity === "bad").length;
  const n = rows.length;
  return `${n} ${n === 1 ? "thing needs" : "things need"} you${urgent ? ` · ${urgent} ${urgent === 1 ? "is" : "are"} urgent` : ""}`;
}
