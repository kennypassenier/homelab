// The notification centre (feat-overview-5, feat-ops-8, feat-ops-9): the
// notices, newest first; mark read; the push switch, the "long action"
// threshold, the snooze with its "snoozed until", and a mute per stack.

import { act, loadNotices, onAct, patchNoticeSettings, send } from "../act.js";
import { notify, refusalCallout } from "../actui.js";
import {
  badgeCell,
  bindTableUrl,
  h,
  stateWord,
  tableBlock,
  td,
} from "../dom.js";
import { formatTime, humanDuration } from "../format.js";
import {
  KIND_ORDER,
  SNOOZE_CHOICES,
  noticeRows,
  settingsBody,
  snoozeState,
  stackMuteRows,
} from "../notices.js";
import { sortKeys } from "../sortkeys.js";
import { current, subscribe } from "../store.js";
import {
  attachDataTables,
  compare,
  dataTable,
} from "/static/kp/js/datatable.js";
import { attachSwitches } from "/static/kp/js/forms.js";

/**
 * A kp switch. In a table cell it goes without its On/Off words (both are
 * in the DOM, which the table would read as the cell's text); the label
 * then says the state.
 * @param {string} id
 * @param {string | Node} label
 * @param {boolean} [words]
 */
function switchEl(id, label, words = true) {
  const input = h("input", {
    class: "kp-switch__input",
    type: "checkbox",
    role: "switch",
    id,
  });
  const wrap = h(
    "label",
    { class: "kp-switch" },
    input,
    ...(words
      ? [h("span", { class: "kp-switch__state", "aria-hidden": "true" })]
      : []),
    typeof label === "string" ? h("span", null, label) : label,
  );
  return { wrap, input };
}

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const keys = sortKeys();
  const err = h("div");
  const push = switchEl("notify-push", "Push to the phone (kyu)");
  const longInput = h("input", {
    class: "kp-field__input",
    type: "number",
    id: "notify-long",
    "aria-describedby": "notify-long-help",
    min: "1",
    max: "1440",
    step: "1",
  });
  const longSave = h("button", { type: "button", class: "kp-button" }, "Save");
  const snoozeSel = h(
    "select",
    { class: "kp-field__input", id: "notify-snooze-minutes" },
    ...SNOOZE_CHOICES.map((c) =>
      h("option", { value: String(c.minutes) }, c.label),
    ),
  );
  snoozeSel.value = "60";
  const snoozeBtn = h(
    "button",
    { type: "button", class: "kp-button", id: "notify-snooze" },
    "Snooze",
  );
  const unsnooze = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--ghost",
      id: "notify-unsnooze",
    },
    "End the snooze",
  );
  const snoozeText = h("p", {
    class: "snooze-state",
    id: "snooze-state",
    role: "status",
  });
  const markAll = h(
    "button",
    { type: "button", class: "kp-button", id: "notify-read-all" },
    "Mark all read",
  );
  const unreadText = h("span", { class: "measured", id: "notify-unread" });

  const notices = tableBlock({
    remember: "notices",
    caption: "Notifications, newest first",
    search: "Search notifications",
    state: "loading",
    nothing: "No notifications yet.",
    columns: [
      { label: "When", sort: "time" },
      { label: "Kind", sort: "text", order: KIND_ORDER, filter: "choice" },
      { label: "Stack", sort: "text", filter: "choice" },
      { label: "What", sort: "text", cls: "wide" },
      { label: "Push", sort: "text" },
      {
        label: "Read",
        sort: "text",
        order: "unread,read",
        filter: "choice",
        cls: "col-button",
      },
    ],
  });
  notices.tbody.id = "notices";
  const stacks = tableBlock({
    remember: "notify-stacks",
    caption: "Per stack",
    search: "Search stacks",
    state: "loading",
    nothing: "The host manages no stacks yet.",
    columns: [
      { label: "Stack", sort: "text" },
      { label: "Unread", sort: "number" },
      { label: "Push", sort: "text", order: "muted,on", filter: "choice" },
    ],
  });
  /** @type {Map<string, {input: HTMLInputElement, word: HTMLElement, unread: HTMLElement}>} */
  const stackRows = new Map();
  stacks.tbody.id = "notify-stacks";

  root.replaceChildren(
    h("h1", null, "Notifications"),
    err,
    h(
      "section",
      { class: "kp-card notify-settings", "aria-label": "Settings" },
      h("h2", null, "Settings"),
      push.wrap,
      h(
        "p",
        { class: "measured" },
        "A notice always lands in this list; the switch decides whether it also goes to the phone.",
      ),
      // The help goes under the row, not in the field: in the field it made
      // the field taller than the button beside it, and the row's
      // bottom-aligned button sat level with the help text instead of the
      // input (Kenny, 2026-09-29).
      h(
        "div",
        { class: "inline-fields" },
        h(
          "div",
          { class: "kp-field" },
          h(
            "label",
            { class: "kp-field__label", for: "notify-long" },
            "Push when an action ran at least (minutes)",
          ),
          longInput,
        ),
        longSave,
      ),
      h(
        "p",
        { class: "kp-field__help inline-fields__help", id: "notify-long-help" },
        "A shorter action pushes only when it failed.",
      ),
      h(
        "div",
        { class: "inline-fields" },
        h(
          "div",
          { class: "kp-field" },
          h(
            "label",
            { class: "kp-field__label", for: "notify-snooze-minutes" },
            "Snooze every notification for",
          ),
          snoozeSel,
        ),
        snoozeBtn,
        unsnooze,
      ),
      snoozeText,
    ),
    h(
      "div",
      { class: "title-row" },
      h("h2", null, "Notices"),
      unreadText,
      markAll,
    ),
    notices.wrap,
    h("h2", null, "Per stack"),
    h(
      "p",
      { class: "measured" },
      "A muted stack's notices still land in the list; they never go to the phone.",
    ),
    stacks.wrap,
  );
  const detachSwitches = attachSwitches(root);
  const detach = attachDataTables(root, { compare: keys.compare(compare) });
  const nTable = dataTable(notices.wrap);
  const sTable = dataTable(stacks.wrap);
  const unbindN = bindTableUrl(nTable, "notices");
  const unbindS = bindTableUrl(sTable, "nstacks");

  /** @param {Partial<import("../notices.js").NotifySettings>} change */
  const saveSettings = async (change) => {
    const s = act.notices?.settings;
    if (!s) return;
    const r = await send(
      "PUT",
      "/data/notifications/settings",
      settingsBody(s, change),
      "the notification settings",
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
  longSave.addEventListener("click", () => {
    const m = Number(longInput.value);
    if (!Number.isFinite(m) || m < 1 || m > 1440) {
      longInput.focus();
      notify("Pick 1 to 1440 minutes.", "warning");
      return;
    }
    void saveSettings({ long_action_s: Math.round(m * 60) });
  });
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
  snoozeBtn.addEventListener(
    "click",
    () => void snooze(Number(snoozeSel.value)),
  );
  unsnooze.addEventListener("click", () => void snooze(0));
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
  notices.tbody.addEventListener("click", async (e) => {
    const b = /** @type {Element} */ (e.target).closest("button[data-read]");
    if (!b) return;
    const id = Number(/** @type {HTMLElement} */ (b).dataset.read);
    const r = await send(
      "POST",
      "/data/notifications/read",
      { ids: [id], read: true },
      "marking read",
    );
    if (!r.ok) err.replaceChildren(refusalCallout(r.error));
    else void loadNotices();
  });
  stacks.tbody.addEventListener("change", async (e) => {
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

  let noticesSig = "";
  let stacksSig = "";
  const paintSnooze = () => {
    const s = act.notices?.settings;
    const st = snoozeState(s, Date.now() / 1000);
    snoozeText.textContent = st.text;
    snoozeText.dataset.on = String(st.on);
    unsnooze.hidden = !st.on;
  };
  const render = () => {
    const snap = act.notices;
    if (!snap) {
      if (act.failed.notices) {
        notices.failed(act.failed.notices);
        stacks.failed(act.failed.notices);
      }
      return;
    }
    push.input.checked = snap.settings.push;
    if (document.activeElement !== longInput)
      longInput.value = String(Math.round(snap.settings.long_action_s / 60));
    longInput.title = `now ${humanDuration(snap.settings.long_action_s)}`;
    paintSnooze();
    unreadText.textContent = `${snap.unread} unread`;
    markAll.disabled = snap.unread === 0;
    const rows = noticeRows(snap.notices);
    const sig = JSON.stringify(rows.map((r) => [r.id, r.read]));
    const noticesChanged = sig !== noticesSig;
    if (noticesChanged) {
      noticesSig = sig;
      notices.tbody.replaceChildren(
        ...rows.map((r) =>
          h(
            "tr",
            {
              "data-notice": String(r.id),
              "data-kp-row-key": String(r.id),
              class: r.read === "unread" ? "unread" : "",
            },
            td(keys.note("time", formatTime(r.at), r.at)),
            badgeCell(r.kind),
            td(r.stack),
            h(
              "td",
              null,
              h("strong", null, r.title),
              ...(r.body ? [h("br"), r.body] : []),
              ...(r.job
                ? [
                    " ",
                    h("a", { href: `/app/jobs?job=${r.job}` }, `job ${r.job}`),
                  ]
                : []),
            ),
            td(r.push),
            r.read === "unread"
              ? h(
                  "td",
                  null,
                  h(
                    "button",
                    {
                      type: "button",
                      class: "kp-button kp-button--sm",
                      "data-read": String(r.id),
                    },
                    "Mark read",
                  ),
                )
              : h("td", null, h("span", { class: "read-mark" }, "read")),
          ),
        ),
      );
    }
    notices.ready({ refresh: noticesChanged });
    const names = (current().fleet?.stacks ?? []).map((s) => s.name);
    const mrows = stackMuteRows(names, snap);
    const msig = JSON.stringify(mrows.map((r) => r.stack));
    let changed = msig !== stacksSig;
    if (changed) {
      stacksSig = msig;
      stackRows.clear();
      stacks.tbody.replaceChildren(
        ...mrows.map((r) => {
          const word = stateWord("on", ["on", "muted"]);
          const sw = switchEl(`mute-${r.stack}`, word, false);
          sw.input.dataset.mute = r.stack;
          sw.input.setAttribute("aria-label", `Push for ${r.stack}`);
          const unread = td("", "num");
          stackRows.set(r.stack, { input: sw.input, word, unread });
          return h(
            "tr",
            { "data-kp-row-key": r.stack },
            td(r.stack),
            unread,
            h("td", null, sw.wrap),
          );
        }),
      );
    }
    // A toggle changes its row in place: the same switch stays under the
    // pointer and keeps the keyboard focus (Kenny, 2026-09-29).
    for (const r of mrows) {
      const row = stackRows.get(r.stack);
      if (!row) continue;
      const text = r.muted ? "muted" : "on";
      if (row.word.textContent !== text) {
        row.word.textContent = text;
        changed = true;
      }
      if (row.input.checked === r.muted) row.input.checked = !r.muted;
      if (row.unread.textContent !== String(r.unread)) {
        row.unread.textContent = String(r.unread);
        changed = true;
      }
    }
    stacks.ready({ refresh: changed });
  };
  const off = onAct("notices", render);
  const unsub = subscribe(render);
  const timer = setInterval(paintSnooze, 1000);
  const first = () => {
    if (act.notices) return;
    const words = "Reading the notifications from the dashboard…";
    notices.loading({ words });
    stacks.loading({ words });
    void loadNotices();
  };
  root.addEventListener("kp-datatable-retry", first);
  first();
  render();
  return () => {
    off();
    unsub();
    root.removeEventListener("kp-datatable-retry", first);
    clearInterval(timer);
    unbindN();
    unbindS();
    detach();
    detachSwitches();
  };
}
