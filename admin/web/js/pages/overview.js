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
  errorBox,
  h,
  fetchJson,
  fillStatGrid,
  sectionHeader,
  selectCell,
  tableBlock,
  td,
} from "../dom.js";
import { register } from "../drivehooks.js";
import { openImport } from "../importstack.js";
import { openNewStack } from "../newstack.js";
import { stackHref } from "../router.js";
import { current, subscribe } from "../store.js";
import { mount as mountApplySection } from "./apply.js";
import { attachDataTables, dataTable } from "/static/kp/js/datatable.js";
import { declareField, viaForm } from "../drivable.js";

// review M5: every page field Live view may set is declared (drivable.js
// `declareField`); the client and the dashboard refuse any other.
const BATCH_ACTION = declareField({
  id: "batch-action",
  page: "overview",
  what: "the action a batch runs on the selected stacks",
});

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
      id: BATCH_ACTION,
      "aria-label": "Action for the selected stacks",
    },
    h("option", { value: "" }, "Choose an action…"),
  );
  const batchRun = h(
    "button",
    { type: "button", class: "kp-button kp-button--primary", id: "batch-open" },
    "Run on the selected…",
  );
  // fix-239: Live view reaches these through their forms: `homelab ui open
  // batch <action>`, `open new-stack`, `open import`.
  viaForm(batchRun, "batch");
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
  viaForm(newStack, "new-stack");
  newStack.addEventListener("click", () => void openNewStack(ctx.navigate));
  // TUI parity: `homelab import`, a bundle as a new stack.
  const importBtn = h(
    "button",
    { type: "button", class: "kp-button", id: "import-stack" },
    "Import…",
  );
  viaForm(importBtn, "import");
  importBtn.addEventListener("click", () => void openImport(ctx.navigate));
  // TUI parity ([CHANGED]): comparing runs latch once per stack on the
  // dashboard, so it runs when asked, not on every visit. fix-210 (Kenny,
  // 2026-10-02): the button used to sit loose at the top of the page,
  // meaning something different from the fleet-wide apply plan right next
  // to it — both now live inside the one "Apply the whole fleet" section
  // below, each its own clearly described step.
  const compareBtn = h(
    "button",
    {
      type: "button",
      class: "kp-button",
      id: "drift-compare",
      title:
        "Check every stack against its files and mark the ones that differ [CHANGED] in the table above.",
    },
    "Compare with the files",
  );
  const driftError = h("div", { class: "drift-error" });
  // fix-210: the Apply page's whole functionality, as a collapsible
  // section that starts loading only once opened (health.js's lazy-block
  // pattern) — never a page nobody asked to read. `?section=apply`
  // (`router.js` redirectFor's "apply" case, the old `/apply` address)
  // opens it and scrolls it into view, the same convention Health already
  // uses for `?block=`.
  const applyStep1 = h(
    "div",
    { class: "apply-step" },
    sectionHeader(
      "Which stacks differ from their files",
      "A quick per-stack check: marks each stack [CHANGED] in the table above when what runs does not match its stack files. Covers the whole fleet at once; a single stack's own page has the same check for just that stack.",
      { level: "h3" },
    ),
    h("div", { class: "title-row" }, compareBtn),
    driftError,
  );
  const applyPlanBody = h("div", { class: "apply-step" });
  const applySection = /** @type {HTMLDetailsElement} */ (
    h(
      "details",
      { class: "health-block", id: "apply-section" },
      h(
        "summary",
        null,
        sectionHeader(
          "Apply the whole fleet",
          "Plan and apply every pending change across the fleet in one confirmed batch, including a stack whose directory was deleted.",
          { level: "h2" },
        ),
      ),
      h("div", { class: "health-block__body" }, applyStep1, applyPlanBody),
    )
  );
  let applyMounted = false;
  let stopApply = () => {};
  const mountApplyOnce = () => {
    if (applyMounted) return;
    applyMounted = true;
    applyPlanBody.replaceChildren(
      sectionHeader(
        "What applying would change",
        "The full plan for every stack, read from the host and the working copy; nothing runs until Apply… is confirmed.",
        { level: "h3" },
      ),
    );
    const planBody = h("div");
    applyPlanBody.append(planBody);
    stopApply = mountApplySection(planBody);
  };
  applySection.addEventListener("toggle", () => {
    if (applySection.open) mountApplyOnce();
  });
  root.replaceChildren(
    h(
      "div",
      { class: "title-row" },
      h("h1", null, "Stacks"),
      h("span", { class: "actions-row" }, importBtn, newStack),
    ),
    h(
      "p",
      { class: "section-head__desc measured" },
      "The host itself and every stack it manages, live; open a stack or act on several at once.",
    ),
    h(
      "section",
      { class: "kp-card host", "aria-label": "Host", id: "host" },
      sectionHeader(
        "Host",
        "The Proxmox host itself: CPU, RAM, disk and uptime, read live from the host daemon.",
      ),
      card,
    ),
    sectionHeader(
      "Stacks",
      "Every stack the host manages, with its live state; click a row to open that stack, or tick several for a batch action.",
    ),
    t.wrap,
    measured,
    applySection,
  );
  // feat-shell-1: `/stacks?deploy-all=1` (Deploy all changes) is this
  // section's 3.71.0 address; `?section=apply` from before still opens it.
  const q = new URLSearchParams(location.search);
  if (q.get("section") === "apply" || q.get("deploy-all") === "1") {
    applySection.open = true;
    mountApplyOnce();
    queueMicrotask(() => applySection.scrollIntoView({ block: "start" }));
  }
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
    driftError.replaceChildren();
    const r = await fetchJson(
      `/data/drift${fresh ? "?fresh=1" : ""}`,
      "drift",
      abort.signal,
    );
    compareBtn.disabled = false;
    if (!r.ok) {
      driftError.replaceChildren(errorBox(r.error));
      return;
    }
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
    stopApply();
  };
}
