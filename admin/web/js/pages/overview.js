// Overview (feat-overview-1): the host card and the fleet table, live.

import { gb, hostCard, measuredAgo, stackState } from "../fleet.js";
import { badgeCell, h, tableBlock, td } from "../dom.js";
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
  const measured = h("p", { class: "measured", id: "measured" });
  // Kenny, 2026-09-28: every table is the kp-themes datatable, no row
  // selection, every column sortable, Shift+click adds a sort key, and each
  // table remembers its own sort.
  const t = tableBlock({
    remember: "fleet",
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
  root.replaceChildren(
    h("h1", null, "Overview"),
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

  // A click anywhere on a stack's row opens its page; the name is a real
  // link for the keyboard and for opening in a new tab.
  t.tbody.addEventListener("click", (e) => {
    const target = /** @type {Element} */ (e.target);
    if (target.closest("a")) return;
    const tr = target.closest("tr");
    if (tr?.dataset.stack) ctx.navigate(stackHref(tr.dataset.stack));
  });

  const tick = () => {
    const f = current().fleet;
    if (f) measured.textContent = measuredAgo(f.measured_at, Date.now() / 1000);
  };

  const render = () => {
    const f = current().fleet;
    if (!f) return;
    card.replaceChildren(
      ...hostCard(f).flatMap((x) => [
        h("dt", null, x.label),
        h("dd", null, x.value),
      ]),
    );
    const rows = f.stacks.map((s) => {
      const tr = h("tr", { class: "link-row", "data-stack": s.name });
      const name = h("td", null, h("a", { href: stackHref(s.name) }, s.name));
      tr.append(
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
    tick();
  };

  const unsub = subscribe(render);
  const timer = setInterval(tick, 1000);
  render();
  return () => {
    unsub();
    clearInterval(timer);
    detach();
  };
}
