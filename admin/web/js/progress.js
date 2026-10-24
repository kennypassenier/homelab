// progress.js: the ONE module that decides what a run's progress says.
//
// Kenny, 2026-10-24: "Maak voor deze vooruitgangscomponent een javascript
// gedeelte aan dat ALLES van de vooruitgang regelt volgens een bepaalde
// standaard, zodat het altijd werkt. Als er een job of een ander element of
// proces gebruik van wilt maken, spreekt het de standaardised javascript van
// deze component aan. Ik wil NIET dat dit nog eens misgaat."
//
// Pure: no DOM, no clock, no network. Everything a progress view prints about
// steps, bytes, the one-line report and the time left is worked out here, and
// the drawing module (runview.js) only draws what these functions return.
// A job, a batch or the Update flow feeds it plain data through the contract
// below and never computes or formats progress itself.
//
// THE CONTRACT (each point has a test in test/progress.test.js):
//
//  1. Steps. `stepStates(rows)` returns the rows in the order given, each with
//     one of done | running | pending | skipped | failed, and AT MOST ONE
//     running (a second one is shown as pending: the host runs one step at a
//     time). The order is never changed and no state is invented.
//  2. Bytes. `mergeBytes(prev, reading, stepKey)` folds the host's byte
//     readings into ONE figure per step: readings of different transfers
//     (a local snapshot, an upload, another app) are summed, each transfer's
//     figure only ever grows, and a new step starts from nothing. So the figure
//     cannot flip between transfers and cannot shrink.
//  3. A byte figure is real. `byteFigure(bytes)` says "unknown" (total not
//     known yet), "moving" (done < total: "12.4 of 77.8 MB"), or "complete"
//     (everything counted has moved: "77.8 MB moved", no bar). A figure whose
//     done equals its total from the first reading never claims progress.
//  4. One number format. Every size is written by `sizeText` (a dot for the
//     decimal, 1024-based units), in every sentence, never half by the
//     browser's locale.
//  5. The line. `progressLine(report)` is the one sentence under the bar:
//     "Step 2 of 5 · 12.4 of 77.8 MB · about 25 s left", a part left out when
//     it is not known.
//  6. Time left. `steadyEta()` returns a function that, within one step, never
//     lets the estimate grow: a different reading of the same step (another
//     member's) cannot make "25 s left" flip back to "8 s" and up again.

/** @typedef {"done" | "running" | "pending" | "skipped" | "failed"} StepState */
/**
 * @typedef {{label?: string, done: number, total: number | null,
 *   rate_bps?: number | null}} ByteReading
 * @typedef {{label: string, done: number, total: number | null,
 *   rate_bps: number | null,
 *   parts?: Record<string, {done: number, total: number | null}>,
 *   step?: string}} ByteFigureState
 */

/** The dashboard's row states, as the step list draws them. @type {Record<string, StepState>} */
export const STEP_STATE = {
  ok: "done",
  bad: "failed",
  run: "running",
  wait: "pending",
  skip: "skipped",
};

/**
 * Contract 1: the steps as drawn.
 * @template {{state: string}} R
 * @param {R[]} rows
 * @returns {(Omit<R, "state"> & {state: StepState})[]}
 */
export function stepStates(rows) {
  let running = false;
  return rows.map((r) => {
    /** @type {StepState} */
    let state = STEP_STATE[r.state] ?? /** @type {any} */ (r.state);
    if (!["done", "running", "pending", "skipped", "failed"].includes(state))
      state = "pending";
    if (state === "running") {
      if (running) state = "pending";
      running = true;
    }
    return { ...r, state };
  });
}

/**
 * Contract 4: a size, one way everywhere.
 * @param {number | null | undefined} bytes
 * @returns {string | null}
 */
export function sizeText(bytes) {
  if (bytes == null || !Number.isFinite(bytes) || bytes < 0) return null;
  if (bytes < 1024) return `${Math.round(bytes)} B`;
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
 * Contract 2: one byte figure per step.
 * @param {ByteFigureState | null | undefined} prev what the job held
 * @param {ByteReading} reading the host's newest reading
 * @param {string | number} stepKey the step the job is in now
 * @returns {ByteFigureState}
 */
export function mergeBytes(prev, reading, stepKey) {
  const key = String(stepKey);
  const same = prev?.step === key && prev.parts;
  /** @type {Record<string, {done: number, total: number | null}>} */
  const parts = same ? { ...prev.parts } : {};
  const name = reading.label || "transfer";
  const old = parts[name];
  const total =
    reading.total != null && reading.total > 0
      ? Math.max(reading.total, old?.total ?? 0)
      : (old?.total ?? null);
  parts[name] = { done: Math.max(reading.done, old?.done ?? 0), total };
  const all = Object.values(parts);
  const known = all.every((p) => p.total != null && p.total > 0);
  return {
    label: all.length === 1 ? name : `${all.length} transfers`,
    done: all.reduce((n, p) => n + p.done, 0),
    total: known ? all.reduce((n, p) => n + (p.total ?? 0), 0) : null,
    rate_bps: reading.rate_bps ?? null,
    parts,
    step: key,
  };
}

/**
 * Contract 3: what a byte figure honestly says.
 * @param {{done: number, total: number | null} | null | undefined} b
 * @returns {{kind: "unknown" | "moving" | "complete", percent: number | null,
 *   text: string} | null}
 */
export function byteFigure(b) {
  if (!b) return null;
  const done = sizeText(b.done) ?? "0 B";
  if (!b.total || b.total <= 0)
    return { kind: "unknown", percent: null, text: `${done} so far` };
  if (b.done >= b.total)
    return {
      kind: "complete",
      percent: 100,
      text: `${sizeText(b.total)} moved`,
    };
  const percent = Math.min(99, Math.floor((b.done / b.total) * 100));
  return {
    kind: "moving",
    percent,
    text: `${done} of ${sizeText(b.total)}`,
  };
}

/**
 * Contract 6: an estimate that never grows within a step.
 * @returns {(stepKey: string | number | undefined, etaSeconds: number | undefined) => number | undefined}
 */
export function steadyEta() {
  /** @type {string} */
  let at = "";
  /** @type {number | undefined} */
  let shown;
  return (stepKey, etaSeconds) => {
    const key = String(stepKey ?? "");
    if (key !== at) {
      at = key;
      shown = etaSeconds;
      return shown;
    }
    if (etaSeconds == null) return undefined;
    shown = shown == null ? etaSeconds : Math.min(shown, etaSeconds);
    return shown;
  };
}

/**
 * @param {number} s
 * @returns {string}
 */
function etaText(s) {
  if (s < 90) return `${Math.max(1, Math.round(s))} s`;
  const m = Math.round(s / 60);
  return m < 90 ? `${m} min` : `${Math.round(m / 60)} h`;
}

/**
 * Contract 5: the one sentence under the bar.
 * @param {{step?: number, steps?: number,
 *   bytes?: {done: number, total: number | null} | null,
 *   etaSeconds?: number}} r
 * @returns {string}
 */
export function progressLine(r) {
  /** @type {string[]} */
  const parts = [];
  if (r.step != null && r.steps != null && r.steps > 0)
    parts.push(`Step ${r.step} of ${r.steps}`);
  const fig = byteFigure(r.bytes);
  if (fig) parts.push(fig.text);
  if (r.etaSeconds != null && Number.isFinite(r.etaSeconds))
    parts.push(`about ${etaText(r.etaSeconds)} left`);
  return parts.join(" · ");
}
