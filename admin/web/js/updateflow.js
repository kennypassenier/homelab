// redesign-flows-1 (redesign 3.71.0, FLOWS.md §3.1, demo flows/update.html;
// Kenny approved 2026-10-03, decision "one Update flow"): the two update
// paths of 3.70 — a stack's Update action (pull what a moving tag points at
// now) and the Map's stale-image "Update to vX" (move a pinned image) —
// become ONE flow a layman follows: see what has a newer version, see what
// it changes, back up, update, verify, done. This is its pure half: what
// the list holds, what is picked first, the impact in words, the steps the
// run shows and the summary. No DOM, no fetch, no clock.

import { humanDuration } from "./format.js";
import { newerApps, plural } from "./inboxrows.js";
import { releaseUrl } from "./staleimages.js";

/**
 * One thing the flow can update.
 * `pin`: a pinned image a stack file holds (`key`: `<app>/<service>`),
 * moved from `from` to `to` by a commit and a deploy; `pull`: the stack's
 * apps on a moving tag (like `:latest`), pulled and recreated by the
 * stack's Update action, which rolls an app back when it is not healthy.
 * @typedef {{id: string, kind: "pin" | "pull" | "release", stack: string,
 *   container: string, key: string | null, from: string, to: string,
 *   major: boolean, notes: string | null, app?: string | null}} Item
 *   `app` (pull only, redesign-openpoints-2): the one app the pull is for,
 *   when the opener picked an app on a moving tag (a hub app's Update…);
 *   absent: every app of the stack on a moving tag.
 * @typedef {{all: true, only?: string[]} |
 *   {all: false, stack: string, app?: string | null}} Scope
 *   `only`: the stacks a Stacks batch ticked (every app of those stacks
 *   with a newer version; redesign-integrate-5)
 */

/**
 * The flow's address (redesign-flows-11, its own route since the review):
 * `/update?all=1` for every app with a newer version, `/update?stack=…`
 * (and `&app=<key>` with one app picked) for one stack.
 * @param {string | null} [stack] null: everything with a newer version
 * @param {string | null} [app] the stale row's `<app>/<service>` key
 */
export function updateHref(stack = null, app = null) {
  if (!stack) return "/update?all=1";
  const p = new URLSearchParams({ stack });
  if (app) p.set("app", app);
  return `/update?${p}`;
}

/**
 * The flow's address for the stacks a Stacks batch ticked: one stack's own
 * flow, or every newer app of those stacks (redesign-integrate-5).
 * @param {string[]} stacks
 */
export function updateHrefFor(stacks) {
  if (stacks.length === 1) return updateHref(stacks[0]);
  return `/update?${new URLSearchParams({ all: "1", only: stacks.join(",") })}`;
}

/**
 * The flow's scope from `/update`'s query; null when it names none.
 * @param {string} search
 * @returns {Scope | null}
 */
export function scopeOf(search) {
  const q = new URLSearchParams(search);
  const stack = q.get("stack");
  if (stack) return { all: false, stack, app: q.get("app") };
  const only = (q.get("only") ?? "").split(",").filter(Boolean);
  if (q.get("all") != null)
    return only.length ? { all: true, only } : { all: true };
  return null;
}

/**
 * The old address (`/needs-you?update=all`, `/needs-you?update=<stack>&app=…`)
 * sent on to the flow's own; null when the query is not the flow's.
 * @param {string} search
 */
export function legacyUpdateHref(search) {
  const q = new URLSearchParams(search);
  const u = q.get("update");
  if (u == null) return null;
  return u === "" || u === "all" ? updateHref() : updateHref(u, q.get("app"));
}

/**
 * The area the nav marks: Stacks for one stack's update, the Inbox for all.
 * @param {string} search
 */
export const flowArea = (search) => {
  const s = scopeOf(search);
  return s && (!s.all || s.only) ? "overview" : "needs-you";
};

/**
 * What the list holds: the pinned images with a newer version (all of
 * them, or one stack's), and for one stack also its "pull" row.
 * @param {any} staleBody `/data/stale-images`
 * @param {Scope} scope
 * @returns {Item[]}
 */
export function itemsFor(staleBody, scope) {
  // redesign-final-h1: every app with a newer version (`newerApps`, the
  // one source Stacks and the Inbox count from); one that comes with a
  // homelab release is listed as such, never ticked.
  /** @type {Item[]} */
  const out = newerApps(staleBody)
    .filter((a) =>
      scope.all
        ? !scope.only || scope.only.includes(a.stack)
        : a.stack === scope.stack,
    )
    .map((a) => ({
      id: a.release
        ? `release:${a.stack}:${a.container}`
        : `pin:${a.stack}:${a.key}`,
      kind: /** @type {"pin" | "release"} */ (a.release ? "release" : "pin"),
      stack: a.stack,
      container: a.container,
      key: a.key,
      from: a.from,
      to: a.to,
      major: a.major,
      notes: releaseUrl(a.upstream, a.to),
    }))
    // The apps this flow moves first; the release ones after them.
    .sort(
      (x, y) => Number(x.kind === "release") - Number(y.kind === "release"),
    );
  // redesign-openpoints-2: an opener that picked an app no pinned row is
  // (an app on a moving tag) gets the pull row for that app alone. A pin
  // key (`<app>/<service>`) whose newer version is gone is no app name.
  const pullApp =
    !scope.all &&
    scope.app &&
    !scope.app.includes("/") &&
    !out.some(
      (i) =>
        i.key === scope.app ||
        (i.kind === "release" && i.container === scope.app),
    )
      ? scope.app
      : null;
  if (!scope.all)
    out.push({
      id: `pull:${scope.stack}`,
      kind: "pull",
      stack: scope.stack,
      container: pullApp ?? "apps on a moving tag",
      ...(pullApp ? { app: pullApp } : {}),
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
    const one = items.find(
      (i) => i.key === scope.app || (i.kind === "pull" && i.app === scope.app),
    );
    if (one) return new Set([one.id]);
    // redesign-final-h1: the app comes with a homelab release: nothing
    // here moves it, so nothing starts ticked.
    if (items.some((i) => i.kind === "release" && i.container === scope.app))
      return new Set();
  }
  const pins = items.filter((i) => i.kind === "pin");
  return new Set(
    (pins.length ? pins : items.filter((i) => i.kind !== "release")).map(
      (i) => i.id,
    ),
  );
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
  const one =
    scope.app &&
    items.find(
      (i) => i.key === scope.app || (i.kind === "pull" && i.app === scope.app),
    );
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
      i.kind === "pin" || i.app
        ? i.container
        : `${i.stack}'s apps on a moving tag`,
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
  // redesign-flows-6: the job rolls a pinned app back by itself, and the
  // host's own update does it for an app on a moving tag — the demo's
  // words hold for both.
  void chosen;
  return {
    value: "Backup first, auto roll back",
    ctx: "if the app is not healthy within 2 min",
  };
}

/** The undo, as the demo's Undo later tile says it. */
export const undoWords = () => ({
  value: "Roll back, 1 click",
  ctx: "for 7 days, from the stack's History",
});

/**
 * The moves the one server job gets (`POST /data/actions/_host/
 * update-apps`): a pin with the image lines step 2 resolved, a pull with
 * its app when one is named.
 * @param {Item[]} chosen
 * @param {Map<string, import("./pinupdate.js").Move>} moves
 * @returns {{updates: string}}
 */
export function updatesBody(chosen, moves) {
  const out = chosen.map((i) => {
    if (i.kind === "pull")
      return i.app
        ? { stack: i.stack, kind: "pull", app: i.app }
        : { stack: i.stack, kind: "pull" };
    const m = moves.get(i.id);
    return {
      stack: i.stack,
      kind: "pin",
      key: i.key,
      app: i.container,
      from: m?.from,
      to: m?.to,
    };
  });
  return { updates: JSON.stringify(out) };
}

/**
 * The rows the page shows: the job's own (`job.flow.rows`), then the log
 * comparison the page reads itself once the job is past its verify.
 * @param {{rows: {id: string, step: number, title: string, desc: string,
 *   state: string, note?: string, took_s?: number}[]}} flow
 * @param {{state: string, note: string} | null} logs
 * @returns {{id: string, step: number, title: string, desc: string,
 *   state: "wait" | "run" | "ok" | "bad" | "skip", note?: string,
 *   time: string}[]}
 */
export function flowRows(flow, logs) {
  /** @param {string} s */
  const st = (s) =>
    /** @type {"wait" | "run" | "ok" | "bad" | "skip"} */ (
      ["wait", "run", "ok", "bad", "skip"].includes(s) ? s : "wait"
    );
  const rows = flow.rows.map((r) => ({
    id: r.id,
    step: r.step,
    title: r.title,
    desc: r.desc,
    state: st(r.state),
    note: r.note,
    time: r.took_s != null ? humanDuration(r.took_s) : "",
  }));
  rows.push({
    id: "logs",
    step: 5,
    title: "Verify: no new errors in the logs",
    desc: "the first 2 minutes compared with the hour before",
    state: st(logs?.state ?? "wait"),
    note: logs?.note,
    time: "",
  });
  return rows;
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
  return `${parts.join("; ")}. It answered its health check, logs look like before, and the backup from step 3 stays for 7 days.`;
}

/**
 * The version a pinned image line names (`…/demo-api:v3.0.0@sha256:…` →
 * `v3.0.0`), as the host's `pinned_version` reads it.
 * @param {string} line
 */
export function lineVersion(line) {
  const name = line.split("@")[0];
  const last = name.split("/").pop() ?? name;
  const i = last.indexOf(":");
  return i < 0 ? "" : last.slice(i + 1);
}

/**
 * The Roll back… moves of a finished job, from the moves it ran (a reload
 * or another tab has no step-2 state of its own).
 * @param {{stack: string, kind: string, key?: string, from?: string,
 *   to?: string}[]} items the job's `flow.items`
 * @returns {Map<string, import("./pinupdate.js").Move>} by item id
 */
export function movesOf(items) {
  /** @type {Map<string, import("./pinupdate.js").Move>} */
  const out = new Map();
  for (const i of items)
    if (i.kind === "pin" && i.key && i.from && i.to)
      out.set(`pin:${i.stack}:${i.key}`, {
        stack: i.stack,
        key: i.key,
        file: `stacks/${i.stack}/${i.key.split("/")[0]}/docker-compose.yml`,
        from: i.from,
        to: i.to,
        from_version: lineVersion(i.from),
        to_version: lineVersion(i.to),
      });
  return out;
}

/**
 * Whether an update job belongs on this flow's page: every app's page
 * shows any, a stack's page one that touched that stack.
 * @param {{stack: string}[]} items the job's `flow.items`
 * @param {Scope} scope
 */
export const scopeMatches = (items, scope) =>
  scope.all
    ? !scope.only || items.some((i) => scope.only?.includes(i.stack))
    : items.some((i) => i.stack === scope.stack);
