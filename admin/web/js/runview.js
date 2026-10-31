// one-progress-view-1 (Kenny, 2026-10-23): ONE progress view for every
// running action. Update, Update all, a single Deploy, Deploy all changes
// and the Jobs page used to draw their own: a page, a dialog, a side panel.
// This is the shared one, modelled on the Update flow page (update.js,
// which he likes): the steps with their progress on top, the logs below or
// beside them, and nothing moves while the run lasts.
//
// "ik haat het als elementen heen en weer springen"; "verschijnen mag,
// verschuiven niet" (Kenny, 2026-10-22/23). So the layout is two columns,
// each its own section with a size of its own and its own scroll, in two
// equal halves (Kenny, 2026-10-24: "deel het beeld in twee"); below 64rem
// the halves stack, the progress first and the log under it, as the Update
// flow page of 4.0.0 did:
//
//   head              title + sub and the three figures · the state badge
//   progress half     the bar · the step list (scrolls) · the time bar
//   log half          the host's lines (scrolls) · the result area (scrolls)
//
// A part that appears (a failure, the host's words, the command to run it
// again) appears INSIDE its own reserved area and moves nothing else; its
// opening animation plays once, when it appears, never on an update. Every
// part is redrawn only when its content changed (one signature per part), so
// a finished run draws nothing again.
//
// The module is a view: it takes a plain description of what to show
// (`RunSpec`) from the caller — jobpanel.js builds one from a job, update.js
// from the Update flow's rows — and draws it.

import { h, progressBar, setProgress } from "./dom.js";
import { humanDuration } from "./format.js";
import { jobLogViewer } from "./joblog.js";
import { setCode } from "/static/kp/dist/kp-themes.min.js";
import { progressLine, steadyEta } from "./progress.js";
import { attachProblems, setProblems } from "/static/kp/dist/kp-themes.min.js";
import { setStackbar } from "/static/kp/dist/kp-themes.min.js";
import {
  attachSteplists,
  buildSteplist,
  setStep,
} from "/static/kp/dist/kp-themes.min.js";
import { attachTicks } from "/static/kp/dist/kp-themes.min.js";
import {
  attachTimerDiscs,
  timerDiscOf,
} from "/static/kp/dist/kp-themes.min.js";

/**
 * @typedef {{id: string, title: string, note: string,
 *   state: import("/static/kp/dist/kp-themes.min.js").StepState,
 *   time?: string}} RunStep
 * @typedef {{label: string, tone: string}} RunBadge
 * @typedef {{title: string, sub: string, badge: RunBadge | null,
 *   restarts: boolean}} RunHead
 * @typedef {{done: number, of: string, note: string, took: string,
 *   median: string, delta: {tone: string, text: string} | null}} RunFigures
 * @typedef {{percent: number | null, note: string,
 *   bytes: {percent: number | null, text: string} | null,
 *   line: {step?: number, steps?: number,
 *   bytes?: {done: number, total: number | null} | null,
 *   etaSeconds?: number} | null}} RunBar
 * @typedef {{kind: "empty", message: string} | {kind: "bar", parts:
 *   {key: string, label: string, value: number, bad: boolean}[],
 *   whole: number, restLabel: string, medianS: number | null,
 *   note: string}} RunTime
 * @typedef {{running: boolean, scaleMin: number, leftS: number | null,
 *   text: string}} RunDisc
 * @typedef {{problems: object[], code: {text: string, title: string} | null,
 *   outcome: {tone: string, title: string, text: string} | null,
 *   cli: string, basis: string}} RunResult
 * @typedef {{head?: RunHead, figures?: RunFigures, bar?: RunBar,
 *   steps?: RunStep[], time?: RunTime, disc?: RunDisc,
 *   result?: RunResult}} RunSpec
 */

/**
 * A one-line note: the full words are also its title, so a long one that
 * is cut at the end of the line (the line never wraps, so nothing below it
 * moves) can still be read.
 * @param {HTMLElement} el
 * @param {string} text
 */
const setNote = (el, text) => {
  el.textContent = text;
  if (text) el.title = text;
  else el.removeAttribute("title");
};

/** A part's content, as one string: equal means nothing to redraw. @param {unknown} x */
export const sigOf = (x) => JSON.stringify(x) ?? "";

/**
 * One signature guard per part (redraw-unchanged-1): `run(key, sig, draw)`
 * calls `draw` only when `sig` differs from the part's last one, and says
 * whether it did.
 * @returns {(key: string, sig: unknown, draw: () => void) => boolean}
 */
export function signatureGuard() {
  /** @type {Map<string, string>} */
  const last = new Map();
  return (key, sig, draw) => {
    const s = sigOf(sig);
    if (last.get(key) === s) return false;
    last.set(key, s);
    draw();
    return true;
  };
}

/**
 * Lays `items` on a step list that stays where it is: rows already there
 * are changed in place (and only when something about them changed), new
 * rows are appended, surplus rows removed from the end. The list is never
 * rebuilt, so a scroll position, a focus and every row above a change
 * survive it.
 * @param {HTMLElement} host the container the list lives in
 * @param {RunStep[]} items
 * @param {Map<string, string>} seen each row's last signature, by id
 * @param {string} label the list's accessible name
 */
export function syncSteps(host, items, seen, label) {
  let list = /** @type {HTMLElement | null} */ (
    host.querySelector(":scope > .kp-steplist")
  );
  if (!list) {
    list = buildSteplist([], { label });
    host.prepend(list);
    attachSteplists(host);
    seen.clear();
  }
  const have = [...list.querySelectorAll(":scope > .kp-steplist__item")].map(
    (el) => el.getAttribute("data-kp-step-id") ?? "",
  );
  const want = new Set(items.map((i) => i.id));
  // Rows that are no longer in the plan leave from the end.
  for (const id of have)
    if (!want.has(id)) {
      list
        .querySelector(`:scope > [data-kp-step-id="${CSS.escape(id)}"]`)
        ?.remove();
      seen.delete(id);
    }
  /** @type {RunStep[]} */
  const fresh = [];
  for (const it of items) {
    const sig = sigOf([it.title, it.note, it.state, it.time ?? ""]);
    if (!seen.has(it.id)) {
      fresh.push(it);
      seen.set(it.id, sig);
      continue;
    }
    if (seen.get(it.id) === sig) continue;
    seen.set(it.id, sig);
    setStep(list, it.id, {
      state: it.state,
      title: it.title,
      note: it.note,
      time: it.time ?? "",
    });
  }
  if (fresh.length) {
    const more = buildSteplist(
      fresh.map((i) => ({ ...i, time: i.time ?? "" })),
      { label },
    );
    for (const li of [...more.children]) list.append(li);
    attachSteplists(host);
  }
}

/**
 * The running row's offset inside the scroll area, so the area can follow
 * the run without ever scrolling the page (scrollIntoView would).
 * @param {HTMLElement} area
 */
function followRunning(area) {
  const row = area.querySelector(
    '.kp-steplist__item[data-state="running"], .kp-steplist__item[data-state="failed"]',
  );
  if (!(row instanceof HTMLElement)) return;
  // The area moves only when the running row is not wholly in view, and then
  // to a row boundary (the row before it stays above, for context): a half
  // row is never left cut at the top. A row in view keeps its place.
  const box = area.getBoundingClientRect();
  const r = row.getBoundingClientRect();
  const top = r.top - box.top + area.scrollTop;
  const bottom = top + r.height;
  if (top >= area.scrollTop && bottom <= area.scrollTop + area.clientHeight)
    return;
  area.scrollTop = Math.max(0, Math.round(top - r.height));
}

/**
 * The step area is as tall as its rows, up to its maximum, and the maximum
 * is a whole number of rows, so a row is never left cut at the bottom (nor,
 * with followRunning, at the top) and no empty band stands between the list
 * and what is under it.
 * @param {HTMLElement} area
 */
function fitRows(area) {
  const row = area.querySelector(".kp-steplist__item");
  if (!(row instanceof HTMLElement) || area.hidden) return;
  const rowH = row.getBoundingClientRect().height;
  if (!(rowH > 0)) return;
  area.style.maxBlockSize = "";
  const max = parseFloat(getComputedStyle(area).maxBlockSize);
  if (!Number.isFinite(max)) return;
  const borders = area.offsetHeight - area.clientHeight;
  const rows = Math.max(2, Math.floor((max - borders) / rowH));
  area.style.maxBlockSize = `${rows * rowH + borders}px`;
}

/**
 * Builds the shared progress view.
 * @param {{label: string, jobId?: number, compact?: boolean,
 *   framed?: boolean, steps?: boolean, logHeight?: string,
 *   logOpts?: Parameters<typeof jobLogViewer>[1], dataJob?: string,
 *   log?: ReturnType<typeof jobLogViewer>}} opts
 *   `compact`: no heading (inside a dialog that has its own); `framed`: the
 *   view draws its own card frame (a page; a dialog is the frame already —
 *   run-dialog-double-frame-1); `steps: false` leaves the step list out
 *   (the Restore wizard draws its own); `log`: a log viewer the caller
 *   built (a test passes a stub)
 */
export function buildRunView(opts) {
  const seenSteps = new Map();
  let fitted = false;
  const guard = signatureGuard();
  /** @type {Map<string, boolean>} whether each appearing part was shown */
  const shown = new Map();

  // ---- the head ----------------------------------------------------------
  const title = h("h3", { class: "rv-title" });
  const sub = h("p", { class: "measured rv-sub" });
  const badgeHost = h("span", { class: "rv-badge" });
  // The head is built after the figures (below), which sit in it.

  // ---- the progress column -----------------------------------------------
  const doneN = h("b", { "data-kp-tick": "" }, "0");
  const ofN = h("span", null, "");
  const stepsNote = h("p", { class: "measured rv-note rv-steps-note" });
  const tookDd = h("dd", { class: "rv-fact" }, "—");
  const medianText = h("span", null, "—");
  const delta = h("span", { class: "kp-badge", hidden: "" });
  const figures = h(
    "dl",
    { class: "rv-figures job-figures", "aria-label": "The run in figures" },
    h(
      "div",
      { class: "rv-figure" },
      h("dt", null, "Steps done"),
      h("dd", { class: "rv-fact" }, doneN, " of ", ofN),
    ),
    h("div", { class: "rv-figure" }, h("dt", null, "Time taken"), tookDd),
    h(
      "div",
      { class: "rv-figure" },
      h("dt", null, "Median for this operation"),
      h("dd", { class: "rv-fact" }, medianText, " ", delta),
    ),
  );
  const bar = progressBar("How far the run is", null, "rv-progress");
  const barNote = h("p", { class: "measured rv-note rv-bar-note" });
  const bytesBar = progressBar("How much data has moved", null, "rv-bytes");
  // The one line under the bar is written by progress.js (contract 5).
  const jobLine = h("p", {
    class: "kp-jobline",
    "aria-label": "Where the run stands",
  });
  const eta = steadyEta();
  jobLine.setAttribute("role", "status");
  jobLine.setAttribute("aria-live", "off");
  jobLine.setAttribute("aria-atomic", "true");
  jobLine.classList.add("measured", "rv-note");
  const discEl = h("div", {
    class: "kp-timer-disc rv-disc__dial",
    "data-kp-timer-disc": "1m",
    "data-kp-timer-disc-name": "Time left on the run",
    "data-kp-timer-disc-controls": "none",
  });
  const discText = h("p", { class: "measured rv-note rv-disc__text" });
  const disc = h("div", { class: "rv-disc" }, discEl);
  const now = h(
    "div",
    { class: "rv-now" },
    h("div", { class: "rv-bar" }, bar, barNote, bytesBar, jobLine, discText),
    disc,
  );
  // A flow that needs one more control above its steps (the Update flow's
  // layout choice) puts it here: a row of its own, always there.
  const lead = h("div", { class: "rv-lead" });
  lead.hidden = true;
  // A step that begins is listed at once, at full height: the run's own
  // progress is not news to play an arrival for (Kenny, 2026-10-24: the
  // running step was missing from the list while its row grew in).
  const stepsList = h("div", {
    class: "rv-steps-list",
    "data-kp-arrive": "none",
    hidden: opts.steps === false ? "" : null,
  });
  // Another way to see the same steps (the Update flow's board) shares the
  // step area: the caller shows one and hides the other.
  const alt = h("div", { class: "rv-steps-alt", hidden: "" });
  const stepsArea = h(
    "div",
    {
      class: "rv-steps",
      role: "group",
      "aria-label": "Steps of the run",
      tabindex: "0",
      hidden: opts.steps === false ? "" : null,
    },
    stepsList,
    alt,
  );
  const timeBar = h("figure", { class: "kp-stackbar rv-time" });
  // The legend of a long run has a row per step: the bar lives in an area
  // of its own with its own scroll, so a long legend moves nothing.
  const timeArea = h(
    "div",
    { class: "rv-time-area", "data-kp-arrive": "none" },
    timeBar,
  );
  const timeNote = h("p", { class: "measured rv-note rv-time-note" });
  // The time bar closes the progress half: its legend and its footnote have
  // a place of their own, whole.
  const timeBlock = h(
    "div",
    {
      class: "rv-time-block",
      role: "group",
      "aria-label": "Where the time went",
    },
    timeArea,
    timeNote,
  );
  const progressCol = h(
    "section",
    { class: "rv-col rv-col--progress", "aria-label": "Progress" },
    stepsNote,
    now,
    lead,
    stepsArea,
    timeBlock,
  );
  // The head is split like the body below it (Kenny, 2026-10-24: "het
  // splitsen van het kader moet dus ook dat element omvatten"): title and
  // sub in the left half; the three figures at the start of the right half,
  // the state badge at its end.
  const head = h(
    "header",
    { class: "rv-head" },
    h("div", { class: "rv-head__id" }, ...(opts.compact ? [] : [title]), sub),
    h("div", { class: "rv-head__side" }, figures, badgeHost),
  );

  // ---- the log column ----------------------------------------------------
  const jobLog =
    opts.log ??
    jobLogViewer(opts.jobId ?? 0, {
      height: opts.logHeight ?? "var(--rv-log-h)",
      tools: "search,wrap,level,copy",
      ...(opts.logOpts ?? {}),
    });
  jobLog.element.classList.add("rv-log");
  const restarts = h(
    "div",
    {
      class: "kp-alert kp-alert--warning rv-part",
      role: "status",
      hidden: "",
    },
    "This job restarts the dashboard: the page loses its link for a moment and reads the job's end after the restart.",
  );
  const problems = h("div", {
    class: "kp-problems rv-part",
    "data-kp-problems": "",
    "data-kp-problems-mode": "each",
    hidden: "",
  });
  const code = h("div", { class: "kp-code rv-part", hidden: "" });
  const outcome = h("div", { class: "rv-part", hidden: "" });
  const basis = h("p", { class: "measured rv-note rv-part", hidden: "" });
  const cli = h("div", { class: "rv-cli rv-part", hidden: "" });
  const resultEmpty = h(
    "p",
    { class: "measured rv-result__empty" },
    "Nothing to report yet. If the run fails, what happened and the command to run it again appear here.",
  );
  const result = h(
    "div",
    {
      class: "rv-result",
      role: "region",
      "aria-label": "What happened",
      tabindex: "0",
    },
    resultEmpty,
    restarts,
    problems,
    code,
    outcome,
    basis,
    cli,
  );
  const logCol = h(
    "section",
    { class: "rv-col rv-col--log", "aria-label": "Log" },
    jobLog.element,
    result,
  );

  const element = h(
    "section",
    {
      class: `rv job-panel${opts.framed ? " kp-card" : ""}${opts.compact ? " rv--compact" : ""}`,
      "aria-label": opts.label,
      ...(opts.dataJob ? { "data-job": opts.dataJob } : {}),
    },
    head,
    h("div", { class: "rv-grid" }, progressCol, logCol),
  );
  const plate = h("div", { class: "rv-plate", hidden: "" });
  element.append(plate);
  const detachKp = [attachTicks(element), attachProblems(element)];
  let discWired = false;

  /**
   * Runs `draw` only when `sig` differs from this part's last one.
   * @param {string} key
   * @param {unknown} sig
   * @param {() => void} draw
   */
  const memo = (key, sig, draw) => void guard(key, sig, draw);

  /**
   * Shows or hides a part of the result area. The opening animation is
   * played once, on the change from hidden to shown (never on an update).
   * @param {string} key
   * @param {HTMLElement} el
   * @param {boolean} on
   */
  const reveal = (key, el, on) => {
    const was = shown.get(key) === true;
    shown.set(key, on);
    el.hidden = !on;
    if (on && !was) {
      el.classList.add("rv-appear");
      el.addEventListener(
        "animationend",
        () => el.classList.remove("rv-appear"),
        { once: true },
      );
    }
    const any = [...shown.values()].some(Boolean);
    resultEmpty.hidden = any;
  };

  /** @param {RunSpec} spec */
  const paint = (spec) => {
    if (spec.head) {
      const hd = spec.head;
      memo("head", hd, () => {
        title.textContent = hd.title;
        sub.textContent = hd.sub;
        badgeHost.replaceChildren(
          ...(hd.badge
            ? [
                h(
                  "span",
                  { class: `kp-badge kp-badge--${hd.badge.tone}` },
                  hd.badge.label,
                ),
              ]
            : []),
        );
        reveal("restarts", restarts, hd.restarts);
      });
    }
    if (spec.figures) {
      const f = spec.figures;
      memo("figures", f, () => {
        doneN.textContent = String(f.done);
        ofN.textContent = f.of;
        setNote(stepsNote, f.note);
        // Kept in place when empty: the line is reserved, never removed.
        stepsNote.style.visibility = f.note ? "" : "hidden";
        tookDd.textContent = f.took;
        medianText.textContent = f.median;
        delta.hidden = f.delta == null;
        if (f.delta) {
          delta.className = `kp-badge kp-badge--${f.delta.tone}`;
          delta.textContent = f.delta.text;
        }
      });
    }
    if (spec.bar) {
      const b = spec.bar;
      memo("bar", [b.percent, b.note], () => {
        setProgress(bar, b.percent);
        setNote(barNote, b.note);
      });
      memo("bytes", b.bytes, () => {
        bytesBar.hidden = b.bytes == null;
        if (b.bytes) setProgress(bytesBar, b.bytes.percent);
      });
      // The estimate never grows within a step (progress.js, contract 6), so
      // it is settled before the line's signature is read.
      const line = b.line
        ? { ...b.line, etaSeconds: eta(b.line.step, b.line.etaSeconds) }
        : null;
      memo("line", line, () => {
        jobLine.textContent = line ? progressLine(line) : "";
        jobLine.hidden = line == null;
      });
    }
    if (spec.steps && opts.steps !== false) {
      const items = spec.steps;
      memo("steps", items, () => {
        syncSteps(stepsList, items, seenSteps, "Steps of the run");
        if (!fitted && items.length) {
          fitted = true;
          fitRows(stepsArea);
        }
        followRunning(stepsArea);
      });
    }
    if (spec.time) {
      const t = spec.time;
      memo("time", t, () => {
        if (t.kind === "empty") {
          setStackbar(timeBar, {
            title: "Where the time went",
            state: "empty",
            message: t.message,
          });
          setNote(timeNote, "");
          return;
        }
        setStackbar(timeBar, {
          title: "Where the time went",
          total: t.whole,
          format: (/** @type {number} */ s) => humanDuration(s),
          restLabel: t.restLabel,
          parts: t.parts.map((p) => ({
            key: p.key,
            label: p.label,
            value: p.value,
            ...(p.bad ? { tone: /** @type {const} */ ("destructive") } : {}),
          })),
          ...(t.medianS != null
            ? {
                marks: [
                  {
                    at: Math.min(1, t.medianS / t.whole),
                    label: `median ${humanDuration(t.medianS)}`,
                  },
                ],
              }
            : {}),
        });
        setNote(timeNote, t.note);
      });
    }
    if (spec.disc) {
      const d = spec.disc;
      // The dial itself ticks on its own; only a change of the estimate by
      // more than 3 s restarts it.
      memo("disc", [d.running, d.text], () => {
        setNote(discText, d.text);
        disc.classList.toggle("rv-disc--off", !d.running);
      });
      if (d.running && d.leftS != null) {
        if (!discWired) {
          discWired = true;
          discEl.setAttribute("data-kp-timer-disc-scale", String(d.scaleMin));
          attachTimerDiscs(disc);
        }
        const clk = timerDiscOf(discEl);
        if (
          clk &&
          (clk.state() !== "running" ||
            Math.abs(clk.remaining() / 1000 - d.leftS) > 3)
        ) {
          clk.reset(Math.max(1, d.leftS));
          clk.start();
        }
      } else timerDiscOf(discEl)?.pause();
    }
    if (spec.result) {
      const r = spec.result;
      memo("problems", r.problems, () => {
        setProblems(problems, /** @type {any} */ (r.problems));
        reveal("problems", problems, r.problems.length > 0);
      });
      // redraw-unchanged-1: the host's words are drawn once per text; the
      // code block's opening animation never replays on an update.
      memo("code", r.code, () => {
        if (r.code) setCode(code, { ...r.code, maxLines: 6 });
        reveal("code", code, r.code != null);
      });
      memo("outcome", r.outcome, () => {
        outcome.replaceChildren(
          ...(r.outcome
            ? [
                h(
                  "div",
                  {
                    class: `kp-alert kp-alert--${r.outcome.tone}`,
                    role: "status",
                    "data-kp-semantic": "",
                  },
                  h(
                    "span",
                    { class: "kp-alert__body" },
                    h(
                      "span",
                      { class: "kp-alert__label" },
                      `${r.outcome.title}: `,
                    ),
                    r.outcome.text,
                  ),
                ),
              ]
            : []),
        );
        reveal("outcome", outcome, r.outcome != null);
      });
      memo("basis", r.basis, () => {
        basis.textContent = r.basis;
        reveal("basis", basis, r.basis !== "");
      });
    }
  };

  /**
   * The command line of the run, in its own part of the result area: the
   * label on its own line, the command block under it
   * (workstation-line-1). Built by the caller (actui's labeledCliCommand)
   * and only replaced when the command changes.
   * @param {string} command
   * @param {() => HTMLElement} build
   */
  const setCli = (command, build) =>
    memo("cli", command, () => {
      cli.replaceChildren(...(command ? [build()] : []));
      reveal("cli", cli, command !== "");
    });

  return {
    element,
    plate,
    /** The slots a flow may fill: above the steps, beside the list. */
    slots: { lead, alt, stepsList, stepsArea },
    log: jobLog,
    paint,
    setCli,
    /** @param {boolean} on */
    setLead: (on) => {
      lead.hidden = !on;
    },
    stop: () => {
      jobLog.stop();
      timerDiscOf(discEl)?.stop();
      for (const off of detachKp) off();
    },
  };
}
