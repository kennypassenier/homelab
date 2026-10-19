// System › Notification rules (feat-overview-5, feat-ops-9; redesign-final-c3,
// 3.71.0's final review), rebuilt on kp-themes 11.0.0 (feat-kp11demo-4, the
// review demo app-homelab-inbox, approved 2026-10-19): who gets bothered
// and when. A Delivery card — hold every push for a while (kp snooze
// picker, never more than the 7 days the server takes), the daily digest's
// time (kp time field, 24 hours, saved as you change it, with Undo), push
// to the phone off only behind a deliberate tick — and a Routing card: each
// stack's push on or off (kp facets: one request with the whole muted list)
// and the last five notices as these rules would have sent them. The
// notice list itself lives in the Inbox since 3.71.0, so this page holds
// the rules only.
//
// Not here yet because the server has no field for it (the demo marked
// each as sample data): quiet hours, push per kind and severity, and a
// "Send a test push" endpoint.

import { act, loadNotices, onAct, patchNoticeSettings, send } from "../act.js";
import { refusalCallout } from "../actui.js";
import { h } from "../dom.js";
import { declare, declareField, drivable } from "../drivable.js";
import { putState } from "../inboxcards.js";
import { settingsBody, stackMuteRows } from "../notices.js";
import {
  digestLast,
  pushChip,
  pushVerdict,
  snoozeMinutes,
} from "../notifyrules.js";
import { current, subscribe } from "../store.js";
import { ensureStyle, pageHeader, toastRegion } from "../ui.js";
import { ackGate, attachAckGates } from "/static/kp/js/ack-gate.js";
import { attachCardStates } from "/static/kp/js/card-state.js";
import {
  attachFacets,
  setFacetItems,
  setFacetValues,
} from "/static/kp/js/facets.js";
import { attachFootprints } from "/static/kp/js/footprint.js";
import { attachNotices, setNotices } from "/static/kp/js/notices.js";
import { toast } from "/static/kp/js/overlays.js";
import {
  attachSnoozes,
  setSnoozeBusy,
  setSnoozedUntil,
} from "/static/kp/js/snooze.js";
import { attachTimes, setTime } from "/static/kp/js/timefield.js";
import { undoToast } from "/static/kp/js/undo-toast.js";

// review M5: every page field Live view may set is declared.
const NOTIFY_DIGEST = declareField({
  id: "notify-digest",
  page: "notifications",
  what: "the time of the daily digest (saves as you change it)",
});
const PUSH_OFF_ACK = declareField({
  id: "push-off-ack",
  page: "notifications",
  what: "the tick that lets Turn push off go: nothing reaches the phone until push is turned on again",
});

// fix-239: Live view reaches every rule.
const PUSH_TO_PHONE = declare({
  id: "push-to-phone",
  page: "notifications",
  opens: "run",
  what: "turn pushing urgent notifications to the phone on again",
  shows: "while push to the phone is off",
});
const PUSH_OFF_OPEN = declare({
  id: "push-off-open",
  page: "notifications",
  opens: "view",
  what: "open the box with the tick that lets push to the phone be turned off",
  shows: "while push to the phone is on",
});
const PUSH_OFF = declare({
  id: "push-off",
  page: "notifications",
  opens: "run",
  what: "turn push to the phone off (needs the tick in the box first)",
  shows: "after Turn push off… was pressed",
  reach: [{ do: "click", control: "push-off-open" }],
});
const STACK_PUSH = declare({
  id: "stack-push",
  page: "notifications",
  opens: "run",
  row: "<stack>",
  what: "turn one stack's pushes on or off (muted: the Inbox only)",
});
const STACK_PUSH_ONLY = declare({
  id: "stack-push-only",
  page: "notifications",
  opens: "run",
  row: "<stack>",
  what: "push for this stack only: every other stack is muted",
});
const STACK_PUSH_ALL = declare({
  id: "stack-push-all",
  page: "notifications",
  opens: "run",
  what: "turn every stack's pushes on or off together",
});
// redesign-final-c3: the demo's snooze choices, one press each; kp11: the
// kp snooze picker's chips (1 hour, 4 hours, tonight, tomorrow, Monday, a
// date and time).
const SNOOZE_FOR = declare({
  id: "snooze-for",
  // The snooze length was a select (field notify-snooze-minutes) with a
  // Snooze button (control snooze); it is one press per choice now.
  was: ["notify-snooze-minutes", "snooze"],
  page: "notifications",
  opens: "run",
  row: "1h|4h|tonight|tomorrow|monday|pick",
  what: "hold every push for 1 hour, 4 hours, until tonight 18:00, tomorrow 07:00, Monday 07:00 or a date and time (at most 7 days); pick is the button of the date panel",
});
const END_SNOOZE = declare({
  id: "end-snooze",
  page: "notifications",
  opens: "run",
  what: "end the snooze now",
  shows: "while notifications are snoozed",
  reach: [{ do: "click", control: "snooze-for", row: "1h" }],
});

/**
 * One row of the Delivery form grid: its name, its control, a note and the
 * buttons, each on the grid's shared tracks.
 * @param {string} id
 * @param {string} title
 * @param {Node[]} control
 * @param {Node[]} [value]
 * @param {Node[]} [actions]
 */
const rule = (id, title, control, value = [], actions = []) =>
  h(
    "div",
    { class: "kp-formgrid__row" },
    h("span", { class: "kp-formgrid__label", id }, title),
    h("div", { class: "kp-formgrid__control" }, ...control),
    h("div", { class: "kp-formgrid__value" }, ...value),
    h("div", { class: "kp-formgrid__actions" }, ...actions),
  );

/** @param {string} text @param {string} [id] */
const hint = (text, id) => h("p", { class: "nr-hint", id }, text);

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  ensureStyle("/css/pages/notifications.css");
  root.classList.add("nr-page");
  /** @type {(() => void)[]} */
  const cleanups = [];
  const err = h("div", { class: "nr-err" });
  const head = pageHeader({
    title: "Notification rules",
    desc: "Who gets bothered and when: only urgent notices reach the phone at once; the rest wait for the daily digest. The notices themselves are in the Inbox.",
  });

  // ── Delivery ───────────────────────────────────────────────────────
  const state = h("span", {
    class: "kp-badge kp-badge--success kp-state-word kp-state-word--center",
    id: "notify-push-state",
    "data-kp-words":
      "Push on · not snoozed\nSnoozed until 20/10/2026 07:00\nPush off",
  });
  const snoozeEl = h("div", {
    class: "kp-snooze nr-snooze",
    "data-kp-snooze": "",
    "data-kp-look": "chips",
    "data-kp-snooze-choices": "1h 4h tonight tomorrow monday pick",
    "data-kp-snooze-morning": "07:00",
    "data-kp-snooze-max": "7d",
    "aria-labelledby": "nr-snooze-l",
  });
  const digestEl = h("div", {
    class: "kp-time nr-digest",
    "data-kp-time": "",
    "data-kp-name": "digest_at",
    "data-kp-value": "",
    "data-kp-label": "Daily digest at",
    "data-kp-time-step": "15",
    "data-kp-time-presets": "Morning=09:00,Evening=18:00",
    "data-kp-time-id": NOTIFY_DIGEST,
  });
  const digestHint = hint("", "nr-digest-hint");
  const pushOpen = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button nr-col-btn",
        "aria-expanded": "false",
        "aria-controls": "nr-push-gate",
        title: "Open the box with the tick that lets push be turned off",
      },
      "Turn push off…",
    ),
    PUSH_OFF_OPEN,
  );
  const pushOn = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--primary nr-col-btn",
        title: "Pushes go to the phone again",
        hidden: "",
      },
      "Turn push on",
    ),
    PUSH_TO_PHONE,
  );
  const pushOff = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--destructive nr-col-btn",
        id: "nr-push-off",
        title: "Nothing reaches the phone until push is turned on again",
      },
      "Turn push off",
    ),
    PUSH_OFF,
  );
  const pushKeep = h(
    "button",
    {
      type: "button",
      class: "kp-button nr-col-btn",
      title: "Leave push on",
    },
    "Keep push on",
  );
  const gate = h(
    "div",
    {
      class: "kp-alert kp-alert--warning nr-gate",
      id: "nr-push-gate",
      hidden: "",
      "data-kp-ack": "",
      "data-kp-ack-for": "#nr-push-off",
      "data-kp-ack-key": "push-off",
      "data-kp-ack-label":
        "I understand nothing reaches my phone until I turn it on again",
      "data-kp-ack-tone": "warning",
    },
    h(
      "div",
      { class: "kp-alert__body" },
      h("strong", null, "Push off holds everything, including failures."),
      " The Inbox keeps filling and the daily digest still goes out, but a failed backup will not wake you.",
    ),
    h("div", { class: "nr-gate__actions" }, pushOff, pushKeep),
  );
  const delivery = h(
    "section",
    {
      class: "kp-card nr-card",
      id: "delivery",
      "data-kp-state": "loading",
      "aria-labelledby": "nr-delivery-t",
    },
    h(
      "div",
      { class: "kp-card__header" },
      h("h2", { class: "kp-card__title", id: "nr-delivery-t" }, "Delivery"),
      h("div", { class: "kp-card__actions" }, state),
    ),
    h(
      "p",
      { class: "kp-card__body" },
      "What reaches your phone from pve and its stacks, and when. Saved as you change it; times are Brussels time.",
    ),
    h(
      "div",
      { class: "kp-card__stage" },
      h(
        "div",
        { class: "kp-card__data" },
        h(
          "div",
          {
            class: "kp-formgrid",
            "data-kp-formgrid-lines": "lines",
            "aria-label": "Delivery settings",
          },
          rule(
            "nr-snooze-l",
            "Hold every push",
            [snoozeEl],
            [
              hint(
                "For a night, a weekend or a holiday. The Inbox keeps filling; only the pushes and the digest wait. At most 7 days.",
              ),
            ],
          ),
          rule("nr-digest-l", "Daily digest at", [digestEl], [digestHint]),
          rule("nr-push-l", "Push to phone", [gate], [], [pushOpen, pushOn]),
        ),
      ),
    ),
  );

  // ── Routing ────────────────────────────────────────────────────────
  const routingCount = h("span", {
    class: "kp-timestamp",
    "aria-live": "polite",
  });
  const stacksEl = h(
    "div",
    {
      class: "kp-facets nr-facets",
      "data-kp-facets": "",
      "data-kp-facets-all": "Every stack",
      role: "group",
      "aria-labelledby": "nr-stacks-h",
    },
    h("ul", { class: "kp-facets__list" }),
  );
  const stacksSay = hint(
    "Unread counts are exact. “Only” switches every other stack off. Every change is saved at once, as one request with the whole list of muted stacks.",
  );
  const preview = h("div", {
    class: "kp-notices nr-notices",
    "data-kp-notices": "",
    "data-kp-notices-look": "page",
    "data-kp-keys": "off",
    "data-kp-toast-new": "none",
    "data-kp-count-in-title": "off",
    "aria-labelledby": "nr-preview-h",
    id: "nr-preview",
  });
  const routing = h(
    "section",
    {
      class: "kp-card nr-card nr-card--routing",
      id: "per-stack",
      "data-kp-state": "loading",
      "aria-labelledby": "nr-routing-t",
    },
    h(
      "div",
      { class: "kp-card__header" },
      h("h2", { class: "kp-card__title", id: "nr-routing-t" }, "Routing"),
      h("div", { class: "kp-card__actions" }, routingCount),
    ),
    h(
      "p",
      { class: "kp-card__body" },
      "Decides, for every stack, whether it pushes to your phone or waits for the digest, and shows what the last five notices would have done under these rules. A muted stack still lands in the Inbox; it never pushes.",
    ),
    // The last five notices are the notices centre's own component, with its
    // own busy card and failed plate, so it stands beside the card's stage
    // (a state of the stacks' data would hide it) on the card's grid.
    h(
      "div",
      { class: "nr-routing__preview" },
      h(
        "h3",
        { class: "nr-h", id: "nr-preview-h" },
        "The last five notices under these rules",
      ),
      preview,
    ),
    h(
      "div",
      { class: "kp-card__stage nr-routing__stage" },
      h(
        "div",
        { class: "kp-card__data nr-routing__stacks" },
        h("h3", { class: "nr-h", id: "nr-stacks-h" }, "By stack"),
        stacksEl,
        stacksSay,
      ),
    ),
  );

  root.replaceChildren(head.el, err, delivery, routing);
  cleanups.push(
    attachCardStates(root),
    attachSnoozes(delivery),
    attachTimes(delivery),
    attachAckGates(delivery),
    attachFacets(routing),
    attachNotices(routing),
    attachFootprints(routing),
  );

  // ── Live view marks on what the package draws ──────────────────────
  const markSnooze = () => {
    for (const b of /** @type {NodeListOf<HTMLElement>} */ (
      snoozeEl.querySelectorAll(".kp-snooze__chip")
    ))
      drivable(b, SNOOZE_FOR, b.dataset.value ?? "");
    const apply = snoozeEl.querySelector(
      ".kp-snooze__actions .kp-button--primary",
    );
    if (apply instanceof HTMLElement) drivable(apply, SNOOZE_FOR, "pick");
    const end = snoozeEl.querySelector(".kp-snooze__end");
    if (end instanceof HTMLElement) drivable(end, END_SNOOZE);
  };
  const snoozeWatch = new MutationObserver(markSnooze);
  snoozeWatch.observe(snoozeEl, { childList: true, subtree: true });
  cleanups.push(() => snoozeWatch.disconnect());
  markSnooze();
  const tick = gate.querySelector("input[type=checkbox]");
  if (tick instanceof HTMLInputElement) tick.id = PUSH_OFF_ACK;

  /** @param {string} stack @returns {HTMLInputElement | null} */
  const facetBox = (stack) =>
    stacksEl.querySelector(
      `.kp-facets__list [data-value="${CSS.escape(stack)}"] .kp-facet__check`,
    );
  const markStacks = () => {
    for (const row of stacksEl.querySelectorAll(
      ".kp-facets__list [data-value]",
    )) {
      const stack = row.getAttribute("data-value") ?? "";
      const box = row.querySelector(".kp-facet__check");
      if (box instanceof HTMLElement) drivable(box, STACK_PUSH, stack);
      const only = row.querySelector(".kp-facet__only");
      if (only instanceof HTMLElement) drivable(only, STACK_PUSH_ONLY, stack);
    }
    const all = stacksEl.querySelector("[data-kp-facet-all] .kp-facet__check");
    if (all instanceof HTMLElement) drivable(all, STACK_PUSH_ALL);
  };

  // ── Saving ─────────────────────────────────────────────────────────
  /** @param {Partial<import("../notices.js").NotifySettings>} change */
  const saveSettings = async (change) => {
    const s = act.notices?.settings;
    if (!s) return { ok: false, error: null };
    const r = await send(
      "PUT",
      "/data/notifications/settings",
      settingsBody(s, change),
      "the notification rules",
    );
    if (!r.ok) {
      err.replaceChildren(refusalCallout(r.error, "destructive", "Not saved"));
      return { ok: false, error: r.error };
    }
    err.replaceChildren();
    patchNoticeSettings(r.body ?? change);
    return { ok: true, error: null };
  };
  const region = () => toastRegion(root);

  // The snooze: a choice holds every push until its moment.
  snoozeEl.addEventListener("kp-snooze", async (event) => {
    const until = Date.parse(/** @type {CustomEvent} */ (event).detail.until);
    setSnoozeBusy(snoozeEl, true);
    const r = await send(
      "POST",
      "/data/notifications/snooze",
      { minutes: snoozeMinutes(until, Date.now()) },
      "the snooze",
    );
    setSnoozeBusy(snoozeEl, false);
    if (!r.ok) {
      err.replaceChildren(
        refusalCallout(r.error, "destructive", "Not snoozed"),
      );
      paintDelivery();
      return;
    }
    err.replaceChildren();
    patchNoticeSettings({ snooze_until: r.body?.snooze_until ?? null });
  });
  snoozeEl.addEventListener("kp-snooze-end", async () => {
    const r = await send(
      "POST",
      "/data/notifications/snooze",
      { minutes: 0 },
      "the snooze",
    );
    if (!r.ok) {
      err.replaceChildren(refusalCallout(r.error, "destructive", "Not ended"));
      paintDelivery();
      return;
    }
    err.replaceChildren();
    patchNoticeSettings({ snooze_until: null });
  });

  // The digest saves as it changes; Undo puts the old time back.
  /** @type {string} */
  let digestSaved = "";
  digestEl.addEventListener("kp-time-change", async (event) => {
    const d = /** @type {CustomEvent} */ (event).detail;
    const next = String(d.value || "");
    // A half-typed time is the field's own business until it is valid.
    if (!next && String(d.text ?? "").trim() !== "") return;
    if (next === digestSaved) return;
    const previous = digestSaved;
    const r = await saveSettings({ digest_at: next === "" ? null : next });
    if (!r.ok) {
      setTime(digestEl, previous);
      toast(
        `Not saved. The digest stays ${previous ? `at ${previous}` : "off"}.`,
        { region: region(), ms: 6000 },
      );
      return;
    }
    digestSaved = next;
    undoToast(
      next === "" ? "Daily digest turned off" : `Digest set to ${next}`,
      {
        mode: "revert",
        region: region(),
        ms: 6000,
        max: 2,
        undo: () => {
          digestSaved = previous;
          setTime(digestEl, previous);
          void saveSettings({ digest_at: previous === "" ? null : previous });
        },
      },
    );
  });

  // Push to the phone off sits behind a deliberate tick.
  const closeGate = () => {
    gate.hidden = true;
    pushOpen.setAttribute("aria-expanded", "false");
    ackGate(gate)?.set(false);
  };
  pushOpen.addEventListener("click", () => {
    gate.hidden = false;
    pushOpen.setAttribute("aria-expanded", "true");
    gate.scrollIntoView({ block: "nearest" });
  });
  pushKeep.addEventListener("click", () => {
    closeGate();
    pushOpen.focus();
  });
  pushOff.addEventListener("click", async () => {
    const r = await saveSettings({ push: false });
    if (!r.ok) return;
    closeGate();
    pushOn.focus();
  });
  pushOn.addEventListener("click", async () => {
    const r = await saveSettings({ push: true });
    if (r.ok) pushOpen.focus();
  });

  // The stacks: one request carries the whole muted list.
  let stackNames = /** @type {string[]} */ ([]);
  stacksEl.addEventListener("kp-facet-change", async (event) => {
    const on = new Set(
      /** @type {string[]} */ (
        /** @type {CustomEvent} */ (event).detail.values
      ),
    );
    const muted = stackNames.filter((n) => !on.has(n));
    const r = await saveSettings({ muted_stacks: muted });
    if (!r.ok) paintRouting();
  });
  routing.addEventListener("kp-card-retry", () => void retry(routing));
  delivery.addEventListener("kp-card-retry", () => void retry(delivery));
  /** @param {HTMLElement} which */
  const retry = async (which) => {
    putState(which, "loading", { words: "Reading the rules…" });
    await loadNotices();
    paintAll();
  };

  // ── Painting ───────────────────────────────────────────────────────
  const failed = () => act.failed.notices;
  const failedState = {
    title: "The rules could not be read",
    text: "The dashboard did not get them from the host. Try again.",
  };

  const paintDelivery = () => {
    const snap = act.notices;
    if (!snap) {
      putState(
        delivery,
        failed() ? "error" : "loading",
        failed() ? failedState : { words: "Reading the rules…" },
      );
      return;
    }
    putState(delivery, "ready");
    const s = snap.settings;
    const c = pushChip(s, Date.now() / 1000);
    state.textContent = c.text;
    state.className =
      `kp-badge ${c.tone === "ok" ? "kp-badge--success" : c.tone === "warn" ? "kp-badge--warning" : ""} kp-state-word kp-state-word--center`.replace(
        /\s+/g,
        " ",
      );
    pushOpen.hidden = !s.push;
    pushOn.hidden = s.push;
    if (!s.push) closeGate();
    setSnoozedUntil(
      snoozeEl,
      c.snoozed && s.snooze_until ? s.snooze_until * 1000 : null,
    );
    if (!digestEl.contains(document.activeElement)) {
      digestSaved = s.digest_at ?? "";
      setTime(digestEl, digestSaved);
    }
    digestHint.textContent = `${
      s.digest_at
        ? "One message with everything that did not need to wake you."
        : "No digest: only urgent notices push."
    } ${digestLast(snap.last_digest ?? null)} Empty the field for no digest.`;
  };

  let lastRouting = "";
  const paintRouting = () => {
    const snap = act.notices;
    if (!snap) {
      putState(
        routing,
        failed() ? "error" : "loading",
        failed() ? failedState : { words: "Reading the rules…" },
      );
      // The preview is the package's notices centre: its own busy card and
      // failed plate while the rules are not here.
      lastRouting = "";
      setNotices(preview, {
        state: failed() ? "error" : "loading",
        items: [],
        sources: [],
        ...(failed() ? { error: failedState.title } : {}),
      });
      return;
    }
    const names = (current().fleet?.stacks ?? []).map((s) => s.name);
    const rows = stackMuteRows(names, snap);
    stackNames = rows.map((r) => r.stack);
    if (rows.length === 0) {
      putState(routing, "empty", {
        title: "No stacks yet",
        text: "Stacks appear here once the host knows them.",
      });
      lastRouting = "";
      setNotices(preview, { state: "empty", items: [], sources: [] });
      return;
    }
    putState(routing, "ready");
    const now = Date.now() / 1000;
    const last = [...snap.notices]
      .sort((a, b) => b.at - a.at || b.id - a.id)
      .slice(0, 5);
    // Drawn again only when a stack, a count, a rule or one of the five
    // changed: the page paints on every live event.
    const sig = JSON.stringify([
      rows,
      snap.settings,
      last.map((n) => [n.id, n.title, n.level, n.stack]),
      Math.floor(now / 60),
    ]);
    if (sig === lastRouting) return;
    lastRouting = sig;
    const muted = rows.filter((r) => r.muted).length;
    routingCount.textContent = `${rows.length - muted} of ${rows.length} stacks push`;
    setFacetItems(
      stacksEl,
      rows.map((r) => ({
        value: r.stack,
        label: r.stack,
        count: r.unread,
        pattern: r.stack,
        checked: !r.muted,
      })),
    );
    setFacetValues(
      stacksEl,
      rows.filter((r) => !r.muted).map((r) => r.stack),
    );
    markStacks();
    for (const r of rows) {
      const box = facetBox(r.stack);
      if (box) box.id = `mute-${r.stack}`;
    }
    setNotices(preview, {
      state: "ready",
      items: last.map((n) => {
        const v = pushVerdict(n, snap.settings, now);
        return {
          key: `n:${n.id}`,
          severity:
            n.level === "critical"
              ? "critical"
              : n.level === "warning"
                ? "warning"
                : "info",
          title: n.title,
          since: n.at * 1000,
          consequence: v.why,
          read: !v.pushed,
          ...(n.stack ? { source: n.stack } : {}),
        };
      }),
      sources: [],
    });
  };

  const paintAll = () => {
    paintDelivery();
    paintRouting();
  };
  cleanups.push(onAct("notices", paintAll), subscribe(paintAll));
  // The state word follows a snooze that ends by itself.
  const clock = window.setInterval(paintDelivery, 30_000);
  cleanups.push(() => window.clearInterval(clock));
  if (!act.notices) void loadNotices();
  paintAll();
  return () => {
    for (const c of cleanups) c();
    root.classList.remove("nr-page");
  };
}
