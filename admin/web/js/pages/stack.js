// The stack hub (redesign-stackhub, release 3.71.0; Kenny approved the hub
// demo 2026-10-03: ~/.local/share/homelab/redesign-3.71/flows/stack-hub.html
// with stack.html, FLOWS.md §1.3 — implemented as drawn). Everything about
// one stack together, reached from Stacks, the Inbox, Ctrl K and every link
// that names a stack.
//
// The header keeps the same slots on every stack: the identity mark, the
// name and its state, the meta chips, and `Back up · Update · Deploy
// (primary) · More ▾` (b, u, d, `.`), More grouped Data · Change · Pause ·
// Native service · Tools · Remove. The tabs, in order of use: Overview
// (the stack's slice of Needs you, five KPI tiles, "Is it healthy?" with
// the manual checks, Recent history with who did it), Logs (a side column
// of apps and levels, one toolbar), Apps (version and a newer one, Logs /
// Update… / Publish… per row), Backups (per app: newest snapshot, the last
// 14 nights, Restore… / Verify…), History (Started by You / Claude /
// Nightly round), Settings (Secrets, Size and network, Files with the
// editor, Firewall, and a folded Danger zone). Every action of the old
// 13-button area is still here (header, More, Danger zone), and every
// control a person can click is reachable through Live view (`declare`
// with `at`, or a catalog form's `data-action`).
//
// Data: the fleet store (state, apps, restarts, env), /data/drift,
// /data/stacks/{s}/edit (address, size, firewall, images, files),
// /data/stale-images, /data/backups/{s} + /data/backup-calendar?stack=,
// /data/logs (Loki), /data/history + /data/incidents, /data/manual-checks,
// /data/charts?stack= (how full the disk is), the Inbox and the action
// catalog. What no source serves is said plainly, never invented.

import { act, catalogReady, onAct } from "../act.js";
import { openAction } from "../actiondialog.js";
import { onAnswered } from "../answer.js";
import { checkRows } from "../checks.js";
import {
  errorBox,
  fetchJson,
  fetchReport,
  h,
  slowRead,
  tabRow,
} from "../dom.js";
import { declare, drivable, viaForm } from "../drivable.js";
import { driven } from "../drivehooks.js";
import {
  checksEditTab,
  firewallTab,
  openPublishDialog,
  settingsTab,
} from "../editpanels.js";
import { formatDateTime, humanDuration } from "../format.js";
import { openIncident } from "../incident.js";
import { inboxNow, onInbox } from "../inbox.js";
import { finished } from "../jobs.js";
import { mountJobPanel } from "../jobpanel.js";
import { lineTime, logsUrl } from "../logs.js";
import { openPinRollback, openPinUpdate } from "../pinupdate.js";
import { movesOf } from "../updateflow.js";
import { openRollback } from "../rollbackdialog.js";
import { STACK_TABS, stackHref } from "../router.js";
import { incidentRows } from "../activity.js";
import {
  DANGER,
  LEVELS,
  LOG_WINDOWS,
  agoParts,
  appRows,
  attentionItems,
  backupRows,
  backupStanding,
  backupTimes,
  errorCount,
  fileList,
  filterFeed,
  headerActions,
  headerChips,
  healthChecks,
  historyFeed,
  hubKpis,
  hubState,
  levelGroup,
  logSources,
  logView,
  logWindow,
  moreGroups,
  noIncidentsText,
  restartsDay,
  shortWhen,
  sizeFacts,
  staleFor,
  WHO_CHIPS,
} from "../stackhub.js";
import { stackEntries, stackIncidents, stackChecks } from "../stacktabs.js";
import { current, subscribe } from "../store.js";
import { setParams } from "../urlstate.js";
import { attachDataTables } from "/static/kp/js/datatable.js";
import { watchTabOverflow } from "/static/kp/js/overlays.js";
import { mount as mountSecrets } from "./secrets.js";
import {
  attentionBand,
  chip,
  dot,
  emptyState,
  ensureStyle,
  keyRow,
  liveStatus,
  kpiStrip,
  moreMenu,
  section,
  segSwitch,
  skeletonLines,
  skeletonTable,
  stackMark,
  toolbar,
} from "../ui.js";

/** How far back the History tab reads. */
const HISTORY_DAYS = 30;
/** How many lines one Logs read asks Loki for. */
const LOG_LIMIT = 1000;

// ── Live view (fix-239, invariant 39): every page control of the hub,
// found from anywhere through its `at` (the stack's own tab, from the row).
/** @param {string} tab */
const at = (tab) => (/** @type {string | null} */ row) =>
  row ? stackHref(row.split("/")[0], /** @type {any} */ (tab)) : null;
/** redesign-flows-6: an update's Roll back from the stack's History. */
const UNDO_UPDATE = declare({
  id: "stack-undo-update",
  page: "stack",
  opens: "dialog",
  row: "<stack>/<app>/<service>",
  at: (row) =>
    row ? `/stacks/${encodeURIComponent(row.split("/")[0])}/history` : null,
  what: "Roll back one app an Update moved in the last 7 days: its earlier image line, backed up, committed and deployed",
  shows: "once the Update flow moved an app of this stack in the last 7 days",
});

const MORE = declare({
  id: "stack-more",
  page: "stack",
  opens: "run",
  row: "<stack>",
  at: at("overview"),
  what: "open the stack header's More menu (Data, Change, Pause, Tools, Remove)",
});
const COMPARE = declare({
  id: "stack-compare",
  page: "stack",
  opens: "run",
  row: "<stack>",
  at: at("overview"),
  what: "compare this one stack with its files now (the Matches its files tile)",
  shows: "on the Matches its files tile",
});
const LOG_FILTER = declare({
  id: "stack-log-filter",
  page: "stack",
  opens: "view",
  row: "<stack>/<apps|levels>/<value|all>",
  at: at("logs"),
  what: "turn one app or level of the Logs tab on or off (all: every one again)",
});
const LOG_WINDOW = declare({
  id: "stack-log-window",
  page: "stack",
  opens: "view",
  row: "<stack>/<seconds>",
  at: at("logs"),
  what: "read the Logs tab over 15 min, 1 h, 24 h or 7 d (900, 3600, 86400, 604800)",
});
const LOG_FOLLOW = declare({
  id: "stack-log-follow",
  page: "stack",
  opens: "view",
  row: "<stack>",
  at: at("logs"),
  what: "follow the newest log lines (every 5 s), or stop following",
});
const WHO = declare({
  id: "stack-history-who",
  page: "stack",
  opens: "view",
  row: "<stack>/<you|claude|night>",
  at: at("history"),
  what: "show only the operations one starter began; each click turns one on or off",
});
const APP_UPDATE = declare({
  id: "stack-app-update",
  page: "stack",
  // redesign-integrate-8: since redesign-flows-1 the one Update flow, an
  // address (openPinUpdate), not a dialog.
  opens: "view",
  shows: "on a stack whose pinned image has a newer release",
  row: "<stack>/<app>/<service>",
  at: at("apps"),
  what: "an app's Update… to its newer version: open the Update flow with that app picked",
});
const INCIDENT = declare({
  id: "stack-incident",
  page: "stack",
  opens: "dialog",
  row: "<stack>/<bundle>",
  at: at("history"),
  what: "open one failed operation's saved incident bundle",
});
const FOLD = declare({
  id: "stack-fold",
  page: "stack",
  opens: "view",
  row: "<stack>/<editor|firewall|danger|checks>",
  at: (row) =>
    row
      ? row.endsWith("/checks")
        ? stackHref(row.split("/")[0])
        : `${stackHref(row.split("/")[0], "settings")}`
      : null,
  what: "open or fold one of the hub's folded parts: the file editor, the firewall rules, the danger zone, the checks it is judged on",
});

// Coordinator rule (Kenny, 2026-10-03): every clickable element the hub
// draws carries a declared, stable Live view id. The rows name the stack
// first, so `at` finds the tab the control lives on from anywhere.
/** @param {string | null} row @param {number} i */
const seg = (row, i) => (row ?? "").split("/")[i] ?? "";
const HEAD = declare({
  id: "stack-head",
  page: "stack",
  opens: "dialog",
  row: "<stack>/<backup|update|deploy>",
  at: at("overview"),
  what: "the header's Back up, Update or Deploy (b, u, d): opens that action's dialog",
});
const MENU_ITEM = declare({
  id: "stack-more-item",
  page: "stack",
  opens: "dialog",
  row: "<stack>/<item>",
  at: at("overview"),
  what: "one entry of the header's More menu (restore, verify-restore, change-secret, rollback, resize, guards, disable or enable, the native ones, export, console, danger)",
  shows: "in the header's More menu",
  reach: [{ do: "click", control: "stack-more", row: "*" }],
});
const FIX = declare({
  id: "stack-fix",
  page: "stack",
  opens: "dialog",
  row: "<stack>/<problem>",
  at: at("overview"),
  what: "the fix of one row of the stack's Needs you band (deploy, backup, unpark, pin update, or a link to where it is fixed)",
});
const TAB = declare({
  id: "stack-tab",
  page: "stack",
  opens: "view",
  row: "<stack>/<overview|logs|apps|backups|history|settings>",
  at: at("overview"),
  what: "open one of the hub's six tabs",
});
const KPI = declare({
  id: "stack-kpi",
  page: "stack",
  opens: "view",
  row: "<stack>/<apps|restarts|backup|errors>",
  at: at("overview"),
  what: "open the tab a KPI tile stands for",
});
const LINK = declare({
  id: "stack-link",
  page: "stack",
  opens: "view",
  row: "<stack>/<tab>/<link>",
  at: (row) =>
    row ? stackHref(seg(row, 0), /** @type {any} */ (seg(row, 1))) : null,
  what: "a link of the hub to a related page or tab (all history, the logs' errors, an app's logs, the host log, Map, Planned, Backups, the files, the fleet firewall)",
});
const CHECK_ANSWER = declare({
  id: "stack-check-answer",
  page: "stack",
  opens: "dialog",
  row: "<stack>/<check>",
  at: at("overview"),
  what: "answer one manual check of Is it healthy?",
  shows: "while a manual check of this stack waits for an answer",
});
const LOG_SEARCH = declare({
  id: "stack-log-search",
  page: "stack",
  opens: "view",
  row: "<stack>",
  at: at("logs"),
  what: "the Logs tab's Lines containing box",
});
const LOG_RETRY = declare({
  id: "stack-log-retry",
  page: "stack",
  opens: "run",
  row: "<stack>",
  at: at("logs"),
  what: "ask Loki for the lines again after it did not answer",
  shows: "after Loki did not answer",
});
const APP_PULL = declare({
  id: "stack-app-pull",
  page: "stack",
  // redesign-integrate-8: since redesign-flows-1 Update opens the one
  // Update flow, an address (a native stack's own update: its dialog).
  opens: "view",
  row: "<stack>/<app>",
  at: at("apps"),
  what: "an unpinned app's Update…: the Update flow for its stack (pull the newest image, recreate it, with rollback)",
});
const APP_PUBLISH = declare({
  id: "stack-app-publish",
  page: "stack",
  opens: "dialog",
  row: "<stack>/<app>",
  at: at("apps"),
  what: "give one app a hostname on the gateway (and a tile)",
});
const BACKUP_CARD = declare({
  id: "stack-backups-card",
  page: "stack",
  opens: "dialog",
  row: "<stack>/<backup|restore>",
  at: at("backups"),
  what: "the Backups card's Back up now or Restore…",
});
const BACKUP_ROW = declare({
  id: "stack-backup-row",
  page: "stack",
  opens: "dialog",
  row: "<stack>/<app>/<restore|verify>",
  at: at("backups"),
  what: "one app's Restore… or Verify… on the Backups tab",
});
const HISTORY_SEARCH = declare({
  id: "stack-history-search",
  page: "stack",
  opens: "view",
  row: "<stack>",
  at: at("history"),
  what: "the History tab's Find box",
});
const OPEN_EDITOR = declare({
  id: "stack-open-editor",
  page: "stack",
  opens: "view",
  row: "<stack>/<files|size>",
  at: at("settings"),
  what: "open the file editor: Files' Open the editor…, Size and network's Edit…",
});
const DANGER_BTN = declare({
  id: "stack-danger",
  page: "stack",
  opens: "dialog",
  row: "<stack>/<destroy|forget|wipe|prune-orphans>",
  at: (row) =>
    row ? `${stackHref(seg(row, 0), "settings")}?section=danger` : null,
  what: "one destructive action of Settings ▸ Danger zone; its dialog asks for the stack's name",
});

/**
 * @typedef {{name: string, tab: import("../router.js").StackTab,
 *   navigate: (href: string) => void}} Params
 * @typedef {{s: import("../fleet.js").Stack | null, inFleet: boolean,
 *   fleetRead: boolean, drift: import("../stackhub.js").Drift | null,
 *   driftRead: boolean, edit: any | null, editError: string | null,
 *   catalog: import("../actionforms.js").Catalog | null}} Shared
 */

/**
 * @param {HTMLElement} root
 * @param {Params} params
 * @returns {() => void}
 */
export function mount(root, params) {
  // host.css: the key row and the segmented switch are the Host kit's.
  ensureStyle("/css/pages/host.css");
  ensureStyle("/css/pages/stack.css");
  root.classList.add("sh-page");
  const name = params.name;
  const abort = new AbortController();
  /** @type {(() => void)[]} */
  const stops = [];
  /** @type {Shared} */
  const S = {
    s: null,
    inFleet: false,
    fleetRead: false,
    drift: null,
    driftRead: false,
    edit: null,
    editError: null,
    catalog: null,
  };
  /** Tell the open tab the shared readings changed. */
  const bus = new EventTarget();
  const changed = () => bus.dispatchEvent(new Event("change"));

  // ── header ────────────────────────────────────────────────────────────
  /**
   * @param {string} label @param {string} title @param {string} key
   * @param {boolean} [primary]
   */
  const headButton = (label, title, key, primary = false) =>
    h(
      "button",
      {
        type: "button",
        class: `kp-button${primary ? " kp-button--primary" : ""}`,
        title: `${title} (${key})`,
        "aria-keyshortcuts": key,
      },
      label,
    );
  const backupBtn = headButton(
    "Back up",
    "A snapshot of every app's data now",
    "b",
  );
  const updateBtn = headButton(
    "Update",
    "Newest image for one app or all, with rollback",
    "u",
  );
  const deployBtn = headButton(
    "Deploy",
    "Make the stack match its files",
    "d",
    true,
  );
  /** @param {HTMLElement} b @param {string} action */
  const bindAction = (b, action) => {
    b.dataset.action = action;
    viaForm(b, action);
  };
  const head = headerActions(null);
  bindAction(backupBtn, head.backup);
  bindAction(updateBtn, head.update);
  bindAction(deployBtn, head.deploy);
  drivable(backupBtn, HEAD, `${name}/backup`);
  drivable(updateBtn, HEAD, `${name}/update`);
  drivable(deployBtn, HEAD, `${name}/deploy`);
  for (const b of [backupBtn, updateBtn, deployBtn])
    b.addEventListener(
      "click",
      () => void openAction(name, b.dataset.action ?? "", { openRollback }),
    );

  // redesign-openpoints-3: the shared more menu (ui.js), grouped.
  const menu = moreMenu({
    label: "Every other action on this stack (.)",
    button: {
      text: "More ▾",
      class: "kp-button",
      keys: ".",
      mark: (b) => drivable(b, MORE, name),
    },
    groups: [],
  });
  stops.push(menu.stop);
  const header = hubHeader({
    name,
    desc: "Everything about this one stack in one place: its health, logs, apps, backups, history and settings.",
    actions: [backupBtn, updateBtn],
    primary: deployBtn,
    more: menu.el,
  });

  /** @param {string} tab @param {string} [section] */
  const go = (tab, section) => {
    const href = `${stackHref(name, /** @type {any} */ (tab))}${section ? `?section=${section}` : ""}`;
    if (tab === params.tab && section) {
      history.replaceState(history.state, "", href);
      bus.dispatchEvent(new CustomEvent("section", { detail: section }));
      return;
    }
    params.navigate(href);
  };
  const compare = () =>
    document.dispatchEvent(new CustomEvent("stack-drift-compare"));
  const paintMenu = () => {
    const groups = moreGroups({
      stack: name,
      native: S.s?.native ?? null,
      enabled: S.s ? S.s.enabled : null,
      vmid: S.s?.vmid ?? null,
    });
    const cat = S.catalog;
    menu.fill(
      groups
        .map((g) => ({
          group: g.group,
          items: g.items.flatMap(
            /** @returns {import("../ui.js").MenuEntry[]} */ (it) => {
              /** @type {import("../ui.js").MenuEntry} */
              const base = {
                label: it.label,
                hint: it.hint,
                danger: it.danger,
                mark: (e) => drivable(e, MENU_ITEM, `${name}/${it.key}`),
              };
              switch (it.kind) {
                case "action": {
                  const entry = cat?.actions.find(
                    (a) => a.action === it.action,
                  );
                  if (cat && (!entry || entry.target !== "stack")) return [];
                  const refused =
                    entry && name === cat?.self_stack && entry.refused_for_self
                      ? "Never on the dashboard's own stack"
                      : null;
                  return [
                    {
                      ...base,
                      disabled: refused,
                      attrs: { "data-action": it.action },
                      mark: (e) =>
                        viaForm(
                          drivable(e, MENU_ITEM, `${name}/${it.key}`),
                          it.action,
                        ),
                      onClick: () =>
                        void openAction(name, it.action, { openRollback }),
                    },
                  ];
                }
                case "rollback":
                  return [
                    {
                      ...base,
                      attrs: { "data-action": "deploy-commit" },
                      mark: (e) =>
                        viaForm(
                          drivable(e, MENU_ITEM, `${name}/${it.key}`),
                          "deploy-commit",
                        ),
                      onClick: () => void openRollback(name),
                    },
                  ];
                case "go":
                  return [{ ...base, onClick: () => go(it.tab, it.section) }];
                case "href":
                  return [{ ...base, href: it.href, download: it.download }];
                default:
                  return [];
              }
            },
          ),
        }))
        .filter((g) => g.items.length > 0),
    );
  };
  paintMenu();
  void catalogReady().then((c) => {
    if (abort.signal.aborted) return;
    S.catalog = c;
    paintMenu();
  });

  // ── tabs ──────────────────────────────────────────────────────────────
  const tabs = tabRow(
    `Stack ${name}`,
    STACK_TABS.map((t) => ({
      href: stackHref(name, t.tab),
      label: t.label,
      current: t.tab === params.tab,
    })),
  );
  tabs.classList.add("sh-tabs");
  tabs
    .querySelectorAll("a.kp-tab")
    .forEach((a, i) =>
      drivable(
        /** @type {HTMLElement} */ (a),
        TAB,
        `${name}/${STACK_TABS[i].tab}`,
      ),
    );
  const appsCount = h("span", { class: "sh-count", hidden: true });
  tabs
    .querySelector(`a[href="${CSS.escape(stackHref(name, "apps"))}"]`)
    ?.append(appsCount);
  const panel = h("div", {
    class: "kp-tabs__panel sh-panel",
    role: "tabpanel",
    id: "stack-panel",
    "aria-label": STACK_TABS.find((t) => t.tab === params.tab)?.label ?? "",
  });
  const keys = keyRow([
    ["b", "back up"],
    ["u", "update"],
    ["d", "deploy"],
    [".", "more"],
    ["1–6", "tabs"],
    ["Ctrl K", "do anything"],
  ]);
  root.replaceChildren(
    header.el,
    h("div", { class: "sh-tabbar" }, tabs, keys),
    panel,
  );
  stops.push(
    watchTabOverflow(
      tabs,
      () =>
        /** @type {HTMLElement | null} */ (
          tabs.querySelector('[aria-selected="true"]')
        ) ?? undefined,
    ),
  );

  // ── shared readings ───────────────────────────────────────────────────
  const paintHead = () => {
    const f = current().fleet;
    S.fleetRead = f != null;
    S.s = f?.stacks.find((x) => x.name === name) ?? null;
    S.inFleet = S.s != null;
    const st = f ? hubState(S.s) : { label: "reading…", tone: "" };
    header.setState(st);
    header.setChips(
      headerChips({ s: S.s, manifest: S.edit?.manifest, drift: S.drift }),
    );
    if (f) header.live.set(f.measured_at);
    const acts = headerActions(S.s?.native ?? null);
    bindAction(backupBtn, acts.backup);
    bindAction(updateBtn, acts.update);
    const n = S.s?.apps_total ?? S.edit?.manifest?.apps?.length ?? null;
    appsCount.hidden = n == null;
    appsCount.textContent = n == null ? "" : String(n);
    // The tab title carries the state (DESIGN_LANGUAGE §9.4).
    if (f) {
      const tab = STACK_TABS.find((t) => t.tab === params.tab);
      document.title = `● ${name} · ${st.label} · ${params.tab === "overview" ? "Homelab" : `${tab?.label} · Homelab`}`;
    }
    paintMenu();
    changed();
  };
  stops.push(subscribe(paintHead));

  const readDrift = async (/** @type {boolean} */ fresh) => {
    const r = await fetchJson(
      `/data/drift${fresh ? "?fresh=1" : ""}`,
      "drift",
      abort.signal,
    );
    S.driftRead = true;
    if (r.ok) {
      const d = r.body.stacks?.[name] ?? null;
      S.drift = d
        ? { ...d, measured_at: d.measured_at ?? r.body.measured_at ?? null }
        : null;
    }
    paintHead();
  };
  const onCompare = () => void readDrift(true).catch(() => {});
  document.addEventListener("stack-drift-compare", onCompare);
  stops.push(() =>
    document.removeEventListener("stack-drift-compare", onCompare),
  );
  void readDrift(false).catch(() => {});

  const readEdit = async () => {
    const r = await fetchJson(
      `/data/stacks/${encodeURIComponent(name)}/edit`,
      `the files of ${name}`,
      abort.signal,
    );
    if (r.ok) {
      S.edit = r.body;
      S.editError = r.body.manifest ? null : (r.body.manifest_error ?? null);
    } else S.editError = r.error.why;
    paintHead();
  };
  void readEdit().catch(() => {});
  paintHead();

  // ── keys: b u d . (not while typing, not inside a chord or a dialog) ──
  let lastKey = { key: "", at: 0 };
  /** @param {KeyboardEvent} e */
  const onKey = (e) => {
    const prev = lastKey;
    lastKey = { key: e.key, at: Date.now() };
    if (e.ctrlKey || e.metaKey || e.altKey || e.defaultPrevented) return;
    const t = /** @type {HTMLElement | null} */ (e.target);
    if (t?.closest("input, textarea, select, [contenteditable]")) return;
    if (document.querySelector("dialog[open]")) return;
    if (prev.key === "g" && Date.now() - prev.at < 1500) return;
    /** @type {Record<string, HTMLElement>} */
    const map = { b: backupBtn, u: updateBtn, d: deployBtn };
    if (map[e.key]) {
      e.preventDefault();
      map[e.key].click();
    } else if (e.key === ".") {
      e.preventDefault();
      menu.toggle();
    }
  };
  document.addEventListener("keydown", onKey);
  stops.push(() => document.removeEventListener("keydown", onKey));

  /** @type {Record<string, (p: HTMLElement) => () => void>} */
  const panels = {
    overview: (p) => overviewTab(p, ctx),
    logs: (p) => logsTab(p, ctx),
    apps: (p) => appsTab(p, ctx),
    backups: (p) => backupsTab(p, ctx),
    history: (p) => historyTab(p, ctx),
    settings: (p) => settingsHub(p, ctx),
  };
  /** @type {Ctx} */
  const ctx = {
    name,
    params,
    S,
    signal: abort.signal,
    go,
    compare,
    /** @param {() => void} f */
    onChange: (f) => {
      bus.addEventListener("change", f);
      return () => bus.removeEventListener("change", f);
    },
    /** @param {(s: string) => void} f */
    onSection: (f) => {
      const g = (/** @type {Event} */ e) =>
        f(/** @type {CustomEvent} */ (e).detail);
      bus.addEventListener("section", g);
      return () => bus.removeEventListener("section", g);
    },
    reloadEdit: () => void readEdit().catch(() => {}),
  };
  stops.push(panels[params.tab](panel));
  return () => {
    abort.abort();
    for (const s of stops.reverse()) s();
  };
}

/**
 * What every tab gets from the hub.
 * @typedef {{name: string, params: Params, S: Shared, signal: AbortSignal,
 *   go: (tab: string, section?: string) => void, compare: () => void,
 *   onChange: (f: () => void) => () => void,
 *   onSection: (f: (s: string) => void) => () => void,
 *   reloadEdit: () => void}} Ctx
 */

/** Now, unix seconds. */
const nowS = () => Math.floor(Date.now() / 1000);

/**
 * A button that opens a catalog action's dialog (Live view: the form).
 * @param {Ctx} c
 * @param {string} action
 * @param {{label: string, title: string, cls?: string,
 *   preset?: Record<string, string>,
 *   drive: [string, string]}} o `drive`: the declared id and its row
 */
function actionButton(c, action, o) {
  const b = h(
    "button",
    {
      type: "button",
      class: `kp-button ${o.cls ?? ""}`.trim(),
      "data-action": action,
      title: o.title,
    },
    o.label,
  );
  viaForm(drivable(b, o.drive[0], o.drive[1]), action);
  b.addEventListener(
    "click",
    () =>
      void openAction(c.name, b.dataset.action ?? action, {
        openRollback,
        ...(o.preset ? { preset: o.preset } : {}),
      }),
  );
  return b;
}

/**
 * A link styled as a button, marked as the hub's `stack-link` on `row`.
 * @param {string} href @param {string} label @param {string} title
 * @param {string} row `<stack>/<tab>/<link>`
 */
const linkButton = (href, label, title, row, cls = "kp-button--sm") =>
  drivable(
    h("a", { class: `kp-button ${cls}`, href, title }, label),
    LINK,
    row,
  );
/**
 * A plain link, marked as the hub's `stack-link` on `row`.
 * @param {string} href @param {string} row @param {...import("../dom.js").Child} kids
 */
const link = (href, row, ...kids) =>
  drivable(h("a", { href }, ...kids), LINK, row);

// ─────────────────────────────────────────────────────────────── Overview

/**
 * Overview (FLOWS.md §1.3): the running jobs, this stack's slice of Needs
 * you, five KPI tiles each a link into its tab, then "Is it healthy?" and
 * Recent history with who did it; last, folded, the checks the stack is
 * judged on (checks.yml's editor: `/stacks/{s}/checks` lands here).
 * @param {HTMLElement} panel
 * @param {Ctx} c
 */
function overviewTab(panel, c) {
  const { name, S } = c;
  /** @type {(() => void)[]} */
  const stops = [];
  const running = h("div", { class: "sh-running" });
  const band = attentionBand([]);
  const strip = kpiStrip(
    hubKpis({
      s: null,
      last: null,
      night: "unknown",
      errors: undefined,
      drift: null,
      now: nowS(),
    }).map((k) => ({
      ...k,
      tone: k.tone === "ok" ? "" : k.tone,
      href: stackHref(name, /** @type {any} */ (k.tab)),
    })),
    { loading: true },
  );
  strip.el.setAttribute("aria-label", "This stack at a glance");
  for (const [key, t] of strip.tiles)
    if (t.el instanceof HTMLAnchorElement)
      drivable(t.el, KPI, `${name}/${key}`);
  // The drift tile holds a button, so it is not a link itself.
  const driftTile = strip.tiles.get("drift");
  const compareBtn = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm sh-kpi__btn",
        title:
          "Check only this one stack against its files now (every stack at once: Deploy all changes on Stacks)",
      },
      "Compare now",
    ),
    COMPARE,
    name,
  );
  compareBtn.addEventListener("click", () => {
    compareBtn.setAttribute("aria-busy", "true");
    c.compare();
  });
  // The drift tile holds the Compare button, so it is a plain tile (a
  // button inside a link is invalid): its label, value and context move
  // over, and the tile's own `set` keeps painting them.
  const plain = h("div", {
    class: "nx-kpi sh-kpi--plain",
    "data-loading": "",
  });
  if (driftTile) {
    plain.append(...driftTile.el.childNodes);
    driftTile.el.replaceWith(plain);
    // redesign-integrate-8: ui.js's tile draws its fourth row only for a
    // spark or a meter, which this tile has neither of; the button went
    // into a row that was never there, so Compare now was never drawn.
    const act = h("span", { class: "sh-kpi__act" }, compareBtn);
    const slot = plain.querySelector(".nx-kpi__spark");
    if (slot) slot.replaceWith(act);
    else plain.append(act);
  }

  const checksList = h("ul", { class: "sh-checks", "aria-live": "polite" });
  checksList.append(skeletonLines(6, "Reading the checks"));
  const checkedAt = h("span", null, "checking…");
  const healthy = section({
    id: "healthy",
    title: "Is it healthy?",
    desc: "Every check the host runs on this stack, in one place — the answer to “why is it red?” when it is.",
  });
  healthy.body.append(checksList);
  healthy.setFoot([
    checkedAt,
    link(
      `${stackHref(name, "logs")}?lvl=e`,
      `${name}/logs/errors`,
      "Recent errors in Logs",
    ),
  ]);
  healthy.el.classList.add("sh-span-6");

  const feedList = h("ul", { class: "sh-feed" });
  feedList.append(skeletonLines(4, "Reading the history"));
  const recent = section({
    title: "Recent history",
    desc: "What happened to this stack and who did it, newest first.",
    tools: [
      linkButton(
        stackHref(name, "history"),
        "All history",
        "Every operation of the last 30 days, with filters",
        `${name}/history/all-history`,
      ),
    ],
  });
  recent.body.append(feedList);
  recent.el.classList.add("sh-span-6");

  // checks.yml, folded: the editor Live view's `checks` form lands on.
  const judged = section({
    id: "stack-checks",
    title: "Checks it is judged on",
    desc: "The before/after checks, nightly probes and manual questions in checks.yml, one app at a time.",
    collapsible: true,
    open:
      new URLSearchParams(location.search).get("section") === "checks" ||
      driven(),
  });
  judged.el.classList.add("sh-span-12");
  const judgedSummary = judged.el.querySelector("summary");
  if (judgedSummary) drivable(judgedSummary, FOLD, `${name}/checks`);
  const judgedHost = h("div");
  judged.body.append(judgedHost);
  stops.push(checksEditTab(judgedHost, { name }));

  panel.replaceChildren(
    h(
      "div",
      { class: "sh-stack" },
      running,
      band.el,
      strip.el,
      h("div", { class: "sh-grid" }, healthy.el, recent.el, judged.el),
    ),
  );

  // running jobs on this stack, each with its live panel (feat-ops-6)
  /** @type {Map<number, () => void>} */
  const jobPanels = new Map();
  const paintRunning = () => {
    const live = act.jobs.filter((j) => j.stack === name && !finished(j.state));
    const keep = new Set([...live.map((j) => j.job), ...jobPanels.keys()]);
    for (const id of keep)
      if (!jobPanels.has(id)) {
        const p = mountJobPanel(id);
        jobPanels.set(id, p.stop);
        running.prepend(h("section", { class: "kp-card sh-job" }, p.element));
      }
  };
  stops.push(onAct("jobs", paintRunning));
  paintRunning();
  stops.push(() => {
    for (const s of jobPanels.values()) s();
  });

  // readings this tab adds
  /** @type {{times: number[] | null, noBackup: boolean}} */
  const bk = { times: null, noBackup: false };
  /** @type {number | null | undefined} */
  let errors;
  /** @type {number | null | undefined} */
  let diskPct;
  /** @type {any[] | null} */
  let manual = null;
  /** @type {import("../stackhub.js").Stale[]} */
  let stale = [];
  /** @type {any[] | null} */
  let history = null;
  let checkedAtS = 0;
  /** undefined while read; null without Prometheus (or a native stack).
   * @type {import("../stackhub.js").RestartsDay | null | undefined} */
  let restarts24;
  /** @type {import("../stackhub.js").WatchTarget[] | null} */
  let watch = null;

  const paint = () => {
    const now = nowS();
    const stand = backupStanding({
      times: bk.times,
      noBackup: bk.noBackup,
      now,
    });
    // A native stack's units are not containers: cAdvisor has no restart
    // series for them, so the host's own counter is what there is.
    const day = S.s?.native ? null : restarts24;
    const k = hubKpis({
      s: S.s,
      stack: name,
      last: stand.last,
      night: stand.night,
      errors,
      drift: S.drift,
      restarts24: day,
      now,
    });
    for (const t of k) {
      const tile = strip.tiles.get(t.key);
      if (!tile) continue;
      tile.set({
        label: t.label,
        value: t.value,
        unit: t.unit,
        ctx: t.ctx,
        tone: t.tone === "ok" ? "" : (t.tone ?? null),
        title: t.title,
        spark: t.spark ?? [],
        href: t.href ?? stackHref(name, /** @type {any} */ (t.tab)),
      });
      if (S.fleetRead) delete tile.el.dataset.loading;
    }
    const dt = k.find((t) => t.key === "drift");
    if (dt) {
      if (S.driftRead) delete plain.dataset.loading;
      plain.dataset.tone = dt.tone ?? "";
      plain.title = dt.title ?? "";
      compareBtn.removeAttribute("aria-busy");
    }
    const open = (manual ?? []).filter((m) => m.answer.tone !== "ok").length;
    band.set(
      attentionItems({
        stack: name,
        s: S.s,
        drift: S.drift,
        night: stand.night,
        last: stand.last,
        stale,
        openChecks: open,
        now,
        inbox: inboxNow().items.filter((i) => i.stack === name),
      })
        .filter((p) => S.fleetRead || p.key !== "not-deployed")
        .map((p) => ({
          key: p.key,
          tone: p.tone,
          title: p.title,
          text: p.text,
          action: problemAction(c, p),
        })),
    );
    if (S.fleetRead) {
      const rows = healthChecks({
        s: S.s,
        night: stand.night,
        diskPct,
        drift: S.drift,
        restarts24: day,
        watch,
        manual: manual ?? [],
      });
      checksList.replaceChildren(
        ...rows.map((r) =>
          h(
            "li",
            { "data-key": r.key },
            dot(r.tone === "unknown" ? "" : r.tone, r.text),
            r.check
              ? h(
                  "span",
                  { class: "sh-checks__act" },
                  h("span", null, r.verdict),
                  answerButton(name, r.check, () => void readChecks()),
                )
              : h("span", null, r.verdict),
          ),
        ),
      );
      checkedAt.textContent = checkedAtS
        ? `checked ${humanDuration(now - checkedAtS)} ago`
        : "checking…";
    }
    if (history) {
      const rows = historyFeed(history).slice(0, 4);
      feedList.replaceChildren(
        ...(rows.length
          ? rows.map((r) => feedRow(r))
          : [
              h(
                "li",
                { class: "sh-feed__empty" },
                `Nothing was done with ${name} in the last ${HISTORY_DAYS} days.`,
              ),
            ]),
      );
    }
  };
  stops.push(c.onChange(paint));
  stops.push(onInbox(paint));

  const readChecks = async () => {
    const r = await fetchReport(
      "/data/manual-checks",
      "the manual checks",
      c.signal,
    );
    if (r.ok) {
      const now = r.report?.now ?? nowS();
      manual = checkRows(stackChecks(r.report?.checks ?? [], name), now);
    } else manual = [];
    checkedAtS = nowS();
    paint();
  };
  stops.push(onAnswered(() => void readChecks().catch(() => {})));

  void (async () => {
    const enc = encodeURIComponent(name);
    const tasks = [
      readChecks(),
      (async () => {
        const [b, cal] = await Promise.all([
          fetchJson(`/data/backups/${enc}`, `${name}'s backups`, c.signal),
          slowRead(
            `/data/backup-calendar?stack=${enc}`,
            `${name}'s backup nights`,
            c.signal,
          ).catch(() => ({ ok: false })),
        ]);
        const calBody = cal.ok ? /** @type {any} */ (cal).body : null;
        bk.noBackup = (calBody?.no_backup ?? []).includes(name);
        bk.times =
          b.ok || calBody
            ? backupTimes({
                calendar: calBody?.stacks?.[name] ?? [],
                repos: b.ok ? (b.body?.repos ?? []) : [],
              })
            : [];
        paint();
      })(),
      (async () => {
        const r = await fetchJson(
          logsUrl(name, { since: "3600", app: "", q: "" }, LOG_LIMIT),
          "the logs",
          c.signal,
        );
        errors = r.ok ? errorCount(r.body.lines ?? []) : null;
        paint();
      })(),
      (async () => {
        // One read for the disk (its newest point) and the restarts of the
        // last 24 h (hour by hour, for the tile's sparkline).
        const r = await fetchJson(
          `/data/charts?stack=${enc}&range=24h`,
          "the charts",
          c.signal,
        );
        diskPct = r.ok ? diskOf(r.body) : null;
        restarts24 = r.ok ? restartsDay(r.body) : null;
        paint();
      })(),
      (async () => {
        const r = await fetchJson("/data/watch", "the watch", c.signal);
        watch = r.ok ? (r.body.targets ?? []) : null;
        paint();
      })(),
      (async () => {
        const r = await slowRead(
          "/data/stale-images",
          "stale images",
          c.signal,
        );
        stale = r.ok ? staleFor(name, r.body.images ?? []) : [];
        paint();
      })(),
      (async () => {
        const since = nowS() - HISTORY_DAYS * 86400;
        const r = await fetchReport(
          `/data/history?since=${since}`,
          "the history",
          c.signal,
        );
        history = r.ok ? stackEntries(r.report?.entries ?? [], name) : [];
        paint();
      })(),
    ];
    await Promise.allSettled(tasks);
  })();
  paint();
  return () => {
    for (const s of stops.reverse()) s();
  };
}

/**
 * The newest "Disk used (root filesystem)" point of `/data/charts?stack=`.
 * @param {any} body
 * @returns {number | null}
 */
function diskOf(body) {
  const p = (body?.panels ?? []).find((/** @type {any} */ x) =>
    /^disk used/i.test(x?.panel?.title ?? ""),
  );
  const pts = p?.series?.[0]?.points ?? [];
  const v = pts.at(-1)?.[1];
  return typeof v === "number" && Number.isFinite(v) ? v : null;
}

/**
 * A manual check's Answer… (the catalog form `answer-check`).
 * @param {string} stack
 * @param {string} id
 * @param {() => void} after
 */
function answerButton(stack, id, after) {
  const b = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--sm",
      "data-action": "answer-check",
      "data-check": id,
      title: "Record your answer: ok, not ok, or not ok accepted for a while",
    },
    "Answer…",
  );
  viaForm(drivable(b, CHECK_ANSWER, `${stack}/${id}`), "answer-check");
  b.addEventListener("click", async () => {
    const ctl = await openAction("_host", "answer-check", {
      preset: { check: id },
    });
    if (ctl) void ctl.closed.then(after);
  });
  return b;
}

/**
 * The fix of one "needs you" row.
 * @param {Ctx} c
 * @param {import("../stackhub.js").Problem} p
 * @returns {HTMLElement}
 */
function problemAction(c, p) {
  const a = p.act;
  switch (a.kind) {
    case "action":
      return actionButton(c, a.action, {
        label: a.label,
        title: a.title,
        preset: a.preset,
        drive: [FIX, `${c.name}/${p.key}`],
      });
    case "pin": {
      const st = a.stale;
      const b = drivable(
        h(
          "button",
          { type: "button", class: "kp-button", title: a.title },
          a.label,
        ),
        APP_UPDATE,
        `${c.name}/${st.key}`,
      );
      b.addEventListener(
        "click",
        () =>
          void openPinUpdate({
            stack: c.name,
            container: st.container,
            key: /** @type {string} */ (st.key),
            pinned: st.pinned,
            latest: st.latest,
            upstream: st.upstream,
          }),
      );
      return b;
    }
    case "go": {
      const b = h(
        "a",
        {
          class: "kp-button",
          href: `${stackHref(c.name, /** @type {any} */ (a.tab))}${a.section ? `?section=${a.section}` : ""}`,
          title: a.title,
        },
        a.label,
      );
      return drivable(b, FIX, `${c.name}/${p.key}`);
    }
    case "href":
    default:
      return drivable(
        h(
          "a",
          {
            class: "kp-button",
            href: /** @type {any} */ (a).href ?? "/inbox",
            title: a.title,
          },
          a.label,
        ),
        FIX,
        `${c.name}/${p.key}`,
      );
  }
}

/**
 * One row of a history feed: the outcome's dot, what happened with who
 * started it as a chip, its detail under it, and when.
 * @param {ReturnType<typeof historyFeed>[number]} r
 */
function feedRow(r) {
  return h(
    "li",
    { "data-who": r.who.key },
    dot(r.tone),
    h(
      "span",
      { class: "sh-feed__what" },
      h("b", null, r.what),
      chip(r.who.label, {
        tone: /** @type {any} */ (r.who.key === "claude" ? "claude" : ""),
      }),
      h("span", { class: "sh-feed__detail" }, r.detail),
    ),
    h(
      "time",
      {
        datetime: new Date(r.start * 1000).toISOString(),
        // The full moment with its year (a `now` of 0 is never this year).
        title: shortWhen(r.start, 0),
      },
      whenText(r.start),
    ),
  );
}

/** "1 h ago" for today, else "30 Sep 12:14" (the demo's). @param {number} unix */
function whenText(unix) {
  const d = nowS() - unix;
  if (d < 86400) {
    const p = agoParts(d);
    return `${p.value} ${p.unit}`;
  }
  return shortWhen(unix);
}

// ─────────────────────────────────────────────────────────────── Logs

/**
 * Logs (the hub demo): a side column of the apps (with counts) and the
 * levels, one toolbar (Lines containing at its own width, the window, the
 * count and Follow on the right), the lines, and a foot that says where
 * they came from. The host's own log of what it did to this stack lives in
 * Activity ▸ Host log, which the description says.
 * @param {HTMLElement} panel
 * @param {Ctx} c
 */
function logsTab(panel, c) {
  const { name, S } = c;
  const q0 = new URLSearchParams(location.search);
  /** @type {{since: string, q: string, follow: boolean}} */
  const st = {
    since: logWindow(q0.get("since")),
    q: q0.get("q") ?? "",
    follow: q0.get("follow") !== "0",
  };
  /** @type {import("../logs.js").LogLine[]} */
  let lines = [];
  let readAt = 0;
  let logql = "";
  /** @type {import("../doctor.js").RouteError | null} */
  let failed = null;
  let loading = true;

  const side = sideFilters({
    label: "Log filters",
    parts: [
      { key: "apps", label: "Apps", values: [] },
      {
        key: "levels",
        label: "Level",
        values: LEVELS.map((l) => ({ value: l.value, label: l.label })),
      },
    ],
    hint: "Each click turns one on or off. Esc shows everything again.",
    onChange: () => {
      only.app = null;
      only.lvl = null;
      writeUrl();
      paint();
    },
    mark: (b, part, value) =>
      drivable(b, LOG_FILTER, `${name}/${part}/${value}`),
  });
  // `?app=` (an Apps row's Logs) shows that one app; `?lvl=e` the errors —
  // until the first click in the side column.
  const only = { app: q0.get("app"), lvl: q0.get("lvl") };
  if (only.lvl) side.only("levels", only.lvl);

  const count = h(
    "span",
    { class: "sh-count-text", role: "status" },
    "reading…",
  );
  const followIn = /** @type {HTMLInputElement} */ (
    h("input", {
      type: "checkbox",
      class: "kp-switch__input",
      role: "switch",
      "aria-label": "Follow the newest lines",
    })
  );
  followIn.checked = st.follow;
  const followSw = h(
    "label",
    {
      class: "kp-switch sh-follow",
      title: "Read the newest lines every 5 s and keep the view at the bottom",
    },
    followIn,
    h("span", null, "Follow"),
  );
  drivable(followIn, LOG_FOLLOW, name);
  const win = segSwitch({
    label: "Time window",
    items: [...LOG_WINDOWS],
    value: st.since,
    onChange: (v) => {
      st.since = v;
      writeUrl();
      void load(false).catch(() => {});
    },
    mark: (b, v) => drivable(b, LOG_WINDOW, `${name}/${v}`),
  });
  /** @type {ReturnType<typeof setTimeout> | null} */
  let typing = null;
  const tb = toolbar({
    search: {
      placeholder: "Lines containing",
      value: st.q,
      onInput: (v) => {
        st.q = v.trim();
        if (typing) clearTimeout(typing);
        typing = setTimeout(() => {
          writeUrl();
          void load(false).catch(() => {});
        }, 400);
      },
    },
    groups: [
      h(
        "div",
        { class: "nx-tb__group", role: "group", "aria-label": "Window" },
        h("b", null, "Window"),
        win.el,
      ),
    ],
    state: [count, followSw],
  });
  if (tb.search) drivable(tb.search, LOG_SEARCH, name);
  const pre = h("pre", {
    class: "sh-log",
    tabindex: "0",
    "aria-label": `Log lines of ${name}`,
  });
  pre.append(skeletonLines(12, "Asking Loki for the lines"));
  const footLeft = h("span", null, "Loki");
  const footRight = h("span", null, "");
  const card = h(
    "section",
    { class: "kp-card nx-card sh-logcard", "aria-labelledby": "logs-h" },
    h(
      "div",
      { class: "nx-card__head" },
      h("h2", { id: "logs-h" }, "Logs"),
      h(
        "p",
        { class: "section-head__desc" },
        "What the apps of this stack print, live. The host's own log of what it did to this stack is in ",
        link(
          "/activity?view=host-log",
          `${name}/logs/host-log`,
          "Activity, Host log",
        ),
        ".",
      ),
    ),
    tb.el,
    pre,
    h("div", { class: "sh-foot" }, footLeft, footRight),
  );
  panel.replaceChildren(h("div", { class: "sh-side" }, side.el, card));

  const writeUrl = () => {
    const search = setParams(location.search, {
      since: st.since === "900" ? null : st.since,
      q: st.q || null,
      follow: st.follow ? null : "0",
      app: null,
      lvl: null,
    });
    history.replaceState(history.state, "", location.pathname + search);
  };

  const paint = () => {
    /** @type {string[]} */
    const apps = S.s?.apps?.map((a) => a.name) ?? S.edit?.manifest?.apps ?? [];
    const sources = logSources(apps, lines);
    side.setValues(
      "apps",
      sources.map((s) => ({ value: s, label: s })),
    );
    if (only.app) side.only("apps", only.app);
    const v = logView(lines, side.off("apps"), side.off("levels"));
    const pad = Math.max(12, ...v.shown.map((l) => (l.source || "—").length));
    side.setCounts("apps", v.sources);
    side.setCounts("levels", v.levels);
    if (failed) {
      pre.replaceChildren(
        h(
          "div",
          { class: "kp-alert kp-alert--destructive", role: "alert" },
          h("strong", null, "Loki did not answer"),
          h("p", null, `${failed.why}${failed.fix ? ` — ${failed.fix}` : ""}`),
          retryButton(),
        ),
      );
      count.textContent = "no lines";
      return;
    }
    if (loading) return;
    const atBottom =
      pre.scrollHeight - pre.scrollTop - pre.clientHeight < 24 || st.follow;
    if (v.total === 0)
      pre.replaceChildren(
        emptyState({
          title: "No lines in this window",
          text: st.q
            ? `Loki holds no line containing “${st.q}” in the last ${winLabel(st.since)}; widen the window or clear the search.`
            : `Loki holds no lines of ${name} in the last ${winLabel(st.since)}; widen the window.`,
        }),
      );
    else
      pre.replaceChildren(
        ...v.shown.map((l) => {
          const g = levelGroup(l.level);
          return h(
            "span",
            { class: "sh-log__line", "data-src": l.source, "data-lvl": g },
            h("span", { class: "sh-log__t" }, lineTime(l.ts_ms).split(" ")[1]),
            " ",
            h("span", { class: "sh-log__a" }, (l.source || "—").padEnd(pad)),
            " ",
            h(
              "span",
              { class: `sh-log__l sh-log__l--${g}` },
              g === "e" ? "error" : g === "w" ? "warn " : "info ",
            ),
            " ",
            l.line,
            "\n",
          );
        }),
      );
    if (atBottom) pre.scrollTop = pre.scrollHeight;
    count.textContent =
      v.shown.length === v.total
        ? `${v.total} ${v.total === 1 ? "line" : "lines"}`
        : `${v.shown.length} of ${v.total} lines`;
    footLeft.textContent = `Loki, read ${humanDuration(nowS() - readAt)} ago${logql ? ` · ${logql}` : ""}`;
    footRight.textContent = st.follow
      ? "Following the newest line"
      : "Paused: turn Follow on for new lines";
  };
  const retryButton = () => {
    const b = h(
      "button",
      { type: "button", class: "kp-button kp-button--sm" },
      "Try again",
    );
    drivable(b, LOG_RETRY, name);
    b.addEventListener("click", () => void load(false).catch(() => {}));
    return b;
  };
  /** @param {string} v */
  const winLabel = (v) =>
    ({ 900: "15 minutes", 3600: "hour", 86400: "24 hours", 604800: "7 days" })[
      /** @type {900} */ (Number(v))
    ] ?? "window";

  let req = new AbortController();
  c.signal.addEventListener("abort", () => req.abort());
  /** @param {boolean} quiet */
  const load = async (quiet) => {
    req.abort();
    req = new AbortController();
    if (!quiet) {
      loading = true;
      failed = null;
      pre.replaceChildren(skeletonLines(12, "Asking Loki for the lines"));
      count.textContent = "reading…";
    }
    const r = await fetchJson(
      logsUrl(name, { since: st.since, app: "", q: st.q }, LOG_LIMIT),
      "the logs",
      req.signal,
    ).catch(() => null);
    if (!r) return;
    loading = false;
    if (!r.ok) {
      failed = r.error;
      paint();
      return;
    }
    failed = null;
    lines = r.body.lines ?? [];
    logql = r.body.logql ?? "";
    readAt = r.body.measured_at ?? nowS();
    paint();
  };
  /** @type {ReturnType<typeof setInterval> | null} */
  let timer = null;
  const follow = () => {
    if (timer) clearInterval(timer);
    timer = st.follow
      ? setInterval(() => void load(true).catch(() => {}), 5000)
      : null;
  };
  followIn.addEventListener("change", () => {
    st.follow = followIn.checked;
    writeUrl();
    follow();
    paint();
  });
  /** @param {KeyboardEvent} e */
  const esc = (e) => {
    if (e.key !== "Escape" || document.querySelector("dialog[open]")) return;
    if (side.reset()) paint();
  };
  document.addEventListener("keydown", esc);
  const stopChange = c.onChange(paint);
  follow();
  void load(false).catch(() => {});
  return () => {
    if (timer) clearInterval(timer);
    if (typing) clearTimeout(typing);
    req.abort();
    stopChange();
    document.removeEventListener("keydown", esc);
  };
}

// ─────────────────────────────────────────────────────────────── Apps

/**
 * Apps (the hub demo): each app's state, the version its image is pinned
 * to and whether a newer one exists, with Logs, Update… and Publish… per
 * row. A kp datatable: sortable, Shift-click adds a key, remembered, cards
 * on a phone.
 * @param {HTMLElement} panel
 * @param {Ctx} c
 */
function appsTab(panel, c) {
  const { name, S } = c;
  /** @type {import("../stackhub.js").Stale[] | null} */
  let stale = null;
  const tbody = h("tbody");
  const table = datatable({
    remember: "stack-apps",
    columns: [
      { label: "App", sort: "text" },
      { label: "State", sort: "text" },
      { label: "Version", sort: "text" },
      { label: "Newer", sort: "text" },
      { label: "Actions", cls: "sh-right" },
    ],
    tbody,
  });
  const body = h("div", null, skeletonTable(4, 5, "Reading the apps"));
  const card = section({
    title: "Apps",
    desc: "The containers this stack runs, the version each is on, and whether a newer one exists.",
  });
  card.body.append(body);
  card.setFoot([
    "Versions from the stack's files; newer ones from the fleet check's last run.",
    link("/map", `${name}/apps/map`, "Every stale image in Map"),
  ]);
  panel.replaceChildren(card.el);
  let attached = false;
  /** @type {() => void} */
  let detach = () => {};

  const paint = () => {
    if (!S.fleetRead) return;
    const rows = appRows({
      s: S.s,
      manifestApps: S.edit?.manifest?.apps ?? [],
      images: S.edit?.images ?? null,
      stale: stale ?? [],
    });
    if (rows.length === 0) {
      detach();
      attached = false;
      body.replaceChildren(
        emptyState({
          title: "This stack declares no apps",
          text: "An app is added in Settings ▸ Files (Add an app), from a preset or by hand.",
          action: linkButton(
            `${stackHref(name, "settings")}?section=files`,
            "Open the files",
            "Settings ▸ Files",
            `${name}/settings/files`,
            "",
          ),
        }),
      );
      return;
    }
    tbody.replaceChildren(
      ...rows.map((r) => {
        const logs = linkButton(
          `${stackHref(name, "logs")}?app=${encodeURIComponent(r.name)}`,
          "Logs",
          `${r.name}'s log lines`,
          `${name}/logs/app-${r.name}`,
        );
        /** @type {HTMLElement[]} */
        const acts = [logs];
        if (r.newer?.key) {
          const st = r.newer;
          const up = drivable(
            h(
              "button",
              {
                type: "button",
                class: "kp-button kp-button--sm kp-button--primary",
                title: `Back up ${name}, move ${st.container} from ${st.pinned} to ${st.latest}, commit and deploy`,
              },
              "Update…",
            ),
            APP_UPDATE,
            `${name}/${st.key}`,
          );
          up.addEventListener(
            "click",
            () =>
              void openPinUpdate({
                stack: name,
                container: st.container,
                key: /** @type {string} */ (st.key),
                pinned: st.pinned,
                latest: st.latest,
                upstream: st.upstream,
              }),
          );
          acts.push(up);
        } else if (!r.extra && S.inFleet) {
          acts.push(
            actionButton(c, S.s?.native ? "update-native" : "update", {
              label: "Update…",
              title: `Pull the newest image for ${r.name} and recreate it, with rollback`,
              cls: "kp-button--sm",
              preset: S.s?.native ? undefined : { app: r.name },
              drive: [APP_PULL, `${name}/${r.name}`],
            }),
          );
        }
        if (!r.extra) {
          // feat-publish-1: a hostname and a port for this one app.
          const pub = viaForm(
            h(
              "button",
              {
                type: "button",
                class: "kp-button kp-button--sm kp-button--ghost",
                title: `Give ${r.name} a hostname on the gateway (and a tile)`,
              },
              "Publish…",
            ),
            "publish",
          );
          drivable(pub, APP_PUBLISH, `${name}/${r.name}`);
          pub.addEventListener("click", () =>
            openPublishDialog(name, r.name, () => c.reloadEdit()),
          );
          acts.push(pub);
        }
        return h(
          "tr",
          { "data-kp-row-key": r.name },
          h("td", { class: "id" }, r.name),
          h(
            "td",
            { "data-label": "State", "data-sort": r.state ?? "" },
            r.state
              ? dot(r.state === "running" ? "ok" : "bad", r.state)
              : h(
                  "span",
                  { class: "sh-muted" },
                  r.extra ? "a sidecar image" : "not deployed",
                ),
          ),
          h(
            "td",
            { "data-label": "Version", class: "mono", title: r.image || null },
            r.version || h("span", { class: "sh-muted" }, "—"),
          ),
          h(
            "td",
            { "data-label": "Newer", "data-sort": r.newer?.latest ?? "" },
            r.newer
              ? [
                  chip(r.newer.latest, { tone: "info" }),
                  r.newer.major
                    ? chip("major", {
                        tone: "warn",
                        title:
                          "The first number changes: read the release notes first",
                      })
                    : null,
                ]
              : h(
                  "span",
                  { class: "sh-muted" },
                  stale == null ? "checking…" : "up to date",
                ),
          ),
          h("td", { class: "row-actions" }, acts),
        );
      }),
    );
    if (!attached) {
      body.replaceChildren(table);
      detach = attachDataTables(body);
      attached = true;
    }
  };
  const stop = c.onChange(paint);
  void (async () => {
    const r = await slowRead("/data/stale-images", "stale images", c.signal);
    stale = r.ok ? staleFor(name, r.body.images ?? []) : [];
    paint();
  })().catch(() => {});
  paint();
  return () => {
    stop();
    detach();
  };
}

/**
 * A kp datatable without a search bar (a short list needs none):
 * sortable, Shift-click adds a sort key, remembered per table, cards on a
 * phone.
 * @param {{remember: string, columns: {label: string, sort?: string,
 *   cls?: string}[], tbody: HTMLElement}} spec
 */
function datatable(spec) {
  return h(
    "div",
    {
      class: "kp-datatable sh-table",
      "data-kp-datatable": "",
      "data-kp-sort-multi": "",
      "data-kp-cards": "",
      "data-kp-remember": spec.remember,
      "data-kp-page-sizes": "none",
    },
    h(
      "div",
      { class: "kp-table-wrap" },
      h(
        "table",
        { class: "kp-table" },
        h(
          "thead",
          null,
          h(
            "tr",
            null,
            spec.columns.map((col, i) =>
              h(
                "th",
                {
                  "data-kp-sort": col.sort ?? null,
                  class: col.cls ?? null,
                  // review 3: the table opens sorted by its first column, so
                  // the phone's card sort reads "Sort by App · Ascending"
                  // rather than "Sort by None" beside a dead Ascending.
                  "aria-sort": i === 0 && col.sort ? "ascending" : null,
                },
                col.label,
              ),
            ),
          ),
        ),
        spec.tbody,
      ),
    ),
  );
}

// ─────────────────────────────────────────────────────────────── Backups

/**
 * Backups (the hub demo): this stack's repositories, one per app — the
 * newest snapshot, the last 14 nights, how many are kept — with Back up
 * now and Restore… on the card and Restore… / Verify… per row.
 * @param {HTMLElement} panel
 * @param {Ctx} c
 */
function backupsTab(panel, c) {
  const { name, S } = c;
  const body = h("div", null, skeletonTable(4, 5, "Reading the backups"));
  const native = () => S.s?.native === true;
  const backupNow = actionButton(c, "backup", {
    label: "Back up now",
    title: "A snapshot of every app's data now",
    drive: [BACKUP_CARD, `${name}/backup`],
  });
  const restore = actionButton(c, "restore", {
    label: "Restore…",
    title: "Bring one app's data back to a chosen night (a safety copy first)",
    cls: "kp-button--primary",
    drive: [BACKUP_CARD, `${name}/restore`],
  });
  const card = section({
    title: "Backups of this stack",
    desc: "One snapshot per app every night, kept as its retention says. Restore brings one app's data back to a chosen night.",
    tools: [backupNow, restore],
  });
  card.body.append(body);
  card.setFoot([
    h(
      "span",
      null,
      "The schedule lives in ",
      link(
        "/activity?view=planned",
        `${name}/backups/planned`,
        "Activity ▸ Planned",
      ),
      "; every stack's backups in ",
      link(
        `/backups?stack=${encodeURIComponent(name)}`,
        `${name}/backups/backups-page`,
        "Backups",
      ),
      ".",
    ),
  ]);
  panel.replaceChildren(card.el);
  /** @type {any} */
  let read = null;
  /** @type {import("../doctor.js").RouteError | null} */
  let failed = null;
  let noBackup = false;
  /** @type {() => void} */
  let detach = () => {};

  const paint = () => {
    const nat = native() || read?.native === true;
    for (const [b, a] of /** @type {[HTMLElement, string][]} */ ([
      [backupNow, nat ? "backup-native" : "backup"],
      [restore, nat ? "restore-native" : "restore"],
    ])) {
      b.dataset.action = a;
      b.dataset.driveForm = a;
    }
    if (failed) {
      detach();
      body.replaceChildren(
        h(
          "div",
          { class: "kp-alert kp-alert--destructive", role: "alert" },
          h("strong", null, "The backups did not read"),
          h("p", null, `${failed.why}${failed.fix ? ` — ${failed.fix}` : ""}`),
        ),
      );
      return;
    }
    if (!read) return;
    if (noBackup || (read.repos ?? []).length === 0) {
      detach();
      body.replaceChildren(
        emptyState({
          title: noBackup
            ? `${name} keeps no data to back up`
            : `${name} has no backup yet`,
          text: noBackup
            ? "It declares no app data, by design; there is nothing to snapshot."
            : "The first snapshot is written tonight at the nightly round, or now with Back up now.",
        }),
      );
      return;
    }
    const rows = backupRows({ repos: read.repos ?? [], now: nowS() });
    const tbody = h(
      "tbody",
      null,
      rows.map((r) =>
        h(
          "tr",
          { "data-kp-row-key": r.app },
          h("td", { class: "id" }, r.app),
          h(
            "td",
            {
              "data-label": "Newest",
              "data-sort": String(r.newest?.time ?? 0),
            },
            r.newest
              ? [
                  h("span", { class: "mono" }, r.newest.short_id || "—"),
                  ` · ${whenText(r.newest.time)}`,
                ]
              : h("span", { class: "sh-muted" }, r.error ?? "none yet"),
          ),
          h(
            "td",
            { "data-label": "14 nights", "data-sort": String(r.missed) },
            nightStrip(r.cells),
          ),
          h("td", { class: "num", "data-label": "Kept" }, String(r.kept)),
          h(
            "td",
            { class: "row-actions" },
            actionButton(c, nat ? "restore-native" : "restore", {
              label: "Restore…",
              title: `Bring ${r.app}'s data back from a snapshot`,
              cls: "kp-button--sm",
              preset: nat
                ? r.newest?.short_id
                  ? { snapshot: r.newest.short_id }
                  : undefined
                : {
                    app: r.app,
                    ...(r.newest?.short_id
                      ? { snapshot: r.newest.short_id }
                      : {}),
                  },
              drive: [BACKUP_ROW, `${name}/${r.app}/restore`],
            }),
            nat
              ? null
              : actionButton(c, "verify-restore", {
                  label: "Verify…",
                  title: `Prove ${r.app}'s newest snapshot restores, without touching live data`,
                  cls: "kp-button--sm kp-button--ghost",
                  preset: { app: r.app },
                  drive: [BACKUP_ROW, `${name}/${r.app}/verify`],
                }),
          ),
        ),
      ),
    );
    detach();
    body.replaceChildren(
      datatable({
        remember: "stack-backups",
        columns: [
          { label: "App", sort: "text" },
          { label: "Newest snapshot", sort: "number" },
          { label: "Last 14 nights", sort: "number" },
          { label: "Kept", sort: "number", cls: "num" },
          { label: "Actions", cls: "sh-right" },
        ],
        tbody,
      }),
      h(
        "p",
        { class: "sh-legend" },
        h("i", { class: "sh-strip__ok" }),
        "snapshot that night",
        h("i", { class: "sh-strip__miss" }),
        "missed",
        h("i", { class: "sh-strip__before" }),
        "before its oldest kept snapshot",
      ),
    );
    detach = attachDataTables(body);
  };
  const stop = c.onChange(paint);
  void (async () => {
    const enc = encodeURIComponent(name);
    const [b, cal] = await Promise.all([
      fetchJson(`/data/backups/${enc}`, `${name}'s backups`, c.signal),
      slowRead(
        `/data/backup-calendar?stack=${enc}`,
        `${name}'s backup nights`,
        c.signal,
      ).catch(() => ({ ok: false })),
    ]);
    if (b.ok) read = b.body;
    else failed = b.error;
    /** @type {any} */
    const calAny = cal;
    noBackup = cal.ok ? (calAny.body?.no_backup ?? []).includes(name) : false;
    paint();
  })().catch(() => {});
  return () => {
    stop();
    detach();
  };
}

// ─────────────────────────────────────────────────────────────── History

/**
 * History (the hub demo): every operation on this stack over 30 days, who
 * started it (You / Claude / Nightly round, plain-click chips) and how it
 * ended, with a search; then its failed operations' incident bundles.
 * @param {HTMLElement} panel
 * @param {Ctx} c
 */
function historyTab(panel, c) {
  const { name } = c;
  const q0 = new URLSearchParams(location.search);
  /** @type {Set<string>} */
  const who = new Set((q0.get("by") ?? "").split(",").filter(Boolean));
  let q = q0.get("q") ?? "";
  /** @type {ReturnType<typeof historyFeed> | null} */
  let rows = null;
  /** @type {import("../doctor.js").RouteError | null} */
  let failed = null;
  const count = h(
    "span",
    { class: "sh-count-text", role: "status" },
    "reading…",
  );
  const chips = WHO_CHIPS.map((ch) => {
    const b = drivable(
      h(
        "button",
        {
          type: "button",
          class: "nx-chip-toggle",
          "aria-pressed": String(who.has(ch.value)),
          "data-value": ch.value,
          title: `Show only what ${ch.label} started; each click turns it on or off`,
        },
        ch.label,
      ),
      WHO,
      `${name}/${ch.value}`,
    );
    b.addEventListener("click", () => {
      if (who.has(ch.value)) who.delete(ch.value);
      else who.add(ch.value);
      b.setAttribute("aria-pressed", String(who.has(ch.value)));
      writeUrl();
      paint();
    });
    return b;
  });
  const tb = toolbar({
    search: {
      placeholder: "Find: action, app, error",
      label: "Find in history",
      value: q,
      onInput: (v) => {
        q = v;
        writeUrl();
        paint();
      },
    },
    groups: [
      h(
        "div",
        {
          class: "nx-tb__group",
          role: "group",
          "aria-label": "Started by, click to show or hide",
        },
        h("b", null, "Started by"),
        h("span", { class: "nx-chips" }, chips),
      ),
    ],
    state: [count],
  });
  if (tb.search) drivable(tb.search, HISTORY_SEARCH, name);
  const list = h("ul", { class: "sh-feed sh-feed--full" });
  list.append(skeletonLines(6, "Reading the history"));
  const card = h(
    "section",
    { class: "kp-card nx-card", "aria-labelledby": "hist-h" },
    h(
      "div",
      { class: "nx-card__head" },
      h("h2", { id: "hist-h" }, "History"),
      h(
        "p",
        { class: "section-head__desc" },
        "Every operation on this stack, who started it, and how it ended.",
      ),
    ),
    tb.el,
    list,
  );
  const incList = h("ul", { class: "sh-feed" });
  incList.append(skeletonLines(2, "Reading the incidents"));
  const incidents = section({
    title: "Incidents",
    desc: "Each failed operation on this stack kept a bundle of what it saw; Show opens it.",
  });
  incidents.body.append(incList);
  // redesign-flows-6: the updates of the last 7 days, each with its Roll
  // back (the demo's "1 click, for 7 days, from the stack's History").
  const updatesAbort = new AbortController();
  const updates = updatesCard(c.name, updatesAbort.signal);
  panel.replaceChildren(
    h("div", { class: "sh-stack" }, card, updates, incidents.el),
  );

  const writeUrl = () => {
    const search = setParams(location.search, {
      by: who.size ? [...who].join(",") : null,
      q: q || null,
    });
    history.replaceState(history.state, "", location.pathname + search);
  };
  const paint = () => {
    if (failed) {
      list.replaceChildren(
        h(
          "li",
          { class: "kp-alert kp-alert--destructive", role: "alert" },
          h("strong", null, "The history did not read"),
          h(
            "span",
            null,
            `${failed.why}${failed.fix ? ` — ${failed.fix}` : ""}`,
          ),
        ),
      );
      count.textContent = "";
      return;
    }
    if (!rows) return;
    const shown = filterFeed(rows, who, q);
    count.textContent = `${shown.length === rows.length ? rows.length : `${shown.length} of ${rows.length}`} ${rows.length === 1 ? "operation" : "operations"}, ${HISTORY_DAYS} days`;
    list.replaceChildren(
      ...(shown.length
        ? shown.map((r) => feedRow(r))
        : [
            h(
              "li",
              { class: "sh-feed__empty" },
              rows.length
                ? "Nothing matches: clear the search or turn a chip off."
                : `Nothing was done with ${name} in the last ${HISTORY_DAYS} days.`,
            ),
          ]),
    );
  };
  /** @param {KeyboardEvent} e */
  const esc = (e) => {
    if (e.key !== "Escape" || document.querySelector("dialog[open]")) return;
    if (!who.size) return;
    who.clear();
    for (const b of chips) b.setAttribute("aria-pressed", "false");
    writeUrl();
    paint();
  };
  document.addEventListener("keydown", esc);
  const load = async () => {
    const since = nowS() - HISTORY_DAYS * 86400;
    const [hr, ir] = await Promise.all([
      fetchReport(`/data/history?since=${since}`, "the history", c.signal),
      fetchReport("/data/incidents", "the incidents", c.signal),
    ]);
    if (hr.ok) {
      failed = null;
      rows = historyFeed(stackEntries(hr.report?.entries ?? [], name));
    } else failed = hr.error;
    paint();
    if (ir.ok) {
      const names = incidentRows(
        stackIncidents(ir.report?.incidents ?? [], name),
      );
      incList.replaceChildren(
        ...(names.length
          ? names.map((x) => {
              const b = drivable(
                h(
                  "button",
                  {
                    type: "button",
                    class: "kp-button kp-button--sm kp-button--ghost",
                    title: "Open what this operation saw when it failed",
                  },
                  "Show",
                ),
                INCIDENT,
                `${name}/${x.name}`,
              );
              b.addEventListener("click", () => void openIncident(x.name));
              return h(
                "li",
                null,
                dot("bad"),
                h(
                  "span",
                  { class: "sh-feed__what" },
                  h("b", null, x.op),
                  h("span", { class: "sh-feed__detail mono" }, x.name),
                ),
                h(
                  "span",
                  { class: "sh-feed__end" },
                  x.at ? whenText(x.at) : "",
                  b,
                ),
              );
            })
          : [h("li", { class: "sh-feed__empty" }, noIncidentsText(name))]),
      );
    } else
      incList.replaceChildren(
        h(
          "li",
          { class: "kp-alert kp-alert--destructive", role: "alert" },
          `The incidents did not read: ${ir.error.why}`,
        ),
      );
  };
  const reload = () => void load().catch(() => {});
  const stopAnswers = onAnswered(reload);
  reload();
  return () => {
    stopAnswers();
    updatesAbort.abort();
    document.removeEventListener("keydown", esc);
  };
}

// ─────────────────────────────────────────────────────────────── Settings

/**
 * Settings (the hub demo): Secrets and Size and network side by side, the
 * Files with the editor, the Firewall (its summary always, its rules
 * folded), and the Danger zone folded last. `?section=secrets|firewall|
 * files|danger` opens and scrolls to that part; Live view's edit forms
 * (settings, raw, add-app, firewall …) open theirs by themselves.
 * @param {HTMLElement} panel
 * @param {Ctx} c
 */
function settingsHub(panel, c) {
  const { name, S } = c;
  /** @type {(() => void)[]} */
  const stops = [];
  const want = new URLSearchParams(location.search).get("section");
  const drive = driven();

  const secrets = section({
    id: "secrets",
    title: "Secrets",
    desc: "Passwords and tokens this stack's apps read, sealed with latch. Hidden until you press Reveal; hidden again when you leave.",
  });
  // The secrets pane draws its list and its change drawer side by side
  // (secrets.js, `sx-md--one`): it needs the whole width.
  secrets.el.classList.add("sh-span-12");
  stops.push(mountSecrets(secrets.body, { stack: name }));

  const sizeKv = h("dl", { class: "sh-kv" });
  sizeKv.append(skeletonLines(5, "Reading the stack's files"));
  const editBtn = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm",
        title:
          "Open the editor at the container's settings (a change is saved as a commit and needs a deploy)",
      },
      "Edit…",
    ),
    OPEN_EDITOR,
    `${name}/size`,
  );
  const size = section({
    id: "size",
    title: "Size and network",
    desc: "What the container gets from the host. Changes are saved as a commit and need a deploy.",
    tools: [editBtn],
  });
  size.body.append(sizeKv);
  size.el.classList.add("sh-span-6");

  const fileUl = h("ul", { class: "sh-feed sh-files" });
  fileUl.append(skeletonLines(3, "Reading the stack's files"));
  const editorHost = h("div", { class: "sh-editor" });
  const editor = /** @type {HTMLDetailsElement} */ (
    h(
      "details",
      { class: "sh-fold", open: want === "files" || drive ? true : null },
      h(
        "summary",
        {
          title:
            "The settings form, the raw files, Add an app, storage, latch and tiles",
        },
        "The editor",
      ),
      editorHost,
    )
  );
  const editorSummary = /** @type {HTMLElement} */ (
    editor.querySelector("summary")
  );
  drivable(editorSummary, FOLD, `${name}/editor`);
  const openEditor = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--sm",
      title: "Open the editor: edit, review the change, commit",
    },
    "Open the editor…",
  );
  drivable(openEditor, OPEN_EDITOR, `${name}/files`);
  const files = section({
    id: "files",
    title: "Files",
    desc: "The stack's files in the repository — the truth a deploy makes real. Edit, review the change, commit.",
    tools: [openEditor],
  });
  files.body.append(fileUl, editor);
  files.el.classList.add("sh-span-12");
  // Mounted at once (folded or not): Live view's edit forms reach their
  // fields through the handle the editor registers when it has read.
  stops.push(settingsTab(editorHost, { name }));
  const showEditor = () => {
    editor.open = true;
    queueMicrotask(() =>
      (editorHost.querySelector("#settings") ?? editor).scrollIntoView({
        block: "start",
      }),
    );
  };
  openEditor.addEventListener("click", showEditor);
  editBtn.addEventListener("click", showEditor);

  const fwSummary = h("p", { class: "sh-fw-summary" }, "Reading the firewall…");
  const fwHost = h("div");
  const fwFold = /** @type {HTMLDetailsElement} */ (
    h(
      "details",
      { class: "sh-fold", open: want === "firewall" || drive ? true : null },
      h(
        "summary",
        { title: "Every rule, with Add, Edit and Remove" },
        "Its rules",
      ),
      fwHost,
    )
  );
  drivable(
    /** @type {HTMLElement} */ (fwFold.querySelector("summary")),
    FOLD,
    `${name}/firewall`,
  );
  const firewall = section({
    id: "firewall",
    title: "Firewall",
    desc: "What this stack's firewall lets through, and whether the host enforces it; every stack at once is in System ▸ Firewall.",
    tools: [
      linkButton(
        "/firewall",
        "All stacks",
        "The fleet-wide firewall table",
        `${name}/settings/fleet-firewall`,
      ),
    ],
  });
  firewall.body.append(fwSummary, fwFold);
  firewall.el.classList.add("sh-span-6");
  stops.push(firewallTab(fwHost, { name }));

  const danger = /** @type {HTMLDetailsElement} */ (
    h(
      "details",
      {
        class: "kp-card nx-card nx-card--fold sh-danger sh-span-12",
        id: "danger",
        open: want === "danger" ? true : null,
      },
      h(
        "summary",
        null,
        h(
          "div",
          { class: "nx-card__head" },
          h("h2", null, "Danger zone"),
          h(
            "p",
            { class: "section-head__desc" },
            "Destroy, forget, wipe and prune: each asks you to type the stack's name before it runs.",
          ),
        ),
      ),
      h(
        "div",
        { class: "sh-actions actions-area" },
        DANGER.map((d) => {
          const b = h(
            "button",
            {
              type: "button",
              class: "sh-action sh-action--danger",
              "data-action": d.action,
              title: d.hint,
            },
            h("strong", null, d.label),
            h("span", null, d.hint),
          );
          viaForm(drivable(b, DANGER_BTN, `${name}/${d.action}`), d.action);
          b.addEventListener(
            "click",
            () => void openAction(name, d.action, { openRollback }),
          );
          return b;
        }),
      ),
    )
  );
  drivable(
    /** @type {HTMLElement} */ (danger.querySelector("summary")),
    FOLD,
    `${name}/danger`,
  );

  panel.replaceChildren(
    h(
      "div",
      { class: "sh-grid" },
      secrets.el,
      size.el,
      firewall.el,
      files.el,
      danger,
    ),
  );

  const paint = () => {
    const e = S.edit;
    if (!e && !S.editError) return;
    if (!e?.manifest) {
      sizeKv.replaceChildren(
        h(
          "p",
          { class: "sh-muted" },
          `The stack's files do not read: ${S.editError ?? "no lxc-compose.yml"}.`,
        ),
      );
    } else
      sizeKv.replaceChildren(
        ...sizeFacts({ manifest: e.manifest, diskPct }).flatMap((f) => [
          h("dt", null, f.label),
          h("dd", { class: f.mono ? "mono" : null }, f.value),
        ]),
      );
    const paths = Object.keys(e?.texts ?? {});
    fileUl.replaceChildren(
      ...(paths.length
        ? fileList(paths).map((f) =>
            h(
              "li",
              null,
              h(
                "span",
                { class: "sh-files__icon", "aria-hidden": "true" },
                "▤",
              ),
              h(
                "span",
                { class: "sh-feed__what" },
                h("b", { class: "mono" }, f.path),
                f.what ? h("span", { class: "sh-feed__detail" }, f.what) : null,
              ),
              h("span", null),
            ),
          )
        : [
            h(
              "li",
              { class: "sh-feed__empty" },
              e?.head
                ? `The repository holds no files for ${name}.`
                : "The working copy has no commit yet.",
            ),
          ]),
    );
    const fw = e?.manifest?.firewall;
    const rules = Array.isArray(fw?.rules) ? fw.rules.length : 0;
    fwSummary.replaceChildren(
      ...(e?.manifest
        ? [
            dot(fw?.enabled ? "ok" : fw ? "warn" : ""),
            fw
              ? fw.enabled
                ? `On: ${rules} ${rules === 1 ? "rule" : "rules"}; incoming ${String(fw.policy_in ?? "DROP").toLowerCase()} unless a rule allows it.`
                : `Off: ${rules} ${rules === 1 ? "rule is" : "rules are"} declared but not enforced.`
              : "No firewall is declared for this stack.",
          ]
        : ["The firewall is read from the stack's files, which do not read."]),
    );
  };
  /** @type {number | null} */
  let diskPct = null;
  void (async () => {
    const r = await fetchJson(
      `/data/charts?stack=${encodeURIComponent(name)}&range=1h`,
      "the charts",
      c.signal,
    );
    diskPct = r.ok ? diskOf(r.body) : null;
    paint();
  })().catch(() => {});
  stops.push(c.onChange(paint));
  paint();

  /** @param {string} s */
  const open = (s) => {
    /** @type {Record<string, [HTMLElement, HTMLDetailsElement | null]>} */
    const parts = {
      secrets: [secrets.el, null],
      size: [size.el, null],
      files: [files.el, editor],
      firewall: [firewall.el, fwFold],
      danger: [danger, danger],
    };
    const p = parts[s];
    if (!p) return;
    if (p[1]) p[1].open = true;
    queueMicrotask(() => p[0].scrollIntoView({ block: "start" }));
  };
  if (want) open(want);
  stops.push(c.onSection(open));
  return () => {
    for (const s of stops.reverse()) s();
  };
}

// ── the hub's own blocks (stackkit.js until the 3.71.0 integration; ui.js
// has no hub header, grouped action menu, log side filters or night strip)

/**
 * The hub's header (flows/stack-hub.html): the stack's identity mark, its
 * name and state on the title row with the live status and the actions
 * against the right edge (Back up · Update · Deploy, the primary one, then
 * More), the one-sentence description under it, and the meta chips.
 * Markup the whole-screen invariants read: `.title-row` holding the h1,
 * the description its next sibling `p`, the buttons in `.actions-row`.
 * @param {{name: string, desc: string, actions: Node[], primary: Node,
 *   more: Node}} spec
 */
function hubHeader(spec) {
  const title = /** @type {HTMLHeadingElement} */ (h("h1", null, spec.name));
  const state = h("span", { class: "sh-head__state", id: "stack-state" });
  const live = liveStatus("updated");
  const actions = h(
    "div",
    { class: "actions-row nx-head-actions sh-head__actions" },
    spec.actions,
    spec.primary,
    spec.more,
  );
  const right = h(
    "div",
    { class: "nx-head-right sh-head__right" },
    live.el,
    actions,
  );
  const mark = stackMark(spec.name, 40);
  mark.classList.add("sh-head__mark");
  const row = h(
    "div",
    { class: "title-row sh-head__row" },
    mark,
    title,
    state,
    right,
  );
  const desc = /** @type {HTMLParagraphElement} */ (
    h("p", { class: "section-head__desc nx-head-desc" }, spec.desc)
  );
  const meta = h("div", { class: "sh-head__meta", id: "stack-flags" });
  const e = h("header", { class: "nx-head sh-head" }, row, desc, meta);
  return {
    el: e,
    title,
    live,
    /** @param {{label: string, tone: string}} s */
    setState: (s) =>
      state.replaceChildren(dot(/** @type {any} */ (s.tone), s.label)),
    /**
     * @param {{label: string, tone?: string | null, mono?: boolean,
     *   title?: string}[]} list
     */
    setChips: (list) =>
      meta.replaceChildren(
        ...list.map((c) => {
          const e = chip(c.label, {
            tone: /** @type {any} */ (c.tone ?? ""),
            title: c.title,
          });
          if (c.mono) e.classList.add("mono");
          return e;
        }),
      ),
  };
}

/**
 * The side filter column of a log explorer (DESIGN_LANGUAGE §12, the hub
 * demo's Logs): one part per dimension, each value a toggle row with its
 * count, every value on at first. A plain click turns one on or off (no
 * modifier keys, Kenny 2026-10-03); "All" turns a dimension's values back
 * on; `reset()` (Esc) all of them.
 * @param {{label: string, parts: {key: string, label: string,
 *   values: {value: string, label: string}[]}[], hint: string,
 *   onChange: () => void,
 *   mark?: (b: HTMLElement, part: string, value: string) => void}} spec
 */
function sideFilters(spec) {
  /** @type {Map<string, Set<string>>} values turned OFF, per part */
  const off = new Map(spec.parts.map((p) => [p.key, new Set()]));
  /** @type {Map<string, HTMLElement>} */
  const holders = new Map();
  /** @type {Map<string, {value: string, label: string}[]>} */
  const values = new Map(spec.parts.map((p) => [p.key, p.values]));
  /** @type {Map<string, Map<string, HTMLElement>>} */
  const counts = new Map();
  /** @param {string} key */
  const paint = (key) => {
    const holder = holders.get(key);
    const o = off.get(key) ?? new Set();
    if (!holder) return;
    /** @type {Map<string, HTMLElement>} */
    const cs = new Map();
    holder.replaceChildren(
      ...(values.get(key) ?? []).map((v) => {
        const n = h("small", null);
        cs.set(v.value, n);
        const b = h(
          "button",
          {
            type: "button",
            "aria-pressed": String(!o.has(v.value)),
            "data-value": v.value,
            title: `Show or hide ${v.label}; each click turns it on or off`,
            onclick: () => {
              if (o.has(v.value)) o.delete(v.value);
              else o.add(v.value);
              b.setAttribute("aria-pressed", String(!o.has(v.value)));
              spec.onChange();
            },
          },
          h("span", null, v.label),
          n,
        );
        spec.mark?.(b, key, v.value);
        return b;
      }),
    );
    counts.set(key, cs);
  };
  const parts = spec.parts.map((p) => {
    const holder = h("div", {
      class: "sh-side__opts",
      role: "group",
      "aria-label": `${p.label}: click to show or hide`,
    });
    holders.set(p.key, holder);
    const all = h(
      "button",
      {
        type: "button",
        class: "sh-side__all",
        title: `Show every ${p.label.toLowerCase()} again`,
        onclick: () => {
          off.get(p.key)?.clear();
          paint(p.key);
          spec.onChange();
        },
      },
      "All",
    );
    spec.mark?.(all, p.key, "all");
    paint(p.key);
    return h(
      "div",
      { class: "sh-side__part" },
      h("p", { class: "sh-side__label" }, h("span", null, p.label), all),
      holder,
    );
  });
  const e = h(
    "aside",
    { class: "sh-side__panel", "aria-label": spec.label },
    parts,
    h("p", { class: "sh-hint" }, spec.hint),
  );
  return {
    el: e,
    /** @param {string} key */
    off: (key) => new Set(off.get(key) ?? []),
    /** @param {string} key @param {{value: string, label: string}[]} next */
    setValues: (key, next) => {
      const now = values.get(key) ?? [];
      if (
        now.length === next.length &&
        now.every((v, i) => v.value === next[i].value)
      )
        return;
      values.set(key, next);
      paint(key);
    },
    /**
     * Show only `keep` of a part (a deep link: one app's Logs, the
     * errors): every other value of it turned off.
     * @param {string} key @param {string} keep
     */
    only: (key, keep) => {
      const o = off.get(key);
      if (!o) return;
      const next = (values.get(key) ?? [])
        .map((v) => v.value)
        .filter((v) => v !== keep);
      if (next.length === o.size && next.every((v) => o.has(v))) return;
      o.clear();
      for (const v of next) o.add(v);
      paint(key);
    },
    /** @param {string} key @param {Record<string, number>} n */
    setCounts: (key, n) => {
      for (const [v, c] of counts.get(key) ?? [])
        c.textContent = String(n[v] ?? 0);
    },
    /** @returns {boolean} whether anything was off */
    reset: () => {
      let any = false;
      for (const [k, o] of off) {
        if (o.size) any = true;
        o.clear();
        paint(k);
      }
      return any;
    },
  };
}

/**
 * The 14-night strip of a backup row: one cell per night, oldest first.
 * @param {{night: string, state: string}[]} cells
 */
function nightStrip(cells) {
  const missed = cells.filter((c) => c.state === "miss").map((c) => c.night);
  const got = cells.filter((c) => c.state === "ok").length;
  return h(
    "span",
    {
      class: "sh-strip",
      role: "img",
      "aria-label": `${got} of the last ${cells.length} nights backed up${missed.length ? `; missed ${missed.join(", ")}` : ""}`,
      title: missed.length
        ? `Missed: ${missed.join(", ")}`
        : `${got} of ${cells.length} nights backed up`,
    },
    cells.map((c) =>
      h("i", {
        class: `sh-strip__${c.state}`,
        title: `${c.night}: ${c.state}`,
      }),
    ),
  );
}

/**
 * redesign-flows-6 (review item 4, the demo's "Roll back, 1 click, for 7
 * days, from the stack's History"): the updates of this stack the Update
 * flow made in the last 7 days, each pinned app with its Roll back….
 * @param {string} stack
 * @param {AbortSignal} signal
 */
function updatesCard(stack, signal) {
  ensureStyle("/css/pages/update.css");
  const list = h(
    "div",
    { class: "undo-updates__list" },
    h("p", { class: "measured" }, "Reading the updates of the last 7 days…"),
  );
  const card = section({
    id: "undo-updates",
    title: "Updates you can undo",
    desc: "Each app the Update flow moved in the last 7 days; Roll back puts its earlier version back the same way — backup, files, deploy.",
    level: "h3",
    cls: "undo-updates",
  });
  card.body.append(list);
  void (async () => {
    const r = await fetchJson(
      `/data/update-flows?stack=${encodeURIComponent(stack)}`,
      "the updates of the last 7 days",
      signal,
    );
    if (signal.aborted) return;
    if (!r.ok) {
      list.replaceChildren(errorBox(r.error));
      return;
    }
    const rows = (r.body.updates ?? []).flatMap((/** @type {any} */ u) =>
      [...movesOf(u.items ?? []).values()]
        .filter((m) => m.stack === stack)
        .map((m) => {
          const b = drivable(
            h(
              "button",
              {
                type: "button",
                class: "kp-button kp-button--sm",
                title: `Put ${m.from_version} back: backup, files, deploy`,
              },
              `Roll back to ${m.from_version}…`,
            ),
            UNDO_UPDATE,
            `${stack}/${m.key}`,
          );
          b.addEventListener("click", () => openPinRollback(m));
          return h(
            "div",
            { class: "undo-updates__row" },
            h("strong", null, m.key),
            h("span", null, `${m.from_version} → ${m.to_version}`),
            h(
              "span",
              { class: "measured" },
              `${formatDateTime(u.at)} · ${u.by}`,
            ),
            b,
          );
        }),
    );
    list.replaceChildren(
      ...(rows.length
        ? rows
        : [
            h(
              "p",
              { class: "measured" },
              "No app of this stack was updated in the last 7 days.",
            ),
          ]),
    );
  })().catch(() => {});
  return card.el;
}
