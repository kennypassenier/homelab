// redesign-flows-1 (redesign 3.71.0, FLOWS.md §3.1, demo flows/update.html;
// Kenny approved 2026-10-03, decision "one Update flow"): the two update
// paths of 3.70 — a stack's Update action (pull what a moving tag points at
// now) and the Map's stale-image "Update to vX" (move a pinned image) —
// become ONE flow a layman follows: see what has a newer version, see what
// it changes, back up, update, verify, done. This is its pure half: what
// the list holds, what is picked first, the impact in words, the steps the
// run shows and the summary. No DOM, no fetch, no clock.

import { plural, staleApps } from "./inboxrows.js";
import { releaseUrl } from "./staleimages.js";

/**
 * One thing the flow can update.
 * `pin`: a pinned image a stack file holds (`key`: `<app>/<service>`),
 * moved from `from` to `to` by a commit and a deploy; `pull`: the stack's
 * apps on a moving tag (like `:latest`), pulled and recreated by the
 * stack's Update action, which rolls an app back when it is not healthy.
 * @typedef {{id: string, kind: "pin" | "pull", stack: string,
 *   container: string, key: string | null, from: string, to: string,
 *   major: boolean, notes: string | null}} Item
 * @typedef {{all: true} | {all: false, stack: string, app?: string | null}} Scope
 */

/**
 * The flow's address: every app with a newer version, or one stack (and
 * one app of it picked).
 * @param {string | null} [stack] null: everything with a newer version
 * @param {string | null} [app] the stale row's `<app>/<service>` key
 */
export function updateHref(stack = null, app = null) {
  const p = new URLSearchParams({ update: stack ?? "all" });
  if (stack && app) p.set("app", app);
  return `/inbox?${p}`;
}

/**
 * The flow's scope from the Inbox page's query (`?update=all`,
 * `?update=<stack>&app=<key>`); null when the address is not the flow.
 * @param {string} search
 * @returns {Scope | null}
 */
export function scopeOf(search) {
  const q = new URLSearchParams(search);
  const u = q.get("update");
  if (u == null) return null;
  if (u === "" || u === "all") return { all: true };
  return { all: false, stack: u, app: q.get("app") };
}

/**
 * What the list holds: the pinned images with a newer version (all of
 * them, or one stack's), and for one stack also its "pull" row.
 * @param {any} staleBody `/data/stale-images`
 * @param {Scope} scope
 * @returns {Item[]}
 */
export function itemsFor(staleBody, scope) {
  /** @type {Item[]} */
  const out = staleApps(staleBody)
    .filter((a) => scope.all || a.stack === scope.stack)
    .map((a) => ({
      id: `pin:${a.stack}:${a.key}`,
      kind: "pin",
      stack: a.stack,
      container: a.container,
      key: a.key,
      from: a.from,
      to: a.to,
      major: a.major,
      notes: releaseUrl(a.upstream, a.to),
    }));
  if (!scope.all)
    out.push({
      id: `pull:${scope.stack}`,
      kind: "pull",
      stack: scope.stack,
      container: "apps on a moving tag",
      key: null,
      from: "",
      to: "",
      major: false,
      notes: null,
    });
  return out;
}

/**
 * What starts ticked: every pinned move; the pull row only when nothing
 * pinned is newer; one app only when the opener picked it.
 * @param {Item[]} items
 * @param {Scope} scope
 * @returns {Set<string>}
 */
export function firstChosen(items, scope) {
  if (!scope.all && scope.app) {
    const one = items.find((i) => i.key === scope.app);
    if (one) return new Set([one.id]);
  }
  const pins = items.filter((i) => i.kind === "pin");
  return new Set((pins.length ? pins : items).map((i) => i.id));
}

/**
 * The page's title: "Update 3 apps", "Update kp-soft/demo-agent",
 * "Update kp-soft".
 * @param {Item[]} items
 * @param {Scope} scope
 */
export function flowTitle(items, scope) {
  const pins = items.filter((i) => i.kind === "pin");
  if (scope.all) return `Update ${plural(pins.length, "app")}`;
  const one = scope.app && pins.find((i) => i.key === scope.app);
  if (one) return `Update ${one.stack}/${one.container}`;
  return `Update ${scope.stack}`;
}

/**
 * One row's version words: "0.1.4 → 0.2.0", or what a pull does.
 * @param {Item} i
 */
export const versionWords = (i) =>
  i.kind === "pin" ? `${i.from} → ${i.to}` : "whatever its tags point at now";

/**
 * The stacks the chosen items touch, in order.
 * @param {Item[]} chosen
 */
export const stacksOf = (chosen) => [...new Set(chosen.map((i) => i.stack))];

/**
 * The apps that restart: each chosen pin's own, and "every app on a
 * moving tag" for a pull.
 * @param {Item[]} chosen
 */
export function restartWords(chosen) {
  return chosen
    .map((i) =>
      i.kind === "pin" ? i.container : `${i.stack}'s apps on a moving tag`,
    )
    .join(", ");
}

/**
 * The downtime estimate from the stacks' last deploy or update (finished,
 * done), longest of them; null when none ran yet.
 * @param {Item[]} chosen
 * @param {import("./jobs.js").Job[]} jobs
 * @returns {number | null} seconds
 */
export function lastDeployS(chosen, jobs) {
  /** @type {number[]} */
  const took = [];
  for (const stack of stacksOf(chosen)) {
    const last = jobs
      .filter(
        (j) =>
          j.stack === stack &&
          j.state === "done" &&
          (j.action === "deploy" || j.action === "update") &&
          j.started_at != null &&
          j.finished_at != null,
      )
      .sort((a, b) => (b.finished_at ?? 0) - (a.finished_at ?? 0))[0];
    if (last)
      took.push(
        /** @type {number} */ (last.finished_at) -
          /** @type {number} */ (last.started_at),
      );
  }
  return took.length ? Math.max(...took) : null;
}

/**
 * Who notices (from the fleet map's dependencies): every stack that
 * reaches a chosen stack, and the app it reaches.
 * @param {Item[]} chosen
 * @param {{stack: string, depended_on_by: string[]}[]} deps
 * @returns {{who: string, what: string}[]}
 */
export function whoNotices(chosen, deps) {
  /** @type {{who: string, what: string}[]} */
  const out = [];
  for (const i of chosen) {
    const d = deps.find((x) => x.stack === i.stack);
    for (const who of d?.depended_on_by ?? [])
      if (who !== i.stack)
        out.push({
          who,
          what:
            i.kind === "pin"
              ? `${i.stack}/${i.container}`
              : `${i.stack} (its apps on a moving tag)`,
        });
  }
  const seen = new Set();
  return out.filter((x) => {
    const k = `${x.who}>${x.what}`;
    if (seen.has(k)) return false;
    seen.add(k);
    return true;
  });
}

/**
 * The safety net in words: a pull rolls an unhealthy app back by itself
 * (core::ops::update, 2 min); a pinned move's deploy checks the health,
 * and Roll back on the result puts the old version back.
 * @param {Item[]} chosen
 */
export function safetyNet(chosen) {
  const pins = chosen.some((i) => i.kind === "pin");
  const pulls = chosen.some((i) => i.kind === "pull");
  if (pulls && !pins)
    return {
      value: "Backup first, auto roll back",
      ctx: "if the app is not healthy within 2 min",
    };
  return {
    value: "Backup first, health checked",
    ctx: pulls
      ? "a pinned app's deploy checks its health; a moving-tag app rolls back by itself within 2 min"
      : "the deploy waits for the app's health check; Roll back puts the old version back",
  };
}

/**
 * The commit's subject for one stack's moves.
 * @param {Item[]} pins one stack's chosen pins
 */
export function commitSubject(pins) {
  return `Update ${pins.map((i) => `${i.key} ${i.from} → ${i.to}`).join(", ")} (Update flow)`;
}

/**
 * The run's rows (flows/update.html steps 3-5), each with the step it
 * belongs to.
 * @typedef {{id: "backup" | "commit" | "deploy" | "health" | "logs",
 *   step: 3 | 4 | 5, title: string, desc: string}} RunRow
 * @param {Item[]} chosen
 * @returns {RunRow[]}
 */
export function runRows(chosen) {
  const stacks = stacksOf(chosen);
  const pins = chosen.filter((i) => i.kind === "pin");
  /** @type {RunRow[]} */
  const rows = [
    {
      id: "backup",
      step: 3,
      title: `Back up ${stacks.join(", ")}`,
      desc: "a restic snapshot of each app's data; if it fails, nothing else happens",
    },
  ];
  if (pins.length)
    rows.push({
      id: "commit",
      step: 4,
      title: "Change the files and commit",
      desc: "the image line moves to the new version, pushed to the repository",
    });
  rows.push(
    {
      id: "deploy",
      step: 4,
      title: "Deploy: pull the image and restart",
      desc: `${restartWords(chosen)} restart; nothing else`,
    },
    {
      id: "health",
      step: 5,
      title: "Verify: the app is healthy",
      desc: "the container runs and its health check answered",
    },
    {
      id: "logs",
      step: 5,
      title: "Verify: no new errors in the logs",
      desc: "the minutes since the restart compared with the hour before",
    },
  );
  return rows;
}

/**
 * The log check's verdict: errors since the restart against the hour
 * before, per minute. Fewer or as many per minute as before is fine.
 * @param {{ts_ms: number, level: string}[]} lines
 * @param {number} sinceS when the restart began, unix seconds
 * @param {number} nowS
 * @returns {{ok: boolean, after: number, before: number, words: string}}
 */
export function logVerdict(lines, sinceS, nowS) {
  const err = (/** @type {string} */ l) =>
    ["error", "err", "critical", "crit", "fatal", "emerg", "alert"].includes(
      String(l).trim().toLowerCase(),
    );
  let after = 0;
  let before = 0;
  for (const l of lines) {
    if (!err(l.level)) continue;
    const s = l.ts_ms / 1000;
    if (s >= sinceS) after += 1;
    else if (s >= sinceS - 3600) before += 1;
  }
  const minsAfter = Math.max(1, (nowS - sinceS) / 60);
  const ok = after === 0 || after / minsAfter <= before / 60;
  return {
    ok,
    after,
    before,
    words: `${plural(after, "error")} since the restart, ${before} in the hour before`,
  };
}

/**
 * The result's sentence (step 6).
 * @param {Item[]} chosen
 * @param {{failed: string | null}} r
 */
export function doneWords(chosen, r) {
  if (r.failed) return r.failed;
  const pins = chosen.filter((i) => i.kind === "pin");
  const parts = pins.map((i) => `${i.stack}/${i.container} runs ${i.to}`);
  for (const i of chosen.filter((x) => x.kind === "pull"))
    parts.push(
      `${i.stack}'s apps on a moving tag run what their tags point at now`,
    );
  return `${parts.join("; ")}. It answered its health check, the logs look like before, and the backup from step 3 holds the data as it was.`;
}
