// manual-checks-as-info-1 (Kenny, 2026-10-23: "Die manuele checks moeten ook
// geen notificaties meer zijn ... eerder info bij een stack om problemen te
// kunnen debuggen ... Ik ga echt niet elke keer dat ik een stack update, al
// die shit checken"): the checks only a person can confirm are information
// on the page of the stack they belong to (the stack-less standing one on
// Host). A section "When something goes wrong, check this" lists them in a
// grid: what to look at, where, when someone last confirmed it, and an
// optional "I checked it" that only records the date (the answer-check
// action with "ok"). Nothing here is an alert: no count, no tone, nothing
// "open". The pure rows are checks.js `whenToCheckRows`.

import { send } from "./act.js";
import { onAnswered } from "./answer.js";
import { whenToCheckRows } from "./checks.js";
import { ensureStyle, fetchReport, h } from "./dom.js";
import { drivable } from "./drivable.js";
import { formatDateTime } from "./format.js";
import { section } from "./ui.js";

/** @typedef {ReturnType<typeof whenToCheckRows>[number]} WhenRow */

const COLUMNS = ["What to look at", "Where", "Last confirmed", "Confirm"];

/** Placeholder rows while the list is read: the final geometry. */
const SKELETON_ROWS = 3;

/**
 * @param {WhenRow} r
 * @returns {string}
 */
export function confirmedText(r) {
  if (r.confirmed != null) return formatDateTime(r.confirmed);
  if (r.notOk != null) return `Said not ok on ${formatDateTime(r.notOk)}`;
  return "never";
}

/**
 * @param {{
 *   stack: string,
 *   id: string,
 *   drive: string,
 *   signal?: AbortSignal,
 *   emptyText: string,
 * }} spec `stack`: the stack the host files the checks under ("standing"
 *   for the stack-less one); `id`: the section's id; `drive`: the declared
 *   Live view control id of the "I checked it" button (one per row, row key
 *   = the check's id)
 * @returns {{el: HTMLElement, stop: () => void}}
 */
export function mountWhenCheck(spec) {
  ensureStyle("/css/pages/whencheck.css");
  const card = section({
    id: spec.id,
    title: "When something goes wrong, check this",
    desc: "What only a person can look at, with where to look and when someone last confirmed it. Nothing here is an alert; press “I checked it” if you want the date kept.",
  });
  const grid = h("div", {
    class: "wc-grid",
    role: "table",
    "aria-label": "Things to check when something goes wrong",
    "aria-busy": "true",
  });
  card.body.append(grid);

  /** @type {Map<string, {row: HTMLElement, when: HTMLElement, btn: HTMLButtonElement}>} */
  const rowsById = new Map();

  const head = () =>
    h(
      "div",
      { class: "wc-row wc-row--head", role: "row" },
      COLUMNS.map((c) =>
        h("span", { class: "wc-cell", role: "columnheader" }, c),
      ),
    );

  const paintSkeleton = () => {
    rowsById.clear();
    grid.setAttribute("aria-busy", "true");
    grid.replaceChildren(
      head(),
      ...Array.from({ length: SKELETON_ROWS }, () =>
        h(
          "div",
          {
            class: "wc-row wc-row--skeleton",
            role: "row",
            "aria-hidden": "true",
          },
          COLUMNS.map(() => h("span", { class: "wc-cell wc-bar" })),
        ),
      ),
    );
  };

  /** @param {() => void} again */
  const retryButton = (again) => {
    const b = h(
      "button",
      {
        type: "button",
        class: "kp-button",
        title: "Read the checks again",
      },
      "Retry",
    );
    b.addEventListener("click", again);
    return b;
  };

  /** @param {string} title @param {string} text @param {(() => void) | null} [retry] */
  const paintMessage = (title, text, retry = null) => {
    rowsById.clear();
    grid.setAttribute("aria-busy", "false");
    grid.replaceChildren(
      h(
        "div",
        { class: "wc-message", role: "status" },
        h("strong", null, title),
        h("span", null, text),
        retry ? retryButton(retry) : null,
      ),
    );
  };

  /** @param {WhenRow} r */
  const buildRow = (r) => {
    const when = h("span", {
      class: "wc-cell",
      role: "cell",
      "data-label": COLUMNS[2],
    });
    const btn = /** @type {HTMLButtonElement} */ (
      drivable(
        h(
          "button",
          {
            type: "button",
            class: "kp-button kp-button--sm",
            title:
              "Keep today's date as the last time this was confirmed; nothing else changes",
          },
          "I checked it",
        ),
        spec.drive,
        r.id,
      )
    );
    btn.addEventListener("click", () => {
      btn.disabled = true;
      void send(
        "POST",
        "/data/actions/_host/answer-check",
        { check: r.id, verdict: "ok" },
        "the date of this check",
      ).then((res) => {
        btn.disabled = false;
        if (res.ok) void read();
        else btn.title = `${res.error.what}: ${res.error.why}`;
      });
    });
    const row = h(
      "div",
      { class: "wc-row", role: "row" },
      h(
        "span",
        { class: "wc-cell wc-what", role: "cell", "data-label": COLUMNS[0] },
        h("span", { class: "wc-app" }, r.app),
        h("span", null, r.text),
      ),
      h(
        "span",
        { class: "wc-cell", role: "cell", "data-label": COLUMNS[1] },
        r.url
          ? h(
              "a",
              { href: r.url, target: "_blank", rel: "noopener noreferrer" },
              r.url.replace(/^https?:\/\//, ""),
            )
          : h("span", { class: "wc-none" }, "—"),
      ),
      when,
      h("span", { class: "wc-cell wc-act", role: "cell" }, btn),
    );
    return { row, when, btn };
  };

  /** @param {WhenRow[]} rows */
  const paintRows = (rows) => {
    grid.setAttribute("aria-busy", "false");
    if (!rows.length)
      return paintMessage("Nothing to check here", spec.emptyText);
    // Rows are updated in place by the check's id; a refresh keeps focus.
    const wanted = new Set(rows.map((r) => r.id));
    for (const [id] of rowsById) if (!wanted.has(id)) rowsById.delete(id);
    /** @type {HTMLElement[]} */
    const children = [head()];
    for (const r of rows) {
      let cur = rowsById.get(r.id);
      if (!cur) {
        cur = buildRow(r);
        rowsById.set(r.id, cur);
      }
      cur.when.textContent = confirmedText(r);
      cur.when.classList.toggle("wc-never", r.confirmed == null);
      children.push(cur.row);
    }
    grid.replaceChildren(...children);
  };

  let stopped = false;
  const read = async () => {
    const r = await fetchReport(
      "/data/manual-checks",
      "the checks",
      spec.signal,
    );
    if (stopped) return;
    if (!r.ok)
      return paintMessage(
        "The checks could not be read",
        "The host did not answer.",
        () => {
          paintSkeleton();
          void read();
        },
      );
    paintRows(whenToCheckRows(r.report?.checks ?? [], spec.stack));
  };
  paintSkeleton();
  void read();
  const offAnswered = onAnswered(() => void read());
  return {
    el: card.el,
    stop() {
      stopped = true;
      offAnswered();
      card.stop();
    },
  };
}
