// kp11 Activity: the History as kp's data table (the approved demo
// app-homelab-activity, section 3): a heading row per day that folds, one
// row per operation that opens to its facts, its steps and its error. This
// module draws the rows and the opened detail; the table (sort, filters,
// page sizes, remembered state) is kp's.

import { h } from "./dom.js";
import {
  byDay,
  dayLabel,
  outcomeTone,
  outcomeWord,
  stackWord,
} from "./activityview.js";
import { formatDateTime, humanDuration } from "./format.js";
import { buildSteplist, setCode } from "/static/kp/dist/kp-themes.min.js";

/** The History's columns, in order (kp datatable headers). */
export const HISTORY_COLUMNS = /** @type {const} */ ([
  { label: "Started", sort: "number" },
  { label: "Stack", sort: "text", filter: "choice" },
  { label: "Operation", sort: "text", filter: "choice" },
  { label: "By", sort: "text", filter: "choice" },
  {
    label: "Outcome",
    sort: "text",
    order: "Failed,Running,Deferred,Round,Done",
    filter: "choice",
  },
  { label: "Took", sort: "number", cls: "kp-text-end" },
]);

/** Days older than this many are folded when the table first draws. */
export const FOLDED_FROM_DAYS = 3;

/** How long an operation took, or has taken so far. @param {import("./activityview.js").FeedRow} r */
export const tookWords = (r) => (r.took == null ? "—" : humanDuration(r.took));

/**
 * The table's body: a heading row per day, then each operation with its
 * (empty) detail row.
 * @param {import("./activityview.js").FeedRow[]} rows newest first
 * @param {{now: number, stackLink: (name: string) => HTMLElement}} x
 * @returns {HTMLElement[]}
 */
export function historyBody(rows, x) {
  /** @type {HTMLElement[]} */
  const out = [];
  const today = dayStart0(x.now);
  for (const d of byDay(rows)) {
    const age = Math.round((today - d.day) / 86400);
    out.push(
      h(
        "tr",
        {
          "data-kp-row-group": String(d.day),
          "data-kp-folded": age >= FOLDED_FROM_DAYS ? "" : null,
        },
        h(
          "th",
          { scope: "rowgroup", colspan: "6" },
          dayLabel(d.day, x.now),
          " ",
          h("span", { class: "kp-badge", "data-kp-group-count": "" }),
        ),
      ),
    );
    for (const r of d.rows) {
      const stack = stackWord(r);
      out.push(
        h(
          "tr",
          {
            class: `ac-row${r.entry.kind === "phase" ? " ac-row--phase" : ""}`,
            "data-kp-row-key": r.key,
            "data-key": r.key,
            "data-kp-group-of": String(d.day),
            "data-tone": r.state === "failed" ? "bad" : null,
          },
          h(
            "td",
            {
              "data-label": "Started",
              class: "kp-numeric",
              "data-kp-sort-value": String(r.start),
            },
            formatDateTime(r.start),
          ),
          h(
            "td",
            { "data-label": "Stack" },
            r.stack ? x.stackLink(r.stack) : stack,
          ),
          h("td", { "data-label": "Operation" }, r.verb),
          h(
            "td",
            { "data-label": "By" },
            h(
              "span",
              { class: `ac-by-chip ac-by-chip--${r.actor.kind}` },
              r.actor.text,
            ),
          ),
          h(
            "td",
            { "data-label": "Outcome" },
            h(
              "span",
              { class: `kp-badge kp-badge--${outcomeTone(r)}` },
              outcomeWord(r),
            ),
          ),
          h(
            "td",
            {
              "data-label": "Took",
              class: "kp-text-end kp-numeric",
              "data-kp-sort-value": String(Math.round(r.took ?? 0)),
            },
            tookWords(r),
          ),
        ),
        h(
          "tr",
          { "data-kp-row-detail": "", hidden: "" },
          h("td", { colspan: "6" }),
        ),
      );
    }
  }
  return out;
}

/** @param {number} t unix seconds, the start of its local day */
function dayStart0(t) {
  const d = new Date(t * 1000);
  d.setHours(0, 0, 0, 0);
  return Math.floor(d.getTime() / 1000);
}

/**
 * The opened row: its facts and its steps side by side, the error as a code
 * block with Copy, then what can be done about it.
 * @param {import("./activityview.js").FeedRow} r
 * @param {{actions: HTMLElement[]}} x
 * @returns {HTMLElement}
 */
export function historyDetail(r, x) {
  const e = r.entry;
  /** @type {[string, string][]} */
  const facts = [
    ["Operation", r.what],
    ["Started", formatDateTime(r.start)],
    [
      "Finished",
      r.took == null ? "still running" : formatDateTime(r.start + r.took),
    ],
    ["Took", r.took == null ? "so far: —" : humanDuration(r.took)],
    ["By", r.actor.text],
  ];
  /** @type {import("/static/kp/dist/kp-themes.min.js").StepRow[]} */
  const rows =
    e.kind === "op" && e.steps.length
      ? e.steps.map((s, i) => {
          const last = i === e.steps.length - 1;
          const failed = r.state === "failed" && last;
          const time =
            s.end > 0 && s.end >= s.start ? humanDuration(s.end - s.start) : "";
          return {
            id: String(i),
            title: s.step,
            note: failed
              ? "failed here"
              : s.changed
                ? "changed something"
                : "nothing to change",
            state: failed
              ? "failed"
              : r.took == null && last
                ? "running"
                : "done",
            ...(time ? { time } : {}),
          };
        })
      : [
          {
            id: "0",
            title: r.what,
            note:
              e.kind === "phase"
                ? "the nightly round, as one"
                : "this operation, as one step",
            state:
              r.state === "failed"
                ? "failed"
                : r.took == null
                  ? "running"
                  : "done",
            ...(r.took != null ? { time: humanDuration(r.took) } : {}),
          },
        ];
  const detail = h(
    "div",
    { class: "ac-detail" },
    h(
      "dl",
      { class: "kp-kv ac-detail__facts" },
      ...facts.map(([k, v]) =>
        h("div", { class: "kp-kv__row" }, h("dt", null, k), h("dd", null, v)),
      ),
    ),
    buildSteplist(rows, { label: `Steps of ${r.what}` }),
  );
  if (r.error) {
    const code = h("div", { class: "kp-code ac-detail__wide" });
    detail.append(code);
    setCode(code, {
      text: r.error,
      title: r.state === "failed" ? "What the host said" : "Why",
      maxLines: 6,
    });
  }
  detail.append(h("div", { class: "ac-acts ac-detail__wide" }, ...x.actions));
  return detail;
}
