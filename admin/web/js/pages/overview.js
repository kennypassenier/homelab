// Overview (feat-overview-1): the host card and the fleet table, live.
// feat-stacks-5: the fleet table is the one table with row selection; the
// ticked stacks get one action together.

import { agoEl, setAgo } from "../ago.js";
import { gb, hostCard, stackState } from "../fleet.js";
import { catalogReady } from "../act.js";
import { openBatch } from "../actiondialog.js";
import { batchActions } from "../actionforms.js";
import {
  badgeCell,
  bindTableUrl,
  h,
  selectCell,
  tableBlock,
  td,
} from "../dom.js";
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
  const card = h("dl", { class: "facts" });
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
    ],
  });
  t.tbody.id = "stacks";
  t.wrap.querySelector("table")?.classList.add("fleet");
  // feat-stacks-3: a new stack from a preset.
  const newStack = h(
    "button",
    { type: "button", class: "kp-button", id: "new-stack" },
    "New stack…",
  );
  newStack.addEventListener("click", () => void openNewStack(ctx.navigate));
  root.replaceChildren(
    h("div", { class: "title-row" }, h("h1", null, "Overview"), newStack),
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

  const render = () => {
    const f = current().fleet;
    if (!f) return;
    card.replaceChildren(
      ...hostCard(f).flatMap((x) => [
        h("dt", null, x.label),
        h("dd", null, x.value),
      ]),
    );
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
      );
      return tr;
    });
    t.tbody.replaceChildren(...rows);
    // The table sorts the rows it holds; new rows are read again and put
    // in the reader's order.
    table?.refresh();
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

  const unsub = subscribe(render);
  render();
  return () => {
    unsub();
    unbind();
    detach();
  };
}
