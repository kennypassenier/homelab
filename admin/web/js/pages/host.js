// The Host page (redesign-host, release 3.71.0; Kenny approved the demo
// 2026-10-03: ~/.local/share/homelab/redesign-3.71/host.html, implemented
// exactly). It lives under System in the new navigation. Top to bottom:
// the header (live "Reachable · N ms", Run host checks as the primary
// action, Open the console and Host log beside it, the rest in `···`), the
// KPI strip, then two columns — Containers, Host actions grouped by intent
// and Disk on the left (8/12), Connection, About, Templates and Host checks
// on the right (4/12) — and last host.toml as a folded card that shows
// only what the file changes.
//
// Data: the fleet store (load, disk detail, stacks' measured use),
// /data/host/guests (pct list, every 30 s), /data/ping (every 30 s: the
// chip and the latency strip), /data/templates, /data/host-settings,
// /data/disk-growth (the growth line, when Prometheus answers),
// /data/doctor on request, and the action catalog. redesign-host-4: the
// host sends how full the thin pool is, what the guests are promised on
// it, every guest's memory and CPU, its own uptime, the root disk's kind
// and whether its daemon is a signed release; a host older than 3.71.0
// does not, and the page says so instead of inventing a number.

import { act, catalogReady, onAct } from "../act.js";
import { openAction } from "../actiondialog.js";
import { agoEl, setAgo } from "../ago.js";
import { doctorRows } from "../doctor.js";
import { fetchJson, h, slowReport } from "../dom.js";
import { declare, declareField, drivable, viaForm } from "../drivable.js";
import { humanDuration } from "../format.js";
import {
  actionBlurb,
  daemonSignature,
  diskBreakdown,
  gib,
  guestTable,
  guestVisible,
  hostActionGroups,
  hostChecks,
  hostKpis,
  hostSettingsView,
  hostUptime,
  latencyBars,
  meterTone,
  rootGrowth,
  rootVolumeLine,
  settingVisible,
  versionText,
} from "../host.js";
import { finished } from "../jobs.js";
import { mountJobPanel } from "../jobpanel.js";
import { openRollback } from "../rollbackdialog.js";
import { stackHref } from "../router.js";
import { current, subscribe } from "../store.js";
import {
  attentionBand,
  keyRow,
  kpiStrip,
  moreMenu,
  pageHeader,
  section,
  segSwitch,
  sortableTable,
  swatch,
  toggleGroup,
} from "../ui.js";

/** Seconds between two readings of the container list and the ping. */
const EVERY_S = 30;
/** How many pings the latency strip keeps. */
const PINGS = 12;

// Live view (invariant 39): every control on this page that runs
// something or opens a menu is declared here; the action tiles and
// "Build a template…" reach their dialogs as catalog forms.
// review M5: every page field Live view may set is declared (drivable.js
// `declareField`); the client and the dashboard refuse any other.
const GUEST_SEARCH = declareField({
  id: "host-guest-search",
  page: "host",
  what: "search the containers by name",
});

const RUN_CHECKS = declare({
  id: "run-host-checks",
  page: "host",
  opens: "run",
  what: "read the doctor's host-level checks now (about 30 s)",
});
const READ_CHECKS = declare({
  id: "read-host-checks",
  page: "host",
  opens: "run",
  what: "the Host checks card's own Run checks",
});
const PING = declare({
  id: "ping-host",
  page: "host",
  opens: "run",
  what: "send one ping over the pinned line and time the answer",
});
const COPY_PIN = declare({
  id: "copy-host-pin",
  page: "host",
  opens: "run",
  what: "copy the pinned certificate fingerprint",
});
const MORE = declare({
  id: "host-more",
  page: "host",
  opens: "run",
  what: "open the header's menu: presets and the runbook",
});
const GUEST_FILTER = declare({
  id: "container-filter",
  page: "host",
  opens: "run",
  row: "all|running|stopped",
  what: "turn one status of the Containers table on or off (all: every row)",
});
const SETTINGS_VIEW = declare({
  id: "host-settings-view",
  page: "host",
  opens: "run",
  row: "changed|all",
  what: "show only the host settings host.toml changes, or all of them",
});
const SORT_GUESTS = declare({
  id: "sort-host-containers",
  page: "host",
  opens: "view",
  row: "id|container|status|memory|cpu|stack",
  what: "sort the Containers table by one column (Shift adds a further one)",
});
const SORT_SETTINGS = declare({
  id: "sort-host-settings",
  page: "host",
  opens: "view",
  row: "setting|group|value|default",
  what: "sort the host settings table by one column (Shift adds a further one)",
});

/** @param {"ok" | "warn" | "bad" | "live" | ""} tone */
const dot = (tone) =>
  h("span", { class: `hk-dot${tone ? ` hk-dot--${tone}` : ""}` });

/**
 * @param {HTMLElement} root
 * @param {{navigate: (href: string) => void, focus?: "checks"}} ctx `focus`: the card to open on (redesign-final-h5)
 * @returns {() => void}
 */
export function mount(root, ctx) {
  // `nx-ops`: the shared blocks in the ops-kit look the Host demo uses.
  root.classList.add("hk-page", "nx-ops");
  const abort = new AbortController();
  /** @type {(() => void)[]} */
  const stops = [];

  // ── header ────────────────────────────────────────────────────────────
  const reach = h(
    "span",
    { class: "hk-chip", id: "host-reach" },
    dot(""),
    "Checking the line…",
  );
  const daemon = h("span", { id: "host-daemon" });
  const runChecks = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--primary",
        title: "Read the doctor's host-level checks (about 30 s)",
        onclick: () => {
          checksCard.el.scrollIntoView({ block: "nearest" });
          void loadChecks().catch(() => {});
        },
      },
      "Run host checks",
    ),
    RUN_CHECKS,
  );
  const menu = moreMenu({
    label: "More: presets, download the runbook",
    items: [
      {
        label: "Presets",
        hint: "The stack recipes a new stack starts from",
        href: "/presets",
      },
      {
        label: "Download the runbook",
        hint: "The disaster-recovery runbook, as Markdown",
        href: "/data/download/runbook",
        download: "DR_RUNBOOK.md",
      },
    ],
    drive: { id: MORE },
  });
  stops.push(menu.stop);
  const head = pageHeader({
    title: "Host",
    sub: current().fleet?.host.name ?? "",
    desc: "The Proxmox machine every stack runs on: how loaded it is, what runs on it, and the actions that act on the host itself.",
    meta: [reach, daemon],
    live: "measured",
    actions: [
      h(
        "a",
        {
          class: "kp-button",
          href: "/console",
          title: "Open a root shell on the host in this browser",
        },
        "Open the console",
      ),
      h(
        "a",
        {
          class: "kp-button",
          href: "/activity?view=host-log",
          title: "Follow the host daemon's own log, live",
        },
        "Host log",
      ),
    ],
    primary: runChecks,
    more: menu.el,
  });
  const sub = /** @type {HTMLElement | null} */ (
    head.title.querySelector(".nx-head-sub")
  );

  const attention = attentionBand();
  const kpis = kpiStrip(
    hostKpis(
      current().fleet ?? {
        measured_at: 0,
        stacks: [],
        counts: { stacks: 0, online: 0, parked: 0 },
        host: {
          name: "",
          cpu_pct: null,
          ram_used_mb: 0,
          ram_total_mb: 1,
          disk_pct: 0,
        },
      },
      null,
    ),
    { label: "The host right now" },
  );

  // ── Containers ────────────────────────────────────────────────────────
  /** @type {import("../host.js").Guest[] | null} */
  let guests = null;
  let gShow = new Set();
  let gQ = "";
  const gSearch = /** @type {HTMLInputElement} */ (
    h("input", {
      class: "hk-search",
      type: "search",
      id: GUEST_SEARCH,
      placeholder: "Search containers   /",
      "aria-label": "Search containers",
      oninput: (/** @type {Event} */ e) => {
        gQ = /** @type {HTMLInputElement} */ (e.target).value;
        filterGuests();
      },
    })
  );
  const gFilter = toggleGroup({
    label: "Status",
    all: { label: "All", hint: "Every container, running or not" },
    chips: [
      {
        value: "running",
        label: "Running",
        hint: "Click to show or hide running containers",
      },
      {
        value: "stopped",
        label: "Stopped",
        hint: "Click to show or hide stopped containers",
      },
    ],
    onChange: (on) => {
      gShow = on;
      filterGuests();
    },
    drive: { id: GUEST_FILTER },
  });
  const gBody = h("tbody", { id: "host-guests" });
  const gTable = /** @type {HTMLTableElement} */ (
    h(
      "table",
      { class: "hk-tbl" },
      h(
        "thead",
        null,
        h(
          "tr",
          null,
          h("th", { class: "n" }, "ID"),
          h("th", null, "Container"),
          h("th", null, "Status"),
          h("th", { class: "hk-hide-phone" }, "Memory"),
          h(
            "th",
            { class: "n hk-hide-phone", title: "Share of one core" },
            "CPU",
          ),
          h("th", null, "Stack"),
        ),
      ),
      gBody,
    )
  );
  const gSort = sortableTable(gTable, {
    drive: { id: SORT_GUESTS },
    remember: "host-containers",
  });
  const guestsAgo = agoEl("read");
  const guestsCard = section({
    id: "host-containers",
    title: "Containers",
    desc: "Every container and VM on the host, managed by this orchestrator or not.",
    foot: [`Read with pct list every ${EVERY_S} s`, guestsAgo],
  });
  guestsCard.body.append(
    h("div", { class: "hk-filters" }, gSearch, gFilter.el),
    h("div", { class: "hk-scroll" }, gTable),
  );
  /** @param {number} n */
  const skeletonRows = (n) =>
    Array.from({ length: n }, () =>
      h(
        "tr",
        { "aria-hidden": "true" },
        Array.from({ length: 6 }, (_, i) =>
          h(
            "td",
            { class: i === 3 || i === 4 ? "hk-hide-phone" : null },
            h("span", { class: "hk-sk kp-skeleton" }),
          ),
        ),
      ),
    );
  gBody.dataset.kpState = "loading";
  gBody.setAttribute(
    "aria-label",
    "Asking the host for its containers (pct list)…",
  );
  gBody.replaceChildren(...skeletonRows(5));
  const filterGuests = () => {
    const rows = [...gBody.querySelectorAll("tr[data-vmid]")];
    if (!guests) return;
    const view = guestTable(guests, current().fleet);
    const vis = guestVisible(view, { show: gShow, q: gQ });
    for (const tr of rows) {
      const i = view.findIndex(
        (r) => String(r.vmid) === /** @type {HTMLElement} */ (tr).dataset.vmid,
      );
      /** @type {HTMLElement} */ (tr).hidden = i < 0 || !vis[i];
    }
  };
  const paintGuests = () => {
    if (!guests) return;
    const view = guestTable(guests, current().fleet);
    const running = view.filter((r) => r.running).length;
    gFilter.counts({
      all: view.length,
      running,
      stopped: view.length - running,
    });
    if (!view.length) {
      gBody.replaceChildren(
        h(
          "tr",
          { class: "hk-empty-row" },
          h("td", { colspan: 6 }, "The host lists no containers."),
        ),
      );
      return;
    }
    gBody.replaceChildren(
      ...view.map((g) =>
        h(
          "tr",
          {
            "data-vmid": g.vmid,
            "data-status": g.running ? "running" : "stopped",
          },
          h("td", { class: "n mono" }, g.vmid),
          h(
            "td",
            null,
            h(
              "div",
              { class: "hk-name" },
              h("span", { class: "mono" }, g.name),
              g.lock ? h("small", null, `lock: ${g.lock}`) : null,
            ),
          ),
          h(
            "td",
            null,
            h(
              "span",
              { class: "hk-status" },
              dot(g.running ? "ok" : g.tone === "bad" ? "bad" : ""),
              g.status,
            ),
          ),
          h(
            "td",
            { class: "hk-hide-phone", "data-sort": g.ramUsed ?? -1 },
            g.ramUsed != null && g.ramMax
              ? (() => {
                  const fill = h("i");
                  fill.style.width = `${Math.min(100, (g.ramUsed / g.ramMax) * 100)}%`;
                  return h(
                    "div",
                    { class: "hk-bar" },
                    h("span", null, fill),
                    h("span", null, `${gib(g.ramUsed)} / ${gib(g.ramMax)} GiB`),
                  );
                })()
              : h(
                  "span",
                  {
                    class: "hk-muted",
                    title: g.running
                      ? current().fleet?.host.guests_usage
                        ? "Not in the host's last reading yet"
                        : "This host version measures only the containers it manages"
                      : null,
                  },
                  "—",
                ),
          ),
          h(
            "td",
            { class: "n hk-hide-phone", "data-sort": g.cpuPct ?? -1 },
            g.cpuPct == null ? "—" : `${g.cpuPct}%`,
          ),
          h(
            "td",
            { "data-sort": g.stack ?? `~${g.kind}` },
            g.stack
              ? h("a", { class: "hk-link", href: stackHref(g.stack) }, g.stack)
              : h(
                  "span",
                  { class: "hk-tag" },
                  g.kind === "template" ? "template" : "not managed",
                ),
          ),
        ),
      ),
    );
    // A re-paint keeps the sort the person chose.
    gSort.apply();
    filterGuests();
  };
  const loadGuests = async () => {
    const r = await fetchJson(
      "/data/host/guests",
      "the host's containers",
      abort.signal,
    );
    if (!r.ok) {
      if (!guests)
        gBody.replaceChildren(
          h(
            "tr",
            null,
            h(
              "td",
              { colspan: 6 },
              h(
                "div",
                {
                  class: "kp-alert kp-alert--destructive error",
                  role: "alert",
                },
                h("strong", null, `Could not read ${r.error.what}`),
                h("p", null, `Why: ${r.error.why}`),
                r.error.fix ? h("p", null, `What to do: ${r.error.fix}`) : null,
                h(
                  "button",
                  {
                    type: "button",
                    class: "kp-button kp-button--sm",
                    onclick: () => void loadGuests().catch(() => {}),
                  },
                  "Try again",
                ),
              ),
            ),
          ),
        );
      return;
    }
    guests = r.body.guests ?? [];
    delete gBody.dataset.kpState;
    gBody.removeAttribute("aria-label");
    paintGuests();
    setAgo(guestsAgo, r.body.measured_at);
    paintKpis();
  };

  // ── Host actions ──────────────────────────────────────────────────────
  const groupsBox = h(
    "div",
    { class: "hk-act-groups", id: "host-actions" },
    h("p", { class: "hk-muted", role: "status" }, "Reading the actions…"),
  );
  const runningBox = h("div", { class: "hk-running" });
  const actionsCard = section({
    id: "host-actions-card",
    title: "Host actions",
    desc: "What the dashboard can do to the host itself; each opens a dialog that says what will happen first.",
    foot: [
      "Every action becomes a job you can follow on Activity",
      h("a", { class: "hk-link", href: "/activity" }, "Open Activity"),
    ],
  });
  actionsCard.el.classList.add("actions-area");
  actionsCard.body.append(groupsBox, runningBox);
  void catalogReady().then((catalog) => {
    if (abort.signal.aborted) return;
    if (!catalog) {
      groupsBox.replaceChildren(
        h(
          "p",
          { class: "kp-alert kp-alert--destructive", role: "alert" },
          "The dashboard did not send its action catalog; reload the page.",
        ),
      );
      return;
    }
    const entries = catalog.actions.filter((a) => a.target === "host");
    groupsBox.replaceChildren(
      ...hostActionGroups(entries).map((g) =>
        h(
          "div",
          { class: "hk-act-group", "data-group": g.title },
          h(
            "h3",
            null,
            g.title,
            g.full
              ? h(
                  "span",
                  { class: "hk-lock", title: "Needs the full-access token" },
                  "full access",
                )
              : null,
          ),
          g.actions.map((a) =>
            h(
              "button",
              {
                type: "button",
                class: `hk-act${a.destructive ? " hk-act--destructive" : ""}`,
                "data-action": a.action,
                title: `${a.label}: opens a dialog that says what will happen, then starts it`,
                onclick: () =>
                  void openAction(catalog.host_target, a.action, {
                    openRollback,
                  }),
              },
              h("b", null, a.label),
              h("span", null, actionBlurb(a.what)),
              h("em", { "aria-hidden": "true" }, "›"),
            ),
          ),
        ),
      ),
    );
  });
  /** @type {Map<number, () => void>} */
  const panels = new Map();
  const paintRunning = () => {
    const live = act.jobs.filter(
      (j) => j.stack === "_host" && !finished(j.state),
    );
    for (const j of live)
      if (!panels.has(j.job)) {
        const p = mountJobPanel(j.job);
        panels.set(j.job, p.stop);
        runningBox.prepend(p.element);
      }
  };
  stops.push(onAct("jobs", paintRunning));
  paintRunning();

  // ── Disk ──────────────────────────────────────────────────────────────
  const diskBody = h("div", { id: "host-disk" });
  const growthLine = h("span", { id: "host-growth" }, "Growth: reading…");
  const diskAgo = agoEl("read");
  const diskCard = section({
    id: "host-disk-card",
    title: "Disk",
    desc: "What fills the root disk, and the pool every container's own disk is carved from.",
    foot: [growthLine, diskAgo],
  });
  diskCard.body.append(diskBody);
  const paintDisk = () => {
    const f = current().fleet;
    const d = f ? diskBreakdown(f) : null;
    if (!d) {
      diskBody.replaceChildren(
        h(
          "p",
          { class: "hk-muted", role: "status" },
          "The host has not read its disks yet.",
        ),
      );
      return;
    }
    const colour = (/** @type {number} */ i) => `var(--chart-${(i % 5) + 1})`;
    const bar = h("div", {
      class: "hk-stack-bar",
      role: "img",
      "aria-label": `What fills the root volume: ${d.dirs.map((x) => `${x.path} ${Math.round(x.gb)} GB`).join(", ")}, ${d.freeGb} GB free`,
    });
    d.dirs.forEach((x, i) => {
      const seg = h("i", { title: `${x.path} ${x.gb.toFixed(0)} GB` });
      seg.style.width = `${x.pct}%`;
      seg.style.background = colour(i);
      bar.append(seg);
    });
    diskBody.replaceChildren(
      h(
        "div",
        { class: "hk-disk-line" },
        h(
          "span",
          null,
          h("b", null, "Root volume"),
          ` · ${f ? rootVolumeLine(f) : ""}`,
        ),
        h("span", { class: "num" }, `${d.usedPct}% used`),
      ),
      bar,
      h(
        "div",
        { class: "hk-dirs" },
        d.dirs.map((x, i) =>
          h(
            "div",
            null,
            swatch(colour(i)),
            h("span", { class: "mono" }, x.path),
            h("span", { class: "num hk-muted" }, `${x.gb.toFixed(0)} GB`),
          ),
        ),
        h(
          "div",
          null,
          swatch("var(--muted)"),
          h("span", null, "free"),
          h("span", { class: "num hk-muted" }, `${d.freeGb} GB`),
        ),
      ),
      h(
        "div",
        { class: "hk-pool" },
        h(
          "div",
          null,
          h("span", null, "Thin pool"),
          h("b", null, `${Math.round(d.poolGb)} GB`),
        ),
        h(
          "div",
          { id: "host-pool-promised" },
          h("span", null, "Promised to guests"),
          d.pool
            ? h("b", { title: d.pool.promisedNote }, d.pool.promised)
            : h(
                "b",
                { class: "hk-muted" },
                "not reported by this host version",
              ),
        ),
        h(
          "div",
          { id: "host-pool-written" },
          h("span", null, "Really written"),
          d.pool
            ? h("b", null, d.pool.written)
            : h(
                "b",
                { class: "hk-muted" },
                "not reported by this host version",
              ),
        ),
      ),
    );
    setAgo(diskAgo, f?.host.disk_detail?.measured_at ?? null);
  };
  const loadGrowth = async () => {
    const r = await fetchJson("/data/disk-growth", "disk growth", abort.signal);
    if (abort.signal.aborted) return;
    const rootGb = current().fleet?.host.disk_detail?.root_lv_size_gb ?? 0;
    const line = r.ok ? rootGrowth(r.body.rows ?? [], rootGb) : null;
    growthLine.textContent =
      line ??
      (r.ok
        ? "Growth: not enough history yet to say how fast root fills"
        : `Growth not measured: ${r.error.why}`);
  };

  // ── Connection ────────────────────────────────────────────────────────
  /** @type {number[]} */
  const pings = [];
  /** @type {any} */
  let lineFacts = null;
  let lineOk = /** @type {boolean | null} */ (null);
  const facts = h("dl", { class: "hk-facts", id: "host-line" });
  const lat = h("div", {
    class: "hk-lat",
    id: "host-lat",
    role: "img",
    "aria-label": "No ping yet",
  });
  const pingDot = dot("");
  const pingOut = h("span", { id: "host-ping-out" }, "Pinging…");
  const pingBtn = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm",
        title: "Send one ping over the pinned line and time the answer",
        onclick: () => void ping().catch(() => {}),
      },
      "Ping again",
    ),
    PING,
  );
  const pingCount = h("span", null, "last pings");
  const lineCard = section({
    id: "host-connection",
    title: "Connection",
    desc: "The pinned TLS line this dashboard holds to the host daemon.",
    foot: [pingBtn, pingCount],
  });
  lineCard.body.append(
    facts,
    lat,
    h("p", { class: "hk-ping-out", role: "status" }, pingDot, pingOut),
  );
  const copyBtn = drivable(
    h(
      "button",
      {
        type: "button",
        class: "hk-copy",
        title: "Copy the whole fingerprint",
        onclick: () => {
          const pin = lineFacts?.pin;
          if (pin)
            void navigator.clipboard?.writeText(pin).then(
              () => (copyBtn.textContent = "Copied"),
              () => {},
            );
        },
      },
      "Copy",
    ),
    COPY_PIN,
  );
  const paintLine = () => {
    const f = current().fleet;
    /** @type {[string, Node | string][]} */
    const rows = [];
    if (lineFacts) {
      rows.push([
        "Address",
        h("span", { class: "mono" }, lineFacts.address ?? "not configured"),
      ]);
      rows.push(["Set in", lineFacts.address_source ?? "—"]);
      rows.push([
        "Pinned",
        lineFacts.pin
          ? h(
              "span",
              null,
              h(
                "span",
                { class: "mono", title: lineFacts.pin },
                `${lineFacts.pin.slice(0, 26)}…`,
              ),
              copyBtn,
            )
          : "no pin compiled in",
      ]);
    }
    rows.push([
      "Host says",
      lineOk === true
        ? h("span", { class: "hk-status" }, dot("ok"), "matches the pin")
        : lineOk === false
          ? h(
              "span",
              { class: "hk-status" },
              dot("bad"),
              "no answer over the pinned line",
            )
          : f?.host.tls_fingerprint || "not asked yet",
    ]);
    facts.replaceChildren(
      ...rows.flatMap(([k, v]) => [h("dt", null, k), h("dd", null, v)]),
    );
    const l = latencyBars(pings);
    lat.setAttribute("aria-label", l.label);
    lat.replaceChildren(
      ...l.heights.map((hgt) => {
        const i = h("i");
        i.style.height = `${hgt}%`;
        return i;
      }),
    );
    pingCount.textContent = pings.length
      ? `last ${pings.length} ping${pings.length === 1 ? "" : "s"}`
      : "no ping yet";
  };
  const ping = async () => {
    pingBtn.setAttribute("disabled", "");
    pingOut.textContent = "Pinging…";
    const r = await fetchJson("/data/ping", "the ping", abort.signal);
    pingBtn.removeAttribute("disabled");
    if (!r.ok) {
      lineOk = false;
      pingOut.textContent = `No answer: ${r.error.why}`;
    } else {
      lineFacts = r.body.facts;
      lineOk = !!r.body.ok;
      pings.push(r.body.ms);
      if (pings.length > PINGS) pings.shift();
      pingOut.textContent = r.body.ok
        ? `Answered in ${r.body.ms} ms: ${r.body.message}`
        : `No answer after ${r.body.ms} ms: ${r.body.message}`;
    }
    pingDot.className = `hk-dot hk-dot--${lineOk ? "ok" : "bad"}`;
    reach.replaceChildren(
      dot(lineOk ? "live" : "bad"),
      lineOk && r.ok ? `Reachable · ${r.body.ms} ms` : "Not reachable",
    );
    attention.set(
      lineOk
        ? []
        : [
            {
              key: "unreachable",
              tone: "bad",
              title: "The host does not answer over the pinned line",
              text: pingOut.textContent ?? "",
              action: h(
                "button",
                {
                  type: "button",
                  class: "kp-button kp-button--sm",
                  onclick: () => void ping().catch(() => {}),
                },
                "Ping again",
              ),
            },
          ],
    );
    paintLine();
  };

  // ── About, Templates ──────────────────────────────────────────────────
  const about = h("dl", { class: "hk-facts", id: "host-about" });
  const aboutCard = section({
    id: "host-about-card",
    title: "About this host",
    desc: "What the machine is.",
  });
  aboutCard.body.append(about);
  const paintAbout = () => {
    const s = current();
    const f = s.fleet;
    if (!f) return;
    const d = f.host.disk_detail;
    const sig = daemonSignature(f.host);
    const up = hostUptime(f.host);
    /** @type {[string, Node | string][]} */
    const rows = [
      ["Name", f.host.name],
      [
        "Daemon",
        h(
          "span",
          { id: "host-daemon-signature", title: sig.title },
          `${versionText(s.hostVersion, s.hostBuild)} `,
          h(
            "span",
            { class: sig.tone === "ok" ? null : "hk-muted" },
            `(${sig.text})`,
          ),
        ),
      ],
      [
        "Hardware",
        `${f.host.cores_total ? `${f.host.cores_total} cores · ` : ""}${gib(f.host.ram_total_mb).replace(/\.0$/, "")} GiB RAM`,
      ],
      [
        "Root disk",
        d
          ? `${d.root_disk_device || "unknown"} · ${d.root_disk_total_gb.toFixed(0)} GB`
          : "not read yet",
      ],
      [
        "Up for",
        up ??
          h("span", { class: "hk-muted" }, "not reported by this host version"),
      ],
    ];
    about.replaceChildren(
      ...rows.flatMap(([k, v]) => [h("dt", null, k), h("dd", null, v)]),
    );
  };
  const tplList = h(
    "ul",
    { class: "hk-tpl", id: "host-templates" },
    h(
      "li",
      { role: "status" },
      h("span", { class: "hk-sk kp-skeleton" }),
      "Asking the host (pveam and the template containers)…",
    ),
  );
  const tplBuild = viaForm(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm",
        title: "Bake a fresh golden template: opens the dialog first",
        onclick: () => void openAction("_host", "template-build"),
      },
      "Build a template…",
    ),
    "template-build",
  );
  const tplCard = section({
    id: "host-templates-card",
    title: "Templates",
    desc: "The golden templates a new stack is cloned from.",
    foot: [tplBuild, "read with the page"],
  });
  tplCard.body.append(tplList);
  const loadTemplates = async () => {
    const r = await fetchJson("/data/templates", "the templates", abort.signal);
    if (!r.ok) {
      tplList.replaceChildren(
        h(
          "li",
          { class: "kp-alert kp-alert--destructive error", role: "alert" },
          `Could not read ${r.error.what}: ${r.error.why}`,
        ),
      );
      return;
    }
    const t = r.body.templates;
    tplList.replaceChildren(
      ...t.clones.map((/** @type {[number, string]} */ c) =>
        h(
          "li",
          null,
          dot("ok"),
          h(
            "span",
            null,
            h("span", { class: "mono" }, `CT ${c[0]}`),
            ` · ${c[1]}`,
          ),
        ),
      ),
      ...t.os.map((/** @type {string} */ o) =>
        h(
          "li",
          null,
          dot(""),
          h("span", { class: "mono" }, o.replace("local:vztmpl/", "")),
        ),
      ),
      ...(t.clones.length
        ? []
        : [
            h(
              "li",
              { class: "hk-muted" },
              "No golden template yet: build one.",
            ),
          ]),
    );
  };

  // ── Host checks ───────────────────────────────────────────────────────
  const checksBtn = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm kp-button--primary",
        title: "Read the doctor's host-level checks (about 30 s)",
        onclick: () => void loadChecks().catch(() => {}),
      },
      "Run checks",
    ),
    READ_CHECKS,
  );
  const checksNote = h(
    "p",
    { role: "status", id: "host-checks-note" },
    "Not read yet. Reading takes about 30 s on pve; the result stays here until you leave.",
  );
  const checksList = h("ul", {
    class: "hk-checks",
    id: "host-checks",
    hidden: true,
  });
  const checksCard = section({
    id: "host-checks-card",
    title: "Host checks",
    desc: "The doctor's checks about the host itself: disk, state file, clock, certificates.",
  });
  checksCard.body.append(
    h("div", { class: "hk-checks-empty" }, checksNote, checksBtn),
    checksList,
  );
  /** @param {any} report */
  const paintChecks = (report) => {
    const rows = doctorRows({ ...report, checks: hostChecks(report) });
    checksList.hidden = false;
    checksList.replaceChildren(
      ...rows.map((x) =>
        h(
          "li",
          { "data-health": x.health.label },
          dot(x.health.tone),
          h("span", null, h("b", null, x.name), ` · ${x.health.label}`),
          h("small", null, x.detail),
        ),
      ),
    );
    return rows.length;
  };
  /** The note with the one spinner a running read shows. @param {string} t */
  const busyNote = (t) =>
    checksNote.replaceChildren(
      h("span", { class: "kp-spinner", "aria-hidden": "true" }),
      " ",
      t,
    );
  let checking = false;
  const loadChecks = async () => {
    if (checking) return;
    checking = true;
    runChecks.setAttribute("disabled", "");
    checksBtn.setAttribute("disabled", "");
    // Invariant 55 (a slow read): one loading indicator while it runs.
    busyNote("The doctor is running on the host (about 30 s)…");
    const began = Date.now();
    try {
      const r = await slowReport(
        "/data/doctor",
        "the doctor",
        abort.signal,
        (b) => {
          const n = paintChecks(b.report);
          busyNote(
            `${n} host-level checks, the last reading, while the host runs the doctor again.`,
          );
        },
      );
      const secs = Math.round((Date.now() - began) / 1000);
      if (!r.ok) {
        checksNote.textContent = `The doctor did not answer (after ${humanDuration(secs)}): ${r.error.why}`;
        return;
      }
      const n = paintChecks(r.report);
      checksNote.textContent = `${n} host-level checks, read in ${humanDuration(secs)}.`;
    } finally {
      checking = false;
      runChecks.removeAttribute("disabled");
      checksBtn.removeAttribute("disabled");
    }
  };

  // ── Host settings (host.toml) ─────────────────────────────────────────
  /** @type {import("../host.js").SettingRow[]} */
  let sRows = [];
  let sAll = false;
  let sQ = "";
  const sDesc = h(
    "span",
    null,
    "host.toml as the host reads it. Read-only here; Settings changes them.",
  );
  const sView = segSwitch({
    label: "Which settings",
    value: "changed",
    items: [
      {
        value: "changed",
        label: "Changed",
        hint: "Only the settings that differ from their default",
      },
      { value: "all", label: "All", hint: "Every setting host.toml knows" },
    ],
    onChange: (v) => {
      sAll = v === "all";
      filterSettings();
    },
    drive: { id: SETTINGS_VIEW },
  });
  const sSearch = h("input", {
    class: "hk-search",
    type: "search",
    placeholder: "Search settings",
    "aria-label": "Search settings",
    oninput: (/** @type {Event} */ e) => {
      sQ = /** @type {HTMLInputElement} */ (e.target).value;
      filterSettings();
    },
  });
  const sBody = h("tbody", { id: "host-settings" });
  const sTable = /** @type {HTMLTableElement} */ (
    h(
      "table",
      { class: "hk-tbl" },
      h(
        "thead",
        null,
        h(
          "tr",
          null,
          h("th", null, "Setting"),
          h("th", { class: "hk-hide-phone" }, "Group"),
          h("th", null, "Value"),
          h("th", { class: "hk-hide-phone" }, "Default"),
        ),
      ),
      sBody,
    )
  );
  const sSort = sortableTable(sTable, {
    drive: { id: SORT_SETTINGS },
    remember: "host-settings",
  });
  const settingsCard = section({
    id: "host-settings-card",
    title: "Host settings",
    desc: "",
    collapsible: true,
    open: true,
    tools: [
      h(
        "a",
        {
          class: "kp-button kp-button--sm",
          href: "/settings",
          title: "Change host.toml on the Settings page",
        },
        "Edit in Settings",
      ),
    ],
  });
  /** @type {HTMLElement} */ (
    settingsCard.el.querySelector(".section-head__desc")
  ).replaceChildren(sDesc);
  settingsCard.body.append(
    h("div", { class: "hk-filters" }, sView.el, sSearch),
    h("div", { class: "hk-scroll" }, sTable),
  );
  sBody.dataset.kpState = "loading";
  sBody.replaceChildren(
    ...skeletonRows(3).map((r) => {
      r.querySelectorAll("td").forEach((td, i) => {
        if (i > 3) td.remove();
      });
      return r;
    }),
  );
  const filterSettings = () => {
    const vis = settingVisible(sRows, { all: sAll, q: sQ });
    [...sBody.querySelectorAll("tr[data-key]")].forEach((tr, i) => {
      /** @type {HTMLElement} */ (tr).hidden = !vis[i];
    });
  };
  const loadSettings = async () => {
    const r = await fetchJson(
      "/data/host-settings",
      "the host settings",
      abort.signal,
    );
    if (!r.ok) {
      sBody.replaceChildren(
        h(
          "tr",
          null,
          h(
            "td",
            { colspan: 4 },
            h(
              "div",
              { class: "kp-alert kp-alert--destructive error", role: "alert" },
              h("strong", null, `Could not read ${r.error.what}`),
              h("p", null, `Why: ${r.error.why}`),
            ),
          ),
        ),
      );
      return;
    }
    delete sBody.dataset.kpState;
    const v = hostSettingsView(r.body.page);
    sRows = v.rows;
    sDesc.textContent = `host.toml as the host reads it: ${v.changed} of ${v.rows.length} settings changed from their default. Read-only here; Settings changes them.`;
    sView.counts({ changed: v.changed, all: v.rows.length });
    sBody.replaceChildren(
      ...sRows.map((x) =>
        h(
          "tr",
          {
            class: x.set ? "hk-set-row" : null,
            "data-key": x.key,
            "data-set": x.set ? "1" : "0",
          },
          h(
            "td",
            null,
            h(
              "div",
              { class: "hk-name" },
              h("span", null, x.label),
              h("small", { class: "mono" }, x.key),
            ),
          ),
          h("td", { class: "hk-hide-phone hk-muted" }, x.group),
          h("td", { class: x.set ? "mono" : "mono hk-muted" }, x.value),
          h("td", { class: "hk-hide-phone mono hk-muted" }, x.def),
        ),
      ),
      h(
        "tr",
        { class: "hk-empty-row", "data-none": "" },
        h(
          "td",
          { colspan: 4 },
          "host.toml changes nothing: every setting is at its default.",
        ),
      ),
    );
    const none = /** @type {HTMLElement} */ (
      sBody.querySelector("[data-none]")
    );
    none.hidden = v.changed > 0;
    sSort.apply();
    filterSettings();
  };

  // ── assemble ──────────────────────────────────────────────────────────
  root.replaceChildren(
    head.el,
    attention.el,
    kpis.el,
    h(
      "div",
      { class: "hk-cols" },
      h(
        "div",
        { class: "hk-col-main" },
        guestsCard.el,
        actionsCard.el,
        diskCard.el,
      ),
      h(
        "div",
        { class: "hk-col-side" },
        lineCard.el,
        aboutCard.el,
        tplCard.el,
        checksCard.el,
      ),
    ),
    settingsCard.el,
    keyRow([
      ["/", "search containers"],
      ["click a header", "sort"],
      ["G H", "go to Host"],
      ["R", "run host checks"],
      ["Esc", "show every container"],
      ["Ctrl K", "any host action"],
    ]),
  );

  const paintKpis = () => {
    const f = current().fleet;
    if (!f) return;
    for (const k of hostKpis(f, guests))
      kpis.tiles.get(k.key)?.set({
        ...k,
        meter: {
          pct: k.meter.pct,
          mark: k.meter.mark,
          markTitle: "promised to stacks",
          tone: meterTone(k.meter),
        },
      });
  };
  const render = () => {
    const s = current();
    const f = s.fleet;
    if (!f) return;
    if (sub) sub.textContent = ` ${f.host.name}`;
    daemon.textContent = s.hostVersion ? `host daemon ${s.hostVersion}` : "";
    head.live?.set(f.measured_at);
    paintKpis();
    paintDisk();
    paintAbout();
    paintLine();
  };

  // `/` searches the containers, `r` runs the host checks, Esc shows every
  // container again (never while typing, never with a modifier).
  /** @param {KeyboardEvent} e */
  const keys = (e) => {
    const t = /** @type {HTMLElement | null} */ (e.target);
    if (
      e.ctrlKey ||
      e.metaKey ||
      e.altKey ||
      t?.closest("input, textarea, select, [contenteditable], dialog")
    )
      return;
    if (e.key === "/") {
      e.preventDefault();
      gSearch.focus();
    } else if (e.key === "r" || e.key === "R") {
      if (document.querySelector("dialog[open]")) return;
      e.preventDefault();
      void loadChecks().catch(() => {});
    } else if (e.key === "Escape") {
      if (document.querySelector("dialog[open]")) return;
      gFilter.reset();
    }
  };
  document.addEventListener("keydown", keys);

  const unsub = subscribe(() => {
    render();
    paintGuests();
  });
  render();
  void loadGuests().catch(() => {});
  void loadSettings().catch(() => {});
  void loadTemplates().catch(() => {});
  void loadGrowth().catch(() => {});
  void ping().catch(() => {});
  const timer = setInterval(() => {
    void loadGuests().catch(() => {});
    void ping().catch(() => {});
  }, EVERY_S * 1000);
  // redesign-final-h5: `/host?section=doctor` (the old Doctor's address)
  // opens this page on its Host checks card and reads them, instead of
  // mounting the retired Doctor page inside it.
  if (ctx.focus === "checks") {
    requestAnimationFrame(() =>
      checksCard.el.scrollIntoView({ block: "start" }),
    );
    void loadChecks().catch(() => {});
  }
  return () => {
    abort.abort();
    clearInterval(timer);
    unsub();
    document.removeEventListener("keydown", keys);
    stops.forEach((s) => s());
    for (const s of panels.values()) s();
    root.classList.remove("hk-page", "nx-ops");
  };
}
