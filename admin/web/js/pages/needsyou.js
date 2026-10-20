// feat-shell-4 + redesign-flows-2 (redesign 3.71.0, decision "Inbox", demo
// flows/inbox.html with health.html's and notifications.html's sources;
// Kenny approved 2026-10-03), rebuilt on kp-themes 11.0.0 (feat-kp11demo-4,
// the review demo app-homelab-inbox, approved in full 2026-10-19):
// everything waiting for a person, worst first. Each row says what is
// wrong, why it matters, and has the button that fixes it — answering the
// host's question, running a failed backup's fix, reviewing the updates,
// pushing the envs, answering a check — so nobody has to know which page
// it came from. A plain click on a kind shows only those (several may be
// on; Esc shows all again). The counter in the bar is exactly the number of
// rows in the list (invariant 60): both read inboxlist.js's one list,
// which inboxsources.js feeds on every page.
//
// The page: the host's questions (question cards with a clock), the list
// (kp action list with groups, a bar for how serious, version jumps and a
// fold for what went wrong), the manual checks one question per screen,
// the notices of the last 24 hours to tick and act on together
// (inboxcards.js), and, folded, "Worth a look" — not urgent, not counted
// (never drilled backups, a newer host release, the host's full doctor
// report).
//
// `?update=…` is the Update flow (pages/update.js) drawn in this page.

import { act, loadNotices, onAct, send } from "../act.js";
import { openAction, openBatch } from "../actiondialog.js";
import { openDialog, refusalCallout } from "../actui.js";
import { ensureStyle, fetchJson, h, slowRead } from "../dom.js";
import { declare, drivable } from "../drivable.js";
import { formatDateTime } from "../format.js";
import { inboxNow, onInbox, snoozedNow } from "../inboxlist.js";
import {
  KINDS,
  askKey,
  askWords,
  chipsOf,
  countWords,
  groupRows,
  kindOf,
  problemOf,
  shownRows,
  worthRows,
} from "../inboxrows.js";
import {
  mountBulk,
  mountChecks,
  mountQuestions,
  putState,
} from "../inboxcards.js";
import { feedToday, feedUpdates, readChecks } from "../inboxsources.js";
import {
  SNOOZE_MAX,
  endSnooze,
  onSnoozeStatus,
  readSnoozes,
  snoozeRow,
  snoozeStatus,
} from "../rowsnooze.js";
import { current, subscribe } from "../store.js";
import { stage } from "../stage.js";
import { countOf, pageHeader } from "../ui.js";
import { ALERT_DISMISS_EVENT } from "/static/kp/js/overlays.js";
import { legacyUpdateHref, updateHref } from "../updateflow.js";
import { attachCardStates, setCardState } from "/static/kp/js/card-state.js";
import { attachRowKeys, removeRow, setRowBusy } from "/static/kp/js/actions.js";
import { setKeys } from "/static/kp/js/keys.js";
import { setProblems } from "/static/kp/js/problems.js";
import {
  attachSegmented,
  itemsOf,
  resetSegmented,
  setSegmentedItems,
  setSegmentedValues,
  valueOf,
} from "/static/kp/js/segmented.js";
import { registerShortcuts } from "/static/kp/js/shortcuts.js";
import {
  SNOOZE_EVENT,
  attachSnoozes,
  setSnoozeBusy,
} from "/static/kp/js/snooze.js";
import { attachVersionDeltas } from "/static/kp/js/version-delta.js";

const CHECK_AGAIN = declare({
  id: "inbox-check-again",
  page: "needs-you",
  opens: "run",
  what: "read every source of the Inbox again now: the fleet check, Today, the manual checks, the notices",
});
const KIND = declare({
  id: "inbox-kind",
  page: "needs-you",
  opens: "view",
  row: "ask|fail|update|setup|check",
  what: "show only one kind of row, or all again (a plain click turns it on or off)",
});
const EXPLAIN = declare({
  id: "inbox-explain-hide",
  page: "needs-you",
  opens: "view",
  what: "hide the note about what the Inbox replaces",
});
const FIX = declare({
  id: "inbox-fix",
  // redesign-final-c3: the notice list left Notification rules for the
  // Inbox; its old control keeps working by its old name.
  was: ["notice-fix"],
  page: "needs-you",
  opens: "dialog",
  row: "<row key>",
  what: "a row's fix: the action's own dialog, prefilled",
});
const SEEN = declare({
  id: "inbox-mark-seen",
  // redesign-final-c3: the notice list left Notification rules for the
  // Inbox; its old control keeps working by its old name.
  was: ["mark-read"],
  page: "needs-you",
  opens: "run",
  row: "<row key>",
  what: "mark a notice as seen (it leaves the Inbox; Activity keeps it)",
  shows: "on an unread notice",
});
const PUSH_ENVS = declare({
  id: "inbox-push-envs",
  page: "needs-you",
  opens: "dialog",
  what: "deploy every stack without a sealed env, so the host's vault takes a copy (one confirm)",
});
const REVIEW = declare({
  id: "inbox-review-updates",
  page: "needs-you",
  opens: "view",
  row: "all|<stack>",
  what: "open the Update flow from the Updates row: every app with a newer version, or one stack's",
});
const SEE_WHAT = declare({
  id: "inbox-see-what-happened",
  // redesign-final-c3: the notice list left Notification rules for the
  // Inbox; its old control keeps working by its old name.
  was: ["open-notice"],
  page: "needs-you",
  opens: "view",
  row: "<row key>",
  what: "a notice's See what happened: open where it happened",
});
const SOURCE = declare({
  id: "inbox-source",
  page: "needs-you",
  opens: "view",
  row: "<row key>:<chip label>",
  what: "a row's source chip: open its stack or Activity",
});
const WORTH = declare({
  id: "inbox-worth",
  page: "needs-you",
  opens: "view",
  what: "fold or unfold Worth a look (not counted in the counter)",
});
const WORTH_OPEN = declare({
  id: "inbox-worth-open",
  page: "needs-you",
  opens: "view",
  row: "worth:drills|worth:host-release|worth:doctor",
  what: "a Worth a look row's button: drill the backups, update the host, the doctor report",
});

// ask-kp11be-7: snooze a row, optionally with a reminder when it is back.
const SNOOZE_OPEN = declare({
  id: "inbox-snooze",
  page: "needs-you",
  opens: "dialog",
  row: "<row key>",
  what: "snooze a row: it leaves the list and the counter until the moment you pick",
});
const SNOOZE_FOR = declare({
  id: "inbox-snooze-for",
  page: "needs-you",
  opens: "run",
  row: "1h|4h|tonight|tomorrow|monday|pick",
  what: "snooze the row for 1 hour, 4 hours, until tonight 18:00, tomorrow 07:00, Monday 07:00 or a date and time (at most 90 days); pick is the button of the date panel",
  shows: "in the snooze dialog",
  reach: [{ do: "click", control: "inbox-snooze", row: "*" }],
});
const SNOOZE_REMIND = declare({
  id: "inbox-snooze-remind",
  page: "needs-you",
  opens: "view",
  what: "tick or untick: leave a reminder notice in the Inbox when the snoozed row comes back",
  shows: "in the snooze dialog",
  reach: [{ do: "click", control: "inbox-snooze", row: "*" }],
});
const SNOOZE_END = declare({
  id: "inbox-snooze-end",
  page: "needs-you",
  opens: "run",
  row: "<row key>",
  what: "end a snooze now: the row is back in the list",
  shows: "in the Snoozed fold, for a snoozed row",
});
const SNOOZED = declare({
  id: "inbox-snoozed",
  page: "needs-you",
  opens: "view",
  what: "fold or unfold Snoozed rows (not counted in the counter)",
});

/** The bar on a row's start edge, by how serious it is. */
const BAR = /** @type {const} */ ({ bad: "failed", warn: "warn", info: "new" });
/** The kind as a word in the row's first column. */
const WORD = /** @type {const} */ ({
  ask: "Question",
  fail: "Failure",
  update: "Update",
  setup: "Setup",
  check: "Check",
});
const BADGE = /** @type {const} */ ({
  ask: "kp-badge kp-badge--warning",
  fail: "kp-badge kp-badge--destructive",
  update: "kp-badge kp-badge--info",
  setup: "kp-badge",
  check: "kp-badge",
});
/** A chip that only repeats the kind word in the first column is left out. */
const KIND_CHIPS = new Set(["question", "failure", "update", "setup", "check"]);

/** @param {string} key */
const rowId = (key) => key.replace(/[^a-z0-9-]/gi, "-");

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  // redesign-flows-11: the Update flow has its own address; the old
  // `/inbox?update=…` is sent on to it.
  const legacy = legacyUpdateHref(location.search);
  if (legacy) {
    history.replaceState(null, "", legacy);
    queueMicrotask(() => dispatchEvent(new PopStateEvent("popstate")));
    return () => {};
  }
  ensureStyle("/css/pages/needsyou.css");
  root.classList.add("ib-page");
  /** @type {(() => void)[]} */
  const cleanups = [];

  const again = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button",
        title:
          "Run every check again now: the fleet check (about 90 s on the host), Today, the manual checks and the notices (r)",
        "aria-keyshortcuts": "r",
      },
      "Check again",
    ),
    CHECK_AGAIN,
  );
  const head = pageHeader({
    title: "Inbox",
    desc: "Everything waiting for a person, worst first. Each row says what is wrong, why it matters, and has the button that fixes it — you never have to know which page it came from.",
    live: "checked",
    actions: [again],
  });
  // Review item 13: on a phone the header reads title, sentence, then the
  // checked time with Check again (needsyou.css).
  head.el.classList.add("inbox-head");

  // A dismissible explanation: kp's info alert; once closed it stays closed
  // in this browser (kp's alert has no memory of its own).
  const NOTE_KEY = "homelab-inbox-explained";
  /** @type {{el: HTMLElement, close: HTMLElement} | null} */
  let note = null;
  try {
    if (localStorage.getItem(NOTE_KEY)) note = null;
    else note = explainNote();
  } catch {
    note = explainNote();
  }
  function explainNote() {
    const close = h(
      "button",
      {
        type: "button",
        class: "kp-alert__close kp-icon-button",
        "aria-label": "Hide this explanation",
        title: "Hide this explanation for good",
      },
      "✕",
    );
    const el = h(
      "div",
      { class: "kp-alert kp-alert--info", role: "note" },
      h("span", { class: "kp-alert__icon", "aria-hidden": "true" }, "ⓘ"),
      h(
        "div",
        { class: "kp-alert__body" },
        h("b", null, "This one list replaces"),
        " the bell, Health's Today / Doctor / Checks blocks, the question strip under the bar and the stale-image table on the Map. They all still feed it; a row links back to its source.",
      ),
      close,
    );
    el.addEventListener(ALERT_DISMISS_EVENT, () => {
      try {
        localStorage.setItem(NOTE_KEY, "1");
      } catch {
        /* fine: it comes back next time */
      }
    });
    return { el, close };
  }
  if (note) drivable(note.close, EXPLAIN);

  // ---- the list's card ----------------------------------------------------
  /** @type {Set<string>} */
  let kinds = new Set(
    (new URLSearchParams(location.search).get("kind") ?? "")
      .split(",")
      .filter((k) => KINDS.some((x) => x.value === k)),
  );
  const count = h("span", {
    class: "kp-timestamp inbox-shown",
    "aria-live": "polite",
  });
  const chips = h("div", {
    class: "kp-segmented kp-segmented--chips",
    "data-kp-segmented": "multi",
    "aria-label": "Show these kinds",
  });
  const list = h("ul", {
    class: "kp-action-list kp-action-list--lead ib-list",
    "aria-label": "Inbox",
    "data-kp-row-keys": "",
  });
  const keysLine = h("div", { class: "ib-keys" });
  const listCard = h(
    "section",
    {
      class: "kp-card ib-card inbox-card",
      "data-kp-state": "loading",
      "aria-labelledby": "inbox-now-h",
    },
    h(
      "div",
      { class: "kp-card__header" },
      h("h2", { class: "kp-card__title", id: "inbox-now-h" }, "Inbox"),
      h("div", { class: "kp-card__actions" }, count),
    ),
    h(
      "p",
      { class: "kp-card__body" },
      "What needs you on pve and its stacks, worst first. Click a kind to show only those; each click turns one on or off.",
    ),
    h("div", { class: "ib-kinds" }, chips),
    h(
      "div",
      { class: "kp-card__stage" },
      h("div", { class: "kp-card__data" }, list),
    ),
    keysLine,
  );

  const questions = mountQuestions(root);
  const checks = mountChecks(() =>
    /** @type {import("../inboxrows.js").Row[]} */ (inboxNow().items).filter(
      (r) => r.check,
    ),
  );
  const bulk = mountBulk(root);

  // Worth a look: folded, not counted.
  const worthList = h("ul", {
    class: "kp-action-list ib-worth-list",
    "aria-label": "Worth a look",
  });
  const worthStage = stage(worthList);
  const worthN = h("span", null, "");
  const worthSummary = drivable(
    h(
      "summary",
      { title: "Fold or unfold what is worth a look but not urgent" },
      h("strong", null, "Worth a look", worthN),
      // Review item 8: the section's one-sentence description.
      h(
        "span",
        { class: "inbox-worth__desc" },
        "Things that are fine today but deserve a look soon — backups never test-restored, a newer host release, the doctor's report — not urgent and not counted.",
      ),
    ),
    WORTH,
  );
  const worth = h(
    "details",
    {
      class:
        "kp-card ib-card inbox-worth kp-accordion__item kp-accordion__item--bare",
    },
    worthSummary,
    worthStage,
  );

  // Snoozed rows: folded, not counted (ask-kp11be-7).
  const snoozedList = h("ul", {
    class: "kp-action-list ib-snoozed-list",
    "aria-label": "Snoozed rows",
  });
  const snoozedStage = stage(snoozedList);
  const snoozedN = h("span", null, "");
  const snoozedSummary = drivable(
    h(
      "summary",
      { title: "Fold or unfold the rows you snoozed" },
      h("strong", null, "Snoozed", snoozedN),
      h(
        "span",
        { class: "inbox-worth__desc" },
        "Rows you hid until a moment you picked. They come back by themselves; End snooze brings one back now. Not counted.",
      ),
    ),
    SNOOZED,
  );
  const snoozed = h(
    "details",
    {
      class:
        "kp-card ib-card inbox-worth kp-accordion__item kp-accordion__item--bare",
    },
    snoozedSummary,
    snoozedStage,
  );

  root.replaceChildren(
    head.el,
    ...(note ? [note.el] : []),
    questions.el,
    listCard,
    checks.el,
    bulk.el,
    snoozed,
    worth,
  );
  cleanups.push(
    attachCardStates(root),
    attachSegmented(listCard),
    attachRowKeys(listCard),
    () => questions.destroy(),
    () => checks.destroy(),
    () => bulk.destroy(),
  );
  setKeys(keysLine, [
    [["j", "k"], "Next and previous row"],
    ["Enter", "Main button"],
    ["d", "Mark seen"],
    ["r", "Check again"],
    ["Esc", "Show every kind"],
  ]);

  // ---- one row ------------------------------------------------------------
  /**
   * A row's button or link, in its column.
   * @param {{role: "open" | "act" | "seen" | "snooze", label: string, hint: string,
   *   href?: string, primary?: boolean}} o
   */
  const rowButton = (o) => {
    const cls = `kp-button kp-button--sm${o.primary ? " kp-button--primary" : ""}`;
    return o.href
      ? h(
          "a",
          {
            class: cls,
            href: o.href,
            title: o.hint,
            "data-kp-action": o.role,
          },
          o.label,
        )
      : h(
          "button",
          {
            type: "button",
            class: cls,
            title: o.hint,
            "data-kp-action": o.role,
          },
          o.label,
        );
  };

  /**
   * A row's buttons, by what it is.
   * @param {import("../inboxrows.js").Row} r
   * @returns {HTMLElement[]}
   */
  const actsBase = (r) => {
    const kind = kindOf(r);
    const bad = r.severity === "bad";
    if (r.source === "asks") {
      const b = rowButton({
        role: "act",
        label: "Answer",
        hint: "Go to the question: its card says what each answer does and how long the host waits",
        primary: bad,
      });
      b.addEventListener("click", () => questions.focusAsk(r.key.slice(4)));
      return [b];
    }
    if (kind === "update") {
      const stacks = r.stacks ?? [];
      const n = r.apps?.length ?? 0;
      return [
        ...(stacks.length > 1
          ? [
              drivable(
                rowButton({
                  role: "open",
                  label: `Only ${stacks[0]}…`,
                  hint: `Update only ${stacks[0]}'s apps`,
                  href: updateHref(stacks[0]),
                }),
                REVIEW,
                stacks[0],
              ),
            ]
          : []),
        drivable(
          rowButton({
            role: "act",
            label:
              stacks.length === 1 && n === 1
                ? "Review and update…"
                : `Review and update all ${n}…`,
            hint: "See what changes, then back up, update and verify; nothing runs until you confirm",
            href:
              stacks.length === 1 ? updateHref(stacks[0]) : updateHref(null),
          }),
          REVIEW,
          stacks.length === 1 ? stacks[0] : "all",
        ),
      ];
    }
    if (kind === "setup") {
      const b = drivable(
        rowButton({
          role: "act",
          label: "Push the envs…",
          hint: "Deploy each of these stacks so the host's vault takes a copy of its secrets; one confirm for all",
        }),
        PUSH_ENVS,
      );
      b.addEventListener(
        "click",
        () => void openBatch("deploy", r.stacks ?? []),
      );
      return [b];
    }
    if (r.check) {
      const id = r.check;
      const b = rowButton({
        role: "act",
        label: "Answer",
        hint: "Go to the check: one question on the screen, with big answer buttons",
        primary: bad,
      });
      b.addEventListener("click", () => checks.focusCheck(id));
      return [b];
    }
    /** @type {HTMLElement[]} */
    const out = [];
    const notice =
      r.source === "notices" ? Number(r.key.slice("notice:".length)) : null;
    const fix =
      r.fix ??
      (notice != null
        ? (act.notices?.notices.find((n) => n.id === notice)?.fixes?.[0] ??
          null)
        : null);
    if (r.source === "notices")
      out.push(
        drivable(
          rowButton({
            // With a fix the fix is the main button; without one this is.
            role: fix ? "open" : "act",
            label: "See what happened",
            hint: "Open where this happened",
            href: r.href || "/activity",
          }),
          SEE_WHAT,
          r.key,
        ),
      );
    if (fix) {
      const b = drivable(
        rowButton({
          role: "act",
          label: `${fix.label}…`,
          hint: "The action's own dialog, prefilled; nothing runs until you confirm",
          primary: bad,
        }),
        FIX,
        r.key,
      );
      b.addEventListener(
        "click",
        () =>
          void openAction(fix.stack, fix.action, { preset: fix.args ?? {} }),
      );
      out.push(b);
    }
    if (notice != null) {
      const b = drivable(
        rowButton({
          role: "seen",
          label: "Mark seen",
          hint: "It leaves the Inbox; Activity keeps it",
        }),
        SEEN,
        r.key,
      );
      b.addEventListener("click", () => void markSeen(rowOf(b), b, r));
      out.push(b);
    }
    return out;
  };

  /**
   * A row's buttons and, for every row but a host question (it has its own
   * clock), Snooze… (ask-kp11be-7).
   * @param {import("../inboxrows.js").Row} r
   * @returns {HTMLElement[]}
   */
  const actsOf = (r) => {
    const out = actsBase(r);
    if (r.source === "asks") return out;
    const b = drivable(
      rowButton({
        role: "snooze",
        label: "Snooze…",
        hint: "Hide this row until a moment you pick; it comes back by itself, optionally with a reminder",
      }),
      SNOOZE_OPEN,
      r.key,
    );
    b.addEventListener("click", () => openSnooze(r));
    return [...out, b];
  };

  /**
   * The snooze dialog: kp's snooze picker and the reminder tick.
   * @param {import("../inboxrows.js").Row} r
   */
  const openSnooze = (r) => {
    const picker = h("div", {
      class: "kp-snooze ib-snooze",
      "data-kp-snooze": "",
      "data-kp-look": "chips",
      "data-kp-snooze-choices": "1h 4h tonight tomorrow monday pick",
      "data-kp-snooze-morning": "07:00",
      "data-kp-snooze-max": SNOOZE_MAX,
      "aria-label": `Snooze ${r.title}`,
    });
    const remindBox = h("input", {
      class: "kp-switch__input",
      type: "checkbox",
      role: "switch",
      checked: true,
    });
    remindBox.checked = true;
    drivable(remindBox, SNOOZE_REMIND);
    const err = h("div", { class: "ib-snooze__err" });
    const d = openDialog({
      title: "Snooze this row",
      description: `${r.title}. It leaves the list and the counter until the moment you pick, then comes back by itself. Nothing about it is fixed or answered.`,
      body: [
        h(
          "label",
          { class: "kp-switch ib-snooze__remind" },
          remindBox,
          h("span", { class: "kp-switch__state", "aria-hidden": "true" }),
          h("span", null, "Leave a reminder notice when it comes back"),
        ),
        picker,
        err,
      ],
    });
    attachSnoozes(d.dialog);
    const mark = () => {
      for (const b of /** @type {NodeListOf<HTMLElement>} */ (
        picker.querySelectorAll(".kp-snooze__chip[data-value]")
      ))
        if (b.dataset.value) drivable(b, SNOOZE_FOR, b.dataset.value);
      const apply = picker.querySelector(
        ".kp-snooze__actions .kp-button--primary",
      );
      if (apply instanceof HTMLElement) drivable(apply, SNOOZE_FOR, "pick");
    };
    const watch = new MutationObserver(mark);
    watch.observe(picker, { childList: true, subtree: true });
    void d.closed.then(() => watch.disconnect());
    mark();
    picker.addEventListener(SNOOZE_EVENT, async (event) => {
      const at = Date.parse(/** @type {CustomEvent} */ (event).detail.until);
      setSnoozeBusy(picker, true);
      const res = await snoozeRow(r, Math.round(at / 1000), remindBox.checked);
      setSnoozeBusy(picker, false);
      if (!res.ok) {
        err.replaceChildren(
          refusalCallout(res.error, "destructive", "Not snoozed"),
        );
        return;
      }
      d.close();
    });
  };

  /** @param {HTMLElement} el */
  const rowOf = (el) =>
    /** @type {HTMLElement} */ (el.closest(".ib-list > li[data-key]"));

  /**
   * One Inbox row as the approved demo draws it.
   * @param {import("../inboxrows.js").Row} r
   */
  const rowEl = (r) => {
    const kind = kindOf(r);
    const ask =
      r.source === "asks"
        ? current().asks.find((x) => askKey(x) === r.key.slice(4))
        : null;
    const words = ask ? askWords(ask, Date.now() / 1000) : null;
    const text =
      kind === "update"
        ? "Each is backed up first; Roll back puts the old version back."
        : (words?.why ?? r.why);
    const meta = h(
      "span",
      { class: "kp-action-list__meta" },
      ...(ask
        ? [
            h("span", null, "Host question"),
            h("span", null, `asked ${formatDateTime(r.at)}`),
            h("span", { class: "ib-left" }, words?.left ?? ""),
          ]
        : chipsOf(r)
            .filter((c) => !KIND_CHIPS.has(c.label))
            .map((c) =>
              c.href
                ? drivable(
                    h("a", { href: c.href, title: `Open ${c.label}` }, c.label),
                    SOURCE,
                    `${r.key}:${c.label}`,
                  )
                : h("span", null, c.label),
            )),
    );
    const main = h(
      "div",
      { class: "kp-action-list__main" },
      h("strong", { class: "kp-action-list__title" }, words?.title ?? r.title),
      ...(text
        ? [h("span", { class: "kp-action-list__text ib-text" }, text)]
        : []),
      meta,
    );
    if (r.apps?.length) {
      const apps = h("div", { class: "ib-apps" });
      for (const a of r.apps)
        apps.append(
          h(
            "span",
            { class: "ib-app" },
            h("b", null, `${a.stack}/${a.container}`),
            h("span", {
              class: "kp-badge kp-badge--version",
              "data-kp-version-from": a.from,
              "data-kp-version-to": a.to,
              "data-kp-version-app": a.container,
              ...(a.major ? { "data-kp-version-level": "major" } : {}),
              title: a.release
                ? "Comes with a homelab release"
                : `${a.stack}/${a.container}`,
            }),
          ),
        );
      main.append(apps);
    }
    const problem = problemOf(r);
    if (problem) {
      const box = h("div", { class: "kp-problems", "data-kp-problems": "" });
      const more = h(
        "details",
        {
          class:
            "kp-accordion__item kp-accordion__item--bare kp-action-list__more",
        },
        h("summary", null, "What went wrong"),
        box,
      );
      main.append(more);
      // Drawn when the fold opens, so a closed row holds no hidden text.
      more.addEventListener("toggle", () => {
        if (more.open && !box.childElementCount) setProblems(box, [problem]);
      });
    }
    return h(
      "li",
      {
        "data-key": r.key,
        "data-kp-state": BAR[r.severity],
        "data-kind": kind,
        id: rowId(r.key),
      },
      h(
        "span",
        { class: "kp-action-list__lead" },
        h("span", { class: BADGE[kind] }, WORD[kind]),
      ),
      main,
      h("div", { class: "kp-row-actions" }, ...actsOf(r)),
    );
  };

  /** @param {string} name */
  const groupEl = (name) =>
    h(
      "li",
      { class: "kp-action-list__group", "data-ib-group": name },
      h("h3", null, name),
    );

  // ---- paint --------------------------------------------------------------
  /** Rows a button is working on: the list is not redrawn under them. */
  let busyRows = 0;
  let retrying = false;
  let lastSig = "";
  let lastChips = "";

  /**
   * A row's facts as one string: what decides whether its element is drawn
   * again. A question's clock is left out (it ticks in place).
   * @param {import("../inboxrows.js").Row} r
   */
  const sigOf = (r) =>
    JSON.stringify(r.source === "asks" ? { ...r, why: "" } : r);

  /** Lay the wanted nodes in `list`, moving as few as the order allows. @param {Node[]} want */
  const reconcile = (want) => {
    const keep = new Set(want);
    for (const child of [...list.children])
      if (!keep.has(child)) child.remove();
    want.forEach((node, i) => {
      if (list.children[i] !== node)
        list.insertBefore(node, list.children[i] ?? null);
    });
  };

  const paintList = () => {
    if (busyRows > 0) return;
    const { items, ready } = inboxNow();
    const rows = /** @type {import("../inboxrows.js").Row[]} */ (items);
    if (!ready) {
      putState(
        listCard,
        act.failed.notices && !retrying ? "error" : "loading",
        act.failed.notices && !retrying
          ? {
              title: "The Inbox could not be read",
              text: `${act.failed.notices.what}: ${act.failed.notices.why}`,
            }
          : { words: "Reading the Inbox…" },
      );
      count.textContent = "";
      return;
    }
    retrying = false;
    head.live?.set(Date.now() / 1000);
    const counts = new Map(KINDS.map((k) => [k.value, 0]));
    for (const r of rows)
      counts.set(kindOf(r), (counts.get(kindOf(r)) ?? 0) + 1);
    // The chips are drawn again only when a count or a choice moved (the
    // clock repaints this every second while a question waits).
    const chipSig = JSON.stringify([[...counts], [...kinds]]);
    if (chipSig !== lastChips) {
      lastChips = chipSig;
      setSegmentedItems(
        chips,
        KINDS.map((k) => ({
          value: k.value,
          label: k.label,
          count: counts.get(k.value) ?? 0,
        })),
      );
      setSegmentedValues(chips, kinds);
      for (const b of itemsOf(chips)) drivable(b, KIND, valueOf(b));
    }
    const show = shownRows(rows, kinds);
    count.textContent = countWords(show.length, rows.length);
    if (rows.length === 0)
      putState(listCard, "empty", {
        title: "Nothing needs you",
        text: "Every stack is healthy, last night's backups all ran, no app has a newer version and the host has no questions. This list fills itself when that changes, and the counter in the bar shows how many.",
      });
    else if (show.length === 0) {
      const clear = h(
        "button",
        { type: "button", class: "kp-button" },
        "Show every kind",
      );
      clear.addEventListener("click", () => resetSegmented(chips));
      putState(listCard, "empty", {
        title: "Nothing of these kinds",
        text: "Every item that matches the chips is done. Show every kind to see the rest.",
        action: clear,
      });
    } else putState(listCard, "ready");

    // The nodes: a row whose facts did not change keeps its element (its
    // fold, its focus, its version chips).
    const old = new Map(
      [...list.children].map((c) => [
        c.getAttribute("data-key") ?? `g:${c.getAttribute("data-ib-group")}`,
        /** @type {HTMLElement} */ (c),
      ]),
    );
    const sig = show.map(sigOf).join("\n");
    /** @type {Node[]} */
    const want = [];
    for (const g of groupRows(show)) {
      want.push(old.get(`g:${g.name}`) ?? groupEl(g.name));
      for (const r of g.rows) {
        const had = old.get(r.key);
        want.push(had && had.dataset.sig === sigOf(r) ? had : rowEl(r));
        const node = /** @type {HTMLElement} */ (want[want.length - 1]);
        node.dataset.sig = sigOf(r);
      }
    }
    const focused = document.activeElement;
    const focusKey =
      focused && list.contains(focused)
        ? focused.closest("li[data-key]")?.getAttribute("data-key")
        : null;
    if (sig !== lastSig || list.children.length !== want.length) {
      reconcile(want);
      lastSig = sig;
      attachVersionDeltas(list);
      if (focusKey && !list.contains(document.activeElement))
        /** @type {HTMLElement | null} */ (
          list.querySelector(`li[data-key="${CSS.escape(focusKey)}"]`)
        )?.focus();
    }
    // A question's clock ticks in place.
    const now = Date.now() / 1000;
    for (const r of show) {
      if (r.source !== "asks") continue;
      const a = current().asks.find((x) => askKey(x) === r.key.slice(4));
      if (!a) continue;
      const li = list.querySelector(`li[data-key="${CSS.escape(r.key)}"]`);
      const w = askWords(a, now);
      const text = li?.querySelector(".ib-text");
      if (text) text.textContent = w.why;
      const left = li?.querySelector(".ib-left");
      if (left) left.textContent = w.left;
    }
  };

  /** Hold the list still while `work` runs, then draw it once. @param {() => Promise<void>} work */
  const holding = async (work) => {
    busyRows += 1;
    try {
      await work();
    } finally {
      busyRows -= 1;
      paintList();
    }
  };

  /** Take a finished row out, and its heading when it was the last of its group. @param {HTMLElement} row */
  const takeOut = async (row) => {
    const before = row.previousElementSibling;
    const after = row.nextElementSibling;
    const alone =
      before?.classList.contains("kp-action-list__group") &&
      (!after || after.classList.contains("kp-action-list__group"));
    await removeRow(row);
    if (alone) before?.remove();
  };

  /**
   * @param {HTMLElement} row
   * @param {HTMLElement} btn
   * @param {import("../inboxrows.js").Row} r
   */
  const markSeen = (row, btn, r) =>
    holding(async () => {
      if (row.getAttribute("aria-busy") === "true") return;
      setRowBusy(row, true, btn);
      const notice = Number(r.key.slice("notice:".length));
      const res = await send(
        "POST",
        "/data/notifications/read",
        { ids: r.ids ?? [notice], read: true },
        "marking it seen",
      );
      if (!res.ok) {
        setRowBusy(row, false);
        row
          .querySelector(".kp-action-list__main")
          ?.append(refusalCallout(res.error));
        return;
      }
      // The list is read again before the row folds away, so the redraw
      // that follows finds it gone from both.
      await loadNotices();
      await takeOut(row);
    });

  // Enter on a row presses its main button.
  list.addEventListener("kp-row-open", (event) => {
    const row = /** @type {HTMLElement} */ (
      /** @type {CustomEvent} */ (event).detail.row
    );
    /** @type {HTMLElement | null} */ (
      row.querySelector("[data-kp-action='act']")
    )?.click();
  });

  // The chips: several may be on; none on is all.
  chips.addEventListener("kp-segmented-change", (event) => {
    const v = /** @type {string[]} */ (
      /** @type {CustomEvent} */ (event).detail.values
    );
    kinds = new Set(v);
    paintList();
  });
  listCard.addEventListener("kp-card-retry", () => {
    retrying = true;
    paintList();
    void loadNotices().finally(() => {
      retrying = false;
      paintList();
    });
  });

  // The keys: j / k and Enter are the list's own (kp row keys); d marks the
  // row under the focus seen, r checks again, Esc shows every kind.
  const typing = () => document.querySelector("dialog[open]") === null;
  cleanups.push(
    registerShortcuts(listCard, [
      {
        keys: "d",
        what: "Mark seen",
        group: "Inbox",
        run: () => {
          const row = /** @type {HTMLElement | null} */ (
            document.activeElement?.closest(".ib-list > li[data-key]") ?? null
          );
          const btn = /** @type {HTMLElement | null} */ (
            row?.querySelector("[data-kp-action='seen']") ?? null
          );
          if (!btn || !typing()) return false;
          btn.click();
          return undefined;
        },
      },
    ]),
    registerShortcuts(root, [
      {
        keys: "r",
        what: "Check again",
        group: "Inbox",
        run: () => {
          if (!typing() || again.hasAttribute("disabled")) return false;
          again.click();
          return undefined;
        },
      },
      {
        keys: "Escape",
        what: "Show every kind",
        group: "Inbox",
        when: () => kinds.size > 0,
        run: () => {
          if (!typing()) return false;
          resetSegmented(chips);
          return undefined;
        },
      },
    ]),
  );

  // ---- worth a look -------------------------------------------------------
  const abort = new AbortController();
  /**
   * @param {{key: string, title: string, why: string, href: string}} r
   * @param {string} label
   * @param {string} hint
   */
  const worthRow = (r, label, hint) =>
    h(
      "li",
      { "data-key": r.key, "data-kp-state": "new" },
      h(
        "div",
        { class: "kp-action-list__main" },
        h("strong", { class: "kp-action-list__title" }, r.title),
        h("span", { class: "kp-action-list__text" }, r.why),
      ),
      h(
        "div",
        { class: "kp-row-actions" },
        drivable(
          h(
            "a",
            {
              class: "kp-button kp-button--sm",
              href: r.href,
              title: hint,
              "data-kp-action": "act",
            },
            label,
          ),
          WORTH_OPEN,
          r.key,
        ),
      ),
    );
  const readWorth = async () => {
    setCardState(worthStage, "loading", {
      words: "Reading what is worth a look",
    });
    // The fleet's stack names come with the store's first read.
    if (!current().fleet)
      await new Promise((resolve) => {
        const off = subscribe(() => {
          if (!current().fleet) return;
          off();
          resolve(undefined);
        });
      });
    const stacks = (current().fleet?.stacks ?? []).map((s) => s.name);
    const [versions, ...backups] = await Promise.all([
      fetchJson("/data/versions", "the versions", abort.signal),
      ...stacks.map((s) =>
        fetchJson(
          `/data/backups/${encodeURIComponent(s)}`,
          `the backups of ${s}`,
          abort.signal,
        ),
      ),
    ]);
    if (abort.signal.aborted) return;
    const rows = worthRows(
      backups.flatMap((b) => (b.ok ? [b.body] : [])),
      versions.ok ? versions.body : null,
    );
    // M5: the count is the rows the fold holds (the doctor report's row
    // included), exactly.
    worthN.textContent = ` · ${rows.length + 1}`;
    countOf(worthN, worthList, ":scope > li");
    worthList.replaceChildren(
      ...rows.map((r) =>
        worthRow(
          r,
          r.key === "worth:drills" ? "Drill them now…" : "Update the host…",
          r.key === "worth:drills"
            ? "Backups, showing only the repositories never drilled"
            : "The Host page, where the host updates itself",
        ),
      ),
      worthRow(
        {
          key: "worth:doctor",
          title: "The host's full doctor report",
          why: "Every check the host runs on itself, the healthy ones too. Its findings already show above when they need you.",
          href: "/host?section=doctor",
        },
        "Open the report",
        "System ▸ Host, the doctor section",
      ),
    );
    setCardState(worthStage, "ready");
  };

  // ---- Check again --------------------------------------------------------
  const checkAgain = async () => {
    again.setAttribute("disabled", "");
    again.textContent = "Checking…";
    await Promise.all([
      slowRead("/data/stale-images", "the stale images", abort.signal).then(
        (r) => {
          if (r.ok) feedUpdates(r.body);
        },
      ),
      slowRead("/data/today", "today's reading", abort.signal).then((r) => {
        if (r.ok) feedToday(r.body);
      }),
      readChecks(),
      loadNotices(),
    ]).catch(() => {});
    if (abort.signal.aborted) return;
    again.removeAttribute("disabled");
    again.textContent = "Check again";
    head.live?.set(Date.now() / 1000);
  };
  again.addEventListener("click", () => void checkAgain());

  const paintAll = () => {
    paintList();
    questions.paint();
    checks.paint();
    bulk.paint();
  };
  /** The Snoozed fold follows what the server holds. */
  const paintSnoozed = () => {
    const st = snoozeStatus();
    if (st.state === "loading") {
      setCardState(snoozedStage, "loading", { words: "Reading the snoozes" });
      return;
    }
    if (st.state === "error") {
      setCardState(snoozedStage, "error", {
        title: "The snoozed rows could not be read",
        text: st.why,
      });
      return;
    }
    const rows = snoozedNow();
    snoozedN.textContent = ` · ${rows.length}`;
    if (!rows.length) {
      setCardState(snoozedStage, "empty", {
        title: "Nothing is snoozed",
        text: "Snooze… on a row hides it until a moment you pick; it shows here until then.",
      });
      snoozedList.replaceChildren();
      return;
    }
    snoozedList.replaceChildren(
      ...rows.map((e) => {
        const end = drivable(
          h(
            "button",
            {
              type: "button",
              class: "kp-button kp-button--sm",
              title: "Bring this row back into the list now",
              "data-kp-action": "act",
            },
            "End snooze",
          ),
          SNOOZE_END,
          e.key,
        );
        end.addEventListener("click", () => {
          end.setAttribute("disabled", "");
          void endSnooze(e.key).then((res) => {
            if (!res.ok) {
              end.removeAttribute("disabled");
              end.after(refusalCallout(res.error, "destructive", "Not ended"));
            }
          });
        });
        return h(
          "li",
          { "data-key": e.key, "data-kp-state": "new" },
          h(
            "div",
            { class: "kp-action-list__main" },
            h("strong", { class: "kp-action-list__title" }, e.title),
            h(
              "span",
              { class: "kp-action-list__text" },
              `Comes back ${formatDateTime(e.until)}${e.remind ? ", with a reminder notice" : ""}.`,
            ),
          ),
          h("div", { class: "kp-row-actions" }, end),
        );
      }),
    );
    setCardState(snoozedStage, "ready");
  };
  snoozedStage.addEventListener("kp-card-retry", () => {
    setCardState(snoozedStage, "loading", { words: "Reading the snoozes" });
    void readSnoozes().catch(() => {});
  });
  cleanups.push(
    onInbox(() => {
      paintAll();
      paintSnoozed();
    }),
    onSnoozeStatus(paintSnoozed),
    onAct("notices", paintAll),
  );
  paintSnoozed();
  // Open questions count down: their rows and cards follow the clock.
  const clock = window.setInterval(() => {
    if (current().asks.length) paintList();
  }, 1000);
  cleanups.push(() => window.clearInterval(clock));
  paintAll();
  void readWorth().catch(() => {});
  // A fresh fleet check and Today on a visit (their kept answers show at
  // once; the counter follows when they land).
  void slowRead(
    "/data/stale-images",
    "the stale images",
    abort.signal,
    feedUpdates,
  )
    .then((r) => r.ok && feedUpdates(r.body))
    .catch(() => {});
  void slowRead("/data/today", "today's reading", abort.signal, feedToday)
    .then((r) => r.ok && feedToday(r.body))
    .catch(() => {});

  return () => {
    abort.abort();
    for (const c of cleanups) c();
    root.classList.remove("ib-page");
  };
}
