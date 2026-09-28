// One stack's page (feat-stacks-1, first cut): its facts and its apps,
// from the same live snapshot as the overview.

import { measuredAgo, stackDetail } from "../fleet.js";
import { badgeCell, h, tableBlock, td } from "../dom.js";
import { current, subscribe } from "../store.js";
import { attachDataTables, dataTable } from "/static/kp/js/datatable.js";

/**
 * @param {HTMLElement} root
 * @param {{name: string}} params
 * @returns {() => void}
 */
export function mount(root, params) {
  const title = h("h1", null, `Stack ${params.name}`);
  const state = h("span", { class: "state" });
  const facts = h("dl", { class: "facts" });
  const missing = h("p", { class: "kp-alert kp-alert--warning", hidden: "" });
  const measured = h("p", { class: "measured" });
  const t = tableBlock({
    remember: "stack-apps",
    caption: "Apps",
    search: "Search apps",
    columns: [
      { label: "App", sort: "text" },
      {
        label: "Running",
        sort: "text",
        order: "stopped,running",
        filter: "choice",
      },
      { label: "Restarts", sort: "number" },
    ],
  });
  root.replaceChildren(
    h("p", { class: "crumb" }, h("a", { href: "/app/" }, "← Overview")),
    h("div", { class: "title-row" }, title, state),
    missing,
    h("section", { class: "kp-card", "aria-label": "Stack" }, facts),
    t.wrap,
    measured,
  );
  const detach = attachDataTables(root);
  const table = dataTable(t.wrap);

  const tick = () => {
    const f = current().fleet;
    if (f) measured.textContent = measuredAgo(f.measured_at, Date.now() / 1000);
  };

  const render = () => {
    const f = current().fleet;
    if (!f) return;
    const d = stackDetail(f, params.name);
    if (!d) {
      missing.hidden = false;
      missing.textContent = `The host's fleet has no stack called "${params.name}".`;
      facts.replaceChildren();
      state.replaceChildren();
      t.tbody.replaceChildren();
      table?.refresh();
      return;
    }
    missing.hidden = true;
    state.className = `state ${d.state.tone}`;
    state.replaceChildren(h("span", null, d.state.label));
    facts.replaceChildren(
      ...d.facts.flatMap((x) => [
        h("dt", null, x.label),
        h("dd", null, x.value),
      ]),
    );
    t.tbody.replaceChildren(
      ...d.apps.map((a) =>
        h(
          "tr",
          null,
          td(a.name),
          badgeCell({ label: a.running, tone: a.tone }),
          td(a.restarts, "num"),
        ),
      ),
    );
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
