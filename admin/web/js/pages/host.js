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
  fetchReport,
  fillFacts,
  h,
  progressGroup,
  tableBlock,
  td,
} from "../dom.js";
import { guestRows, hostBars, hostChecks, hostFacts } from "../host.js";
import { stackHref } from "../router.js";
import { current, subscribe } from "../store.js";
import { attachDataTables, dataTable } from "/static/kp/js/datatable.js";

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
  const liveAgo = agoEl("measured", null, { live: true });
  const guestsAgo = agoEl("read");
  const guestErr = h("div");
  const guests = tableBlock({
    remember: "guests",
    caption: "Containers on the host (pct list)",
    search: "Search containers",
    state: "loading",
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
  const checksErr = h("div");
  const checksAgo = agoEl("read");
  const checksBtn = h(
    "button",
    { type: "button", class: "kp-button", id: "host-checks-read" },
    "Read the host checks",
  );
  const checksNote = h(
    "p",
    { class: "measured", role: "status" },
    "The doctor reads for about a minute on pve; the disk and the state file are among its host-level checks.",
  );
  const checks = tableBlock({
    remember: "host-checks",
    caption: "Host checks (from the doctor)",
    search: "Search checks",
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
  const settingsErr = h("div");
  const settingsAgo = agoEl("read");
  const settingsT = tableBlock({
    remember: "host-toml",
    caption: "host.toml, as the host reads it",
    search: "Search settings",
    state: "loading",
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
  tplBuild.addEventListener(
    "click",
    () => void openAction("_host", "template-build"),
  );
  const tplOut = h("div", { id: "host-templates" });
  root.replaceChildren(
    h("h1", null, "Host"),
    h(
      "section",
      { class: "kp-card", "aria-label": "Load" },
      bars,
      h("p", null, liveAgo),
    ),
    h("section", { class: "kp-card", "aria-label": "Facts" }, facts),
    h(
      "section",
      { class: "kp-card", "aria-label": "The line to the host" },
      h(
        "div",
        { class: "title-row" },
        h("h2", null, "The line to the host"),
        pingBtn,
      ),
      line,
      pingOut,
    ),
    h(
      "p",
      { class: "actions-row host-links" },
      h("a", { class: "kp-button", href: "/app/shell" }, "Open the shell"),
      h("a", { class: "kp-button", href: "/app/log" }, "Live log"),
      h("a", { class: "kp-button", href: "/app/presets" }, "Presets"),
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
      h("a", { href: "/app/settings" }, "the Settings page"),
      " changes them.",
    ),
    settingsErr,
    settingsT.wrap,
    h("p", null, settingsAgo),
    h("h2", null, "Templates"),
    h("div", { class: "actions-row" }, tplBtn, tplBuild),
    tplOut,
    h("h2", null, "Containers"),
    guestErr,
    guests.wrap,
    h("p", null, guestsAgo),
    h("h2", null, "Host checks"),
    h("div", { class: "title-row" }, checksNote, checksBtn),
    checksErr,
    checks.wrap,
    h("p", null, checksAgo),
  );
  const detach = attachDataTables(root);
  const settingsTable = dataTable(settingsT.wrap);
  const unbindS = bindTableUrl(settingsTable, "hosttoml");
  const guestTable = dataTable(guests.wrap);
  const checksTable = dataTable(checks.wrap);
  const unbindG = bindTableUrl(guestTable, "guests");
  const unbindC = bindTableUrl(checksTable, "hostchecks");
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
    settingsTable?.state("loading");
    const r = await fetchJson(
      "/data/host-settings",
      "the host settings",
      abort.signal,
    );
    if (!r.ok) {
      settingsErr.replaceChildren(errorBox(r.error));
      settingsTable?.state("failed");
      return;
    }
    settingsErr.replaceChildren();
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
    settingsTable?.refresh();
    settingsTable?.state("ready");
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
  const loadGuests = async () => {
    const r = await fetchJson(
      "/data/host/guests",
      "the host's containers",
      abort.signal,
    );
    if (!r.ok) {
      guestErr.replaceChildren(errorBox(r.error));
      guestTable?.state("failed");
      return;
    }
    guestErr.replaceChildren();
    lastGuests = r.body.guests ?? [];
    paintGuests();
    guestTable?.state("ready");
    setAgo(guestsAgo, r.body.measured_at);
  };

  const loadChecks = async () => {
    checksBtn.disabled = true;
    checksNote.textContent = "Reading… the doctor takes about a minute.";
    checksTable?.state("loading");
    const r = await fetchReport("/data/doctor", "the doctor", abort.signal);
    checksBtn.disabled = false;
    checksBtn.textContent = "Read again";
    if (!r.ok) {
      checksErr.replaceChildren(errorBox(r.error));
      checksTable?.state("failed");
      checksNote.textContent = "The doctor did not answer.";
      return;
    }
    checksErr.replaceChildren();
    const rows = doctorRows({ ...r.report, checks: hostChecks(r.report) });
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
    checksTable?.refresh();
    checksTable?.state("ready");
    checksNote.textContent = `${rows.length} host-level checks.`;
    setAgo(checksAgo, Date.now() / 1000);
  };
  checksBtn.addEventListener("click", () => void loadChecks().catch(() => {}));

  const retry = (/** @type {Event} */ e) => {
    if (guests.wrap.contains(/** @type {Node} */ (e.target)))
      void loadGuests().catch(() => {});
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
    root.removeEventListener("kp-datatable-retry", retry);
    detach();
  };
}
