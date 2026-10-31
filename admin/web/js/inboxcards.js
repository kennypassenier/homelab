// kp11 (feat-kp11demo-4, Kenny approved the kp-themes review demo
// app-homelab-inbox in full, 2026-10-19): the cards of the Inbox page
// that kp-themes 11.0.0 draws for us beside the list itself.
//
//   questions   the host's open questions as question cards, each with a
//               clock, the consequence of each answer and its safe answer
//   bulk        the notices of the last 24 hours, to tick and act on
//               together (Mark seen with an Undo window, Fix all as one
//               batch)
//
// Each is `mountX(...)` returning `{el, paint, destroy}`: `el` is the card,
// `paint()` reads the live state again (the page calls it on every change),
// `destroy()` lets go. The pure parts (the question's spec, the bulk rows,
// the Fix all rule) are in inboxrows.js, where node tests hold them.

import { act, loadNotices, send } from "./act.js";
import { openBatch } from "./actiondialog.js";
import { answerBody, openAsks } from "./asks.js";
import { h } from "./dom.js";
import { declare, drivable } from "./drivable.js";
import { formatDateTime } from "./format.js";
import {
  askId,
  askKey,
  askSpec,
  bulkRows,
  fixAllBatch,
  fixAllRule,
  plural,
} from "./inboxrows.js";
import { kindBadge, levelBadge } from "./notices.js";
import { current, rereadStore, setAsks, subscribe } from "./store.js";
import { toastRegion } from "./ui.js";
import {
  attachCardStates,
  setCardState,
  askQuestion,
  attachQuestions,
  questionOf,
  attachDisabledReasons,
  attachRowSelect,
  attachSelectionBars,
  setSelectionRules,
  undoToast,
} from "/static/kp/dist/kp-themes.min.js";

const ANSWER = declare({
  id: "inbox-answer",
  page: "needs-you",
  opens: "run",
  row: "<ask key>:allow|stop",
  what: "answer the host's question: allow, or leave it stopped",
  shows: "while the host asks whether an operation may go on",
});
const ANSWER_ALL = declare({
  id: "inbox-answer-all-stop",
  page: "needs-you",
  opens: "run",
  what: "answer every open question with its safe answer, leave it stopped",
  shows: "while the host asks a question",
});
const BULK_SEEN = declare({
  id: "inbox-bulk-seen",
  page: "needs-you",
  opens: "run",
  what: "mark every ticked notice seen, after a 6 s window to take it back",
  shows: "while a notice of the last 24 hours is ticked",
});
const BULK_FIX = declare({
  id: "inbox-bulk-fix",
  page: "needs-you",
  opens: "dialog",
  what: "fix every ticked notice as one batch, with one confirm",
  shows: "while a notice of the last 24 hours is ticked",
});
const BULK_UNDO = declare({
  id: "inbox-bulk-undo",
  page: "needs-you",
  opens: "run",
  what: "take a bulk Mark seen back before its clock ends",
  shows: "for 6 seconds after a bulk Mark seen",
});

/** @type {WeakMap<HTMLElement, string>} */
const shownState = new WeakMap();

/**
 * `setCardState`, but only when the state or its words changed: the pages
 * paint on every live event, and a card must not redraw its plate for it.
 * @param {HTMLElement} el
 * @param {import("/static/kp/dist/kp-themes.min.js").CardState} state
 * @param {import("/static/kp/dist/kp-themes.min.js").CardStateOptions} [options]
 */
export function putState(el, state, options) {
  const key = `${state}|${options?.title ?? ""}|${options?.text ?? ""}|${options?.words ?? ""}`;
  if (shownState.get(el) === key) return;
  shownState.set(el, key);
  setCardState(el, state, options);
}

let cardSerial = 0;

/**
 * One card of the Inbox page: a kp card with its title, the sentence that
 * says what it does, its right-hand readout and a body that states replace.
 * @param {{title: string, desc: string, id?: string, actions?: Node[],
 *   body: Node[], before?: Node[]}} spec
 */
function card(spec) {
  cardSerial += 1;
  const titleId = `ib-card-${cardSerial}-t`;
  const el = h(
    "section",
    {
      class: "kp-card ib-card",
      "data-kp-state": "loading",
      "aria-labelledby": titleId,
      id: spec.id,
    },
    h(
      "div",
      { class: "kp-card__header" },
      h("h2", { class: "kp-card__title", id: titleId }, spec.title),
      h("div", { class: "kp-card__actions" }, ...(spec.actions ?? [])),
    ),
    h("p", { class: "kp-card__body" }, spec.desc),
    ...(spec.before ?? []),
    h(
      "div",
      { class: "kp-card__stage" },
      h("div", { class: "kp-card__data" }, ...spec.body),
    ),
  );
  return el;
}

/**
 * @param {HTMLElement} el
 * @param {string} name
 * @param {(event: CustomEvent) => void} fn
 */
const on = (el, name, fn) =>
  el.addEventListener(name, (event) => fn(/** @type {CustomEvent} */ (event)));

// ═══ the host's questions ═══════════════════════════════════════════════

/**
 * The host's open questions as question cards (demo section 3).
 * @param {HTMLElement} page the page root, for the toast region
 */
export function mountQuestions(page) {
  const box = h("div", {
    class: "ib-questions",
    "data-kp-questions": "",
    "data-kp-title": "off",
    "aria-label": "Questions from the host",
  });
  const count = h("b", { class: "ib-qcount", "aria-live": "polite" });
  const all = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm",
        title:
          "Answer every open question with its safe answer: leave it stopped",
      },
      "Leave all stopped",
    ),
    ANSWER_ALL,
  );
  const el = card({
    title: "Questions from the host",
    desc: "When an operation on pve stops and waits for you, the card says what each answer would set in motion and how long the host still waits; if nobody answers, the safe answer happens.",
    id: "inbox-questions",
    actions: [count, all],
    body: [box],
  });
  attachCardStates(el);
  attachQuestions(box);
  const detachReasons = attachDisabledReasons(el);

  /** @type {Map<string, {ask: import("./asks.js").Ask, handle: import("/static/kp/dist/kp-themes.min.js").QuestionHandle}>} */
  const cards = new Map();
  const OPEN = ["waiting", "urgent", "refused", "sending"];

  const openCards = () =>
    [...cards.values()].filter((c) => OPEN.includes(c.handle.state()));

  const markAnswers = () => {
    for (const { ask, handle } of cards.values())
      for (const b of /** @type {NodeListOf<HTMLElement>} */ (
        handle.element.querySelectorAll(".kp-question__button")
      ))
        drivable(
          b,
          ANSWER,
          `${askKey(ask)}:${b.dataset.kpValue === "allow" ? "allow" : "stop"}`,
        );
  };

  const readout = () => {
    const open = openCards().length;
    count.textContent = plural(open, "question") + " waiting";
    all.toggleAttribute("disabled", open === 0);
    if (open === 0)
      all.setAttribute("data-kp-disabled-reason", "No question is waiting");
    else all.removeAttribute("data-kp-disabled-reason");
    const read = current().asksRead;
    if (cards.size === 0 && read !== "ready") {
      putState(
        el,
        read,
        read === "error"
          ? {
              title: "The questions could not be read",
              text: "The host's open questions did not come through; they appear here as soon as a read succeeds.",
            }
          : { words: "Reading the questions…" },
      );
      return;
    }
    putState(
      el,
      cards.size === 0 ? "empty" : "ready",
      cards.size === 0
        ? {
            title: "No question from the host",
            text: "When an operation on pve stops and asks, its question appears here with the time the host still waits.",
          }
        : undefined,
    );
  };

  const paint = () => {
    const now = Date.now() / 1000;
    const open = openAsks(current().asks, now);
    const ids = new Set(open.map(askId));
    for (const a of open) {
      const id = askId(a);
      if (cards.has(id)) continue;
      cards.set(id, { ask: a, handle: askQuestion(box, askSpec(a)) });
    }
    // A question answered somewhere else (the terminal, another tab) is
    // taken off while it still waits; one this card answered keeps its
    // outcome until it is dismissed.
    for (const [id, c] of cards)
      if (
        !ids.has(id) &&
        ["waiting", "urgent", "refused"].includes(c.handle.state())
      ) {
        c.handle.dismiss();
        cards.delete(id);
      }
    markAnswers();
    readout();
  };

  on(box, "kp-question-answer", async (e) => {
    const d = e.detail;
    const entry = cards.get(d.id);
    if (!entry) return;
    const res = await send(
      "POST",
      "/data/asks/answer",
      answerBody(entry.ask, d.value === "allow"),
      "the answer",
    );
    if (res.ok) {
      d.respond(true);
      // Taken off the store's list now; the card keeps its outcome.
      setAsks(
        current().asks.filter(
          (x) => !(x.id === entry.ask.id && x.boot === entry.ask.boot),
        ),
      );
    } else d.respond(false, res.error.why || "the host did not answer");
  });
  on(box, "kp-question-dismiss", (e) => {
    cards.delete(e.detail.id);
    readout();
  });
  on(box, "kp-question-expire", readout);
  all.addEventListener("click", () => {
    for (const { handle } of openCards()) {
      const q = questionOf(handle.element);
      if (q && ["waiting", "urgent", "refused"].includes(q.state()))
        q.answer("stop");
    }
  });
  const watch = new MutationObserver(() => {
    markAnswers();
    readout();
  });
  watch.observe(box, {
    subtree: true,
    attributes: true,
    attributeFilter: ["data-kp-state"],
    childList: true,
  });
  const off = subscribe(paint);
  el.addEventListener("kp-card-retry", () => void rereadStore());
  paint();

  return {
    el,
    paint,
    /** Take the first answer button of a question into the view and focus it. @param {string} key */
    focusAsk(key) {
      const entry = [...cards.values()].find((c) => askKey(c.ask) === key);
      const target = entry?.handle.element;
      if (!target) return;
      target.scrollIntoView({ block: "center" });
      /** @type {HTMLElement | null} */ (
        target.querySelector(".kp-question__button")
      )?.focus();
    },
    destroy() {
      off();
      watch.disconnect();
      detachReasons();
    },
  };
}

// ═══ the notices of the last 24 hours ═══════════════════════════════════

const TONE_BADGE = /** @type {const} */ ({
  bad: "kp-badge kp-badge--destructive",
  warn: "kp-badge kp-badge--warning",
  ok: "kp-badge kp-badge--success",
  info: "kp-badge",
});
const TONE_STATE = /** @type {const} */ ({
  bad: "failed",
  warn: "warn",
  ok: "new",
  info: "new",
});

/**
 * The notices of the last 24 hours, to tick and act on together (demo
 * section 2).
 * @param {HTMLElement} page the page root, for the toast region
 */
export function mountBulk(page) {
  const count = h("span", { class: "kp-timestamp", "aria-live": "polite" });
  const seenBtn = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button",
        "data-kp-selection-action": "seen",
        title:
          "Mark the ticked notices seen; you can take it back for 6 seconds",
      },
      "Mark seen",
    ),
    BULK_SEEN,
  );
  const fixBtn = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--primary",
        "data-kp-selection-action": "fix",
        title: "Fix the ticked notices as one batch; one confirm",
      },
      "Fix all",
    ),
    BULK_FIX,
  );
  const bar = h(
    "div",
    {
      class: "kp-selection ib-bar",
      "data-kp-selection": "",
      "data-kp-selection-for": "ib-bulk",
      "data-kp-placement": "inline",
      "data-kp-undo-ms": "6000",
      id: "ib-bulk-bar",
      "aria-label": "Selected notices",
    },
    h("div", { class: "kp-selection__actions" }, seenBtn, fixBtn),
  );
  const list = h("ul", {
    class: "kp-action-list kp-action-list--lead ib-bulk",
    "data-kp-select-keys": "",
    "aria-label": "Notices of the last 24 hours",
    id: "ib-bulk",
  });
  const el = card({
    title: "Notices of the last 24 hours",
    desc: "Lets you clear a run of notices in one go: tick them with the keyboard or the mouse, then mark them seen or fix them together; Undo stays for 6 seconds.",
    id: "inbox-bulk",
    actions: [count],
    body: [bar, list],
  });
  attachCardStates(el);

  /** Notices a Mark seen is still waiting to send: out of the list at once. @type {Set<number>} */
  const held = new Set();
  /** @type {Map<number, import("./notices.js").Notice>} */
  let byId = new Map();
  let allIds = /** @type {number[]} */ ([]);
  /** @type {(() => void)[]} */
  let detach = [];
  let lastSig = "";

  /** @param {import("./notices.js").Notice} n */
  const rowEl = (n) => {
    const tone = levelBadge(n).tone;
    const word = kindBadge(n.kind).label;
    return h(
      "li",
      {
        "data-kp-row-key": `n:${n.id}`,
        "data-kp-state": TONE_STATE[tone],
        "data-kp-row-label": n.title,
      },
      h(
        "span",
        { class: "kp-action-list__lead" },
        h("input", {
          class: "kp-field__check",
          type: "checkbox",
          "data-kp-select-row": "",
          value: `n:${n.id}`,
          "aria-label": `Select ${n.title}`,
        }),
      ),
      h(
        "div",
        { class: "kp-action-list__main" },
        h("strong", { class: "kp-action-list__title" }, n.title),
        h(
          "span",
          { class: "kp-action-list__text" },
          n.consequence || n.body || "",
        ),
        h(
          "span",
          { class: "kp-action-list__meta" },
          h(
            "span",
            { class: TONE_BADGE[tone] },
            word.charAt(0).toUpperCase() + word.slice(1),
          ),
          ...(n.stack ? [h("span", null, n.stack)] : []),
          h("span", null, formatDateTime(n.at)),
        ),
      ),
    );
  };

  const paint = () => {
    const snap = act.notices;
    if (!snap) {
      putState(
        el,
        act.failed.notices ? "error" : "loading",
        act.failed.notices
          ? {
              title: "The notices could not be read",
              text: "The dashboard did not get them. Try again.",
            }
          : { words: "Reading the notices…" },
      );
      count.textContent = "";
      return;
    }
    const { rows, matching, ids } = bulkRows(
      snap.notices.filter((n) => !held.has(n.id)),
      Date.now() / 1000,
    );
    byId = new Map(snap.notices.map((n) => [n.id, n]));
    allIds = ids;
    count.textContent = `${rows.length} shown · ${matching} match`;
    if (rows.length === 0) {
      putState(el, "empty", {
        title: "No unread notice in the last 24 hours",
        text: "A notice nobody marked seen appears here to tick and act on with others.",
      });
      lastSig = "";
      return;
    }
    putState(el, "ready");
    const sig = JSON.stringify([
      rows.map((n) => [n.id, n.title, n.read]),
      matching,
    ]);
    if (sig === lastSig) return;
    lastSig = sig;
    const ticked = new Set(
      [...list.querySelectorAll("[data-kp-select-row]:checked")].map(
        (b) => /** @type {HTMLInputElement} */ (b).value,
      ),
    );
    list.replaceChildren(...rows.map(rowEl));
    for (const b of /** @type {NodeListOf<HTMLInputElement>} */ (
      list.querySelectorAll("[data-kp-select-row]")
    ))
      b.checked = ticked.has(b.value);
    if (matching > rows.length)
      bar.setAttribute("data-kp-select-matching", String(matching));
    else bar.removeAttribute("data-kp-select-matching");
    for (const d of detach) d();
    detach = [attachRowSelect(el), attachSelectionBars(el)];
    setSelectionRules(bar, {
      fix: (selected) =>
        fixAllRule(
          selected.map((i) => Number(i.key.slice(2))),
          byId,
        ),
    });
  };

  on(bar, "kp-selection-action", (e) => {
    const { action, keys, matching } = e.detail;
    const ids = /** @type {number[]} */ (
      matching
        ? allIds
        : keys.map((/** @type {string} */ k) => Number(k.slice(2)))
    );
    if (!ids.length) return;
    if (action === "fix") {
      const batch = fixAllBatch(ids, byId);
      if (batch) void openBatch(batch.action, batch.stacks);
      return;
    }
    if (action !== "seen") return;
    for (const id of ids) held.add(id);
    paint();
    const release = () => {
      for (const id of ids) held.delete(id);
      lastSig = "";
      paint();
    };
    const shown = undoToast(`${plural(ids.length, "notice")} marked seen`, {
      region: toastRegion(page),
      ms: e.detail.undoMs ?? 6000,
      max: 3,
      commit: async () => {
        const res = await send(
          "POST",
          "/data/notifications/read",
          { ids, read: true },
          "marking the notices seen",
        );
        if (!res.ok) throw new Error(res.error.why);
        await loadNotices();
        for (const id of ids) held.delete(id);
      },
      undo: release,
      restore: release,
    });
    for (const b of shown.toast.querySelectorAll("button"))
      if ((b.textContent ?? "").trim() === "Undo") drivable(b, BULK_UNDO);
  });

  paint();

  return {
    el,
    paint,
    destroy() {
      for (const d of detach) d();
      detach = [];
    },
  };
}
