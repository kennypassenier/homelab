// redesign-backups (Backups redesign 3.71, approved by Kenny 2026-10-03): what the
// Backups page shows, worked out without a DOM — the nights of the
// coverage heatmap, each cell's state, the whole-fleet row, the KPI tiles,
// the repository table's groups, filters and sort, and the page's address.
// Pure, so the node tests pin the same shapes the browser draws.

/** One day in seconds. */
export const DAY = 86400;

/** @param {number} n */
const pad = (n) => String(n).padStart(2, "0");

/**
 * The night a moment belongs to, as `YYYY-MM-DD` in the viewer's own time
 * zone: a night runs from noon to noon and carries its evening's date, so
 * the 03:00 round after Wednesday evening is "Wednesday's night", and a
 * backup made by hand on Thursday afternoon counts for Thursday's.
 * @param {number} unix seconds
 * @returns {string}
 */
export function nightKey(unix) {
  const d = new Date((unix - 12 * 3600) * 1000);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/**
 * A night `days` later (negative: earlier), calendar arithmetic, so a
 * daylight-saving change never skips or repeats a night.
 * @param {string} key YYYY-MM-DD
 * @param {number} days
 */
export function addDays(key, days) {
  const [y, m, d] = key.split("-").map(Number);
  const t = new Date(Date.UTC(y, m - 1, d + days));
  return `${t.getUTCFullYear()}-${pad(t.getUTCMonth() + 1)}-${pad(t.getUTCDate())}`;
}

/**
 * The newest night that is over (its round ran by 06:00 the next
 * morning: 30 hours after that night's own midnight), and the night now
 * under way. They are the same night between 06:00 and noon.
 * @param {number} now unix seconds
 * @returns {{last: string, current: string}}
 */
export function nightsNow(now) {
  return { last: nightKey(now - 18 * 3600), current: nightKey(now) };
}

/**
 * The `n` nights the heatmap shows, oldest first, ending `offset` nights
 * before `end`.
 * @param {string} end
 * @param {number} n
 * @param {number} offset
 */
export function nightRange(end, n, offset) {
  const last = addDays(end, -offset);
  return Array.from({ length: n }, (_, i) => addDays(last, i - (n - 1)));
}

/**
 * One stack as the page has read it so far.
 * @typedef {{status: "pending"} |
 *   {status: "failed", reason: string} |
 *   {status: "ok", noBackup: boolean, reason?: string | null,
 *    times: number[], native: boolean, repos: Repo[]}} StackRead
 * @typedef {{id: string, short_id: string, time: number}} Snap
 * @typedef {{last_attempt: number, last_pass: number,
 *   last_error?: string | null}} Drill
 * @typedef {{owner: string, newest_snapshot?: Snap | null,
 *   snapshot_count: number, snapshots: Snap[], size_bytes?: number | null,
 *   drill?: Drill | null, error?: string | null,
 *   measured_at?: number | null}} Repo
 * @typedef {"load" | "unread" | "none" | "before" | "ok" | "wait" |
 *   "miss"} CellState
 */

/**
 * The nights a stack wrote at least one snapshot on, each with its
 * snapshot times, oldest first.
 * @param {number[]} times
 * @returns {Map<string, number[]>}
 */
export function nightsOf(times) {
  /** @type {Map<string, number[]>} */
  const out = new Map();
  for (const t of [...times].sort((a, b) => a - b)) {
    const k = nightKey(t);
    const list = out.get(k) ?? [];
    list.push(t);
    out.set(k, list);
  }
  return out;
}

/**
 * One heatmap cell: `ok` a snapshot that night; `miss` none although the
 * stack keeps data and had history by then; `before` a night before its
 * first snapshot; `wait` tonight, whose round has not run yet; `none` a
 * stack that keeps no data by design; `load` still reading; `unread` the
 * read failed.
 * @param {StackRead} s
 * @param {string} night
 * @param {{last: string, current: string}} now
 * @returns {CellState}
 */
export function cellState(s, night, now) {
  if (s.status === "pending") return "load";
  if (s.status === "failed") return "unread";
  if (s.noBackup) return "none";
  const nights = nightsOf(s.times);
  if (nights.has(night)) return "ok";
  if (night > now.last) return "wait";
  const first = [...nights.keys()][0];
  if (first != null && night < first) return "before";
  return "miss";
}

/**
 * The whole-fleet cell of one night: how many of the stacks expected to
 * back up that night did, and whether no stack had history yet.
 * @param {Record<string, StackRead>} stacks
 * @param {string} night
 * @param {{last: string, current: string}} now
 */
export function fleetNight(stacks, night, now) {
  let ok = 0;
  let expected = 0;
  let before = 0;
  let wait = 0;
  for (const s of Object.values(stacks)) {
    const st = cellState(s, night, now);
    if (st === "ok") (ok++, expected++);
    else if (st === "miss") expected++;
    else if (st === "before") before++;
    else if (st === "wait") wait++;
  }
  return {
    ok,
    expected,
    before: expected === 0 && before > 0,
    wait: expected === 0 && wait > 0,
    pct: expected === 0 ? 0 : Math.round((ok / expected) * 100),
  };
}

/**
 * What one stack did on one night, for the side panel and the hover card:
 * its state, the snapshot times, and each app's snapshot ids that night.
 * @param {StackRead} s
 * @param {string} night
 * @param {{last: string, current: string}} now
 */
export function stackNight(s, night, now) {
  const state = cellState(s, night, now);
  if (s.status !== "ok") return { state, times: [], ids: [] };
  const times = nightsOf(s.times).get(night) ?? [];
  const ids = s.repos.flatMap((r) =>
    (r.snapshots ?? [])
      .filter((x) => nightKey(x.time) === night)
      .map((x) => ({ owner: r.owner, short_id: x.short_id, time: x.time })),
  );
  return { state, times, ids };
}

/**
 * A repository's restore-drill state.
 * @param {Repo} r
 * @returns {"never" | "failed" | "passed"}
 */
export function drillState(r) {
  if (!r.drill || !r.drill.last_attempt) return "never";
  if (r.drill.last_error) return "failed";
  return "passed";
}

/** @param {"never" | "failed" | "passed"} d */
export const drillWords = (d) =>
  d === "never" ? "never drilled" : d === "failed" ? "failed" : "passed";

/**
 * The seven nights ending at `last`, each true when the repository wrote a
 * snapshot that night (the row's tick strip).
 * @param {Repo} r
 * @param {string} last
 */
export function ticks(r, last) {
  const have = new Set((r.snapshots ?? []).map((x) => nightKey(x.time)));
  return Array.from({ length: 7 }, (_, i) => have.has(addDays(last, i - 6)));
}

/**
 * A size in bytes the way a person reads it; null when unknown.
 * @param {number | null | undefined} bytes
 */
export function humanBytes(bytes) {
  if (bytes == null || !Number.isFinite(bytes) || bytes < 0) return null;
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v < 10 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
}

/**
 * Every repository of every stack read so far, with its stack.
 * @param {Record<string, StackRead>} stacks
 * @returns {(Repo & {stack: string, native: boolean})[]}
 */
export function allRepos(stacks) {
  return Object.entries(stacks).flatMap(([stack, s]) =>
    s.status === "ok"
      ? s.repos.map((r) => ({ ...r, stack, native: s.native }))
      : [],
  );
}

/**
 * The five KPI tiles' numbers, exact.
 * @param {Record<string, StackRead>} stacks
 * @param {number} now unix seconds
 * @param {number | null} retired entries kept from retired stacks; null
 *   while that read is out
 */
export function kpis(stacks, now, retired) {
  const nights = nightsNow(now);
  const settled = Object.values(stacks).every((s) => s.status !== "pending");
  const expected = Object.entries(stacks).filter(([, s]) =>
    ["ok", "miss"].includes(cellState(s, nights.last, nights)),
  );
  const missed = expected
    .filter(([, s]) => cellState(s, nights.last, nights) === "miss")
    .map(([n]) => n);
  const repos = allRepos(stacks);
  let newest = /** @type {{time: number, where: string} | null} */ (null);
  for (const r of repos) {
    const t = r.newest_snapshot?.time;
    if (t != null && (!newest || t > newest.time))
      newest = { time: t, where: `${r.stack}/${r.owner}` };
  }
  const drills = { passed: 0, failed: 0, never: 0 };
  for (const r of repos) drills[drillState(r)]++;
  return {
    settled,
    lastNight: {
      night: nights.last,
      covered: expected.length - missed.length,
      expected: expected.length,
      missed,
    },
    newest,
    repositories: {
      count: repos.length,
      snapshots: repos.reduce((a, r) => a + (r.snapshot_count ?? 0), 0),
      stacks: new Set(repos.map((r) => r.stack)).size,
    },
    drills: { ...drills, total: repos.length },
    retired,
  };
}

/**
 * Why a stack has no repository row, in the words its group row shows.
 * @param {StackRead} s
 */
export function stackWhy(s) {
  if (s.status === "pending") return "reading…";
  if (s.status === "failed") return `could not read it: ${s.reason}`;
  if (s.noBackup) return "keeps no data by design";
  if (s.repos.length === 0)
    return s.reason
      ? `no repository found on the backup target (${s.reason})`
      : "no repository found on the backup target";
  const n = s.repos.length;
  return `${n} ${n === 1 ? "repository" : "repositories"}${s.native ? " · native service" : ""}`;
}

/**
 * @typedef {{key: "app" | "age" | "snaps" | "size" | "drill",
 *   dir: 1 | -1}} SortKey
 * @typedef {{stacks: Set<string>, q: string, undrilled: boolean,
 *   sort: SortKey[], collapsed: Set<string>, open: string | null}} TableView
 */

/**
 * The value one sort key reads off a repository.
 * @param {Repo} r
 * @param {SortKey["key"]} k
 * @param {number} now
 * @returns {string | number}
 */
export function sortValue(r, k, now) {
  switch (k) {
    case "app":
      return r.owner;
    case "age":
      return r.newest_snapshot ? now - r.newest_snapshot.time : Infinity;
    case "snaps":
      return r.snapshot_count ?? 0;
    case "size":
      return r.size_bytes ?? -1;
    default:
      return drillWords(drillState(r));
  }
}

/**
 * A click on a sort header: the first click sorts ascending, the next
 * descending, the third clears it; with `add` (Shift) the key joins the
 * existing ones as a further sort instead of replacing them.
 * @param {SortKey[]} sort
 * @param {SortKey["key"]} key
 * @param {boolean} add
 * @returns {SortKey[]}
 */
export function nextSort(sort, key, add) {
  const i = sort.findIndex((s) => s.key === key);
  if (add) {
    if (i < 0) return [...sort, { key, dir: 1 }];
    return sort.map((s, j) =>
      j === i ? { key, dir: /** @type {1 | -1} */ (-s.dir) } : s,
    );
  }
  if (i === 0 && sort.length === 1)
    return sort[0].dir > 0 ? [{ key, dir: -1 }] : [];
  return [{ key, dir: 1 }];
}

/**
 * The repository table, grouped per stack: every stack the fleet has, in
 * name order, each with the repositories that pass the filters, sorted.
 * A stack whose repositories all fall to a filter drops out while a text
 * or drill filter is on (a stack filter keeps only its own stacks).
 * @param {Record<string, StackRead>} stacks
 * @param {TableView} v
 * @param {number} now
 */
export function repoGroups(stacks, v, now) {
  const q = v.q.trim().toLowerCase();
  const out = [];
  for (const name of Object.keys(stacks).sort((a, b) => a.localeCompare(b))) {
    if (v.stacks.size && !v.stacks.has(name)) continue;
    const s = stacks[name];
    const all = s.status === "ok" ? s.repos : [];
    const rows = all
      .filter(
        (r) =>
          (!q || `${name} ${r.owner}`.toLowerCase().includes(q)) &&
          (!v.undrilled || drillState(r) === "never"),
      )
      .sort((a, b) => {
        for (const { key, dir } of v.sort) {
          const x = sortValue(a, key, now);
          const y = sortValue(b, key, now);
          if (x < y) return -dir;
          if (x > y) return dir;
        }
        return 0;
      });
    if ((q || v.undrilled) && rows.length === 0) continue;
    out.push({ stack: name, read: s, total: all.length, rows });
  }
  return out;
}

/**
 * The page's own state from its address: the pinned night, the stack
 * filter, the drill filter, the repository filter text, the sort.
 * @param {string} search location.search
 */
export function viewFromSearch(search) {
  const p = new URLSearchParams(search);
  const night = p.get("night");
  /** @type {SortKey[]} */
  const sort = [];
  for (const part of (p.get("sort") ?? "").split(",").filter(Boolean)) {
    const [key, dir] = part.split(":");
    if (["app", "age", "snaps", "size", "drill"].includes(key))
      sort.push({
        key: /** @type {SortKey["key"]} */ (key),
        dir: dir === "desc" ? -1 : 1,
      });
  }
  return {
    night: night && /^\d{4}-\d{2}-\d{2}$/.test(night) ? night : null,
    stacks: new Set((p.get("stacks") ?? "").split(",").filter(Boolean)),
    undrilled: p.get("drills") === "never",
    q: p.get("q") ?? "",
    sort,
    section: p.get("section"),
  };
}

/**
 * The address parameters for the page's state; null removes one.
 * @param {{night: string | null, stacks: Set<string>, undrilled: boolean,
 *   q: string, sort: SortKey[]}} v
 * @returns {Record<string, string | null>}
 */
export function searchFromView(v) {
  return {
    night: v.night,
    stacks: v.stacks.size ? [...v.stacks].sort().join(",") : null,
    drills: v.undrilled ? "never" : null,
    q: v.q.trim() || null,
    sort: v.sort.length
      ? v.sort.map((s) => `${s.key}:${s.dir > 0 ? "asc" : "desc"}`).join(",")
      : null,
  };
}

/**
 * How far back the heatmap has to page so `night` is in view.
 * @param {string} end the newest night shown at offset 0
 * @param {string} night
 * @param {number} n nights on screen
 */
export function offsetFor(end, night, n) {
  if (night >= end) return 0;
  const [y1, m1, d1] = end.split("-").map(Number);
  const [y2, m2, d2] = night.split("-").map(Number);
  const back = Math.round(
    (Date.UTC(y1, m1 - 1, d1) - Date.UTC(y2, m2 - 1, d2)) / (DAY * 1000),
  );
  return Math.floor(back / n) * n;
}
