// The host page (feat-overview-2): pve's load and facts, live from the fleet
// reading; the containers `pct list` reports; and, on request, the doctor's
// host-level checks (the disk among them), which take about a minute.

import { mountActionsArea } from "../actionsarea.js";
import { openAction } from "../actiondialog.js";
import { hostSettingRows } from "../parity.js";
import { agoEl, setAgo } from "../ago.js";
import { doctorRows } from "../doctor.js";
import {
  badgeCell,
  bindTableUrl,
  errorBox,
  fetchJson,
  fillFacts,
  h,
  progressGroup,
  slowReport,
  tableBlock,
  td,
} from "../dom.js";
import { humanDuration } from "../format.js";
import {
  diskDetailFacts,
  guestRows,
  hostBars,
  hostChecks,
  hostFacts,
  topDirRows,
} from "../host.js";
import { stackHref } from "../router.js";
import { current, subscribe } from "../store.js";
import { attachDataTables, dataTable } from "/static/kp/js/datatable.js";
import { viaForm } from "../drivable.js";

/** Seconds between two readings of the container list. */
const GUESTS_EVERY_S = 30;

/**
 * @param {HTMLElement} root
 * @param {{navigate: (href: string) => void}} ctx
 * @returns {() => void}
 */
export function mount(root, ctx) {
  const bars = h("div", { id: "host-bars" });
  const facts = h("dl", { class: "facts", id: "host-facts" });
  const diskFacts = h("dl", { class: "facts", id: "host-disk-facts" });
  const topDirs = tableBlock({
    remember: "host-top-dirs",
    caption: "Biggest directories on root",
    search: "Search directories",
    nothing: "Not read yet.",
    columns: [
      { label: "Directory", sort: "text" },
      { label: "Size", sort: "number" },
    ],
  });
  const liveAgo = agoEl("measured", null, { live: true });
  const guestsAgo = agoEl("read");
  const guests = tableBlock({
    remember: "guests",
    caption: "Containers on the host (pct list)",
    search: "Search containers",
    state: "loading",
    nothing: "The host lists no containers.",
    columns: [
      { label: "vmid", sort: "number" },
      { label: "Name", sort: "text" },
      {
        label: "Status",
        sort: "text",
        order: "stopped,running",
        filter: "choice",
      },
      { label: "Lock", sort: "text" },
      { label: "Stack", sort: "text" },
    ],
  });
  // feat-stacks-4: the four host-wide actions.
  const actions = mountActionsArea({ host: true });
  const checksAgo = agoEl("read");
  const checksBtn = h(
    "button",
    { type: "button", class: "kp-button", id: "host-checks-read" },
    "Read the host checks",
  );
  const checksNote = h(
    "p",
    { class: "measured", role: "status" },
    "The doctor reads for about half a minute on pve; the disk and the state file are among its host-level checks.",
  );
  const checks = tableBlock({
    remember: "host-checks",
    caption: "Host checks (from the doctor)",
    search: "Search checks",
    nothing: "The doctor reported no host-level checks.",
    columns: [
      { label: "Check", sort: "text" },
      {
        label: "Health",
        sort: "text",
        order: "fail,warn,ok",
        filter: "choice",
      },
      { label: "Detail", sort: "text", cls: "wide" },
      { label: "What to do", sort: "text" },
    ],
  });
  // TUI parity: the line to the host (ping, where its address came from,
  // the pin and the certificate), host.toml as the host reads it, the
  // templates, and the pages the TUI reached from here.
  const line = h("dl", { class: "facts", id: "host-line" });
  const pingBtn = h(
    "button",
    { type: "button", class: "kp-button", id: "host-ping" },
    "Ping",
  );
  const pingOut = h("p", {
    class: "measured",
    role: "status",
    id: "host-ping-out",
  });
  const settingsAgo = agoEl("read");
  const settingsT = tableBlock({
    remember: "host-toml",
    caption: "host.toml, as the host reads it",
    search: "Search settings",
    state: "loading",
    nothing: "host.toml sets nothing.",
    columns: [
      { label: "Group", sort: "text", filter: "choice" },
      { label: "Key", sort: "text" },
      { label: "Setting", sort: "text" },
      { label: "Value", sort: "text", cls: "wide" },
      { label: "From", sort: "text", filter: "choice" },
    ],
  });
  const tplBtn = h(
    "button",
    { type: "button", class: "kp-button", id: "host-templates-read" },
    "Read the templates",
  );
  const tplBuild = h(
    "button",
    { type: "button", class: "kp-button", id: "host-template-build" },
    "Build a template…",
  );
  // fix-239: Live view reaches it as `homelab ui open template-build`.
  viaForm(tplBuild, "template-build");
  tplBuild.addEventListener(
    "click",
    () => void openAction("_host", "template-build"),
  );
  const tplOut = h("div", { id: "host-templates" });
  root.replaceChildren(
    h("h1", null, "Host"),
    h(
      "p",
      { class: "section-head__desc measured" },
      "The Proxmox host itself: its resources, settings, templates and the containers it carries.",
    ),
    h(
      "section",
      { class: "kp-card", "aria-label": "Load" },
      h("h2", null, "Load"),
      h(
        "p",
        { class: "section-head__desc measured" },
        "CPU, RAM and disk the host itself is using right now.",
      ),
      bars,
      h("p", null, liveAgo),
    ),
    h(
      "section",
      { class: "kp-card", "aria-label": "Disk" },
      h("h2", null, "Disk"),
      h(
        "p",
        { class: "section-head__desc measured" },
        // fix-222 (Kenny, 2026-10-02): "root disk 48%" said nothing about
        // which disk that is or what is on it — this names the volume, the
        // physical disk behind it, the local-lvm pool beside it, and the
        // biggest directories using the space.
        "Which disk the root filesystem lives on, the local-lvm pool every container's own disk is carved from, and what is using the space.",
      ),
      diskFacts,
      h("h3", null, "Biggest directories on root"),
      topDirs.wrap,
    ),
    h(
      "section",
      { class: "kp-card", "aria-label": "Facts" },
      h("h2", null, "Facts"),
      h(
        "p",
        { class: "section-head__desc measured" },
        "What the host is: its version, uptime and hardware.",
      ),
      facts,
    ),
    h(
      "section",
      { class: "kp-card", "aria-label": "The line to the host" },
      h(
        "div",
        { class: "title-row" },
        h("h2", null, "The line to the host"),
        pingBtn,
      ),
      h(
        "p",
        { class: "section-head__desc measured" },
        "The TLS connection this dashboard holds to the host daemon, and a way to test it.",
      ),
      line,
      pingOut,
    ),
    h(
      "p",
      { class: "actions-row host-links" },
      h("a", { class: "kp-button", href: "/console" }, "Open the console"),
      h(
        "a",
        { class: "kp-button", href: "/activity?view=host-log" },
        "Host log",
      ),
      h("a", { class: "kp-button", href: "/presets" }, "Presets"),
      h(
        "a",
        {
          class: "kp-button",
          href: "/data/download/runbook",
          download: "DR_RUNBOOK.md",
        },
        "Download the runbook",
      ),
    ),
    actions.element,
    h("h2", null, "Host settings"),
    h(
      "p",
      { class: "measured" },
      "Read-only here; ",
      h("a", { href: "/settings" }, "the Settings page"),
      " changes them.",
    ),
    settingsT.wrap,
    h("p", null, settingsAgo),
    h("h2", null, "Templates"),
    h(
      "p",
      { class: "section-head__desc measured" },
      "The golden container templates new stacks are built from; build a fresh one here.",
    ),
    h("div", { class: "actions-row" }, tplBtn, tplBuild),
    tplOut,
    h("h2", null, "Containers"),
    h(
      "p",
      { class: "section-head__desc measured" },
      "Every container (LXC/VM) on the host, whether or not this orchestrator manages it.",
    ),
    guests.wrap,
    h("p", null, guestsAgo),
    h("div", { class: "title-row" }, h("h2", null, "Host checks"), checksBtn),
    h(
      "p",
      { class: "section-head__desc measured" },
      "Checks about the host itself, read on request (not per stack).",
    ),
    checksNote,
    checks.wrap,
    h("p", null, checksAgo),
  );
  const detach = attachDataTables(root);
  const settingsTable = dataTable(settingsT.wrap);
  const unbindS = bindTableUrl(settingsTable, "hosttoml");
  const guestTable = dataTable(guests.wrap);
  const checksTable = dataTable(checks.wrap);
  const topDirsTable = dataTable(topDirs.wrap);
  // Read on request: no empty table that looks like a load before that.
  checks.wrap.hidden = true;
  const unbindG = bindTableUrl(guestTable, "guests");
  const unbindC = bindTableUrl(checksTable, "hostchecks");
  const unbindTD = bindTableUrl(topDirsTable, "hosttopdirs");
  const abort = new AbortController();

  const render = () => {
    const s = current();
    const f = s.fleet;
    if (!f) return;
    bars.replaceChildren(progressGroup(hostBars(f)));
    fillFacts(
      facts,
      hostFacts(f, { version: s.hostVersion, build: s.hostBuild }),
    );
    fillFacts(diskFacts, diskDetailFacts(f));
    topDirs.tbody.replaceChildren(
      ...topDirRows(f).map((r) => h("tr", null, td(r.path), td(r.gb, "num"))),
    );
    topDirsTable?.refresh();
    setAgo(liveAgo, f.measured_at);
    paintLine();
  };

  /** @type {any} */
  let lineFacts = null;
  const paintLine = () => {
    const f = current().fleet;
    /** @type {{label: string, value: string}[]} */
    const rows = [];
    if (lineFacts) {
      rows.push(
        { label: "Address", value: lineFacts.address ?? "not configured" },
        { label: "Set in", value: lineFacts.address_source ?? "—" },
        {
          label: "Pinned certificate",
          value: lineFacts.pin ?? "no pin compiled in",
        },
      );
    }
    rows.push({
      label: "Host's TLS fingerprint",
      value: f?.host.tls_fingerprint || "not reported by the host",
    });
    fillFacts(line, rows);
  };
  const ping = async () => {
    pingBtn.disabled = true;
    pingOut.textContent = "Pinging…";
    const r = await fetchJson("/data/ping", "the ping", abort.signal);
    pingBtn.disabled = false;
    if (!r.ok) {
      pingOut.textContent = `No answer: ${r.error.why}`;
      return;
    }
    lineFacts = r.body.facts;
    paintLine();
    pingOut.textContent = r.body.ok
      ? `The host answered in ${r.body.ms} ms: ${r.body.message}`
      : `No answer after ${r.body.ms} ms: ${r.body.message}`;
  };
  pingBtn.addEventListener("click", () => void ping().catch(() => {}));

  const loadSettings = async () => {
    settingsT.loading({ words: "Reading host.toml from the host…" });
    const r = await fetchJson(
      "/data/host-settings",
      "the host settings",
      abort.signal,
    );
    if (!r.ok) {
      settingsT.failed(r.error);
      return;
    }
    settingsT.tbody.replaceChildren(
      ...hostSettingRows(r.body.page).map((x) =>
        h(
          "tr",
          null,
          td(x.group),
          td(x.key, "mono"),
          td(x.label),
          td(x.value, "mono"),
          td(x.source),
        ),
      ),
    );
    settingsT.ready();
    setAgo(settingsAgo, r.body.measured_at ?? Date.now() / 1000);
  };

  const loadTemplates = async () => {
    tplBtn.disabled = true;
    tplOut.replaceChildren(
      h(
        "p",
        { class: "measured" },
        "Asking the host (pveam and the template containers)…",
      ),
    );
    const r = await fetchJson("/data/templates", "the templates", abort.signal);
    tplBtn.disabled = false;
    if (!r.ok) {
      tplOut.replaceChildren(errorBox(r.error));
      return;
    }
    const t = r.body.templates;
    tplOut.replaceChildren(
      h("h3", null, "Golden templates (a deploy clones these)"),
      t.clones.length
        ? h(
            "ul",
            null,
            ...t.clones.map((/** @type {[number, string]} */ c) =>
              h("li", { class: "mono" }, `CT ${c[0]} · ${c[1]}`),
            ),
          )
        : h("p", { class: "measured" }, "None: build one."),
      h("h3", null, "OS templates (a build bakes from these)"),
      t.os.length
        ? h(
            "ul",
            null,
            ...t.os.map((/** @type {string} */ o) =>
              h("li", { class: "mono" }, o),
            ),
          )
        : h("p", { class: "measured" }, "None listed."),
    );
  };
  tplBtn.addEventListener("click", () => void loadTemplates().catch(() => {}));

  /** @type {import("../host.js").Guest[]} */
  let lastGuests = [];
  const paintGuests = () => {
    guests.tbody.replaceChildren(
      ...guestRows(lastGuests, current().fleet).map((g) =>
        h(
          "tr",
          null,
          td(String(g.vmid), "num"),
          td(g.name),
          badgeCell(g.status),
          td(g.lock),
          g.stack
            ? h("td", null, h("a", { href: stackHref(g.stack) }, g.stack))
            : td("not managed"),
        ),
      ),
    );
    guestTable?.refresh();
  };
  /** @param {boolean} [show] a first read or a retry: say it loads (the
   * timer's reads refresh quietly) */
  const loadGuests = async (show = false) => {
    if (show || lastGuests.length === 0)
      guests.loading({
        words: "Asking the host for its containers (pct list)…",
      });
    const r = await fetchJson(
      "/data/host/guests",
      "the host's containers",
      abort.signal,
    );
    if (!r.ok) {
      guests.failed(r.error);
      return;
    }
    lastGuests = r.body.guests ?? [];
    paintGuests();
    guests.ready();
    setAgo(guestsAgo, r.body.measured_at);
  };

  /**
   * Show one doctor answer's host-level checks.
   * @param {any} report
   */
  const paintChecks = (report) => {
    const rows = doctorRows({ ...report, checks: hostChecks(report) });
    checks.tbody.replaceChildren(
      ...rows.map((x) =>
        h(
          "tr",
          null,
          td(x.name),
          badgeCell(x.health),
          td(x.detail),
          td(x.remedy),
        ),
      ),
    );
    return rows.length;
  };
  const checksWords = {
    words: "Asking the host to run the doctor…",
    expect: 30,
  };

  const loadChecks = async () => {
    checksBtn.disabled = true;
    checks.wrap.hidden = false;
    checks.loading(checksWords);
    checksNote.textContent = "The doctor is running on the host.";
    let r;
    try {
      // slow-reads: the dashboard's last doctor answer at once, while the
      // host runs it again.
      r = await slowReport("/data/doctor", "the doctor", abort.signal, (b) => {
        const n = paintChecks(b.report);
        checks.ready();
        checks.loading({ ...checksWords, overlay: false });
        checksNote.textContent = `${n} host-level checks, the last reading, while the host runs the doctor again.`;
        setAgo(checksAgo, b.read_at ?? Date.now() / 1000);
      });
    } finally {
      checksBtn.disabled = false;
    }
    if (!r.ok) {
      const secs = checks.failed(r.error);
      checksNote.textContent = `The doctor did not answer (after ${humanDuration(secs)}).`;
      return;
    }
    const n = paintChecks(r.report);
    const secs = checks.ready();
    checksNote.textContent = `${n} host-level checks, read in ${humanDuration(secs)}.`;
    setAgo(checksAgo, r.body.read_at ?? Date.now() / 1000);
  };
  checksBtn.addEventListener("click", () => void loadChecks().catch(() => {}));

  const retry = (/** @type {Event} */ e) => {
    if (guests.wrap.contains(/** @type {Node} */ (e.target)))
      void loadGuests(true).catch(() => {});
    else if (settingsT.wrap.contains(/** @type {Node} */ (e.target)))
      void loadSettings().catch(() => {});
    else void loadChecks().catch(() => {});
  };
  root.addEventListener("kp-datatable-retry", retry);
  const unsub = subscribe(() => {
    render();
    if (lastGuests.length) paintGuests();
  });
  render();
  void loadGuests().catch(() => {});
  void loadSettings().catch(() => {});
  void ping().catch(() => {});
  const timer = setInterval(
    () => void loadGuests().catch(() => {}),
    GUESTS_EVERY_S * 1000,
  );
  void ctx;
  return () => {
    abort.abort();
    actions.stop();
    clearInterval(timer);
    unsub();
    unbindG();
    unbindC();
    unbindS();
    unbindTD();
    root.removeEventListener("kp-datatable-retry", retry);
    detach();
  };
}
