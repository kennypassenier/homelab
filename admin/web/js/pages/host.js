// The host page (feat-overview-2): pve's load and facts, live from the fleet
// reading; the containers `pct list` reports; and, on request, the doctor's
// host-level checks (the disk among them), which take about a minute.

import { mountActionsArea } from "../actionsarea.js";
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
  root.replaceChildren(
    h("h1", null, "Host"),
    h(
      "section",
      { class: "kp-card", "aria-label": "Load" },
      bars,
      h("p", null, liveAgo),
    ),
    h("section", { class: "kp-card", "aria-label": "Facts" }, facts),
    actions.element,
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
  };

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
    else void loadChecks().catch(() => {});
  };
  root.addEventListener("kp-datatable-retry", retry);
  const unsub = subscribe(() => {
    render();
    if (lastGuests.length) paintGuests();
  });
  render();
  void loadGuests().catch(() => {});
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
    root.removeEventListener("kp-datatable-retry", retry);
    detach();
  };
}
