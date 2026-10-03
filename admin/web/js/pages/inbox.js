// feat-shell-4 + redesign-flows-2 (redesign 3.71.0, decision "Inbox", demo
// flows/inbox.html with health.html's and notifications.html's sources;
// Kenny approved 2026-10-03): everything waiting for a person, worst first.
// Each row says what is wrong, why it matters, and has the button that
// fixes it — answering the host's question, unlocking a failed backup,
// reviewing the updates, pushing the envs, answering a check — so nobody
// has to know which page it came from. A plain click on a kind shows only
// those (several may be on; Esc shows all again). The counter in the bar
// is exactly the number of rows here (invariant 60): both read inbox.js's
// one list, which inboxsources.js feeds on every page.
//
// Below the list, folded: "Worth a look" — not urgent, not counted (never
// drilled backups, a newer host release, the host's full doctor report).
//
// `?update=…` is the Update flow (pages/update.js) drawn in this page.

import { act, loadNotices, send } from "../act.js";
import { openAction, openBatch } from "../actiondialog.js";
import { refusalCallout } from "../actui.js";
import { answerBody } from "../asks.js";
import { ensureStyle, fetchJson, h, slowRead } from "../dom.js";
import { declare, drivable } from "../drivable.js";
import { inboxNow, onInbox } from "../inbox.js";
import {
  KINDS,
  askWords,
  chipsOf,
  headline,
  kindOf,
  shownRows,
  worthRows,
} from "../inboxrows.js";
import { feedToday, feedUpdates, readChecks } from "../inboxsources.js";
import { current, setAsks, subscribe } from "../store.js";
import {
  doneMark,
  explainNote,
  pageHeader,
  segSwitch,
  skeletonLines,
  toggleChips,
  toolbar,
} from "../ui.js";
import { legacyUpdateHref, updateHref } from "../updateflow.js";

const CHECK_AGAIN = declare({
  id: "inbox-check-again",
  page: "inbox",
  opens: "run",
  what: "read every source of the Inbox again now: the fleet check, Today, the manual checks, the notices",
});
const KIND = declare({
  id: "inbox-kind",
  page: "inbox",
  opens: "view",
  row: "ask|fail|update|setup|check",
  what: "show only one kind of row, or all again (a plain click turns it on or off)",
});
const ORDER = declare({
  id: "inbox-order",
  page: "inbox",
  opens: "view",
  row: "worst|newest",
  what: "order the Inbox worst first or newest first",
});
const EXPLAIN = declare({
  id: "inbox-explain-hide",
  page: "inbox",
  opens: "view",
  what: "hide the note about what the Inbox replaces",
});
const ANSWER = declare({
  id: "inbox-answer",
  page: "inbox",
  opens: "run",
  row: "<ask key>:allow|stop",
  what: "answer the host's question: allow, or leave it stopped",
  shows: "while the host asks whether an operation may go on",
});
const FIX = declare({
  id: "inbox-fix",
  page: "inbox",
  opens: "dialog",
  row: "<row key>",
  what: "a row's fix: the action's own dialog, prefilled",
});
const SEEN = declare({
  id: "inbox-mark-seen",
  page: "inbox",
  opens: "run",
  row: "<row key>",
  what: "mark a notice as seen (it leaves the Inbox; Activity keeps it)",
  shows: "on an unread notice without a fix",
});
const PUSH_ENVS = declare({
  id: "inbox-push-envs",
  page: "inbox",
  opens: "dialog",
  what: "deploy every stack without a sealed env, so the host's vault takes a copy (one confirm)",
});
const REVIEW = declare({
  id: "inbox-review-updates",
  page: "inbox",
  opens: "view",
  row: "all|<stack>",
  what: "open the Update flow from the Updates row: every app with a newer version, or one stack's",
});
const SEE_WHAT = declare({
  id: "inbox-see-what-happened",
  page: "inbox",
  opens: "view",
  row: "<row key>",
  what: "a notice's See what happened: open where it happened",
});
const SOURCE = declare({
  id: "inbox-source",
  page: "inbox",
  opens: "view",
  row: "<row key>:<chip label>",
  what: "a row's source chip: open its stack or Activity",
});
const WORTH = declare({
  id: "inbox-worth",
  page: "inbox",
  opens: "view",
  what: "fold or unfold Worth a look (not counted in the counter)",
});
const WORTH_OPEN = declare({
  id: "inbox-worth-open",
  page: "inbox",
  opens: "view",
  row: "worth:drills|worth:host-release|worth:doctor",
  what: "a Worth a look row's button: drill the backups, update the host, the doctor report",
});
const ANSWER_CHECK = declare({
  id: "inbox-answer-check",
  page: "inbox",
  opens: "dialog",
  row: "<check id>:pass|fail|later",
  what: "answer a manual check: it passes, it fails, or not now (asks again in 7 days)",
  shows: "while a manual check waits for an answer",
});

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
  ensureStyle("/css/pages/inbox.css");

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
  // checked time with Check again (inbox.css).
  head.el.classList.add("inbox-head");

  const note = explainNote("homelab-inbox-explained", [
    h("b", null, "This one list replaces"),
    " the bell, Health's Today / Doctor / Checks blocks, the question strip under the bar and the stale-image table on the Map. They all still feed it; a row links back to its source.",
  ]);
  if (note) drivable(note.close, EXPLAIN);

  // The list's card: heading + sentence, the toolbar, the rows, the foot.
  const title = h("h2", { id: "inbox-now-h" }, "Reading the Inbox…");
  /** @type {Set<string>} */
  let kinds = new Set(
    (new URLSearchParams(location.search).get("kind") ?? "")
      .split(",")
      .filter((k) => KINDS.some((x) => x.value === k)),
  );
  /** @type {"worst" | "newest"} */
  let order = "worst";
  const chips = toggleChips({
    label: "Show",
    chips: KINDS.map((k) => ({ value: k.value, label: k.label, count: 0 })),
    selected: kinds,
    onChange: (s) => {
      kinds = s;
      paint();
    },
    drive: { id: KIND },
  });
  const shown = h("span", { class: "inbox-shown" });
  const seg = segSwitch({
    label: "Order",
    items: [
      {
        value: "worst",
        label: "Worst first",
        hint: "Urgent first, then the rest, newest first within each",
      },
      {
        value: "newest",
        label: "Newest",
        hint: "The newest row first, whatever its weight",
      },
    ],
    value: order,
    onChange: (v) => {
      order = /** @type {"worst" | "newest"} */ (v);
      paint();
    },
    mark: (b, v) => void drivable(b, ORDER, v),
  });
  const tb = toolbar({ groups: [chips.el], state: [shown, seg.el] });
  const list = h("ul", {
    class: "nx-inbox inbox-list",
    "aria-labelledby": "inbox-now-h",
  });
  const card = h(
    "section",
    { class: "kp-card nx-card inbox-card", "aria-labelledby": "inbox-now-h" },
    h(
      "div",
      { class: "nx-card__head" },
      title,
      h(
        "p",
        { class: "section-head__desc" },
        "Worst first. Click a kind to show only those; each click turns one on or off.",
      ),
    ),
    tb.el,
    list,
    h(
      "div",
      { class: "inbox-foot" },
      h(
        "span",
        null,
        "Fed by: host questions · failed jobs · the fleet check · health checks · the nightly round · manual checks",
      ),
      h(
        "span",
        null,
        h("kbd", { class: "nx-kbd" }, "j"),
        " ",
        h("kbd", { class: "nx-kbd" }, "k"),
        " move · ",
        h("kbd", { class: "nx-kbd" }, "Enter"),
        " main button",
      ),
    ),
  );

  // Nothing needs you: a card that teaches what it watches.
  const clear = h(
    "section",
    {
      class: "kp-card nx-card inbox-clear",
      "aria-labelledby": "inbox-clear-h",
      hidden: "",
    },
    doneMark("ok"),
    h("h2", { id: "inbox-clear-h" }, "Nothing needs you"),
    h(
      "p",
      { class: "inbox-hint" },
      "Every stack is healthy, last night's backups all ran, no app has a newer version and the host has no questions. This list fills itself when that changes, and the counter in the bar shows how many.",
    ),
  );

  // Worth a look: folded, not counted.
  const worthList = h("ul", { class: "nx-inbox inbox-list" });
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
    { class: "kp-card nx-card inbox-worth" },
    worthSummary,
    worthList,
  );

  root.replaceChildren(head.el, ...(note ? [note.el] : []), card, clear, worth);

  // ---- one row ----------------------------------------------------------
  /** @param {string} cls @param {string} label @param {string} hint */
  const btn = (cls, label, hint) =>
    h(
      "button",
      { type: "button", class: `kp-button ${cls}`.trim(), title: hint },
      label,
    );
  /** @param {string} href @param {string} label @param {string} hint @param {boolean} [primary] */
  const link = (href, label, hint, primary = false) =>
    h(
      "a",
      {
        class: `kp-button${primary ? " kp-button--primary" : ""}`,
        href,
        title: hint,
      },
      label,
    );

  /**
   * A row's buttons, by what it is.
   * @param {import("../inboxrows.js").Row} r
   * @returns {Node[]}
   */
  const actsOf = (r) => {
    const kind = kindOf(r);
    if (r.source === "asks") {
      const a = current().asks.find(
        (x) => `ask:${x.boot ?? ""}:${x.id}` === r.key,
      );
      if (!a) return [];
      const k = r.key.slice(4);
      const stop = drivable(
        btn(
          "",
          "Leave it stopped",
          a.if_stopped
            ? `The host ${a.if_stopped}`
            : "The operation stops here",
        ),
        ANSWER,
        `${k}:stop`,
      );
      const allow = drivable(
        btn(
          "kp-button--primary",
          "Allow",
          a.if_allowed ? `The host ${a.if_allowed}` : "The operation goes on",
        ),
        ANSWER,
        `${k}:allow`,
      );
      const answer = async (/** @type {boolean} */ yes) => {
        stop.setAttribute("disabled", "");
        allow.setAttribute("disabled", "");
        const res = await send(
          "POST",
          "/data/asks/answer",
          answerBody(a, yes),
          "the answer",
        );
        if (res.ok)
          setAsks(
            current().asks.filter((x) => !(x.id === a.id && x.boot === a.boot)),
          );
        else {
          stop.removeAttribute("disabled");
          allow.removeAttribute("disabled");
          list
            .querySelector(`#${rowId(r.key)} .nx-inbox__what`)
            ?.append(refusalCallout(res.error));
        }
      };
      stop.addEventListener("click", () => void answer(false));
      allow.addEventListener("click", () => void answer(true));
      return [stop, allow];
    }
    if (kind === "update") {
      const stacks = r.stacks ?? [];
      const n = r.apps?.length ?? 0;
      return [
        ...(stacks.length > 1
          ? [
              drivable(
                link(
                  updateHref(stacks[0]),
                  `Only ${stacks[0]}…`,
                  `Update only ${stacks[0]}'s apps`,
                ),
                REVIEW,
                stacks[0],
              ),
            ]
          : []),
        drivable(
          link(
            stacks.length === 1 ? updateHref(stacks[0]) : updateHref(null),
            stacks.length === 1 && n === 1
              ? "Review and update…"
              : `Review and update all ${n}…`,
            "See what changes, then back up, update and verify; nothing runs until you confirm",
            true,
          ),
          REVIEW,
          stacks.length === 1 ? stacks[0] : "all",
        ),
      ];
    }
    if (kind === "setup") {
      const b = drivable(
        btn(
          "kp-button--primary",
          "Push the envs…",
          "Deploy each of these stacks so the host's vault takes a copy of its secrets; one confirm for all",
        ),
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
      /** @param {string} label @param {string} hint @param {string} verdict @param {string} which @param {boolean} [primary] */
      const answerBtn = (label, hint, verdict, which, primary = false) => {
        const b = drivable(
          btn(primary ? "kp-button--primary" : "", label, hint),
          ANSWER_CHECK,
          `${id}:${which}`,
        );
        b.addEventListener(
          "click",
          () =>
            void openAction("_host", "answer-check", {
              preset: {
                check: id,
                verdict,
                ...(verdict === "accept" ? { days: "7", note: "Not now" } : {}),
              },
            }),
        );
        return b;
      };
      return [
        answerBtn("Not now", "Asks again in 7 days", "accept", "later"),
        answerBtn(
          "It fails…",
          "Record that it fails, with a note",
          "nok",
          "fail",
        ),
        answerBtn("It passes", "Record that it passes", "ok", "pass", true),
      ];
    }
    /** @type {Node[]} */
    const out = [];
    const notice =
      r.source === "notices" ? Number(r.key.slice("notice:".length)) : null;
    if (r.source === "notices")
      out.push(
        drivable(
          link(
            r.href || "/activity",
            "See what happened",
            "Open where this happened",
          ),
          SEE_WHAT,
          r.key,
        ),
      );
    const fix =
      r.fix ??
      (notice != null
        ? (act.notices?.notices.find((n) => n.id === notice)?.fixes?.[0] ??
          null)
        : null);
    if (fix) {
      const b = drivable(
        btn(
          "kp-button--primary",
          `${fix.label}…`,
          "The action's own dialog, prefilled; nothing runs until you confirm",
        ),
        FIX,
        r.key,
      );
      b.addEventListener(
        "click",
        () =>
          void openAction(fix.stack, fix.action, { preset: fix.args ?? {} }),
      );
      out.push(b);
    } else if (notice != null) {
      const b = drivable(
        btn(
          "kp-button--primary",
          "Mark as seen",
          "It leaves the Inbox; Activity keeps it",
        ),
        SEEN,
        r.key,
      );
      b.addEventListener("click", async () => {
        b.setAttribute("disabled", "");
        const res = await send(
          "POST",
          "/data/notifications/read",
          { ids: [notice], read: true },
          "marking it seen",
        );
        if (res.ok) void loadNotices();
        else b.removeAttribute("disabled");
      });
      out.push(b);
    }
    return out;
  };

  /** @param {import("../inboxrows.js").Row} r @param {number} at */
  const rowEl = (r, at) => {
    const ask =
      r.source === "asks"
        ? current().asks.find((x) => `ask:${x.boot ?? ""}:${x.id}` === r.key)
        : null;
    const words = ask ? askWords(ask, at) : null;
    return h(
      "li",
      {
        class: "nx-inbox__row",
        "data-severity": r.severity,
        "data-kind": kindOf(r),
        "data-key": r.key,
        id: rowId(r.key),
        tabindex: "-1",
      },
      h("span", {
        class: `nx-sev nx-sev--${r.severity}`,
        title:
          r.severity === "bad"
            ? "urgent"
            : r.severity === "warn"
              ? "warning"
              : "worth a look",
      }),
      h(
        "div",
        { class: "nx-inbox__what" },
        h("strong", null, words?.title ?? r.title),
        ...((words?.why ?? r.why)
          ? [h("span", null, words?.why ?? r.why)]
          : []),
        h(
          "span",
          { class: "inbox-src" },
          ...chipsOf(r).map((c) =>
            c.href
              ? drivable(
                  h(
                    "a",
                    {
                      class: "kp-badge",
                      href: c.href,
                      title: `Open ${c.label}`,
                    },
                    c.label,
                  ),
                  SOURCE,
                  `${r.key}:${c.label}`,
                )
              : h("span", { class: "kp-badge" }, c.label),
          ),
        ),
      ),
      h("div", { class: "nx-inbox__acts" }, ...actsOf(r)),
    );
  };

  // ---- paint ----------------------------------------------------------
  let lastKey = "";
  const paint = () => {
    const { items, ready } = inboxNow();
    if (!ready) {
      list.replaceChildren(
        h(
          "li",
          { class: "inbox-loading" },
          skeletonLines(4, "Reading the Inbox"),
        ),
      );
      return;
    }
    head.live?.set(Date.now() / 1000);
    const rows = /** @type {import("../inboxrows.js").Row[]} */ (items);
    const empty = rows.length === 0;
    card.hidden = empty;
    clear.hidden = !empty;
    title.textContent = headline(rows);
    chips.setChips(
      KINDS.map((k) => ({
        value: k.value,
        label: k.label,
        count: rows.filter((r) => kindOf(r) === k.value).length,
      })),
    );
    const show = shownRows(rows, kinds, order);
    shown.textContent = `${show.length} of ${rows.length} shown`;
    // Redraw only when the rows changed (a question's countdown ticks
    // every second; its words update in place).
    const now = Date.now() / 1000;
    const key = show.map((r) => `${r.key}|${r.title}|${r.why}`).join("\n");
    if (key === lastKey) {
      for (const r of show) {
        if (r.source !== "asks") continue;
        const a = current().asks.find(
          (x) => `ask:${x.boot ?? ""}:${x.id}` === r.key,
        );
        const span = list.querySelector(
          `#${rowId(r.key)} .nx-inbox__what > span`,
        );
        if (a && span) span.textContent = askWords(a, now).why;
      }
      return;
    }
    lastKey = key;
    list.replaceChildren(...show.map((r) => rowEl(r, now)));
  };

  // ---- worth a look ---------------------------------------------------
  const abort = new AbortController();
  const readWorth = async () => {
    worthList.replaceChildren(
      h("li", null, skeletonLines(2, "Reading what is worth a look")),
    );
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
    worthN.textContent = ` · ${rows.length}`;
    worthList.replaceChildren(
      ...rows.map((r) =>
        h(
          "li",
          {
            class: "nx-inbox__row",
            "data-severity": "info",
            "data-key": r.key,
          },
          h("span", { class: "nx-sev nx-sev--info" }),
          h(
            "div",
            { class: "nx-inbox__what" },
            h("strong", null, r.title),
            h("span", null, r.why),
          ),
          h(
            "div",
            { class: "nx-inbox__acts" },
            drivable(
              link(
                r.href,
                r.key === "worth:drills"
                  ? "Drill them now…"
                  : "Update the host…",
                r.key === "worth:drills"
                  ? "Backups, showing only the repositories never drilled"
                  : "The Host page, where the host updates itself",
              ),
              WORTH_OPEN,
              r.key,
            ),
          ),
        ),
      ),
      h(
        "li",
        {
          class: "nx-inbox__row",
          "data-severity": "info",
          "data-key": "worth:doctor",
        },
        h("span", { class: "nx-sev nx-sev--info" }),
        h(
          "div",
          { class: "nx-inbox__what" },
          h("strong", null, "The host's full doctor report"),
          h(
            "span",
            null,
            "Every check the host runs on itself, the healthy ones too. Its findings already show above when they need you.",
          ),
        ),
        h(
          "div",
          { class: "nx-inbox__acts" },
          drivable(
            link(
              "/host?section=doctor",
              "Open the report",
              "System ▸ Host, the doctor section",
            ),
            WORTH_OPEN,
            "worth:doctor",
          ),
        ),
      ),
    );
  };

  // ---- Check again ----------------------------------------------------
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

  // ---- keys: j / k move, Enter the row's main button, r check again ----
  /** @param {KeyboardEvent} e */
  const onKey = (e) => {
    if (e.defaultPrevented || e.ctrlKey || e.metaKey || e.altKey) return;
    const t = /** @type {HTMLElement | null} */ (e.target);
    if (t?.closest?.("input, textarea, select, [contenteditable]")) return;
    if (document.querySelector("dialog[open]")) return;
    const rows = /** @type {HTMLElement[]} */ ([
      ...list.querySelectorAll(":scope > li.nx-inbox__row"),
    ]);
    const at = rows.findIndex(
      (r) => r === document.activeElement || r.contains(document.activeElement),
    );
    if (e.key === "j" || e.key === "k") {
      if (!rows.length) return;
      e.preventDefault();
      const n =
        e.key === "j"
          ? Math.min(rows.length - 1, at + 1)
          : Math.max(0, at < 0 ? 0 : at - 1);
      rows[n].focus();
      rows[n].scrollIntoView({ block: "nearest" });
    } else if (
      e.key === "Enter" &&
      at >= 0 &&
      document.activeElement === rows[at]
    ) {
      const main = /** @type {HTMLElement | null} */ (
        rows[at].querySelector(".nx-inbox__acts > :last-child")
      );
      if (main) {
        e.preventDefault();
        main.click();
      }
    } else if (e.key === "r") {
      e.preventDefault();
      again.click();
    } else if (e.key === "Escape" && kinds.size) {
      chips.reset();
    }
  };
  document.addEventListener("keydown", onKey);

  const off = onInbox(paint);
  paint();
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
    off();
    abort.abort();
    document.removeEventListener("keydown", onKey);
  };
}
