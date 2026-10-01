// One stack's page (feat-stacks-1) with its tabs: overview, apps, history,
// logs and checks. The tab is in the path (/app/stacks/media/logs), what a
// tab filters on is in the query string (feat-overview-8).

import { mountActionsArea } from "../actionsarea.js";
import {
  checksEditTab,
  firewallTab,
  openPublishDialog,
  settingsTab,
} from "../editpanels.js";
import { historyRows, incidentRows } from "../activity.js";
import { agoEl, setAgo } from "../ago.js";
import { checkRows } from "../checks.js";
import { answerButton, onAnswered } from "../answer.js";
import { showButton } from "../incident.js";
import {
  badgeCell,
  bindTableUrl,
  fetchJson,
  fetchReport,
  fillFacts,
  h,
  tabRow,
  tableBlock,
  td,
} from "../dom.js";
import { stackDetail } from "../fleet.js";
import { badge } from "../actui.js";
import { driftFact, stackFlags } from "../parity.js";
import { formatDateTime, humanDuration } from "../format.js";
import {
  JOURNAL,
  SINCE,
  appChoices,
  logRows,
  logSettings,
  logsUrl,
} from "../logs.js";
import { STACK_TABS, stackHref } from "../router.js";
import { sortKeys } from "../sortkeys.js";
import { stackChecks, stackEntries, stackIncidents } from "../stacktabs.js";
import { current, subscribe } from "../store.js";
import { setParams } from "../urlstate.js";
import {
  attachDataTables,
  compare,
  dataTable,
} from "/static/kp/js/datatable.js";
import { attachLogs } from "/static/kp/js/log.js";
import { watchTabOverflow } from "/static/kp/js/overlays.js";

/** How far back the history tab reads. */
const HISTORY_DAYS = 30;

/**
 * @typedef {{name: string, tab: import("../router.js").StackTab,
 *   navigate: (href: string) => void}} Params
 */

/**
 * @param {HTMLElement} root
 * @param {Params} params
 * @returns {() => void}
 */
export function mount(root, params) {
  const title = h("h1", null, `Stack ${params.name}`);
  const state = h("span", { class: "state" });
  // TUI parity: [OFF] [CHANGED] [NOENV], as the TUI flags a stack.
  const flags = h("span", { class: "stack-flags", id: "stack-flags" });
  /** @type {any} */
  let drift = null;
  const missing = h("p", { class: "kp-alert kp-alert--warning", hidden: "" });
  const panel = h("div", {
    class: "kp-tabs__panel",
    role: "tabpanel",
    id: "stack-panel",
    "aria-label": STACK_TABS.find((t) => t.tab === params.tab)?.label ?? "",
  });
  const tabs = tabRow(
    `Stack ${params.name}`,
    STACK_TABS.map((t) => ({
      href: stackHref(params.name, t.tab),
      label: t.label,
      current: t.tab === params.tab,
    })),
  );
  root.replaceChildren(
    h("p", { class: "crumb" }, h("a", { href: "/app/" }, "← Overview")),
    h("div", { class: "title-row" }, title, h("span", null, state, flags)),
    missing,
    tabs,
    panel,
  );

  // The header is live on every tab: the state badge, or the note that the
  // host has no such stack.
  const header = () => {
    const f = current().fleet;
    if (!f) return;
    const d = stackDetail(f, params.name);
    missing.hidden = d != null;
    if (!d) {
      missing.textContent = `The host's fleet has no stack called "${params.name}" (a stack committed but not deployed yet has only its Settings and Firewall tabs).`;
      state.replaceChildren();
      return;
    }
    state.className = `state ${d.state.tone}`;
    state.replaceChildren(h("span", null, d.state.label));
    const s = f.stacks.find((x) => x.name === params.name);
    flags.replaceChildren(
      ...(s ? stackFlags(s, drift?.state) : []).map((x) => {
        const b = badge({ label: `[${x.label}]`, tone: x.tone });
        b.title = x.title;
        return b;
      }),
    );
  };
  const unsub = subscribe(header);
  header();
  // Drift runs latch per stack on the dashboard: the newest reading is
  // shown, and a new one is made only when asked ("Compare with the files").
  const driftAbort = new AbortController();
  /** @param {boolean} fresh */
  const readDrift = async (fresh) => {
    const r = await fetchJson(
      `/data/drift${fresh ? "?fresh=1" : ""}`,
      "drift",
      driftAbort.signal,
    );
    if (!r.ok) return;
    drift = r.body.stacks?.[params.name] ?? null;
    header();
    document.dispatchEvent(new CustomEvent("stack-drift", { detail: drift }));
  };
  const onCompare = () => void readDrift(true).catch(() => {});
  document.addEventListener("stack-drift-compare", onCompare);
  void readDrift(false).catch(() => {});
  // A row wider than a phone scrolls, the open tab kept in view (kp-themes).
  const stopOverflow = watchTabOverflow(
    tabs,
    () => tabs.querySelector('[aria-selected="true"]') ?? undefined,
  );

  /** @type {Record<string, (p: HTMLElement, params: Params) => () => void>} */
  const panels = {
    overview: overviewTab,
    apps: appsTab,
    history: historyTab,
    logs: logsTab,
    checks: checksTab,
    settings: settingsTab,
    firewall: firewallTab,
  };
  const stop = panels[params.tab](panel, params);
  return () => {
    driftAbort.abort();
    document.removeEventListener("stack-drift-compare", onCompare);
    unsub();
    stopOverflow();
    stop();
  };
}

/** @type {(p: HTMLElement, params: Params) => () => void} */
function overviewTab(panel, params) {
  const facts = h("dl", { class: "facts" });
  const ago = agoEl("measured", null, { live: true });
  const counts = h("p", null);
  // feat-stacks-4: every action on this stack, and its running jobs.
  const actions = mountActionsArea({ stack: params.name });
  const enc = encodeURIComponent(params.name);
  const links = h(
    "p",
    { class: "actions-row stack-links" },
    h(
      "a",
      {
        class: "kp-button",
        href: `/data/download/export/${enc}`,
        download: `${params.name}-bundle.yml`,
      },
      "Export bundle",
    ),
  );
  const shellLink = h(
    "a",
    { class: "kp-button", href: "/app/shell" },
    "Open the shell",
  );
  links.append(shellLink);
  const compareBtn = h(
    "button",
    { type: "button", class: "kp-button", id: "drift-compare" },
    "Compare with the files",
  );
  compareBtn.addEventListener("click", () =>
    document.dispatchEvent(new CustomEvent("stack-drift-compare")),
  );
  links.append(compareBtn);
  panel.replaceChildren(
    h("section", { class: "kp-card", "aria-label": "Stack" }, facts),
    counts,
    h("p", null, ago),
    links,
    actions.element,
  );
  /** @type {any} */
  let drift = null;
  const onDrift = (/** @type {Event} */ e) => {
    drift = /** @type {CustomEvent} */ (e).detail;
    render();
  };
  document.addEventListener("stack-drift", onDrift);
  const render = () => {
    const f = current().fleet;
    const d = stackDetail(f, params.name);
    if (!f || !d) return;
    const s = f.stacks.find((x) => x.name === params.name);
    shellLink.setAttribute("href", `/app/shell?vmid=${s?.vmid ?? ""}`);
    fillFacts(facts, [
      ...d.facts,
      {
        label: "Env",
        value:
          s?.env_sealed === false
            ? "[NOENV] the host holds no sealed env: a deploy fails closed"
            : "sealed on the host",
      },
      { label: "Drift", value: driftFact(drift ?? undefined).label },
    ]);
    const down = d.apps.filter((a) => a.running !== "running").length;
    counts.replaceChildren(
      `${d.apps.length} apps, ${down} not running. `,
      h("a", { href: stackHref(params.name, "apps") }, "See the apps"),
      " · ",
      h("a", { href: stackHref(params.name, "logs") }, "Read the logs"),
    );
    setAgo(ago, f.measured_at);
  };
  const unsub = subscribe(render);
  render();
  return () => {
    unsub();
    document.removeEventListener("stack-drift", onDrift);
    actions.stop();
  };
}

/** @type {(p: HTMLElement, params: Params) => () => void} */
function appsTab(panel, params) {
  const ago = agoEl("measured", null, { live: true });
  const t = tableBlock({
    remember: "stack-apps",
    caption: "Apps",
    search: "Search apps",
    state: "loading",
    nothing: "This stack declares no apps.",
    columns: [
      { label: "App", sort: "text" },
      {
        label: "Running",
        sort: "text",
        order: "stopped,running",
        filter: "choice",
      },
      { label: "Restarts", sort: "number" },
      { label: "Logs", sort: "text" },
      { label: "Publish", sort: "text" },
    ],
  });
  panel.replaceChildren(t.wrap, h("p", null, ago));
  const detach = attachDataTables(panel);
  const table = dataTable(t.wrap);
  const unbind = bindTableUrl(table, "apps");
  t.loading({ words: "Waiting for the host's report of the fleet…" });
  const render = () => {
    const f = current().fleet;
    const d = stackDetail(f, params.name);
    if (!f || !d) return;
    t.tbody.replaceChildren(
      ...d.apps.map((a) => {
        // feat-publish-1 (B): a hostname and a port, turned into
        // gateway_route/extra_routes + traefik-routes.yml and optionally a
        // tile, for this one app.
        const publish = h(
          "button",
          { type: "button", class: "kp-button kp-button--small" },
          "Publish…",
        );
        publish.addEventListener("click", () => {
          openPublishDialog(params.name, a.name, () => void render());
        });
        return h(
          "tr",
          null,
          td(a.name),
          badgeCell({ label: a.running, tone: a.tone }),
          td(a.restarts, "num"),
          h(
            "td",
            null,
            h(
              "a",
              {
                href: `${stackHref(params.name, "logs")}?app=${encodeURIComponent(a.name)}`,
              },
              `${a.name} logs`,
            ),
          ),
          h("td", null, publish),
        );
      }),
    );
    t.ready();
    setAgo(ago, f.measured_at);
  };
  const unsub = subscribe(render);
  render();
  return () => {
    unsub();
    unbind();
    detach();
  };
}

/** @type {(p: HTMLElement, params: Params) => () => void} */
function historyTab(panel, params) {
  const keys = sortKeys();
  /** @param {number | null} unix */
  const time = (unix) =>
    unix == null ? "—" : keys.note("time", formatDateTime(unix), unix);
  const ago = agoEl("read");
  const hist = tableBlock({
    remember: "stack-history",
    caption: `What the host did with ${params.name}, last ${HISTORY_DAYS} days`,
    search: "Search the history",
    state: "loading",
    nothing: `The host did nothing with ${params.name} in the last ${HISTORY_DAYS} days.`,
    columns: [
      { label: "Started", sort: "time" },
      { label: "What", sort: "text" },
      { label: "Took", sort: "duration" },
      {
        label: "Outcome",
        sort: "text",
        order: "failed,running,deferred,ok,done",
        filter: "choice",
      },
      { label: "Detail", sort: "text" },
    ],
  });
  const inc = tableBlock({
    remember: "stack-incidents",
    caption: `Incidents of ${params.name}`,
    search: "Search incidents",
    state: "loading",
    nothing: `No incidents: no operation on ${params.name} has failed.`,
    columns: [
      { label: "When", sort: "time" },
      { label: "Operation", sort: "text" },
      { label: "Bundle", sort: "text" },
      { label: "Read", sort: "text" },
    ],
  });
  panel.replaceChildren(hist.wrap, inc.wrap, h("p", null, ago));
  const detach = attachDataTables(panel, { compare: keys.compare(compare) });
  const histTable = dataTable(hist.wrap);
  const incTable = dataTable(inc.wrap);
  const unbindH = bindTableUrl(histTable, "history");
  const unbindI = bindTableUrl(incTable, "incidents");
  const abort = new AbortController();
  const load = async () => {
    hist.loading({ words: "Reading the history from the host…" });
    inc.loading({ words: "Reading the incidents from the host…" });
    const since = Math.floor(Date.now() / 1000) - HISTORY_DAYS * 86400;
    const [hr, ir] = await Promise.all([
      fetchReport(`/data/history?since=${since}`, "the history", abort.signal),
      fetchReport("/data/incidents", "the incidents", abort.signal),
    ]);
    if (hr.ok) {
      const rows = historyRows(
        stackEntries(hr.report?.entries ?? [], params.name),
      );
      hist.tbody.replaceChildren(
        ...rows.map((x) =>
          h(
            "tr",
            null,
            td(time(x.start)),
            td(x.what),
            td(
              x.took == null
                ? "—"
                : keys.note("duration", humanDuration(x.took), x.took),
              "num",
            ),
            badgeCell(x.outcome),
            td(x.detail),
          ),
        ),
      );
      hist.ready();
    } else hist.failed(hr.error);
    if (ir.ok) {
      const names = stackIncidents(ir.report?.incidents ?? [], params.name);
      inc.tbody.replaceChildren(
        ...incidentRows(names).map((x) =>
          h(
            "tr",
            null,
            td(time(x.at)),
            td(x.op),
            td(x.name, "mono"),
            showButton(x.name),
          ),
        ),
      );
      inc.ready();
    } else inc.failed(ir.error);
    setAgo(ago, Date.now() / 1000);
  };
  const retry = () => void load().catch(() => {});
  panel.addEventListener("kp-datatable-retry", retry);
  const stopAnswers = onAnswered(retry);
  retry();
  return () => {
    abort.abort();
    stopAnswers();
    panel.removeEventListener("kp-datatable-retry", retry);
    unbindH();
    unbindI();
    detach();
  };
}

/** @type {(p: HTMLElement, params: Params) => () => void} */
function logsTab(panel, params) {
  const keys = sortKeys();
  let settings = logSettings(new URLSearchParams(location.search));
  const appSel = h("select", { class: "kp-field__input", id: "logs-app" });
  let appsShown = "";
  // The choices follow the fleet: a deep link can arrive before it does.
  const fillApps = () => {
    const apps =
      stackDetail(current().fleet, params.name)?.apps.map((a) => a.name) ?? [];
    const sig = apps.join(",");
    if (sig === appsShown && appSel.options.length > 0) return;
    appsShown = sig;
    appSel.replaceChildren(
      ...appChoices(apps).map((c) => h("option", { value: c.value }, c.label)),
    );
    // An app the fleet does not list (yet) is still a valid address.
    if (!new Set([...apps, JOURNAL, ""]).has(settings.app))
      appSel.append(h("option", { value: settings.app }, settings.app));
    appSel.value = settings.app;
  };
  fillApps();
  const unsubApps = subscribe(fillApps);
  const sinceSel = h(
    "select",
    { class: "kp-field__input", id: "logs-since" },
    ...SINCE.map((c) => h("option", { value: c.value }, c.label)),
  );
  sinceSel.value = settings.since;
  const text = h("input", {
    class: "kp-field__input",
    id: "logs-q",
    type: "search",
    placeholder: "Only lines containing…",
  });
  text.value = settings.q;
  const follow = h("input", {
    class: "kp-field__check",
    type: "checkbox",
    id: "logs-follow",
  });
  follow.checked = settings.follow;
  const refresh = h(
    "button",
    { type: "button", class: "kp-button" },
    "Refresh",
  );
  const ago = agoEl("read");
  const logql = h("p", { class: "measured mono", id: "logs-logql" });
  const t = tableBlock({
    remember: "logs",
    pageSize: 100,
    pageSizes: "50,100,250,1000",
    caption: `Logs of ${params.name}`,
    search: "Search these lines",
    state: "loading",
    nothing:
      "Loki holds no lines for this choice: widen the window or pick another app.",
    columns: [
      { label: "Time", sort: "time" },
      { label: "App", sort: "text", filter: "choice" },
      {
        label: "Level",
        sort: "text",
        order: "critical,error,warn,info,debug,trace,—",
        filter: "choice",
      },
      { label: "Line", sort: "text", cls: "wide" },
    ],
  });
  const field = (
    /** @type {string} */ label,
    /** @type {HTMLElement} */ control,
  ) =>
    h(
      "div",
      { class: "kp-field logs-field" },
      h("label", { class: "kp-field__label", for: control.id }, label),
      control,
    );
  panel.replaceChildren(
    h(
      "form",
      { class: "logs-controls", role: "search", "aria-label": "Which logs" },
      field("App", appSel),
      field("Window", sinceSel),
      field("Lines containing", text),
      h("label", { class: "logs-follow" }, follow, " Follow (every 5 s)"),
      refresh,
    ),
    t.wrap,
    h("p", null, ago),
    logql,
  );
  const detach = attachDataTables(panel, { compare: keys.compare(compare) });
  const table = dataTable(t.wrap);
  const unbind = bindTableUrl(table, "logs");
  let abort = new AbortController();
  /** @type {ReturnType<typeof setInterval> | null} */
  let timer = null;

  /** @param {boolean} [quiet] Follow's reads every 5 s refresh quietly */
  const load = async (quiet = false) => {
    abort.abort();
    abort = new AbortController();
    if (!quiet) t.loading({ words: "Asking Loki for the lines…" });
    const r = await fetchJson(
      logsUrl(params.name, settings),
      "the logs",
      abort.signal,
    );
    if (!r.ok) {
      t.failed(r.error);
      return;
    }
    const rows = logRows(r.body.lines ?? []);
    t.tbody.replaceChildren(
      ...rows.map((x) => {
        const src = h(
          "td",
          null,
          h("span", { "data-kp-source": x.source }, x.source),
        );
        return h(
          "tr",
          null,
          td(keys.note("time", x.time, x.ms), "num"),
          src,
          x.tone ? badgeCell({ label: x.level, tone: x.tone }) : td(x.level),
          td(x.line, "mono logline"),
        );
      }),
    );
    attachLogs(t.tbody);
    t.ready();
    logql.textContent = `${rows.length} lines · LogQL: ${r.body.logql}`;
    setAgo(ago, r.body.measured_at ?? Date.now() / 1000);
  };

  const apply = () => {
    settings = {
      app: appSel.value,
      since: sinceSel.value,
      q: text.value.trim(),
      follow: follow.checked,
    };
    const search = setParams(location.search, {
      app: settings.app,
      since: settings.since === "3600" ? null : settings.since,
      q: settings.q,
      follow: settings.follow ? "1" : null,
    });
    history.replaceState(history.state, "", location.pathname + search);
    if (timer) clearInterval(timer);
    timer = settings.follow
      ? setInterval(() => void load(true).catch(() => {}), 5000)
      : null;
    void load().catch(() => {});
  };
  appSel.addEventListener("change", apply);
  sinceSel.addEventListener("change", apply);
  follow.addEventListener("change", apply);
  text.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      apply();
    }
  });
  refresh.addEventListener("click", apply);
  panel.addEventListener("kp-datatable-retry", apply);
  apply();
  return () => {
    abort.abort();
    unsubApps();
    if (timer) clearInterval(timer);
    panel.removeEventListener("kp-datatable-retry", apply);
    unbind();
    detach();
  };
}

/**
 * feat-checks-1 (B): this tab used to only read `checks.yml` (the manual
 * answers table below); it now also edits it, one app at a time, above
 * that table.
 * @type {(p: HTMLElement, params: Params) => () => void}
 */
function checksTab(panel, params) {
  const editHost = h("div");
  const tableHost = h("div");
  panel.replaceChildren(editHost, tableHost);
  const stopEdit = checksEditTab(editHost, params);
  const keys = sortKeys();
  /** @param {number | null} unix */
  const time = (unix) =>
    unix == null ? "never" : keys.note("time", formatDateTime(unix), unix);
  const ago = agoEl("read");
  const t = tableBlock({
    remember: "stack-checks",
    caption: `Manual checks of ${params.name}`,
    search: "Search checks",
    state: "loading",
    nothing: `No manual checks are registered for ${params.name}.`,
    columns: [
      { label: "App", sort: "text" },
      {
        label: "Answer",
        sort: "text",
        order: "not ok,open,accepted,ok",
        filter: "choice",
      },
      { label: "Question", sort: "text", cls: "wide" },
      { label: "Answered", sort: "time" },
      { label: "Note", sort: "text" },
      { label: "Answer", sort: "text" },
    ],
  });
  tableHost.replaceChildren(t.wrap, h("p", null, ago));
  const detach = attachDataTables(tableHost, {
    compare: keys.compare(compare),
  });
  const table = dataTable(t.wrap);
  const unbind = bindTableUrl(table, "checks");
  const abort = new AbortController();
  const load = async () => {
    t.loading({ words: "Reading the manual checks from the host…" });
    const r = await fetchReport(
      "/data/manual-checks",
      "the manual checks",
      abort.signal,
    );
    if (!r.ok) {
      t.failed(r.error);
      return;
    }
    const now = r.report?.now ?? Math.floor(Date.now() / 1000);
    const rows = checkRows(
      stackChecks(r.report?.checks ?? [], params.name),
      now,
    );
    t.tbody.replaceChildren(
      ...rows.map((x) =>
        h(
          "tr",
          null,
          td(x.app),
          badgeCell(x.answer),
          td(x.text),
          td(time(x.answered)),
          td(x.note),
          answerButton(x.id, () => void load().catch(() => {})),
        ),
      ),
    );
    t.ready();
    setAgo(ago, Date.now() / 1000);
  };
  const retry = () => void load().catch(() => {});
  panel.addEventListener("kp-datatable-retry", retry);
  retry();
  return () => {
    stopEdit();
    abort.abort();
    panel.removeEventListener("kp-datatable-retry", retry);
    unbind();
    detach();
  };
}
