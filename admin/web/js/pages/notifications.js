// The notification centre (feat-overview-5, feat-ops-9): the notices,
// newest first; mark read; the push switch, the daily digest's time, the
// snooze with its "snoozed until", and a mute per stack. Decision
// "Notifications and Grafana" (2026-09-30): every notice shows what, since
// when, the consequence and what to do, its page, and a Fix button that
// opens the action's dialog when the dashboard can run the remedy.

import { act, loadNotices, onAct, patchNoticeSettings, send } from "../act.js";
import { openAction } from "../actiondialog.js";
import { notify, refusalCallout } from "../actui.js";
import {
  badgeCell,
  bindTableUrl,
  h,
  stateWord,
  tableBlock,
  td,
} from "../dom.js";
import { formatDateTime } from "../format.js";
import {
  KIND_ORDER,
  LEVEL_ORDER,
  SNOOZE_CHOICES,
  digestText,
  fixLabel,
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
import { declare, declareField, drivable, fieldId } from "../drivable.js";

// review M5: every page field Live view may set is declared (drivable.js
// `declareField`); the client and the dashboard refuse any other.
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
const NOTIFY_SNOOZE = declareField({
  id: "notify-snooze-minutes",
  page: "notifications",
  what: "how long a snooze lasts",
});
const MUTE = declareField({
  id: "mute",
  page: "notifications",
  what: "mute one stack's notifications",
  row: "<stack>",
});

// fix-239: Live view reaches every notification control (`homelab ui
// click …`; the digest time and the snooze length are `homelab ui type
// notify-digest 07:30` and `homelab ui pick notify-snooze-minutes 60`).
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
  what: "turn one stack's pushes on or off (muted: the centre only)",
});
const SAVE_DIGEST = declare({
  id: "save-digest",
  page: "notifications",
  opens: "run",
  what: "save the bulletin time",
});
const SNOOZE = declare({
  id: "snooze",
  page: "notifications",
  opens: "run",
  what: "snooze every push for the chosen time",
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
const MARK_READ = declare({
  id: "mark-read",
  page: "notifications",
  opens: "run",
  row: "<notification id>",
  what: "mark one notification read",
  shows: "on an unread notification",
});
const OPEN_NOTICE = declare({
  id: "open-notice",
  page: "notifications",
  opens: "view",
  row: "<notification id>",
  what: "open (or fold) one notification's details: what to do and its fixes",
});
const NOTICE_FIX = declare({
  id: "notice-fix",
  page: "notifications",
  opens: "dialog",
  row: "<notification id>:<n>",
  what: "run a notification's suggested fix (the n-th, from 0)",
  shows: "in an opened notification that suggests a fix",
  reach: [{ do: "click", control: "open-notice", row: "*" }],
});

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
  const push = switchEl(NOTIFY_PUSH, "Push to the phone");
  drivable(push.input, PUSH_TO_PHONE);
  const digestInput = h("input", {
    class: "kp-field__input",
    type: "time",
    id: NOTIFY_DIGEST,
    "aria-describedby": "notify-digest-help",
  });
  const digestSave = h(
    "button",
    { type: "button", class: "kp-button", id: "notify-digest-save" },
    "Save",
  );
  drivable(digestSave, SAVE_DIGEST);
  const digestText_ = h("p", {
    class: "measured",
    id: "notify-digest-last",
    role: "status",
  });
  const snoozeSel = h(
    "select",
    { class: "kp-field__input", id: NOTIFY_SNOOZE },
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
  drivable(snoozeBtn, SNOOZE);
  const unsnooze = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--ghost",
      id: "notify-unsnooze",
    },
    "End the snooze",
  );
  drivable(unsnooze, END_SNOOZE);
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
  drivable(markAll, MARK_ALL_READ);
  const unreadText = h("span", { class: "measured", id: "notify-unread" });

  const notices = tableBlock({
    remember: "notices",
    caption: "Notifications, newest first",
    search: "Search notifications",
    state: "loading",
    nothing: "No notifications yet.",
    columns: [
      { label: "When", sort: "time" },
      { label: "Level", sort: "text", order: LEVEL_ORDER, filter: "choice" },
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
    h(
      "p",
      { class: "section-head__desc measured" },
      "Every notice the fleet raised, with its history, and where each kind is sent: only urgent ones reach the phone, the desktop and the lights.",
    ),
    err,
    h(
      "section",
      { class: "kp-card notify-settings", "aria-label": "Settings" },
      h("h2", null, "Settings"),
      push.wrap,
      h(
        "p",
        { class: "measured" },
        "Every notice lands in this list. Only urgent ones go to the phone at once: a service down, a failed backup, update or deploy, a disk almost full or failing. The switch turns those pushes and the digest off.",
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
            { class: "kp-field__label", for: "notify-digest" },
            "Daily digest at (Brussels time)",
          ),
          digestInput,
        ),
        digestSave,
      ),
      h(
        "p",
        {
          class: "kp-field__help inline-fields__help",
          id: "notify-digest-help",
        },
        "One push at that time, only when something waits (unread notices and open Today items, worst first), with a link to this page. Empty: no digest.",
      ),
      digestText_,
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
    h(
      "p",
      { class: "section-head__desc measured" },
      "Every notification the fleet has raised, newest first, with its own history.",
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
  // feat-ops-9 (Kenny, 2026-09-30): the "What" cell shows only the title;
  // the row expands to what/consequence/remedy/link/push reason, keyed by
  // the notice id this render painted last (noticeById).
  notices.wrap.setAttribute("data-kp-expandable", "");
  /** @type {Map<number, ReturnType<typeof noticeRows>[number]>} */
  const noticeById = new Map();
  const detach = attachDataTables(root, {
    compare: keys.compare(compare),
    detail: (row) => {
      const id = Number(row.dataset.notice);
      const r = noticeById.get(id);
      if (!r) return null;
      // Kenny, 2026-10-01: Since/Consequence/What to do used to be three
      // separate paragraphs, each its own line; a kv-grid keeps the label
      // and its value on one row, lined up with the others.
      const rows = [
        ...(r.since != null
          ? [{ label: "Since", value: formatDateTime(r.since) }]
          : []),
        ...(r.consequence
          ? [{ label: "Consequence", value: r.consequence }]
          : []),
        ...(r.remedy ? [{ label: "What to do", value: r.remedy }] : []),
      ];
      return h(
        "div",
        { class: "notice-detail" },
        ...(r.body ? [h("p", null, r.body)] : []),
        ...(rows.length
          ? [
              h(
                "dl",
                { class: "facts" },
                ...rows.flatMap((x) => [
                  h("dt", null, x.label),
                  h("dd", null, x.value),
                ]),
              ),
            ]
          : []),
        ...(r.link || r.job || r.fixes.length
          ? [
              h(
                "div",
                { class: "notice-actions" },
                ...r.fixes.map((f, i) =>
                  drivable(
                    h(
                      "button",
                      {
                        type: "button",
                        class: "kp-button kp-button--sm",
                        "data-fix": `${r.id}:${i}`,
                      },
                      fixLabel(f),
                    ),
                    NOTICE_FIX,
                    `${r.id}:${i}`,
                  ),
                ),
                ...(r.link ? [h("a", { href: r.link }, "Open the page")] : []),
                ...(r.job
                  ? [
                      " ",
                      h(
                        "a",
                        { href: `/activity?view=running&job=${r.job}` },
                        `job ${r.job}`,
                      ),
                    ]
                  : []),
              ),
            ]
          : []),
        h("p", { class: "measured" }, `Push: ${r.push}`),
      );
    },
  });
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
  digestSave.addEventListener("click", () => {
    const v = digestInput.value.trim();
    if (v !== "" && !/^\d\d:\d\d$/.test(v)) {
      digestInput.focus();
      notify("Pick a time, or leave it empty for no digest.", "warning");
      return;
    }
    void saveSettings({ digest_at: v === "" ? null : v });
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
    const fx = /** @type {Element} */ (e.target).closest("button[data-fix]");
    if (fx) {
      const el = /** @type {HTMLElement} */ (fx);
      const [id, i] = (el.dataset.fix ?? "").split(":").map(Number);
      const f = act.notices?.notices.find((n) => n.id === id)?.fixes?.[i];
      // The action's own dialog, prefilled: its review and Confirm decide.
      if (f) void openAction(f.stack, f.action, { preset: f.args ?? {} });
      return;
    }
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
    if (document.activeElement !== digestInput)
      digestInput.value = snap.settings.digest_at ?? "";
    digestText_.textContent = digestText(snap.last_digest);
    paintSnooze();
    unreadText.textContent = `${snap.unread} unread`;
    markAll.disabled = snap.unread === 0;
    const rows = noticeRows(snap.notices);
    const sig = JSON.stringify(
      rows.map((r) => [r.id, r.read, r.remedy, r.fixes.length]),
    );
    const noticesChanged = sig !== noticesSig;
    if (noticesChanged) {
      noticesSig = sig;
      noticeById.clear();
      for (const r of rows) noticeById.set(r.id, r);
      notices.tbody.replaceChildren(
        ...rows.map((r) =>
          drivable(
            h(
              "tr",
              {
                "data-notice": String(r.id),
                "data-kp-row-key": String(r.id),
                class: r.read === "unread" ? "unread" : "",
              },
              td(keys.note("time", formatDateTime(r.at), r.at), "num"),
              badgeCell(r.level),
              badgeCell(r.kind),
              td(r.stack),
              td(r.title, "notify-title"),
              td(r.push),
              r.read === "unread"
                ? h(
                    "td",
                    null,
                    drivable(
                      h(
                        "button",
                        {
                          type: "button",
                          class: "kp-button kp-button--sm",
                          "data-read": String(r.id),
                        },
                        "Mark read",
                      ),
                      MARK_READ,
                      String(r.id),
                    ),
                  )
                : h("td", null, h("span", { class: "read-mark" }, "read")),
            ),
            OPEN_NOTICE,
            String(r.id),
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
          const sw = switchEl(fieldId(MUTE, r.stack), word, false);
          sw.input.dataset.mute = r.stack;
          drivable(sw.input, STACK_PUSH, r.stack);
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
