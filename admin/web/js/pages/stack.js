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

import { act, catalogReady, onAct, send } from "../act.js";
import { openAction } from "../actiondialog.js";
import { onAnswered } from "../answer.js";
import { mountWhenCheck } from "../whencheck.js";
import {
  fetchJson,
  fetchReport,
  h,
  slowRead,
  stateWord,
  tabRow,
  tableBlock,
} from "../dom.js";
import { declare, drivable, viaForm } from "../drivable.js";
import { driven } from "../drivehooks.js";
import {
  checksEditTab,
  firewallTab,
  openPublishDialog,
  settingsTab,
} from "../editpanels.js";
import { formatDateTime, formatDay, humanDuration } from "../format.js";
import { openIncident } from "../incident.js";
import { inboxNow, onInbox } from "../inboxlist.js";
import { finished } from "../jobs.js";
import { mountJobPanel } from "../jobpanel.js";
import { lineTime, logsUrl } from "../logs.js";
import { stage } from "../stage.js";
import { openPinRollback, openPinUpdate } from "../pinupdate.js";
import { isMovingTag } from "../updateflow.js";
import { movesOf } from "../updateflow.js";
import { openRollback } from "../rollbackdialog.js";
import { STACK_TABS, stackHref } from "../router.js";
import { incidentRows } from "../activity.js";
import { addDays } from "../backupsview.js";
import { setProblems } from "/static/kp/dist/kp-themes.min.js";
import { resourcesTab } from "../hubresources.js";
import {
  depItems,
  problemItems,
  statusBlock,
  statusRows,
} from "../hubparts.js";
import {
  CARD_RETRY_EVENT,
  attachCardStates,
  setCardState,
} from "/static/kp/dist/kp-themes.min.js";
import { setDeplist } from "/static/kp/dist/kp-themes.min.js";
import {
  VERSION_SORT,
  attachVersionDeltas,
  releaseUrl,
  versionLevel,
} from "/static/kp/dist/kp-themes.min.js";
import { setCode } from "/static/kp/dist/kp-themes.min.js";
import {
  HEAT_PICK_EVENT,
  setHeat,
  setHeatLegend,
} from "/static/kp/dist/kp-themes.min.js";
import { UPTIME_PICK_EVENT, setUptime } from "/static/kp/dist/kp-themes.min.js";
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
  declaresNatives,
  noIncidentsText,
  restartsDay,
  shortWhen,
  sizeFacts,
  staleFor,
  WHO_CHIPS,
} from "../stackhub.js";
import { stackEntries, stackIncidents } from "../stacktabs.js";
import { current, retryStream, subscribe } from "../store.js";
import { setParams } from "../urlstate.js";
import { attachDataTables } from "/static/kp/dist/kp-themes.min.js";
import { setIdmark } from "/static/kp/dist/kp-themes.min.js";
import { sourceColour } from "../logcolour.js";
import {
  FACET_EVENT,
  attachFacets,
  getFacetValues,
  setFacetCounts,
  setFacetItems,
  setFacetValues,
} from "/static/kp/dist/kp-themes.min.js";
import {
  LOGVIEWER_EVENT,
  LOGVIEWER_RETRY_EVENT,
  appendLogLines,
  attachLogViewers,
  setLogViewer,
} from "/static/kp/dist/kp-themes.min.js";
import { FOLLOW_EVENT } from "/static/kp/dist/kp-themes.min.js";
import { facetGroup } from "../logfacets.js";
import { formGrid, gridRow } from "../gridrow.js";
import { watchTabOverflow } from "/static/kp/dist/kp-themes.min.js";
import { attachToolbars } from "/static/kp/dist/kp-themes.min.js";
import { setTabCount } from "/static/kp/dist/kp-themes.min.js";
import { registerShortcuts } from "/static/kp/dist/kp-themes.min.js";
import { GROUP_STACK, unlessDialog } from "../shortcuts.js";
import {
  FEED_RETRY_EVENT,
  attachFeeds,
  setFeed,
} from "/static/kp/dist/kp-themes.min.js";
import { mount as mountSecrets } from "./secrets.js";
import {
  countOf,
  chip,
  dot,
  ensureStyle,
  keyRow,
  liveStatus,
  kpiStrip,
  moreMenu,
  section,
  segSwitch,
} from "../ui.js";

/** How far back the History tab reads. */
const HISTORY_DAYS = 30;
/** How many lines one Logs read asks Loki for. */
const LOG_LIMIT = 1000;
/** The swatch of a level group's facet row. @type {Record<string, string>} */
const LEVEL_SWATCH = {
  i: "var(--chart-1, var(--primary))",
  w: "var(--warning-foreground)",
  e: "var(--destructive)",
};

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
  // redesign-openpoints-1: Back up and Deploy open their dialog, Update the
  // one Update flow (an address). redesign-final-42: each row says so.
  opens: "dialog",
  rowOpens: { update: "view" },
  row: "<stack>/<backup|update|deploy>",
  at: at("overview"),
  what: "the header's Back up, Update or Deploy (b, u, d): Back up and Deploy open that action's dialog, Update opens the stack's Update flow",
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
const PROBLEM_SEEN = declare({
  id: "stack-problem-seen",
  page: "stack",
  opens: "run",
  row: "<stack>/<problem>",
  at: at("overview"),
  what: "mark one row of the stack's Needs you band as seen (a notice with no fix to run)",
  shows: "on a row whose notice has no fix and no other page to go to",
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
const CHECK_CONFIRM = declare({
  id: "stack-check-confirm",
  page: "stack",
  opens: "run",
  row: "<check id>",
  at: at("overview"),
  what: "keep today's date as the last time one of this stack's checks was confirmed (answer-check with ok); nothing else changes",
  shows: 'on the stack\'s "When something goes wrong, check this" list',
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
  what: "the Backups card's Back up now or Restore",
});
const BACKUP_ROW = declare({
  id: "stack-backup-row",
  page: "stack",
  opens: "dialog",
  row: "<stack>/<app>/<restore|verify>",
  at: at("backups"),
  what: "one app's Restore… or Verify… on the Backups tab",
});
const BACKUP_VIEW = declare({
  id: "stack-backups-view",
  page: "stack",
  opens: "view",
  row: "<stack>/<strips|heat>",
  at: at("backups"),
  what: "show the Backups tab as a strip of 14 nights per app or a grid of 30 nights",
});
const BACKUP_RESTORE_NIGHT = declare({
  id: "stack-backup-restore-night",
  page: "stack",
  opens: "view",
  row: "<stack>/<app>",
  at: at("backups"),
  what: "after pressing a missed night: open Restore on the night before it",
  shows: "after a missed night is pressed",
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
  what: "open the file editor: Files' Open the editor, Size and network's Edit",
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
      declared: declaresNatives(S.edit?.manifest),
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
  /** The tab count of one tab (kp setTabCount: exact digits, nothing before it is counted). */
  const tabCount = (
    /** @type {string} */ tab,
    /** @type {number | null | "loading" | "error"} */ n,
    /** @type {{tone?: import("/static/kp/dist/kp-themes.min.js").TabCountTone | null, unit?: string | null, showUnit?: boolean, title?: string}} */ o = {},
  ) => {
    const a = tabs.querySelector(
      `a[href="${CSS.escape(stackHref(name, tab))}"]`,
    );
    if (!a) return;
    setTabCount(a, n, {
      tone: o.tone ?? null,
      unit: o.unit ?? null,
      showUnit: o.showUnit ?? false,
    });
    if (o.title) a.setAttribute("title", o.title);
  };
  tabCount("apps", "loading");
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
    ["1–7", "tabs"],
    ["Ctrl K", "do anything"],
  ]);
  root.replaceChildren(
    header.el,
    h("div", { class: "sh-tabbar" }, tabs),
    keys,
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

  /** @type {{logs: number | null, history: number | null}} */
  const counts = { logs: null, history: null };
  const paintCounts = () => {
    tabCount("logs", counts.logs, {
      tone: counts.logs ? "error" : null,
      unit: "error,errors",
      showUnit: true,
      title:
        counts.logs == null
          ? undefined
          : `${counts.logs} errors in the logs of the last hour`,
    });
    tabCount("history", counts.history, {
      tone: counts.history ? "warning" : null,
      unit: "incident,incidents",
      showUnit: true,
      title:
        counts.history == null
          ? undefined
          : `${counts.history} failed operations with an incident bundle`,
    });
  };

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
    tabCount("apps", n, { unit: "app,apps" });
    paintCounts();
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

  // ── keys: b u d . (kp's engine: not while typing, not over a dialog, the
  // chord "g b" is its own; the sheet shows them dimmed off a stack's page) ──
  const press = (/** @type {HTMLElement} */ btn) =>
    unlessDialog(() => {
      if (btn.hasAttribute("disabled")) return false;
      btn.click();
      return undefined;
    });
  stops.push(
    registerShortcuts(`page:stack:${name}`, [
      {
        keys: "b",
        what: "Back up this stack",
        group: GROUP_STACK,
        run: press(backupBtn),
      },
      {
        keys: "u",
        what: "Update this stack",
        group: GROUP_STACK,
        run: press(updateBtn),
      },
      {
        keys: "d",
        what: "Deploy this stack",
        group: GROUP_STACK,
        run: press(deployBtn),
      },
      {
        keys: ".",
        what: "Open the more-actions menu",
        group: GROUP_STACK,
        run: unlessDialog(() => {
          menu.toggle();
        }),
      },
    ]),
  );

  /** @type {Record<string, (p: HTMLElement) => () => void>} */
  const panels = {
    overview: (p) => overviewTab(p, ctx),
    resources: (p) => resourcesTab(p, ctx),
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
    /**
     * A tab reports a number it counted; the tab row shows it.
     * @param {"logs" | "history"} key @param {number | null} n
     */
    setCount: (key, n) => {
      counts[key] = n;
      paintCounts();
    },
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
 *   reloadEdit: () => void,
 *   setCount: (key: "logs" | "history", n: number | null) => void}} Ctx
 */

/** Now, unix seconds. */
const nowS = () => Math.floor(Date.now() / 1000);

/**
 * A button that opens a catalog action's dialog (Live view: the form).
 * @param {Ctx} c
 * @param {string} action
 * @param {{label: string, title: string, cls?: string,
 *   preset?: Record<string, string>, slot?: string,
 *   drive: [string, string]}} o `drive`: the declared id and its row;
 *   `slot`: the kp-themes action-column role (its fixed place in a row)
 */
function actionButton(c, action, o) {
  const b = h(
    "button",
    {
      type: "button",
      class: `kp-button ${o.cls ?? ""}`.trim(),
      "data-action": action,
      ...(o.slot ? { "data-kp-action": o.slot } : {}),
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
  // What needs you: kp problems (what, why, the remedy, the command to
  // copy and the fix button), in a card that carries the four data states.
  const needsList = h("div", {
    class: "kp-problems",
    "data-kp-problems": "",
    "data-kp-problems-mode": "fold",
    "aria-live": "polite",
  });
  const needsStage = stage(needsList);
  const depsList = h("div", { class: "kp-deplist", "data-kp-deplist": "" });
  const depsStage = stage(depsList);
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
    class: "kp-kpi sh-kpi--plain",
    "data-loading": "",
  });
  if (driftTile) {
    plain.append(...driftTile.el.childNodes);
    driftTile.el.replaceWith(plain);
    // redesign-integrate-8: ui.js's tile draws its fourth row only for a
    // spark or a meter, which this tile has neither of; the button went
    // into a row that was never there, so Compare now was never drawn.
    const act = h("span", { class: "sh-kpi__act" }, compareBtn);
    const slot = plain.querySelector(".kp-kpi__spark, .kp-kpi__meter");
    if (slot) slot.replaceWith(act);
    else plain.append(act);
  }

  const checksHost = h("div", { class: "sh-checks" });
  const checksStage = stage(checksHost);
  const checksCount = h("p", { class: "sh-checks__count", role: "status" });
  const checks = statusBlock(checksHost, {
    label: `Health checks of ${name}`,
    onRetry: () => void readAll(),
  });
  const checkedAt = h("span", null, "checking…");
  for (const card of [needsStage, checksStage])
    card.addEventListener("kp-card-retry", () => retryStream());
  const healthy = section({
    id: "healthy",
    title: "Is it healthy?",
    desc: "Every check the host runs on this stack, in one place — the answer to “why is it red?” when it is.",
  });
  healthy.body.append(checksCount, checksStage);
  healthy.setFoot([
    checkedAt,
    link(
      `${stackHref(name, "logs")}?lvl=e`,
      `${name}/logs/errors`,
      "Recent errors in Logs",
    ),
  ]);
  healthy.el.classList.add("sh-span-6");
  const needs = section({
    id: "needs",
    title: "What needs you",
    desc: "What is wrong with this stack, why, and what to do about it; the command is one press from your clipboard.",
  });
  needs.body.append(needsStage);
  needs.el.classList.add("sh-span-6");
  const deps = section({
    id: "dependencies",
    title: "Depends on and used by",
    desc: "The stacks this one needs, in the order they must start, and the stacks that need it.",
  });
  deps.body.append(depsStage);
  deps.el.classList.add("sh-span-6");

  const feedEl = historyFeedEl("none");
  stops.push(attachFeeds(feedEl));
  setFeed(feedEl, { state: "loading" });
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
  recent.body.append(feedEl);
  recent.el.classList.add("sh-span-6");

  // checks.yml, folded: the editor Live view's `checks` form lands on.
  const judged = section({
    id: "stack-checks",
    title: "Checks it is judged on",
    desc: "The before/after checks, nightly probes and manual questions of each app's health checks file, one app at a time.",
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

  // manual-checks-as-info-1: the checks only a person can confirm, as
  // information for when something goes wrong, never as a notification.
  const whenWrong = mountWhenCheck({
    stack: name,
    id: "stack-when-wrong",
    drive: CHECK_CONFIRM,
    signal: c.signal,
    emptyText: "This stack declares nothing that only a person can look at.",
  });
  whenWrong.el.classList.add("sh-span-12");
  stops.push(whenWrong.stop);

  panel.replaceChildren(
    h(
      "div",
      { class: "sh-stack" },
      running,
      strip.el,
      h(
        "div",
        { class: "sh-grid" },
        healthy.el,
        needs.el,
        deps.el,
        recent.el,
        whenWrong.el,
        judged.el,
      ),
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
  /** @type {{edges: {from: string, to: string, kind: string}[]} | null} */
  let topo = null;
  /** @type {string | null} */
  let topoFailed = null;
  let topoRead = false;

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
    if (S.fleetRead && !checkedAtS) checkedAtS = now;
    const probs = attentionItems({
      stack: name,
      s: S.s,
      drift: S.drift,
      night: stand.night,
      last: stand.last,
      stale,
      now,
      inboxItems: inboxNow()
        .items.filter((i) => i.stack === name)
        .map((i) => {
          // fix-ux-1: the notice's own fix and remedy ride with its row.
          const n = act.notices?.notices.find((x) =>
            (i.ids ?? []).includes(x.id),
          );
          return {
            ...i,
            ...(n?.remedy ? { remedy: n.remedy } : {}),
            ...(n?.fixes?.[0] ? { fix: n.fixes[0] } : {}),
          };
        }),
    }).filter((p) => S.fleetRead || p.key !== "not-deployed");
    // The host not answering is a failed read, said with the package's plate
    // and its Retry, not a card that loads for ever.
    const hostDown = !S.fleetRead && current().up === false;
    if (hostDown)
      setCardState(needsStage, "error", {
        title: "The host did not answer",
        text: "What needs you on this stack is not known until it does.",
      });
    else if (!S.fleetRead)
      setCardState(needsStage, "loading", { words: "Reading what needs you" });
    else if (!probs.length)
      setCardState(needsStage, "empty", {
        title: "Nothing needs you",
        text: "This stack is fine as far as the host can tell.",
      });
    else {
      setCardState(needsStage, "ready");
      setProblems(
        needsList,
        problemItems(probs, name, (p) => problemAction(c, p)),
        { mode: "fold", foldFrom: 3 },
      );
    }
    if (hostDown)
      setCardState(checksStage, "error", {
        title: "The host did not answer",
        text: "The checks cannot run until it does.",
      });
    else if (!S.fleetRead)
      setCardState(checksStage, "loading", { words: "Running the checks" });
    if (S.fleetRead) {
      const rows = healthChecks({
        s: S.s,
        night: stand.night,
        diskPct,
        drift: S.drift,
        restarts24: day,
        watch,
      });
      const counted = checks.set(statusRows(rows));
      checksCount.textContent = `${counted.ok} of ${counted.total} ${counted.total === 1 ? "check passes" : "checks pass"}`;
      setCardState(checksStage, "ready");
      checkedAt.textContent = checkedAtS
        ? `checked ${humanDuration(now - checkedAtS)} ago`
        : "checking…";
    }
    if (!topoRead)
      setCardState(depsStage, "loading", { words: "Reading the topology" });
    else if (topoFailed)
      setCardState(depsStage, "error", {
        title: "Could not read the topology",
        text: topoFailed,
        retry: false,
      });
    else {
      const items = depItems(topo?.edges ?? [], name, (n) => {
        const st = current().fleet?.stacks.find((x) => x.name === n);
        return !st ? "unknown" : st.online ? "running" : "stopped";
      });
      if (!items.length)
        setCardState(depsStage, "empty", {
          title: "No connection known",
          text: "Declare what this stack talks to in its firewall rules and it shows here.",
        });
      else {
        setCardState(depsStage, "ready");
        setDeplist(depsList, items);
      }
    }
    if (history) {
      const rows = historyFeed(history);
      setFeed(feedEl, {
        state: rows.length ? "ready" : "empty",
        rows: feedRows(rows.slice(0, 4)),
        total: Math.min(4, rows.length),
        emptyText: `Nothing was done with ${name} in the last ${HISTORY_DAYS} days.`,
      });
    }
  };
  stops.push(c.onChange(paint));
  stops.push(onInbox(paint));
  stops.push(() => checks.stop());

  const readAll = async () => {
    const enc = encodeURIComponent(name);
    const tasks = [
      (async () => {
        const r = await fetchJson("/data/topology", "the topology", c.signal);
        topo = r.ok ? (r.body.topology ?? null) : null;
        topoFailed = r.ok ? null : r.error.why;
        topoRead = true;
        paint();
      })(),
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
        c.setCount("logs", errors ?? null);
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
  };
  void readAll();
  paint();
  stops.push(attachCardStates(panel));
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
 * The fix of one "needs you" row.
 * @param {Ctx} c
 * @param {import("../stackhub.js").Problem} p
 * @returns {HTMLElement | null}
 */
function problemAction(c, p) {
  const a = p.act;
  switch (a.kind) {
    case "none":
      return null;
    case "seen": {
      const b = drivable(
        h(
          "button",
          { type: "button", class: "kp-button", title: a.title },
          a.label,
        ),
        PROBLEM_SEEN,
        `${c.name}/${p.key}`,
      );
      b.addEventListener("click", async () => {
        b.setAttribute("disabled", "");
        const res = await send(
          "POST",
          "/data/notifications/read",
          { ids: a.ids, read: true },
          "marking it seen",
        );
        if (!res.ok) b.removeAttribute("disabled");
      });
      return b;
    }
    case "action":
      if (a.stack && a.stack !== c.name) {
        const b = drivable(
          h(
            "button",
            { type: "button", class: "kp-button", title: a.title },
            a.label,
          ),
          FIX,
          `${c.name}/${p.key}`,
        );
        b.addEventListener(
          "click",
          () =>
            void openAction(/** @type {string} */ (a.stack), a.action, {
              preset: a.preset ?? {},
            }),
        );
        return b;
      }
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
 * The history rows as kp feed rows: the outcome's mark, what happened, who
 * started it as the actor chip, the detail and when.
 * @param {ReturnType<typeof historyFeed>} rows
 * @returns {import("/static/kp/dist/kp-themes.min.js").FeedRow[]}
 */
function feedRows(rows) {
  /** @type {Map<string, number>} */
  const seen = new Map();
  return rows.map((r) => {
    const base = `${r.start}-${r.what}`;
    const n = seen.get(base) ?? 0;
    seen.set(base, n + 1);
    return {
      key: n ? `${base}-${n}` : base,
      title: r.what,
      at: r.start * 1000,
      state: r.tone === "bad" ? "failed" : r.tone === "warn" ? "warn" : "ok",
      actor: {
        name: r.who.label,
        kind:
          r.who.key === "you"
            ? "person"
            : r.who.key === "claude"
              ? "agent"
              : "schedule",
      },
      detail: r.detail,
    };
  });
}

/**
 * A kp activity feed for a stack's history (the Overview's four rows, the
 * History tab's 30 days).
 * @param {"day" | "none"} group
 */
function historyFeedEl(group) {
  return h("div", {
    class: "kp-feed",
    "data-kp-feed": "",
    "data-kp-feed-unit": "operation,operations",
    "data-kp-feed-group": group,
    "data-kp-feed-head": "off",
    "aria-label": "History of this stack",
  });
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
 * levels (kp's facet lists), the time window, kp's log viewer (find, wrap,
 * time, Copy, Download, the "N new lines" pill, its own loading, empty and
 * failed states), and a foot that says where the lines came from. The
 * host's own log of what it did to this stack lives in Activity ▸ Host
 * log, which the description says.
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
  // `?app=` (an Apps row's Logs) shows that one app; `?lvl=e` the errors —
  // until the first click in the side column.
  const only = { app: q0.get("app"), lvl: q0.get("lvl") };
  /** The rows made so far, so a row that is there keeps its state. @type {Set<string>} */
  const made = new Set();

  const appsGroup = facetGroup(
    "Apps",
    "Every app",
    "No app has printed a line in this window.",
  );
  const levelsGroup = facetGroup("Level", "Every level", "No levels.");
  const side = h(
    "aside",
    { class: "sh-side__panel", "aria-label": "Log filters" },
    h(
      "div",
      { class: "sh-side__part" },
      h("p", { class: "sh-side__label" }, "Apps"),
      appsGroup,
    ),
    h(
      "div",
      { class: "sh-side__part" },
      h("p", { class: "sh-side__label" }, "Level"),
      levelsGroup,
    ),
    h(
      "p",
      { class: "sh-hint" },
      "Each click turns one on or off; “only” shows just that one. Esc shows everything again.",
    ),
  );

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
  const readNote = h("span", { class: "sh-read", role: "status" }, "");
  const controls = formGrid(
    [
      gridRow({
        label: "Window",
        control: win.el,
        value: readNote,
        wide: false,
      }),
    ],
    "sh-logcontrols",
  );

  const viewer = h("div", {
    class: "kp-logviewer sh-viewer",
    "data-kp-logviewer": "",
    "data-kp-log-max": "2000",
    "data-kp-log-tools": "search,follow,wrap,time,copy,download",
    "data-kp-log-filename": `${name}-logs`,
    "data-kp-log-search-only": "on",
    "data-kp-log-follow": st.follow ? "live" : "paused",
    "data-kp-state": "loading",
    "aria-label": `Log lines of ${name}`,
  });
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
    controls,
    viewer,
    h("div", { class: "sh-foot" }, footLeft, footRight),
  );
  panel.replaceChildren(h("div", { class: "sh-side" }, side, card));
  const detach = [attachFacets(panel), attachLogViewers(panel)];
  const logEl = /** @type {HTMLElement} */ (
    viewer.querySelector(".kp-logviewer__log")
  );
  setLogViewer(viewer, { search: st.q, searchOnly: true, state: "loading" });

  // The Live view marks on what kp draws.
  const stampDrive = () => {
    /** Runs `mark` on the element while it has no Live view mark. @param {Element | null} el @param {(e: HTMLElement) => void} mark */
    const onFresh = (el, mark) => {
      if (el instanceof HTMLElement && !el.dataset.drive) mark(el);
    };
    for (const [group, part] of /** @type {const} */ ([
      [appsGroup, "apps"],
      [levelsGroup, "levels"],
    ])) {
      for (const row of group.querySelectorAll(
        ".kp-facet:not([data-kp-facet-all])",
      ))
        onFresh(row.querySelector(".kp-facet__check"), (e) =>
          drivable(
            e,
            LOG_FILTER,
            `${name}/${part}/${/** @type {HTMLElement} */ (row).dataset.value}`,
          ),
        );
      onFresh(
        group.querySelector("[data-kp-facet-all] .kp-facet__check"),
        (e) => drivable(e, LOG_FILTER, `${name}/${part}/all`),
      );
    }
    onFresh(viewer.querySelector(".kp-logviewer__find"), (e) =>
      drivable(e, LOG_SEARCH, name),
    );
    onFresh(viewer.querySelector("[data-kp-follow-toggle]"), (e) =>
      drivable(e, LOG_FOLLOW, name),
    );
    onFresh(viewer.querySelector("[data-kp-logviewer-retry]"), (e) =>
      drivable(e, LOG_RETRY, name),
    );
  };
  let stampQueued = false;
  const watcher = new MutationObserver(() => {
    if (stampQueued) return;
    stampQueued = true;
    requestAnimationFrame(() => {
      stampQueued = false;
      stampDrive();
    });
  });
  watcher.observe(viewer, { childList: true, subtree: true });
  watcher.observe(side, { childList: true, subtree: true });

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

  /** The viewer's word for a level group. @type {Record<string, string>} */
  const WORD = { i: "info", w: "warning", e: "error" };
  /**
   * Lines as the viewer keeps them: one id per line, also for the same
   * words at the same millisecond, so a poll that returns the window again
   * adds nothing twice.
   * @param {import("../logs.js").LogLine[]} list
   */
  const kpLines = (list) => {
    /** @type {Map<string, number>} */
    const dupes = new Map();
    return list.map((l) => {
      const key = `${l.ts_ms}|${l.source}|${l.stream}|${l.line}`;
      const n = (dupes.get(key) ?? 0) + 1;
      dupes.set(key, n);
      return {
        id: `${key}|${n}`,
        ts: l.ts_ms,
        source: l.source || "—",
        level: WORD[levelGroup(l.level)],
        message: l.line,
      };
    });
  };
  const on = () => ({
    apps: new Set(getFacetValues(appsGroup)),
    levels: new Set(getFacetValues(levelsGroup)),
  });
  /** The lines the filters let into the viewer, oldest first. @param {import("../logs.js").LogLine[]} from */
  const passing = (from) => {
    const { apps, levels } = on();
    return [...from]
      .sort((a, b) => a.ts_ms - b.ts_ms)
      .filter(
        (l) => apps.has(l.source || "—") && levels.has(levelGroup(l.level)),
      );
  };

  const paintSide = () => {
    if (loading) {
      for (const g of [appsGroup, levelsGroup])
        g.setAttribute("aria-busy", "true");
      return;
    }
    /** @type {string[]} */
    const apps = S.s?.apps?.map((a) => a.name) ?? S.edit?.manifest?.apps ?? [];
    const sources = logSources(apps, lines);
    const v = logView(lines, new Set(), new Set());
    setFacetItems(
      appsGroup,
      sources.map((s) => ({
        value: s,
        label: s,
        count: v.sources[s] ?? 0,
        swatch: sourceColour(s),
        pattern: s,
        ...(made.has(`a:${s}`) ? {} : { checked: !only.app || only.app === s }),
      })),
    );
    for (const s of sources) made.add(`a:${s}`);
    setFacetItems(
      levelsGroup,
      LEVELS.map((l) => ({
        value: l.value,
        label: l.label,
        count: v.levels[l.value] ?? 0,
        swatch: LEVEL_SWATCH[l.value],
        ...(made.has(`l:${l.value}`)
          ? {}
          : { checked: !only.lvl || only.lvl === l.value }),
      })),
    );
    for (const l of LEVELS) made.add(`l:${l.value}`);
    setFacetCounts(appsGroup, v.sources, { all: v.total });
    setFacetCounts(levelsGroup, v.levels, { all: v.total });
    // A long name is cut with an ellipsis: its full words are the tooltip.
    for (const n of side.querySelectorAll(".kp-facet__name"))
      /** @type {HTMLElement} */ (n).title = n.textContent ?? "";
  };

  /** @param {number} s */
  const winLabel = (s) =>
    ({ 900: "15 minutes", 3600: "hour", 86400: "24 hours", 604800: "7 days" })[
      /** @type {900} */ (s)
    ] ?? "window";

  const paintFoot = () => {
    footLeft.textContent = failed
      ? "Loki did not answer"
      : readAt
        ? `Loki, read ${humanDuration(nowS() - readAt)} ago${logql ? ` · ${logql}` : ""}`
        : "Asking Loki for the lines";
    footRight.textContent = st.follow
      ? "Following the newest line"
      : "Paused: new lines wait below a marker";
    readNote.textContent = failed
      ? "Loki did not answer"
      : loading
        ? "Asking Loki for the lines"
        : readAt
          ? `Read ${humanDuration(nowS() - readAt)} ago, every 5 s`
          : "";
  };

  /** What the viewer is handed after a filter, a window or a read. */
  const feed = () => {
    viewer.setAttribute(
      "data-kp-log-hint",
      st.q
        ? `Loki holds no line containing “${st.q}” in the last ${winLabel(Number(st.since))}; widen the window or clear the search.`
        : `Loki holds no lines of ${name} in the last ${winLabel(Number(st.since))}; widen the window.`,
    );
    setLogViewer(viewer, {
      lines: loading || failed ? [] : kpLines(passing(lines)),
      state: loading ? "loading" : failed ? "error" : "ready",
      ...(failed
        ? {
            message: `Loki did not answer: ${failed.why}${failed.fix ? ` — ${failed.fix}` : ""}`,
          }
        : {}),
    });
    paintFoot();
  };

  /** @param {Event} e */
  const filterChanged = (e) => {
    only.app = null;
    only.lvl = null;
    // Nothing ticked reads as everything, as the buttons did.
    const g = /** @type {HTMLElement} */ (e.currentTarget);
    if (!getFacetValues(g).length) setFacetValues(g, null);
    feed();
  };
  appsGroup.addEventListener(FACET_EVENT, filterChanged);
  levelsGroup.addEventListener(FACET_EVENT, filterChanged);
  viewer.addEventListener(LOGVIEWER_EVENT, () => {
    // The viewer's own find is the text filter; the address remembers it.
    const q = viewer.getAttribute("data-kp-log-search") ?? "";
    if (q === st.q) return;
    st.q = q;
    writeUrl();
  });
  viewer.addEventListener(LOGVIEWER_RETRY_EVENT, () => {
    void load(false).catch(() => {});
  });
  logEl.addEventListener(FOLLOW_EVENT, (e) => {
    const now = /** @type {CustomEvent} */ (e).detail.following !== false;
    if (now === st.follow) return;
    st.follow = now;
    writeUrl();
    paintFoot();
  });

  let req = new AbortController();
  c.signal.addEventListener("abort", () => req.abort());
  /** @param {boolean} quiet */
  const load = async (quiet) => {
    req.abort();
    req = new AbortController();
    if (!quiet) {
      loading = true;
      failed = null;
      lines = [];
      paintSide();
      feed();
    }
    const r = await fetchJson(
      logsUrl(name, { since: st.since, app: "", q: "" }, LOG_LIMIT),
      "the logs",
      req.signal,
    ).catch(() => null);
    if (!r) return;
    loading = false;
    if (!r.ok) {
      failed = r.error;
      paintSide();
      feed();
      return;
    }
    const wasFailed = failed !== null;
    failed = null;
    lines = r.body.lines ?? [];
    logql = r.body.logql ?? "";
    readAt = r.body.measured_at ?? nowS();
    paintSide();
    // A poll hands over only what is new: the viewer keeps a line once and
    // the pill counts what came while the reader was away.
    if (quiet && !wasFailed) {
      appendLogLines(viewer, kpLines(passing(lines)));
      paintFoot();
    } else feed();
  };
  const timer = setInterval(() => void load(true).catch(() => {}), 5000);
  const tick = setInterval(paintFoot, 1000);
  /** @param {Element} g */
  const allOn = (g) =>
    getFacetValues(g).length ===
    g.querySelectorAll(".kp-facet:not([data-kp-facet-all])").length;
  const stopEsc = registerShortcuts("page:stack-logs", [
    {
      keys: "Escape",
      what: "Show every app and level again",
      group: GROUP_STACK,
      run: unlessDialog(() => {
        const narrowed = !allOn(appsGroup) || !allOn(levelsGroup);
        setFacetValues(appsGroup, null);
        setFacetValues(levelsGroup, null);
        only.app = null;
        only.lvl = null;
        if (narrowed) feed();
      }),
    },
  ]);
  const stopChange = c.onChange(paintSide);
  paintFoot();
  void load(false).catch(() => {});
  return () => {
    clearInterval(timer);
    clearInterval(tick);
    watcher.disconnect();
    req.abort();
    stopChange();
    stopEsc();
    for (const d of detach) d();
  };
}

// ─────────────────────────────────────────────────────────────── Apps

/**
 * Apps (the hub demo): each app's state, the version its image is pinned
 * to and whether a newer one exists, with Logs, Update… and Publish… per
 * row. A kp datatable: sortable, a click adds a key, remembered, cards
 * on a phone.
 * @param {HTMLElement} panel
 * @param {Ctx} c
 */
function appsTab(panel, c) {
  const { name, S } = c;
  /** @type {import("../stackhub.js").Stale[] | null} */
  let stale = null;
  /** The log lines each opened app showed, kept so a repaint does not read them again. @type {Map<string, string[]>} */
  const lines = new Map();
  const block = tableBlock({
    remember: "stack-apps",
    caption: `The apps of ${name}; open one for its image, health and last log lines`,
    captionHidden: true,
    search: null,
    nothing: "This stack declares no apps.",
    columns: [
      { label: "App", sort: "text" },
      { label: "State", sort: "text", filter: "choice" },
      { label: "Version", sort: "text" },
      { label: "Newer", sort: "text" },
      { label: "Restarts", sort: "number", cls: "kp-text-end" },
      { label: "Health", sort: "text" },
      { label: "Actions", sort: null, cls: "sh-right" },
    ],
  });
  for (const [k, v] of [
    ["data-kp-expandable", ""],
    ["data-kp-expand-single", ""],
    ["data-kp-column-menu", ""],
  ])
    block.wrap.setAttribute(k, v);
  // The density chooser sits in the table's bar, remembered with the table.
  const densityBar = h(
    "div",
    { class: "kp-datatable__bar" },
    h("select", {
      class: "kp-field__input kp-datatable__select",
      "data-kp-datatable-density": "",
      "aria-label": "Row density",
    }),
  );
  block.wrap.prepend(densityBar);
  const ths = /** @type {HTMLElement[]} */ ([
    ...block.wrap.querySelectorAll("thead th"),
  ]);
  ths[0].setAttribute("data-kp-column-locked", "");
  // Opens sorted by its first column, so the phone's card sort reads
  // "Sort by App · Ascending" rather than "Sort by None".
  ths[0].setAttribute("aria-sort", "ascending");
  ths[5].setAttribute("data-kp-column-hidden", "");
  const card = section({
    title: "Apps",
    desc: "The containers this stack runs, the version each is on, and whether a newer one exists; open one for its image, health and last log lines.",
  });
  card.body.append(block.wrap);
  card.setFoot([
    "Versions from the stack's files; newer ones from the fleet check's last run.",
    link("/map", `${name}/apps/map`, "Every stale image in Map"),
  ]);
  panel.replaceChildren(card.el);
  const detachTables = attachDataTables(card.el);
  const detachCards = attachCardStates(panel);
  // The table is its own state: its busy card over the rows while the fleet
  // is read, its failed plate when the host does not answer.
  block.loading({ words: "Reading the apps", overlay: true });
  block.wrap.addEventListener("kp-datatable-retry", () => retryStream());
  let drawnKey = "";

  /** @param {string} app */
  const readLines = async (app) => {
    const r = await fetchJson(
      logsUrl(name, { since: "3600", app, q: "" }, 3),
      `${app}'s last log lines`,
      c.signal,
    );
    const got = r.ok
      ? (r.body.lines ?? [])
          .slice(-3)
          .map(
            (/** @type {any} */ l) =>
              `${lineTime(l.ts ?? l.time ?? 0)}  ${l.line ?? l.text ?? l.msg ?? ""}`,
          )
      : [`Could not read the log lines: ${r.error.why}`];
    lines.set(app, got);
    const holder = block.tbody.querySelector(
      `[data-hb-log="${CSS.escape(app)}"]`,
    );
    if (holder instanceof HTMLElement)
      setCode(holder, {
        title: "Last three log lines",
        language: "log",
        text: got.length ? got.join("\n") : "No line in the last hour.",
        wrap: true,
        copy: true,
      });
  };
  block.wrap.addEventListener("kp-datatable-expand", (e) => {
    const d = /** @type {CustomEvent} */ (e).detail;
    if (d.open && d.key) void readLines(String(d.key)).catch(() => {});
  });

  const paint = () => {
    if (!S.fleetRead) {
      if (current().up === false) {
        drawnKey = "";
        block.failed({
          what: "the apps",
          why: "the host did not answer",
          fix: "Try again; the apps come as soon as it does.",
        });
      }
      return;
    }
    const rows = appRows({
      s: S.s,
      manifestApps: S.edit?.manifest?.apps ?? [],
      images: S.edit?.images ?? null,
      stale: stale ?? [],
    });
    if (!rows.length) {
      block.setNothing(
        "This stack declares no apps. Add one in Settings ▸ Files (Add an app), from a preset or by hand.",
      );
      block.tbody.replaceChildren();
      block.ready();
      return;
    }
    const sig = JSON.stringify([
      rows.map((r) => [
        r.name,
        r.state,
        r.version,
        r.newer?.latest,
        r.restarts,
        isMovingTag(r.image),
      ]),
      stale == null,
      S.inFleet,
    ]);
    if (sig === drawnKey) return;
    drawnKey = sig;
    const fleetApps = S.s?.apps ?? [];
    block.tbody.replaceChildren(
      ...rows.flatMap((r) => {
        const health = fleetApps.find((a) => a.name === r.name)?.health ?? "";
        const logs = linkButton(
          `${stackHref(name, "logs")}?app=${encodeURIComponent(r.name)}`,
          "Logs",
          `${r.name}'s log lines`,
          `${name}/logs/app-${r.name}`,
        );
        // kp-themes action columns: one named slot per action, so a row
        // without Update… or Publish… keeps that slot empty.
        logs.dataset.kpAction = "logs";
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
              "Update",
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
          up.dataset.kpAction = "update";
          acts.push(up);
        } else if (
          !r.extra &&
          S.inFleet &&
          // update-nothing-1: a pull changes only an app on a moving tag; one
          // pinned by digest has nothing to pull (its newer version, if any,
          // is the pin row above). A native service updates its own way.
          (S.s?.native || isMovingTag(r.image))
        ) {
          const pull = actionButton(
            c,
            S.s?.native ? "update-native" : "update",
            {
              label: "Update",
              title: `Pull the newest image for ${r.name} and recreate it, with rollback`,
              cls: "kp-button--sm",
              preset: S.s?.native ? undefined : { app: r.name },
              drive: [APP_PULL, `${name}/${r.name}`],
            },
          );
          pull.dataset.kpAction = "update";
          acts.push(pull);
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
              "Publish",
            ),
            "publish",
          );
          drivable(pub, APP_PUBLISH, `${name}/${r.name}`);
          pub.addEventListener("click", () =>
            openPublishDialog(name, r.name, () => c.reloadEdit()),
          );
          pub.dataset.kpAction = "publish";
          acts.push(pub);
        }
        const word =
          r.state === "running"
            ? "up"
            : r.state === "stopped"
              ? "down"
              : r.extra
                ? "sidecar"
                : "not deployed";
        const kv = (
          /** @type {string} */ k,
          /** @type {string} */ v,
          mono = false,
        ) =>
          h(
            "div",
            { class: "kp-kv__row" },
            h("dt", null, k),
            h("dd", mono ? { class: "kp-mono" } : null, v),
          );
        return [
          h(
            "tr",
            { "data-kp-row-key": r.name },
            h(
              "td",
              { "data-kp-sort-value": r.name },
              h(
                "span",
                { class: "hb-app" },
                h("span", {
                  class: "kp-idmark kp-idmark--sm",
                  "data-kp-idmark": r.name,
                  "aria-hidden": "true",
                }),
                h("b", null, r.name),
              ),
            ),
            h(
              "td",
              { "data-kp-sort-value": word, "data-kp-filter-value": word },
              stateWord(word, ["up", "down", "sidecar", "not deployed"]),
            ),
            h(
              "td",
              {
                class: "kp-mono",
                title: r.image || null,
                "data-kp-sort-value": r.version,
              },
              r.version || "—",
            ),
            h(
              "td",
              {
                "data-kp-sort-value": r.newer
                  ? String(
                      VERSION_SORT[
                        versionLevel(
                          r.newer.pinned || r.version,
                          r.newer.latest,
                          {},
                        )
                      ],
                    )
                  : "0",
              },
              r.newer
                ? h("span", {
                    class: "kp-badge kp-badge--version",
                    "data-kp-version-from": r.newer.pinned || r.version,
                    "data-kp-version-to": r.newer.latest,
                    "data-kp-version-app": r.name,
                    "data-kp-version-href":
                      releaseUrl(r.newer.upstream, r.newer.latest) ?? "",
                  })
                : stale == null
                  ? "checking…"
                  : "up to date",
            ),
            h(
              "td",
              {
                class: "kp-text-end",
                "data-kp-sort-value": String(r.restarts ?? -1),
              },
              r.restarts == null ? "—" : String(r.restarts),
            ),
            h("td", { "data-kp-sort-value": health }, health || "not declared"),
            h(
              "td",
              { class: "row-actions" },
              h("div", { class: "kp-row-actions" }, acts),
            ),
          ),
          h(
            "tr",
            { "data-kp-row-detail": "" },
            h(
              "td",
              { colspan: "7" },
              h(
                "div",
                { class: "hb-detail" },
                h(
                  "dl",
                  { class: "kp-kv" },
                  kv("Image", r.image || "not known", true),
                  kv("Version", r.version || "—", true),
                  kv(
                    "Newer",
                    r.newer
                      ? `${r.newer.latest}${r.newer.major ? " (major)" : ""}`
                      : "none known",
                  ),
                  kv(
                    "Restarts",
                    r.restarts == null
                      ? "not reported"
                      : `${r.restarts} since the container started`,
                  ),
                  kv("Health", health || "no health check declared"),
                ),
                h(
                  "figure",
                  {
                    class: "kp-code",
                    "data-kp-code": "",
                    "data-hb-log": r.name,
                  },
                  h("pre", { class: "kp-code__body" }, h("code")),
                ),
              ),
            ),
          ),
        ];
      }),
    );
    attachVersionDeltas(block.tbody);
    for (const el of /** @type {NodeListOf<HTMLElement>} */ (
      block.tbody.querySelectorAll("[data-hb-log]")
    )) {
      const app = el.dataset.hbLog ?? "";
      setCode(el, {
        title: "Last three log lines",
        language: "log",
        text: (lines.get(app) ?? ["Open the row to read them."]).join("\n"),
        wrap: true,
        copy: true,
      });
    }
    block.ready();
  };
  const stop = c.onChange(paint);
  void (async () => {
    const r = await slowRead("/data/stale-images", "stale images", c.signal);
    stale = r.ok ? staleFor(name, r.body.images ?? []) : [];
    drawnKey = "";
    paint();
  })().catch(() => {});
  paint();
  return () => {
    stop();
    detachTables();
    detachCards();
  };
}

/**
 * A kp datatable without a search bar (a short list needs none):
 * sortable, a click adds a sort key, remembered per table, cards on a
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
      "data-kp-sort-multi": "click",
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
  const body = h("div", { class: "hb-backups" });
  const stageEl = stage(body);
  const native = () => S.s?.native === true;
  const backupNow = actionButton(c, "backup", {
    label: "Back up now",
    title: "A snapshot of every app's data now",
    drive: [BACKUP_CARD, `${name}/backup`],
  });
  const restore = actionButton(c, "restore", {
    label: "Restore",
    title: "Bring one app's data back to a chosen night (a safety copy first)",
    cls: "kp-button--primary",
    drive: [BACKUP_CARD, `${name}/restore`],
  });
  const card = section({
    title: "Backups of this stack",
    desc: "One snapshot per app every night, kept as its retention says. Restore brings one app's data back to a chosen night.",
    tools: [backupNow, restore],
  });
  card.body.append(stageEl);
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
  const detachCards = attachCardStates(panel);
  setCardState(stageEl, "loading", { words: "Reading the backups" });
  /** @type {any} */
  let read = null;
  /** @type {import("../doctor.js").RouteError | null} */
  let failed = null;
  let noBackup = false;
  /** @type {() => void} */
  let detach = () => {};
  /** @type {"strips" | "heat"} */
  let backupView = "strips";
  try {
    if (localStorage.getItem("homelab.hub.backups") === "heat")
      backupView = "heat";
  } catch {
    // the choice lasts for this visit
  }

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
      setCardState(stageEl, "error", {
        title: "The backups did not read",
        text: `${failed.why}${failed.fix ? ` — ${failed.fix}` : ""}`,
        retry: false,
      });
      return;
    }
    if (!read) return;
    if (noBackup || (read.repos ?? []).length === 0) {
      detach();
      setCardState(stageEl, "empty", {
        title: noBackup
          ? `${name} keeps no data to back up`
          : `${name} has no backup yet`,
        text: noBackup
          ? "It declares no app data, by design; there is nothing to snapshot."
          : "The first snapshot is written tonight at the nightly round, or now with Back up now.",
      });
      return;
    }
    setCardState(stageEl, "ready");
    const rows = backupRows({ repos: read.repos ?? [], now: nowS() });
    const long = backupRows({
      repos: read.repos ?? [],
      now: nowS(),
      nights: 30,
    });
    detach();
    detach = () => {};
    const dayLabel = (/** @type {string} */ night) => {
      const [y, m, d] = night.split("-").map(Number);
      return formatDay(Date.UTC(y, m - 1, d, 12) / 1000, { timeZone: "UTC" });
    };
    /** @param {string} app @param {string} night */
    const restoreHref = (app, night) =>
      `/restore?stack=${encodeURIComponent(name)}&app=${encodeURIComponent(app)}&snapshot=${encodeURIComponent(addDays(night, -1))}`;
    /** @param {string} app @param {string} night */
    const openRestore = (app, night) => {
      history.pushState(null, "", restoreHref(app, night));
      dispatchEvent(new PopStateEvent("popstate"));
    };
    const strips = h(
      "div",
      { class: "hb-strips" },
      ...rows.map((r) => {
        const el = h("div", {
          class: "kp-uptime",
          "data-kp-uptime": "",
          "data-kp-uptime-count": String(r.cells.length),
          "data-kp-uptime-label": r.app,
          "data-hb-app": r.app,
        });
        const measured = r.cells.filter(
          (x) => x.state === "ok" || x.state === "miss",
        );
        const ok = r.cells.filter((x) => x.state === "ok").length;
        return h(
          "div",
          { class: "hb-strip" },
          el,
          h(
            "p",
            { class: "hb-strip__count" },
            `${ok} of ${measured.length} nights`,
          ),
          h(
            "div",
            { class: "kp-row-actions" },
            actionButton(c, nat ? "restore-native" : "restore", {
              label: "Restore",
              title: `Bring ${r.app}'s data back from a snapshot`,
              // the card's own Restore… size, so the two stand as one column
              slot: "restore",
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
                  label: "Verify",
                  title: `Prove ${r.app}'s newest snapshot restores, without touching live data`,
                  cls: "kp-button--ghost",
                  slot: "verify",
                  preset: { app: r.app },
                  drive: [BACKUP_ROW, `${name}/${r.app}/verify`],
                }),
          ),
        );
      }),
    );
    const heat = h("div", {
      class: "kp-heat hb-heat",
      "data-kp-heat": "",
      "data-kp-heat-latest": "end",
    });
    const views = segSwitch({
      label: "View",
      value: backupView,
      items: [
        {
          value: "strips",
          label: "A strip per app, 14 nights",
          hint: "One strip of the last 14 nights for each app",
        },
        {
          value: "heat",
          label: "A grid of 30 nights",
          hint: "Apps against the last 30 nights",
        },
      ],
      onChange: (v) => {
        backupView = v === "heat" ? "heat" : "strips";
        try {
          localStorage.setItem("homelab.hub.backups", backupView);
        } catch {
          // kept for this visit only
        }
        draw();
      },
      mark: (b, v) => void drivable(b, BACKUP_VIEW, `${name}/${v}`),
    });
    const readout = h("p", { class: "hb-readout", role: "status" });
    body.replaceChildren(
      h(
        "div",
        { class: "hb-ctl" },
        h("b", { class: "hb-ctl__label" }, "View"),
        views.el,
      ),
      strips,
      heat,
      readout,
    );
    // kp's heat legend: the tones of both views (strips and grid).
    setHeatLegend(heat, [
      { tone: "ok", label: "backed up" },
      { tone: "bad", label: "missed" },
      { tone: "before", label: "before the oldest kept backup" },
    ]);
    const draw = () => {
      views.set(backupView);
      strips.hidden = backupView !== "strips";
      heat.hidden = backupView !== "heat";
      if (backupView === "heat")
        setHeat(heat, {
          label: "Backups per app per night",
          columns: long[0].cells.map((x, i) => ({
            key: x.night,
            label: x.night.slice(8, 10) + "/" + x.night.slice(5, 7),
            minor: i % 2 === 1,
          })),
          every: 5,
          rows: long.map((r) => ({
            key: r.app,
            label: r.app,
            cells: r.cells.map((x) => ({
              tone:
                x.state === "ok" ? "ok" : x.state === "miss" ? "bad" : "before",
              label:
                x.state === "ok"
                  ? "backed up"
                  : x.state === "miss"
                    ? "missed"
                    : "before the oldest kept backup",
              tip: `${r.app}, ${dayLabel(x.night)}`,
            })),
          })),
        });
      else
        for (const r of rows) {
          const el = strips.querySelector(
            `[data-hb-app="${CSS.escape(r.app)}"]`,
          );
          if (!(el instanceof HTMLElement)) continue;
          setUptime(el, {
            label: r.app,
            periods: r.cells.map((x) => ({
              key: x.night,
              label: dayLabel(x.night),
              state:
                x.state === "ok" ? "up" : x.state === "miss" ? "down" : "none",
              ...(x.state === "miss"
                ? {
                    details: [
                      `Missed on ${dayLabel(x.night)}: no snapshot was written that night`,
                      `Restore would open on ${dayLabel(addDays(x.night, -1))}`,
                    ],
                  }
                : x.state === "before" ||
                    x.state === "wait" ||
                    x.state === "none"
                  ? {
                      tip: `${dayLabel(x.night)}: ${x.state === "wait" ? "tonight's round has not run yet" : "before the oldest kept backup: nothing to restore, nothing went wrong"}`,
                    }
                  : {}),
            })),
            from: "14 nights ago",
            to: "Last night",
            open: "info",
          });
        }
    };
    strips.addEventListener(UPTIME_PICK_EVENT, (e) => {
      const d = /** @type {CustomEvent} */ (e).detail;
      const app = /** @type {HTMLElement} */ (e.target)
        .closest("[data-hb-app]")
        ?.getAttribute("data-hb-app");
      if (d.period?.state === "down" && app) {
        readout.textContent = `Restore would open on ${dayLabel(addDays(d.period.key, -1))} for ${app}.`;
        const go = h(
          "button",
          { type: "button", class: "kp-button kp-button--sm" },
          "Restore from the night before",
        );
        drivable(go, BACKUP_RESTORE_NIGHT, `${name}/${app}`);
        go.addEventListener("click", () => openRestore(app, d.period.key));
        readout.append(" ", go);
      } else readout.textContent = "";
    });
    heat.addEventListener(HEAT_PICK_EVENT, (e) => {
      const d = /** @type {CustomEvent} */ (e).detail;
      const cell = long
        .find((r) => r.app === d.rowKey)
        ?.cells.find((x) => x.night === d.colKey);
      if (d.picked && cell?.state === "miss") openRestore(d.rowKey, d.colKey);
    });
    draw();
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
    detachCards();
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
  const searchBox = /** @type {HTMLInputElement} */ (
    h("input", {
      type: "search",
      class: "kp-field__input kp-toolbar__input",
      placeholder: "Find: action, app, error",
      "aria-label": "Find in history",
      "aria-keyshortcuts": "/",
    })
  );
  searchBox.value = q;
  searchBox.addEventListener("input", () => {
    q = searchBox.value;
    writeUrl();
    paint();
  });
  const tb = h(
    "div",
    {
      class: "kp-toolbar",
      "data-kp-toolbar": "",
      "data-kp-toolbar-key": "/",
      role: "group",
      "aria-label": "History",
    },
    h(
      "div",
      { class: "kp-toolbar__search" },
      searchBox,
      h("kbd", { class: "kp-kbd kp-toolbar__key", "aria-hidden": "true" }, "/"),
    ),
    h(
      "div",
      { class: "kp-toolbar__filters" },
      h(
        "div",
        {
          class: "kp-toolbar__group",
          role: "group",
          "aria-labelledby": "hist-by-l",
        },
        h("b", { class: "kp-toolbar__label", id: "hist-by-l" }, "Started by"),
        h("span", { class: "nx-chips" }, chips),
      ),
    ),
    h("div", { class: "kp-toolbar__state" }, count),
  );
  drivable(searchBox, HISTORY_SEARCH, name);
  const list = historyFeedEl("day");
  const detachFeed = attachFeeds(list);
  setFeed(list, { state: "loading" });
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
    tb,
    list,
  );
  const detachTb = attachToolbars(tb);
  const incList = h("ul", { class: "sh-feed" });
  const incStage = stage(incList);
  setCardState(incStage, "loading", { words: "Reading the incidents" });
  const incidents = section({
    title: "Incidents",
    desc: "Each failed operation on this stack kept a bundle of what it saw; Show opens it.",
  });
  incidents.body.append(incStage);
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
      setFeed(list, {
        state: "error",
        error: `${failed.why}${failed.fix ? ` — ${failed.fix}` : ""}`,
      });
      count.textContent = "";
      return;
    }
    if (!rows) return;
    const shown = filterFeed(rows, who, q);
    count.textContent = `${shown.length === rows.length ? rows.length : `${shown.length} of ${rows.length}`} ${rows.length === 1 ? "operation" : "operations"}, ${HISTORY_DAYS} days`;
    countOf(count, list, ".kp-feed__row");
    setFeed(list, {
      state: shown.length ? "ready" : "empty",
      rows: feedRows(shown),
      emptyText: rows.length
        ? "Nothing matches: clear the search or turn a chip off."
        : `Nothing was done with ${name} in the last ${HISTORY_DAYS} days.`,
    });
  };
  const stopEsc = registerShortcuts("page:stack-history", [
    {
      keys: "Escape",
      what: "Show everyone's changes again",
      group: GROUP_STACK,
      when: () => who.size > 0,
      reason: "Nobody is picked",
      run: unlessDialog(() => {
        if (!who.size) return false;
        who.clear();
        for (const b of chips) b.setAttribute("aria-pressed", "false");
        writeUrl();
        paint();
        return undefined;
      }),
    },
  ]);
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
      c.setCount("history", names.length);
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
                h("span", { class: "sh-feed__what" }, h("b", null, x.op)),
                h(
                  "span",
                  { class: "sh-feed__end" },
                  x.at ? whenText(x.at) : "",
                  b,
                ),
              );
            })
          : []),
      );
      if (names.length) setCardState(incStage, "ready");
      else
        setCardState(incStage, "empty", {
          title: "No incidents",
          text: noIncidentsText(name),
        });
    } else
      setCardState(incStage, "error", {
        title: "The incidents did not read",
        text: ir.error.why,
      });
  };
  const reload = () => void load().catch(() => {});
  const detachCards = attachCardStates(panel);
  const retry = () => {
    setFeed(list, { state: "loading" });
    setCardState(incStage, "loading", { words: "Reading the incidents" });
    reload();
  };
  list.addEventListener(FEED_RETRY_EVENT, retry);
  incStage.addEventListener(CARD_RETRY_EVENT, retry);
  const stopAnswers = onAnswered(reload);
  reload();
  return () => {
    stopAnswers();
    updatesAbort.abort();
    detachCards();
    detachFeed();
    detachTb();
    stopEsc();
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
  const query = new URLSearchParams(location.search);
  // `?file=` names a file of the Files card, so it opens that card.
  const want = query.get("section") ?? (query.get("file") ? "files" : null);
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

  // The facts on kp's form grid: the names on one edge, the values on the next.
  const sizeKv = h("dl", { class: "kp-kv" });
  const sizeStage = stage(h("div", { class: "kp-formgrid" }, sizeKv));
  setCardState(sizeStage, "loading", { words: "Reading the stack's files" });
  const editBtn = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm",
        title:
          "Open the editor at the container's settings (a change is saved as a commit and needs a deploy)",
      },
      "Edit",
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
  size.body.append(sizeStage);
  size.el.classList.add("sh-span-6");

  const fileUl = h("ul", { class: "sh-feed sh-files" });
  const fileStage = stage(fileUl);
  setCardState(fileStage, "loading", { words: "Reading the stack's files" });
  const editorHost = h("div", { class: "sh-editor" });
  const editor = /** @type {HTMLDetailsElement} */ (
    h(
      "details",
      {
        class: "sh-fold kp-accordion__item kp-accordion__item--bare",
        open: want === "files" || drive ? true : null,
      },
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
    "Open the editor",
  );
  drivable(openEditor, OPEN_EDITOR, `${name}/files`);
  const files = section({
    id: "files",
    title: "Files",
    desc: "The stack's files in the repository — the truth a deploy makes real. Edit, review the change, commit.",
    tools: [openEditor],
  });
  files.body.append(fileStage, editor);
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
      {
        class: "sh-fold kp-accordion__item kp-accordion__item--bare",
        open: want === "firewall" || drive ? true : null,
      },
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
        class:
          "kp-card nx-card nx-card--fold sh-danger sh-span-12 kp-accordion__item kp-accordion__item--bare",
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
      setCardState(sizeStage, "error", {
        title: "The stack's files do not read",
        text: `${S.editError ?? "Its stack file is missing"}.`,
        retry: false,
      });
    } else {
      sizeKv.replaceChildren(
        ...sizeFacts({ manifest: e.manifest, diskPct }).map((f) =>
          h(
            "div",
            { class: "kp-kv__row" },
            h("dt", null, f.label),
            h("dd", { class: f.mono ? "mono" : null }, f.value),
          ),
        ),
      );
      setCardState(sizeStage, "ready");
    }
    const paths = Object.keys(e?.texts ?? {});
    fileUl.replaceChildren(
      ...fileList(paths).map((f) =>
        h(
          "li",
          null,
          h("span", { class: "sh-files__icon", "aria-hidden": "true" }, "▤"),
          h(
            "span",
            { class: "sh-feed__what" },
            h("b", { class: "mono" }, f.path),
            f.what ? h("span", { class: "sh-feed__detail" }, f.what) : null,
          ),
          h("span", null),
        ),
      ),
    );
    if (paths.length) setCardState(fileStage, "ready");
    else
      setCardState(fileStage, "empty", {
        title: "No files",
        text: e?.head
          ? `The repository holds no files for ${name}.`
          : "The working copy has no commit yet.",
      });
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
  stops.push(attachCardStates(panel));
  return () => {
    for (const s of stops.reverse()) s();
  };
}

// ── the hub's own blocks (stackkit.js until the 3.71.0 integration; ui.js
// has no hub header, grouped action menu, log side filters or night strip)

/**
 * The hub's header (flows/stack-hub.html) on kp-themes' `.kp-page-header`:
 * the stack's identity mark, its name, state and live status on the title
 * row, the one-sentence description under it and the meta chips; the
 * actions against the right edge (Back up · Update · Deploy, the primary
 * one, then More). Markup the whole-screen invariants read: `.kp-page-header__line`
 * holding the h1, the description its next sibling `p`, the buttons in
 * `.sh-head__actions`.
 * @param {{name: string, desc: string, actions: Node[], primary: Node,
 *   more: Node}} spec
 */
function hubHeader(spec) {
  const title = /** @type {HTMLHeadingElement} */ (
    h("h1", { class: "kp-page-header__title" }, spec.name)
  );
  const state = h("span", { class: "sh-head__state", id: "stack-state" });
  const live = liveStatus("updated");
  // kp-themes' page header: the actions against the right edge beside the
  // title, description and chips.
  const actions = h(
    "div",
    { class: "kp-page-header__actions nx-head-actions sh-head__actions" },
    spec.actions,
    spec.primary,
    spec.more,
  );
  const mark = h("span", {
    class: "kp-idmark kp-idmark--lg sh-head__mark kp-page-header__lead",
  });
  setIdmark(mark, spec.name);
  // redesign-final X1: the freshness right after the title and its state.
  const row = h(
    "div",
    { class: "kp-page-header__heading sh-head__row" },
    mark,
    h("div", { class: "kp-page-header__line" }, title, state, live.el),
  );
  const desc = /** @type {HTMLParagraphElement} */ (
    h(
      "p",
      {
        class: "kp-page-header__description section-head__desc nx-head-desc",
      },
      spec.desc,
    )
  );
  const meta = h("div", { class: "sh-head__meta", id: "stack-flags" });
  const e = h(
    "header",
    { class: "kp-page-header nx-head sh-head" },
    h(
      "div",
      { class: "kp-page-header__inner" },
      h("div", { class: "nx-head-lead" }, row, desc, meta),
      actions,
    ),
  );
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
 * redesign-flows-6 (review item 4, the demo's "Roll back, 1 click, for 7
 * days, from the stack's History"): the updates of this stack the Update
 * flow made in the last 7 days, each pinned app with its Roll back….
 * @param {string} stack
 * @param {AbortSignal} signal
 */
function updatesCard(stack, signal) {
  ensureStyle("/css/pages/update.css");
  const list = h("div", { class: "undo-updates__list" });
  const listStage = stage(list);
  setCardState(listStage, "loading", {
    words: "Reading the updates of the last 7 days",
  });
  const detachStates = attachCardStates(listStage);
  signal.addEventListener("abort", detachStates, { once: true });
  const card = section({
    id: "undo-updates",
    title: "Updates you can undo",
    desc: "Each app the Update flow moved in the last 7 days; Roll back puts its earlier version back the same way — backup, files, deploy.",
    level: "h3",
    cls: "undo-updates",
  });
  card.body.append(listStage);
  void (async () => {
    const r = await fetchJson(
      `/data/update-flows?stack=${encodeURIComponent(stack)}`,
      "the updates of the last 7 days",
      signal,
    );
    if (signal.aborted) return;
    if (!r.ok) {
      setCardState(listStage, "error", {
        title: "The updates of the last 7 days did not read",
        text: r.error.why,
        retry: false,
      });
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
              `Roll back to ${m.from_version}`,
            ),
            UNDO_UPDATE,
            `${stack}/${m.key}`,
          );
          b.addEventListener("click", () => openPinRollback(m));
          return h(
            "div",
            { class: "undo-updates__row" },
            h("strong", null, m.key),
            h("span", {
              class: "kp-badge kp-badge--version",
              "data-kp-version-from": m.from_version,
              "data-kp-version-to": m.to_version,
              "data-kp-version-app": m.key.split("/")[1] ?? m.key,
            }),
            h(
              "span",
              { class: "measured" },
              `${formatDateTime(u.at)} · ${u.by}`,
            ),
            b,
          );
        }),
    );
    if (!rows.length) {
      setCardState(listStage, "empty", {
        title: "Nothing to undo",
        text: "No app of this stack was updated in the last 7 days.",
      });
      return;
    }
    list.replaceChildren(...rows);
    attachVersionDeltas(list);
    setCardState(listStage, "ready");
  })().catch(() => {});
  return card.el;
}
