// System › Notification rules (feat-overview-5, feat-ops-9; redesign-final-c3,
// 3.71.0's final review): who gets bothered and when, as the approved
// notifications.html draws its rules — a Delivery card (push to the phone,
// the daily digest, a snooze segment) and a Per stack card (each stack's
// unread count and its push switch). The notice list itself lives in the
// Inbox since 3.71.0 (FLOWS.md §2), so this page holds the rules only.

import { act, loadNotices, onAct, patchNoticeSettings, send } from "../act.js";
import { notify, refusalCallout } from "../actui.js";
import { h } from "../dom.js";
import { declare, declareField, drivable, fieldId } from "../drivable.js";
import { settingsBody, stackMuteRows } from "../notices.js";
import { digestWords, pushChip, snoozeChoices } from "../notifyrules.js";
import { current, subscribe } from "../store.js";
import {
  dot,
  ensureStyle,
  pageHeader,
  section,
  segSwitch,
  skeletonLines,
} from "../ui.js";
import { attachSwitches } from "/static/kp/js/forms.js";

// review M5: every page field Live view may set is declared.
const NOTIFY_PUSH = declareField({
  id: "notify-push",
  page: "notifications",
  what: "push notifications to the phone",
});
const NOTIFY_DIGEST = declareField({
  id: "notify-digest",
  page: "notifications",
  what: "the time of the daily digest",
});
const MUTE = declareField({
  id: "mute",
  page: "notifications",
  what: "mute one stack's notifications",
  row: "<stack>",
});

// fix-239: Live view reaches every rule.
const PUSH_TO_PHONE = declare({
  id: "push-to-phone",
  page: "notifications",
  opens: "run",
  what: "turn pushing urgent notifications to the phone on or off",
});
const STACK_PUSH = declare({
  id: "stack-push",
  page: "notifications",
  opens: "run",
  row: "<stack>",
  what: "turn one stack's pushes on or off (muted: the Inbox only)",
});
const SAVE_DIGEST = declare({
  id: "save-digest",
  page: "notifications",
  opens: "run",
  what: "save the digest time",
});
const SNOOZE = declare({
  id: "snooze",
  page: "notifications",
  opens: "run",
  what: "snooze every push for an hour (Snooze 1 h)",
});
// redesign-final-c3: the demo's snooze segment, one press per length.
const SNOOZE_FOR = declare({
  id: "snooze-for",
  // The snooze length was a select (field notify-snooze-minutes) with a
  // Snooze button; it is one press per length now.
  was: ["notify-snooze-minutes"],
  page: "notifications",
  opens: "run",
  row: "60|240|morning|1440",
  what: "snooze every push for 1 h, 4 h, until 07:00 or a day",
});
const END_SNOOZE = declare({
  id: "end-snooze",
  page: "notifications",
  opens: "run",
  what: "end the snooze now",
  shows: "while notifications are snoozed",
  reach: [{ do: "click", control: "snooze" }],
});
const MARK_ALL_READ = declare({
  id: "mark-all-read",
  page: "notifications",
  opens: "run",
  what: "mark every notification read",
  shows: "while a notification is unread",
});

/**
 * A kp switch with its label beside it.
 * @param {string} id
 * @param {string} label
 */
function switchEl(id, label) {
  const input = /** @type {HTMLInputElement} */ (
    h("input", {
      class: "kp-switch__input",
      type: "checkbox",
      role: "switch",
      id,
      "aria-label": label,
    })
  );
  return { wrap: h("label", { class: "kp-switch" }, input), input };
}

/**
 * One rule: its name and sentence on the left, its control on the right,
 * an optional control under both (the demo's `.setting`).
 * @param {string} title
 * @param {Node | string} desc
 * @param {Node | null} control
 * @param {Node} [below]
 */
const setting = (title, desc, control, below) =>
  h(
    "div",
    { class: "nr-setting" },
    h("div", null, h("b", null, title), h("span", null, desc)),
    control ?? h("span"),
    below ?? null,
  );

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  ensureStyle("/css/pages/notifications.css");
  root.classList.add("nr-page");
  const err = h("div");
  const pushState = h("span", { class: "nr-chip", id: "notify-push-state" });
  const digestState = h("span", { class: "nr-chip", id: "notify-digest-at" });
  const unreadState = h("span", { class: "nr-chip", id: "notify-unread" });
  const snoozeHour = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button",
        id: "notify-snooze",
        title: "Hold every push for an hour; the Inbox keeps filling",
      },
      "Snooze 1 h",
    ),
    SNOOZE,
  );
  const markAll = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--primary",
        id: "notify-read-all",
        title: "Mark every notice read; the Inbox shows them still",
      },
      "Mark all read",
    ),
    MARK_ALL_READ,
  );
  const head = pageHeader({
    title: "Notification rules",
    desc: "Who gets bothered and when: only urgent notices reach the phone at once; the rest wait for the daily digest. The notices themselves are in the Inbox.",
    meta: [pushState, digestState, unreadState],
    actions: [snoozeHour, markAll],
  });

  // ── Delivery ───────────────────────────────────────────────────────
  const push = switchEl(NOTIFY_PUSH, "Push to the phone");
  drivable(push.input, PUSH_TO_PHONE);
  const digestInput = /** @type {HTMLInputElement} */ (
    h("input", {
      class: "kp-field__input nr-time",
      type: "time",
      id: NOTIFY_DIGEST,
      "aria-label": "Digest time",
      title: "Empty it for no digest",
    })
  );
  const digestSave = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm",
        id: "notify-digest-save",
        title: "Save the digest time (it also saves as you change it)",
      },
      "Save",
    ),
    SAVE_DIGEST,
  );
  const digestLine = h("span", { id: "notify-digest-last" });
  const snoozeSeg = segSwitch({
    label: "Snooze for",
    value: "",
    items: snoozeChoices(Date.now() / 1000).map((c) => ({
      value: c.value,
      label: c.label,
      hint: c.hint,
    })),
    drive: { id: SNOOZE_FOR },
    onChange: (v) => {
      const c = snoozeChoices(Date.now() / 1000).find((x) => x.value === v);
      if (c) void snooze(c.minutes);
    },
  });
  const unsnooze = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm kp-button--ghost",
        id: "notify-unsnooze",
        title: "End the snooze now; pushes go out again",
      },
      "End the snooze",
    ),
    END_SNOOZE,
  );
  const snoozeText = h("span", { id: "snooze-state", role: "status" });
  const delivery = section({
    id: "delivery",
    title: "Delivery",
    desc: "Where a notice goes besides the Inbox, and when.",
    foot: ["Saved as you change it", "Times are this browser's clock"],
  });
  const deliveryBody = h(
    "div",
    { class: "nr-settings" },
    setting(
      "Push to the phone",
      "Urgent only: a service down, a failed backup, update or deploy, a disk almost full or failing.",
      push.wrap,
    ),
    h("div", { class: "nr-hr" }),
    setting(
      "Daily digest",
      digestLine,
      h("div", { class: "nr-row" }, digestInput, digestSave),
    ),
    h("div", { class: "nr-hr" }),
    setting(
      "Snooze every push",
      h(
        "span",
        null,
        "The Inbox keeps filling; pushes and the digest wait until the snooze ends. ",
        snoozeText,
      ),
      unsnooze,
      snoozeSeg.el,
    ),
  );
  delivery.body.replaceChildren(skeletonLines(4, "Reading the rules"));

  // ── Per stack ──────────────────────────────────────────────────────
  const perStack = section({
    id: "per-stack",
    title: "Per stack",
    desc: "A muted stack still lands in the Inbox; it never pushes.",
  });
  const stackList = h("div", {
    class: "nr-stacks",
    id: "notify-stacks",
    role: "list",
  });
  perStack.body.replaceChildren(skeletonLines(5, "Reading the stacks"));
  /** @type {Map<string, {row: HTMLElement, input: HTMLInputElement, unread: HTMLElement}>} */
  const stackRows = new Map();

  root.replaceChildren(
    head.el,
    err,
    h("div", { class: "nr-cols" }, delivery.el, perStack.el),
  );
  const detachSwitches = attachSwitches(root);

  /** @param {Partial<import("../notices.js").NotifySettings>} change */
  const saveSettings = async (change) => {
    const s = act.notices?.settings;
    if (!s) return;
    const r = await send(
      "PUT",
      "/data/notifications/settings",
      settingsBody(s, change),
      "the notification rules",
    );
    if (!r.ok) {
      err.replaceChildren(refusalCallout(r.error, "destructive", "Not saved"));
      return;
    }
    err.replaceChildren();
    patchNoticeSettings(r.body ?? change);
    notify("Saved.", "success");
  };
  push.input.addEventListener(
    "change",
    () => void saveSettings({ push: push.input.checked }),
  );
  const saveDigest = () => {
    const v = digestInput.value.trim();
    if (v !== "" && !/^\d\d:\d\d$/.test(v)) {
      digestInput.focus();
      notify("Pick a time, or leave it empty for no digest.", "warning");
      return;
    }
    void saveSettings({ digest_at: v === "" ? null : v });
  };
  digestInput.addEventListener("change", saveDigest);
  digestSave.addEventListener("click", saveDigest);
  /** @param {number} minutes */
  const snooze = async (minutes) => {
    const r = await send(
      "POST",
      "/data/notifications/snooze",
      { minutes },
      "the snooze",
    );
    if (!r.ok) {
      err.replaceChildren(
        refusalCallout(r.error, "destructive", "Not snoozed"),
      );
      return;
    }
    err.replaceChildren();
    patchNoticeSettings({ snooze_until: r.body?.snooze_until ?? null });
  };
  snoozeHour.addEventListener("click", () => void snooze(60));
  unsnooze.addEventListener("click", () => {
    snoozeSeg.set("");
    void snooze(0);
  });
  markAll.addEventListener("click", async () => {
    const r = await send(
      "POST",
      "/data/notifications/read",
      {},
      "marking read",
    );
    if (!r.ok) err.replaceChildren(refusalCallout(r.error));
    else void loadNotices();
  });
  stackList.addEventListener("change", async (e) => {
    const input = /** @type {HTMLInputElement} */ (e.target);
    const stack = input.dataset.mute;
    if (!stack) return;
    const r = await send(
      "PUT",
      `/data/notifications/stacks/${encodeURIComponent(stack)}`,
      { muted: !input.checked },
      `the push switch of ${stack}`,
    );
    if (!r.ok) {
      err.replaceChildren(refusalCallout(r.error, "destructive", "Not saved"));
      input.checked = !input.checked;
    } else if (r.body) patchNoticeSettings(r.body);
  });

  let filled = false;
  const paintSnooze = () => {
    const s = act.notices?.settings;
    const c = pushChip(s, Date.now() / 1000);
    pushState.replaceChildren(dot(c.tone), c.text);
    snoozeText.textContent = c.snoozed ? `${c.text}.` : "Not snoozed.";
    unsnooze.hidden = !c.snoozed;
    if (!c.snoozed) snoozeSeg.set("");
  };
  const render = () => {
    const snap = act.notices;
    if (!snap) {
      if (act.failed.notices) {
        delivery.body.replaceChildren(
          refusalCallout(act.failed.notices, "destructive", "Not read"),
        );
        perStack.body.replaceChildren();
      }
      return;
    }
    if (!filled) {
      filled = true;
      delivery.body.replaceChildren(deliveryBody);
      perStack.body.replaceChildren(stackList);
    }
    push.input.checked = snap.settings.push;
    if (document.activeElement !== digestInput)
      digestInput.value = snap.settings.digest_at ?? "";
    digestLine.textContent = digestWords(
      { push: snap.settings.push, digest_at: snap.settings.digest_at ?? null },
      snap.last_digest ?? null,
    );
    digestState.textContent = snap.settings.digest_at
      ? `Digest daily at ${snap.settings.digest_at}`
      : "No daily digest";
    unreadState.textContent =
      snap.unread === 0 ? "All read" : `${snap.unread} unread in the Inbox`;
    markAll.disabled = snap.unread === 0;
    paintSnooze();
    const names = (current().fleet?.stacks ?? []).map((s) => s.name);
    const rows = stackMuteRows(names, snap);
    const sig = rows.map((r) => r.stack).join("|");
    if (sig !== [...stackRows.keys()].join("|")) {
      stackRows.clear();
      stackList.replaceChildren(
        ...rows.map((r) => {
          const sw = switchEl(fieldId(MUTE, r.stack), `Push for ${r.stack}`);
          sw.input.dataset.mute = r.stack;
          sw.input.title = `Off: ${r.stack} still lands in the Inbox, never on the phone`;
          drivable(sw.input, STACK_PUSH, r.stack);
          const unread = h("span", { class: "nr-count" });
          const row = h(
            "div",
            { class: "nr-stack", role: "listitem", "data-stack": r.stack },
            h("b", null, r.stack),
            unread,
            sw.wrap,
          );
          stackRows.set(r.stack, { row, input: sw.input, unread });
          return row;
        }),
      );
    }
    // A toggle changes its row in place: the switch stays under the
    // pointer and keeps the keyboard focus (Kenny, 2026-09-29).
    for (const r of rows) {
      const x = stackRows.get(r.stack);
      if (!x) continue;
      x.row.classList.toggle("is-muted", r.muted);
      if (x.input.checked === r.muted) x.input.checked = !r.muted;
      x.unread.textContent = String(r.unread);
      x.unread.classList.toggle("is-hot", r.unread > 0);
      x.unread.title = `${r.unread} unread`;
    }
  };
  const off = onAct("notices", render);
  const unsub = subscribe(render);
  const timer = setInterval(paintSnooze, 1000);
  if (!act.notices) void loadNotices();
  render();
  return () => {
    off();
    unsub();
    clearInterval(timer);
    detachSwitches();
  };
}
