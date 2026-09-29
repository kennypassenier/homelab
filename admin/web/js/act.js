// Milestone act's half of the store (arch-frontend): the action catalog,
// the jobs with their progress and log lines, the batches, the notices and
// the schedules, read once and kept current by the live channel. Parts of
// the page listen to the part they draw, so a log line redraws only the
// log it belongs to.

import { fetchJson } from "./dom.js";
import { routeError } from "./doctor.js";
import { addLog, applyProgress, upsertJob } from "./jobs.js";
import { addNotice } from "./notices.js";
import { listen } from "./store.js";

/**
 * @typedef {"catalog" | "jobs" | "log" | "batch" | "notices" | "schedules"} Topic
 */

export const act = {
  /** @type {import("./actionforms.js").Catalog | null} */
  catalog: null,
  /** @type {import("./jobs.js").Job[]} */
  jobs: [],
  /** @type {Map<number, import("./jobs.js").LogLine[]>} */
  logs: new Map(),
  /** When each job's last progress arrived, unix seconds. @type {Map<number, number>} */
  progressAt: new Map(),
  /** @type {Map<number, import("./jobs.js").Batch>} */
  batches: new Map(),
  /** @type {import("./notices.js").NotifySnapshot | null} */
  notices: null,
  /** @type {import("./schedules.js").ScheduleList | null} */
  schedules: null,
  /** Whether the jobs list was read once (an empty list is then true). */
  jobsRead: false,
  /** Why the last read of a topic failed, until one succeeds: the pages'
   * tables show it in kp's failed slot. */
  failed: {
    /** @type {import("./doctor.js").RouteError | null} */
    jobs: null,
    /** @type {import("./doctor.js").RouteError | null} */
    notices: null,
  },
};

/** @type {Map<Topic, Set<(detail?: any) => void>>} */
const subs = new Map();

/**
 * @param {Topic} topic
 * @param {(detail?: any) => void} f
 * @returns {() => void} stop
 */
export function onAct(topic, f) {
  let set = subs.get(topic);
  if (!set) {
    set = new Set();
    subs.set(topic, set);
  }
  set.add(f);
  return () => set.delete(f);
}

/** @param {Topic} topic @param {any} [detail] */
const tell = (topic, detail) => subs.get(topic)?.forEach((f) => f(detail));

const now = () => Date.now() / 1000;

/**
 * An action's label from the catalog, or its slug before the catalog came.
 * @param {string} action
 */
export const actionLabel = (action) =>
  act.catalog?.actions.find((a) => a.action === action)?.label ?? action;

/**
 * Send a JSON request that changes something. Every answer comes back as a
 * value: the body on success, the route's `{what, why, fix}` otherwise.
 * @param {"POST" | "PUT" | "DELETE"} method
 * @param {string} url
 * @param {unknown} [body]
 * @param {string} [what]
 * @returns {Promise<{ok: true, status: number, body: any} |
 *   {ok: false, status: number, error: import("./doctor.js").RouteError}>}
 */
export async function send(method, url, body, what = url) {
  /** @type {Response} */
  let r;
  try {
    r = await fetch(url, {
      method,
      headers: {
        accept: "application/json",
        ...(body === undefined ? {} : { "content-type": "application/json" }),
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch {
    return { ok: false, status: 0, error: routeError(what, 0, null) };
  }
  if (r.redirected && new URL(r.url).pathname === "/login") {
    location.assign("/login");
    return { ok: false, status: 401, error: routeError(what, 401, null) };
  }
  /** @type {any} */
  let parsed = null;
  try {
    parsed = r.status === 204 ? null : await r.json();
  } catch {
    parsed = null;
  }
  if (!r.ok)
    return {
      ok: false,
      status: r.status,
      error: routeError(what, r.status, parsed),
    };
  return { ok: true, status: r.status, body: parsed };
}

async function loadCatalog() {
  const r = await fetchJson("/data/actions/catalog", "the action catalog");
  if (r.ok) {
    act.catalog = r.body;
    tell("catalog");
  }
}

export async function loadJobs() {
  const r = await fetchJson("/data/actions/jobs", "the jobs");
  if (!r.ok) {
    act.failed.jobs = r.error;
    tell("jobs");
    return;
  }
  act.failed.jobs = null;
  act.jobsRead = true;
  /** @type {import("./jobs.js").Job[]} */
  let jobs = [];
  for (const j of r.body.jobs ?? []) jobs = upsertJob(jobs, j);
  // Jobs the live channel told about meanwhile are newer than the list.
  for (const j of act.jobs) jobs = upsertJob(jobs, j);
  act.jobs = jobs;
  tell("jobs");
}

export async function loadNotices() {
  const r = await fetchJson("/data/notifications", "the notifications");
  if (!r.ok) {
    act.failed.notices = r.error;
    tell("notices");
    return;
  }
  act.failed.notices = null;
  act.notices = r.body;
  tell("notices");
}

export async function loadSchedules() {
  const r = await fetchJson("/data/schedules", "the schedules");
  if (!r.ok) return r.error;
  act.schedules = r.body;
  tell("schedules");
  return null;
}

/**
 * Lay a settings change the server confirmed onto the snapshot, before
 * (or without) its `notify_settings` event.
 * @param {Partial<import("./notices.js").NotifySettings>} change
 */
export function patchNoticeSettings(change) {
  if (!act.notices) return;
  const settings = { ...act.notices.settings, ...change };
  act.notices = {
    ...act.notices,
    settings,
    snoozed: settings.snooze_until != null && settings.snooze_until > now(),
  };
  tell("notices");
}

let started = false;

/** Read everything once and follow the live channel. */
export function startAct() {
  if (started) return;
  started = true;
  listen("action", (j) => {
    act.jobs = upsertJob(act.jobs, j);
    tell("jobs", j.job);
  });
  listen("action_progress", (ev) => {
    act.jobs = applyProgress(act.jobs, ev);
    act.progressAt.set(ev.job, now());
    tell("jobs", ev.job);
  });
  listen("action_log", (line) => {
    addLog(act.logs, line);
    tell("log", line);
  });
  listen("action_batch", (b) => {
    act.batches.set(b.batch, b);
    tell("batch", b.batch);
  });
  listen("notification", (ev) => {
    act.notices = addNotice(act.notices, ev);
    if (!act.notices) void loadNotices();
    else tell("notices", ev);
  });
  listen("notifications_read", () => void loadNotices());
  listen("notify_settings", (ev) => {
    if (!act.notices) return;
    const s = ev.settings;
    act.notices = {
      ...act.notices,
      settings: s,
      snoozed: s.snooze_until != null && s.snooze_until > now(),
    };
    tell("notices");
  });
  listen("schedules", (list) => {
    act.schedules = list;
    tell("schedules");
  });
  // After a restart the jobs a page shows (a driven dialog's among them)
  // are read again: `resync` only reaches a browser that heard an event
  // before, `reopened` comes on every reconnect.
  const reload = () => {
    void loadJobs();
    void loadNotices();
    void loadSchedules();
  };
  listen("resync", reload);
  listen("reopened", reload);
  void loadCatalog();
  void loadJobs();
  void loadNotices();
  void loadSchedules();
}

/**
 * The catalog, read once; resolves when it is there.
 * @returns {Promise<import("./actionforms.js").Catalog | null>}
 */
export async function catalogReady() {
  if (act.catalog) return act.catalog;
  await loadCatalog();
  return act.catalog;
}
