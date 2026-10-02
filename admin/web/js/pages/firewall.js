// feat-firewall-2: the fleet's firewall on one page. Each stack's state,
// every rule of every stack with the peers named by stack, and the matrix
// of which container may open which port on which other one, read from the
// working copy's stack files. Edits go through each stack's Firewall tab
// (feat-firewall-1).

import { agoEl, setAgo } from "../ago.js";
import {
  badgeCell,
  bindTableUrl,
  fetchJson,
  h,
  tableBlock,
  td,
} from "../dom.js";
import { matrixRows, ruleRows, stackState } from "../fwview.js";
import { stackHref } from "../router.js";
import { listen } from "../store.js";
import { attachDataTables, dataTable } from "/static/kp/js/datatable.js";

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const ago = agoEl("read");
  const head = h("p", { class: "measured" });
  const stacks = tableBlock({
    remember: "firewall-stacks",
    caption: "Each stack's firewall",
    search: "Search stacks",
    state: "loading",
    nothing: "The working copy holds no stack files.",
    columns: [
      { label: "vmid", sort: "number" },
      { label: "Stack", sort: "text" },
      { label: "Address", sort: "text" },
      {
        label: "Firewall",
        sort: "text",
        order:
          "none declared,declared but off,declared, not enforced,in force (repo differs),in force",
        filter: "choice",
      },
      { label: "Inbound", sort: "text", filter: "choice" },
      { label: "Outbound", sort: "text", filter: "choice" },
      { label: "Rules", sort: "number" },
      { label: "Edit", sort: "text" },
    ],
  });
  const rules = tableBlock({
    remember: "firewall-rules",
    caption: "Every rule of every stack",
    search: "Search rules",
    state: "loading",
    nothing: "No stack declares a firewall rule.",
    pageSize: 50,
    pageSizes: "25,50,100,250",
    columns: [
      { label: "Stack", sort: "text", filter: "choice" },
      { label: "#", sort: "number" },
      { label: "Direction", sort: "text", filter: "choice" },
      {
        label: "Action",
        sort: "text",
        order: "ACCEPT,REJECT,DROP",
        filter: "choice",
      },
      { label: "Other side", sort: "text" },
      { label: "Protocol", sort: "text", filter: "choice" },
      { label: "Ports", sort: "text" },
      { label: "Note", sort: "text", cls: "wide" },
      { label: "In force", sort: "text", filter: "choice" },
    ],
  });
  const matrixWrap = h("div", { id: "fw-matrix" });
  root.replaceChildren(
    h("div", { class: "title-row" }, h("h1", null, "Firewall")),
    h(
      "p",
      null,
      "What each container's Proxmox firewall lets through, from the stack files in the working copy. Change a stack's rules on its own Firewall tab.",
    ),
    head,
    stacks.wrap,
    h("h2", null, "Who may reach whom"),
    h(
      "p",
      { class: "measured" },
      "Read row to column: the ports the row's container may open on the column's, where both firewalls let it through (first match, as Proxmox reads them). \"open\": the column's container has no firewall in force.",
    ),
    matrixWrap,
    h(
      "p",
      { class: "measured" },
      // [fix-215]: this page used to draw its own near-identical topology
      // graph below the matrix. One topology now lives on the Fleet view,
      // with a "Show measured traffic" toggle — this links there with the
      // toggle already on, instead of drawing a second one here.
      "See these connections on the ",
      h("a", { href: "/fleetview?traffic=1" }, "Fleet view topology"),
      ".",
    ),
    h("h2", null, "All rules"),
    rules.wrap,
    h("p", null, ago),
  );
  const detach = attachDataTables(root);
  const stacksTable = dataTable(stacks.wrap);
  const rulesTable = dataTable(rules.wrap);
  const unbindS = bindTableUrl(stacksTable, "stacks");
  const unbindR = bindTableUrl(rulesTable, "rules");
  /** @type {() => void} */
  let detachMatrix = () => {};
  const abort = new AbortController();

  const load = async () => {
    const words = "Reading the stack files in the working copy…";
    stacks.loading({ words });
    rules.loading({ words });
    const r = await fetchJson(
      "/data/firewall",
      "the fleet's firewall",
      abort.signal,
    );
    if (!r.ok) {
      stacks.failed(r.error);
      rules.failed(r.error);
      return;
    }
    head.textContent = r.body.head
      ? `Read from the working copy at ${String(r.body.head.commit).slice(0, 10)} · ${r.body.head.subject}`
      : "";
    /** @type {import("../fwview.js").StackFw[]} */
    const list = r.body.stacks ?? [];
    stacks.tbody.replaceChildren(
      ...list.map((s) =>
        h(
          "tr",
          { "data-kp-row-key": s.stack },
          td(String(s.vmid), "num"),
          h("td", null, h("a", { href: stackHref(s.stack) }, s.stack)),
          td(s.ip, "mono"),
          badgeCell(stackState(s)),
          td(s.policy_in ?? "—"),
          td(s.policy_out ?? "—"),
          td(String(s.rules), "num"),
          h(
            "td",
            null,
            h(
              "a",
              {
                href: stackHref(s.stack, "firewall"),
                class: "kp-button kp-button--sm",
              },
              s.declared ? "Edit" : "Declare",
            ),
          ),
        ),
      ),
    );
    stacks.ready();
    rules.tbody.replaceChildren(
      ...ruleRows(r.body.matrix?.rules ?? []).map((x) =>
        h(
          "tr",
          { "data-kp-row-key": x.key },
          h(
            "td",
            null,
            h("a", { href: stackHref(x.stack, "firewall") }, x.stack),
          ),
          td(String(x.n), "num"),
          td(x.dir),
          badgeCell({ label: x.action, tone: x.tone }),
          td(x.peer, "mono"),
          td(x.proto),
          td(x.ports, "mono"),
          td(x.note),
          td(x.inForce),
        ),
      ),
    );
    rules.ready();
    const m = r.body.matrix;
    detachMatrix();
    if (m) {
      const cols = /** @type {string[]} */ (m.stacks);
      const mt = tableBlock({
        remember: "firewall-matrix",
        caption: "Row may reach column on",
        search: "Search the matrix",
        nothing: "No stack has an address to reach.",
        columns: [
          { label: "From ↓ / to →", sort: "text" },
          ...cols.map((c) => ({ label: c, sort: "text" })),
        ],
      });
      mt.wrap.classList.add("fw-matrix");
      mt.tbody.append(
        ...matrixRows(m).map((row) =>
          h(
            "tr",
            { "data-kp-row-key": row.from },
            h("th", { scope: "row" }, row.from),
            ...row.cells.map((c) =>
              h("td", { class: `cell ${c.tone}`, title: c.title }, c.text),
            ),
          ),
        ),
      );
      matrixWrap.replaceChildren(mt.wrap);
      detachMatrix = attachDataTables(matrixWrap);
    }
    setAgo(ago, Date.now() / 1000);
  };

  const retry = () => void load().catch(() => {});
  root.addEventListener("kp-datatable-retry", retry);
  const off = listen("repo", retry);
  retry();
  return () => {
    abort.abort();
    off();
    root.removeEventListener("kp-datatable-retry", retry);
    unbindS();
    unbindR();
    detachMatrix();
    detach();
  };
}
