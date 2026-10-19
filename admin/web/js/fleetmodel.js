// The Stacks page's pure half on kp-themes 11 (feat-kp11demo-12): each stack
// row as the data a kp entity card, a datatable row, the bullets, the share
// bar and the heat grid take. No DOM, no clock of its own, so `node --test`
// drives it. stacksview.js stays the model of a row; this maps it.

import { cellState, nightKey, nightRange, nightsNow } from "./backupsview.js";

/**
 * @typedef {import("./stacksview.js").Row} Row
 * @typedef {import("./stacksview.js").Flag} Flag
 * @typedef {"success" | "warning" | "destructive" | "info"} Tone
 */

/** The state words a table row reserves room for (the widest sets the cell). */
export const STATE_WORDS = "running\ndegraded\noffline\nparked";

/**
 * Whether a stack belongs in the "Needs attention" group: it is down or
 * degraded, missed a backup or carries a red flag. A parked stack is
 * quiet on purpose, so it is never in it.
 * @param {Row} r
 */
export const needsAttention = (r) => r.state.label !== "parked" && r.problem;

/**
 * The group a row sits in when the table groups by state.
 * @param {Row} r
 * @returns {"attention" | "running" | "parked"}
 */
export const stateGroup = (r) =>
  r.state.label === "parked"
    ? "parked"
    : needsAttention(r)
      ? "attention"
      : "running";

/** The state groups, in the order the table lists them. */
export const STATE_GROUPS = /** @type {const} */ ([
  { key: "attention", label: "Needs attention" },
  { key: "running", label: "Running" },
  { key: "parked", label: "Parked" },
]);

/**
 * A badge tone for a state.
 * @param {Row} r
 * @returns {Tone}
 */
export const stateTone = (r) =>
  r.state.label === "parked"
    ? "info"
    : r.state.tone === "bad"
      ? "destructive"
      : r.state.tone === "warn"
        ? "warning"
        : "success";

/** @param {Flag["tone"]} t @returns {Tone | undefined} */
const flagTone = (t) =>
  t === "bad"
    ? "destructive"
    : t === "warn"
      ? "warning"
      : t === "info"
        ? "info"
        : undefined;

/**
 * The card's RAM meter: the share of the memory limit, a tick at 80 %,
 * and "4.1 of 8.0 GB" beside it.
 * @param {Row} r
 * @param {(mb: number | null | undefined) => string} gb
 */
export function ramMeter(r, gb) {
  if (r.state.label === "parked" || r.ramPct == null)
    return {
      label: "RAM",
      value: null,
      mark: 0.8,
      text: r.state.label === "parked" ? "Not running" : "Not measured",
    };
  /** @type {"warning" | "destructive" | undefined} */
  const tone =
    r.ramPct >= 90 ? "destructive" : r.ramPct >= 80 ? "warning" : undefined;
  return {
    label: "RAM",
    value: r.ramPct / 100,
    mark: 0.8,
    ...(tone ? { tone } : {}),
    text: `${gb(r.ramUsed)} of ${gb(r.ramMax)} GB`,
  };
}

/**
 * One stack as a kp entity card.
 * @param {Row} r
 * @param {(mb: number | null | undefined) => string} gb
 * @returns {import("/static/kp/js/entity-card.js").EntityData}
 */
export function entityOf(r, gb) {
  const parked = r.state.label === "parked";
  /** @type {{id: string, label: string, disabled?: string}[]} */
  const actions = [
    {
      id: "deploy",
      label: "Deploy",
      ...(parked ? { disabled: "Parked: unpark it first, then deploy" } : {}),
    },
    { id: "backup", label: "Back up now" },
  ];
  // The stack's own suggested step (Update, or Logs for one that is down)
  // stays one press away, as the cards had it before.
  if (r.action.kind === "update" && !parked)
    actions.push({ id: "update", label: "Update" });
  if (r.action.kind === "logs") actions.push({ id: "logs", label: "Logs" });
  return {
    key: r.name,
    name: r.name,
    vmid: `CT ${r.vmid}`,
    state: {
      label:
        needsAttention(r) && r.state.label === "running"
          ? "needs attention"
          : r.state.label,
      tone: stateTone(r),
    },
    figures: [
      {
        value: parked ? "0" : `${r.appsUp}/${r.appsTotal}`,
        label: "Apps up",
      },
      { value: String(r.restarts), label: "Restarts" },
      {
        value:
          parked || r.cpuPct == null
            ? "off"
            : `${Math.round(r.cpuPct * 10) / 10} %`,
        label: "CPU",
      },
    ],
    flags: r.flags.map((f) => ({
      label: f.label,
      ...(flagTone(f.tone) ? { tone: flagTone(f.tone) } : {}),
    })),
    meters: [ramMeter(r, gb)],
    actions,
  };
}

/**
 * "Hungriest stacks": the running stacks ranked by one measure.
 * @param {Row[]} rows
 * @param {"ram" | "cpu" | "disk"} measure
 * @param {Map<string, {cpu: number | null, mem: number | null, disk: number | null}>} capacity
 *   percent per stack, from `/data/capacity`
 * @param {string | null} only narrow to one stack
 * @param {(mb: number | null | undefined) => string} gb
 * @returns {import("/static/kp/js/bullets.js").BulletRow[]}
 */
export function hungryRows(rows, measure, capacity, only, gb) {
  const val = (/** @type {Row} */ r) => {
    const c = capacity.get(r.name);
    const v =
      measure === "ram"
        ? (r.ramPct ?? c?.mem ?? null)
        : measure === "cpu"
          ? (c?.cpu ?? r.cpuPct)
          : (c?.disk ?? null);
    return v == null ? null : Math.round(v);
  };
  return rows
    .filter((r) => r.state.label !== "parked" && (!only || r.name === only))
    .map((r) => ({ r, v: val(r) }))
    .filter((x) => x.v != null)
    .sort((a, b) => /** @type {number} */ (b.v) - /** @type {number} */ (a.v))
    .slice(0, only ? 1 : 5)
    .map(({ r, v }) => ({
      key: r.name,
      label: r.name,
      lead: r.name,
      value: /** @type {number} */ (v),
      figure: `${v} %`,
      detail:
        measure === "ram" && r.ramMax
          ? `${gb(r.ramUsed)} of ${gb(r.ramMax)} GB`
          : `CT ${r.vmid}`,
      pick: true,
    }));
}

/**
 * The nights grid as kp's heat takes it: one row per stack, `count`
 * columns oldest first, a summary row of how many stacks missed each
 * night. A parked stack is paused, never failed.
 * @param {Row[]} rows
 * @param {Record<string, import("./backupsview.js").StackRead>} reads
 * @param {number} nowS unix seconds
 * @param {number} count nights
 * @param {{every: number, narrow: boolean}} axis
 * @param {(night: string) => string} dayLabel dd/mm/yyyy of a night key
 * @returns {{columns: {key: string, label: string, minor: boolean}[],
 *   summary: import("/static/kp/js/heat.js").HeatRow,
 *   rows: import("/static/kp/js/heat.js").HeatRow[], nights: string[]}}
 */
export function nightsGrid(rows, reads, nowS, count, axis, dayLabel) {
  const now = nightsNow(nowS);
  const nights = nightRange(now.last, count, 0);
  const shortOf = (/** @type {string} */ k) =>
    axis.narrow ? k.slice(8, 10) : `${k.slice(8, 10)}/${k.slice(5, 7)}`;
  const columns = nights.map((k, i) => ({
    key: k,
    label: shortOf(k),
    minor: !(
      i === nights.length - 1 ||
      (i % axis.every === 0 && nights.length - 1 - i >= axis.every)
    ),
  }));
  /** @type {import("/static/kp/js/heat.js").HeatRow[]} */
  const out = rows.map((r) => {
    const read = reads[r.name] ?? { status: /** @type {const} */ ("pending") };
    return {
      key: r.name,
      label: r.name,
      muted: r.state.label === "parked",
      cells: nights.map((k) => {
        const st =
          r.state.label === "parked" ? "paused" : cellState(read, k, now);
        const words =
          st === "ok"
            ? "backed up"
            : st === "miss"
              ? "missed"
              : st === "paused"
                ? "paused"
                : st === "wait"
                  ? "tonight's round has not run"
                  : st === "before"
                    ? "before the first backup"
                    : st === "none"
                      ? "keeps no data"
                      : st === "load"
                        ? "reading"
                        : "not read";
        return {
          tone:
            st === "ok"
              ? "ok"
              : st === "miss"
                ? "bad"
                : st === "paused"
                  ? "paused"
                  : st === "wait"
                    ? "future"
                    : st === "before"
                      ? "before"
                      : st === "load"
                        ? "loading"
                        : "none",
          label: words,
          tip: `${r.name}, ${dayLabel(k)}: ${words}`,
        };
      }),
    };
  });
  const summary = {
    key: "all",
    label: "Missed per night",
    cells: nights.map((_, i) => {
      const states = out.map((row) => row.cells[i].tone);
      const missed = states.filter((t) => t === "bad").length;
      const active = states.filter((t) => t === "ok" || t === "bad").length;
      return {
        tone: missed
          ? /** @type {const} */ ("bad")
          : /** @type {const} */ ("ok"),
        share: missed && active ? missed / active : 1,
        label: `${missed} of ${active} stacks missed`,
        tip: `${dayLabel(nights[i])}: ${missed} of ${active} stacks missed`,
      };
    }),
  };
  return { columns, summary, rows: out, nights };
}

/**
 * The reads of `/data/backup-calendar` as the backups view's stack reads.
 * @param {{stacks?: Record<string, number[]>, no_backup?: string[]} | null} cal
 * @param {Row[]} rows
 * @returns {Record<string, import("./backupsview.js").StackRead>}
 */
export function readsOf(cal, rows) {
  /** @type {Record<string, import("./backupsview.js").StackRead>} */
  const out = {};
  for (const r of rows) {
    if (!cal) out[r.name] = { status: "pending" };
    else if ((cal.no_backup ?? []).includes(r.name))
      out[r.name] = {
        status: "ok",
        noBackup: true,
        times: [],
        native: false,
        repos: [],
      };
    else if (cal.stacks?.[r.name])
      out[r.name] = {
        status: "ok",
        noBackup: false,
        times: cal.stacks[r.name],
        native: false,
        repos: [],
      };
    else out[r.name] = { status: "failed", reason: "not read yet" };
  }
  return out;
}

/**
 * The last `n` backups of a stack, newest first, as the row detail lists
 * them.
 * @param {number[] | undefined} times unix seconds
 * @param {number} n
 */
export const lastBackups = (times, n = 3) =>
  [...(times ?? [])].sort((a, b) => b - a).slice(0, n);

export { nightKey };
