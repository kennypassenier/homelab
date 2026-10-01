// Overview (feat-overview-1): the host card and the fleet table, live.
// feat-stacks-5: the fleet table is the one table with row selection; the
// ticked stacks get one action together.

import { agoEl, setAgo } from "../ago.js";
import { gb, hostCard, stackState } from "../fleet.js";
import { stackFlags } from "../parity.js";
import { catalogReady } from "../act.js";
import { openBatch } from "../actiondialog.js";
import { batchActions } from "../actionforms.js";
import {
  badgeCell,
  bindTableUrl,
  h,
  fetchJson,
  fillStatGrid,
  selectCell,
  tableBlock,
  td,
} from "../dom.js";
import { register } from "../drivehooks.js";
import { openImport } from "../importstack.js";
import { openNewStack } from "../newstack.js";
import { stackHref } from "../router.js";
import { current, subscribe } from "../store.js";
import { attachDataTables, dataTable } from "/static/kp/js/datatable.js";

/** @typedef {{navigate: (href: string) => void}} Ctx */

/**
 * @param {HTMLElement} root
 * @param {Ctx} ctx
 * @returns {() => void}
 */
export function mount(root, ctx) {
  // Kenny, 2026-10-01: the host card's six facts are short stats, not a
  // label/value list — they sit in a stat-grid instead of one long column.
  const card = h("div", { class: "stat-grid stat-grid--3" });
  // feat-overview-4: the reading's age, ticking.
  const ago = agoEl("measured", null, { live: true });
  const measured = h("p", { class: "measured", id: "measured" }, ago);
  // Kenny, 2026-09-28: every table is the kp-themes datatable, no row
  // selection, every column sortable, Shift+click adds a sort key, and each
  // table remembers its own sort.
  const batchSel = h(
    "select",
    {
      class: "kp-field__input batch-action",
      id: "batch-action",
      "aria-label": "Action for the selected stacks",
    },
    h("option", { value: "" }, "Choose an action…"),
  );
  const batchRun = h(
    "button",
    { type: "button", class: "kp-button kp-button--primary", id: "batch-open" },
    "Run on the selected…",
  );
  const t = tableBlock({
    remember: "fleet",
    select: {
      label: "Select every stack on this page",
      actions: [batchSel, batchRun],
    },
    caption: "Stacks",
    search: "Search stacks",
    state: "loading",
    nothing: "The host manages no stacks yet.",
    columns: [
      { label: "vmid", sort: "number" },
      { label: "Stack", sort: "text" },
      {
        label: "State",
        sort: "text",
        order: "offline,degraded,parked,running",
        filter: "choice",
      },
      { label: "Apps up", sort: "number" },
      { label: "Apps", sort: "number" },
      { label: "Restarts", sort: "number" },
      { label: "RAM used (GB)", sort: "number" },
      { label: "RAM limit (GB)", sort: "number" },
      // TUI parity: [OFF] [CHANGED] [NOENV].
      { label: "Flags", sort: "text", filter: "choice" },
    ],
  });
  /** @type {Record<string, {state: import("../parity.js").DriftState}>} */
  let drift = {};
  t.tbody.id = "stacks";
  t.wrap.querySelector("table")?.classList.add("fleet");
  // feat-stacks-3: a new stack from a preset.
  const newStack = h(
    "button",
    { type: "button", class: "kp-button", id: "new-stack" },
    "New stack…",
  );
  newStack.addEventListener("click", () => void openNewStack(ctx.navigate));
  // TUI parity: `homelab import`, a bundle as a new stack.
  const importBtn = h(
    "button",
    { type: "button", class: "kp-button", id: "import-stack" },
    "Import…",
  );
  importBtn.addEventListener("click", () => void openImport(ctx.navigate));
  // TUI parity ([CHANGED]): comparing runs latch once per stack on the
  // dashboard, so it runs when asked, not on every visit.
  const compareBtn = h(
    "button",
    { type: "button", class: "kp-button", id: "drift-compare" },
    "Compare with the files",
  );
  root.replaceChildren(
    h(
      "div",
      { class: "title-row" },
      h("h1", null, "Overview"),
      h("span", { class: "actions-row" }, compareBtn, importBtn, newStack),
    ),
    h(
      "section",
      { class: "kp-card host", "aria-label": "Host", id: "host" },
      card,
    ),
    t.wrap,
    measured,
  );
  const detach = attachDataTables(root);
  const table = dataTable(t.wrap);
  const unbind = bindTableUrl(table, "fleet");

  // A click anywhere on a stack's row opens its page; the name is a real
  // link for the keyboard and for opening in a new tab.
  t.tbody.addEventListener("click", (e) => {
    const target = /** @type {Element} */ (e.target);
    if (target.closest("a, input, .select-col")) return;
    const tr = target.closest("tr");
    if (tr?.dataset.stack) ctx.navigate(stackHref(tr.dataset.stack));
  });

  if (!current().fleet)
    t.loading({ words: "Waiting for the host's report of the fleet…" });
  const render = () => {
    const f = current().fleet;
    if (!f) return;
    fillStatGrid(card, hostCard(f));
    // The ticked stacks survive the live update that redraws the rows.
    const ticked = table?.selected() ?? [];
    const rows = f.stacks.map((s) => {
      const tr = h("tr", {
        class: "link-row",
        "data-stack": s.name,
        "data-kp-row-key": s.name,
      });
      const name = h("td", null, h("a", { href: stackHref(s.name) }, s.name));
      tr.append(
        selectCell(s.name, `Select ${s.name}`),
        td(String(s.vmid), "num"),
        name,
        badgeCell(stackState(s)),
        td(String(s.apps_running), "num"),
        td(String(s.apps_total), "num"),
        td(String(s.restarts ?? 0), "num"),
        td(gb(s.ram_used_mb), "num"),
        td(gb(s.ram_max_mb), "num"),
        td(
          stackFlags(s, drift[s.name]?.state)
            .map((f) => `[${f.label}]`)
            .join(" ") || "—",
          "stack-flags",
        ),
      );
      return tr;
    });
    t.tbody.replaceChildren(...rows);
    // The table sorts the rows it holds; new rows are read again and put
    // in the reader's order.
    t.ready();
    if (ticked.length) table?.select(ticked);
    setAgo(ago, f.measured_at);
  };

  void catalogReady().then((c) => {
    if (!c) return;
    batchSel.append(
      ...batchActions(c).map((a) => h("option", { value: a.action }, a.label)),
    );
  });
  batchRun.addEventListener("click", () => {
    const stacks = table?.selected() ?? [];
    if (!batchSel.value) {
      batchSel.focus();
      return;
    }
    void openBatch(batchSel.value, stacks);
  });

  // Live view (owner decision 2026-09-30): `homelab ui select` ticks rows
  // here, exactly as a click would, so a batch action opened right after
  // it acts on the same stacks.
  const unregister = register("overview", {
    select: (/** @type {string[]} */ stacks) => table?.select(stacks),
  });

  const unsub = subscribe(render);
  render();
  const abort = new AbortController();
  /** @param {boolean} fresh */
  const readDrift = async (fresh) => {
    compareBtn.disabled = true;
    const r = await fetchJson(
      `/data/drift${fresh ? "?fresh=1" : ""}`,
      "drift",
      abort.signal,
    );
    compareBtn.disabled = false;
    if (!r.ok) return;
    drift = r.body.stacks ?? {};
    render();
  };
  compareBtn.addEventListener(
    "click",
    () => void readDrift(true).catch(() => {}),
  );
  void readDrift(false).catch(() => {});
  return () => {
    abort.abort();
    unsub();
    unbind();
    detach();
    unregister();
  };
}
