// Deploy all changes on kp-themes 11 (feat-kp11demo-13, the approved apply
// demo; its plan table was rejected and redone with the theme's own loading
// state over the table's footprint). Pure helpers first: each plan item as
// one table row (action, what changed, files, +N −M), the sentence under the
// table and the rules the selection bar enforces. The DOM builders are
// below.

import { h } from "./dom.js";

/**
 * @typedef {import("./applyplan.js").Plan} Plan
 * @typedef {{stack: string, action: "deploy" | "new" | "destroy" | "broken" | "unchanged",
 *   note: string, fix?: string}} PlanRow
 */

/**
 * Every stack of the plan as one row: what it would do and why.
 * @param {Plan} p
 * @param {(s: string) => number | null} vmidOf
 * @returns {PlanRow[]}
 */
export function planRows(p, vmidOf) {
  /** @type {PlanRow[]} */
  const out = [];
  for (const s of p.deploy)
    out.push({
      stack: s,
      action: p.new.includes(s) ? "new" : "deploy",
      note:
        p.reasons?.[s] ??
        (p.new.includes(s)
          ? "New: creates its container"
          : "Its files differ from what the host applied"),
    });
  for (const s of p.destroy) {
    const id = vmidOf(s);
    out.push({
      stack: s,
      action: "destroy",
      note: `Its directory is gone from the files; stops and removes ${id != null ? `CT ${id}` : "its container"}, its data kept under Backups until you wipe it`,
    });
  }
  for (const [s, why] of p.broken) {
    const [what, fix] = String(why).split(" :: ");
    out.push({
      stack: s,
      action: "broken",
      note: what,
      ...(fix ? { fix: fix.replace(/\S*\/repo\/(stacks\/)/g, "$1") } : {}),
    });
  }
  for (const s of p.unchanged)
    out.push({
      stack: s,
      action: "unchanged",
      note: "Nothing changed: the files and the host agree",
    });
  return out;
}

/**
 * Lines added and removed in a stack's file diffs.
 * @param {import("./plan.js").FileDiff[]} files
 */
export const plusMinus = (files) => ({
  add: files.reduce((t, f) => t + f.added, 0),
  del: files.reduce((t, f) => t + f.removed, 0),
});

/**
 * The sentence's counts for a selection.
 * @param {Plan} p
 * @param {string[]} keys the ticked stacks
 */
export function sentenceCounts(p, keys) {
  const deploy = keys.filter((k) => p.deploy.includes(k));
  return {
    deploy: deploy.length,
    fresh: deploy.filter((k) => p.new.includes(k)).length,
    destroy: keys.filter((k) => p.destroy.includes(k)).length,
  };
}

/**
 * Why Apply is off for this selection, in words (empty: it may go). A
 * destroy never rides along with a deploy.
 * @param {Plan} p
 * @param {string[]} keys
 */
export function applyRule(p, keys) {
  const c = sentenceCounts(p, keys);
  if (c.destroy > 0 && c.deploy > 0)
    return "A destroy never rides along with a deploy: untick it, or apply it on its own";
  if (c.destroy === 0 && c.deploy === 0)
    return "Tick a stack that will be deployed";
  return "";
}

/** @param {number} n @param {string} one */
const plural = (n, one) => `${n} ${n === 1 ? one : `${one}s`}`;

/**
 * The words of the sentence under the table, as a list of parts (text, or
 * a bold number).
 * @param {{deploy: number, fresh: number, destroy: number}} c
 * @returns {({text: string} | {bold: string})[]}
 */
export function sentenceParts(c) {
  return [
    { text: "Will deploy " },
    { bold: String(c.deploy) },
    {
      text: ` ${c.deploy === 1 ? "stack" : "stacks"} (${c.fresh} new), destroy `,
    },
    { bold: String(c.destroy) },
    { text: ", " },
  ];
}

/**
 * A stack's file diffs as kp diff blocks: a header per file with its
 * +N −M, then its hunks (the range in quiet ink, the place in bold) and
 * lines with a sign column that carries the change without colour.
 * @param {import("./plan.js").FileDiff[]} files
 * @returns {HTMLElement}
 */
export function diffFiles(files) {
  if (!files.length)
    return h(
      "p",
      { class: "ap-none" },
      "No file changed since the last apply.",
    );
  return h(
    "div",
    { class: "ap-diffs" },
    ...files.map((f) =>
      h(
        "div",
        { class: "ap-diff" },
        h(
          "div",
          { class: "ap-diff__file" },
          h("span", { class: "kp-mono" }, f.path),
          h("span", { class: "ap-add" }, `+${f.added}`),
          h("span", { class: "ap-del" }, `−${f.removed}`),
        ),
        h(
          "pre",
          { class: "kp-diff", "aria-label": `Changes to ${f.path}` },
          ...f.hunks.flatMap((hk) => [
            h(
              "div",
              { class: "kp-diff__hunk" },
              `@@ −${hk.old_start},${hk.old_len} +${hk.new_start},${hk.new_len} @@`,
            ),
            ...hk.lines.map((l) =>
              h(
                "span",
                {
                  class: "kp-diff__line",
                  "data-kind":
                    l.op === "+" ? "added" : l.op === "-" ? "removed" : "same",
                },
                h(
                  "span",
                  { class: "kp-diff__number" },
                  String((l.op === "+" ? l.new : l.old) ?? ""),
                ),
                h(
                  "span",
                  { class: "kp-diff__sign" },
                  l.op === "+" ? "+" : l.op === "-" ? "-" : " ",
                ),
                h("span", null, l.text),
              ),
            ),
          ]),
        ),
      ),
    ),
  );
}

export { plural };
