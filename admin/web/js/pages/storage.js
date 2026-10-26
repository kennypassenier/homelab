// feat-kpadopt-2 (chassis-rs 3.11.0, kit_pages_in_webapp): the Storage page.
// The kit does not draw `/storage` for a web app that draws the kit's pages
// itself, so this page reads the kit's own JSON (`/api/kit/storage`) and
// draws it in the dashboard's frame: the space against its budget (a warning
// line only, nothing is deleted when it is reached), the two files, one row
// per kind of data with its size, and a careful delete (choose, read-only
// preview, the kind's name typed, one transaction) plus a compact on demand.
//
// Sizes are decimal and human (1 MB = 1 000 000 bytes, as chassis counts),
// counts keep every digit, moments are dd/mm/yyyy HH:mm in Europe/Brussels.

import { send } from "../act.js";
import { notify, openDialog } from "../actui.js";
import { agoEl, setAgo } from "../ago.js";
import { ensureStyle, fetchJson, h } from "../dom.js";
import {
  declare,
  declareField,
  dialogControl,
  drivable,
  fieldId,
} from "../drivable.js";
import { formatDateTime } from "../format.js";
import { buttonGroup, formGrid, gridRow } from "../gridrow.js";
import { stage } from "../stage.js";
import {
  button,
  dataTable,
  failBox,
  pageHeader,
  section,
  tableSlot,
} from "../ui.js";
import {
  CARD_RETRY_EVENT,
  attachCardStates,
  setCardState,
} from "/static/kp/js/card-state.js";
import { buildTypeConfirm } from "/static/kp/js/components.js";

/**
 * @typedef {{name: string, label: string, type: "choice" | "flag" | "text",
 *   values?: string[]}} KindFilter
 * @typedef {{id: string, label: string, description: string, unit: string,
 *   rows: number, bytes: number, oldest: string | null,
 *   newest: string | null, supports_age: boolean,
 *   finished_automatically: boolean, filters: KindFilter[]}} Kind
 * @typedef {{file: string, role: "app" | "kit", total_bytes: number,
 *   db_bytes: number, wal_bytes: number, cleanable: boolean}} StoreFile
 * @typedef {{budget_mb: number, warn_mb: number, default_budget_mb: number,
 *   default_warn_mb: number, min_mb: number, max_mb: number,
 *   can_store: boolean, stored: boolean, budget_fixed: number | null,
 *   warn_fixed: number | null}} BudgetSetting
 * @typedef {{budget: {budget_bytes: number, warn_bytes: number,
 *   total_bytes: number, used_percent: number, level: string,
 *   checked_at: number, setting: BudgetSetting}, files: StoreFile[],
 *   kinds: Kind[], has_app_db: boolean, paused: boolean,
 *   reclaimable_bytes: number, last_compacted_at: number | null}} Overview
 */

// Live view (invariant 39): the ids the kit's own page used, so a script
// written for either finds the same names. A kind's id has its underscores
// as hyphens (`update_records` is `storage-open-clean-update-records`).
/** @param {string} kind */
const cleanId = (kind) => `storage-open-clean-${kind.replaceAll("_", "-")}`;
const CLEAN_NOTICES = declare({
  id: "storage-open-clean-notices",
  page: "storage",
  opens: "dialog",
  what: "open the delete dialog for the notices the dashboard keeps",
});
const CLEAN_SNOOZES = declare({
  id: "storage-open-clean-snoozes",
  page: "storage",
  opens: "dialog",
  what: "open the delete dialog for the snoozes the dashboard keeps",
});
const CLEAN_SCHEDULES = declare({
  id: "storage-open-clean-schedules",
  page: "storage",
  opens: "dialog",
  what: "open the delete dialog for the schedules the dashboard keeps",
});
const CLEAN_UPDATE_RECORDS = declare({
  id: "storage-open-clean-update-records",
  page: "storage",
  opens: "dialog",
  what: "open the delete dialog for the update records the dashboard keeps",
});
const CLEAN_UPTIME = declare({
  id: "storage-open-clean-uptime",
  page: "storage",
  opens: "dialog",
  what: "open the delete dialog for the uptime the dashboard keeps",
});
const CLEAN_CPU_LINES = declare({
  id: "storage-open-clean-cpu-lines",
  page: "storage",
  opens: "dialog",
  what: "open the delete dialog for the cpu lines the dashboard keeps",
});
/**
 * Mark a row's Clean up… button with its kind's declared control.
 * @template {HTMLElement} E
 * @param {E} el
 * @param {string} kind
 * @returns {E}
 */
function markClean(el, kind) {
  el.id = cleanId(kind);
  switch (kind) {
    case "notices":
      return drivable(el, CLEAN_NOTICES);
    case "snoozes":
      return drivable(el, CLEAN_SNOOZES);
    case "schedules":
      return drivable(el, CLEAN_SCHEDULES);
    case "update_records":
      return drivable(el, CLEAN_UPDATE_RECORDS);
    case "uptime":
      return drivable(el, CLEAN_UPTIME);
    case "cpu_lines":
      return drivable(el, CLEAN_CPU_LINES);
    default:
      el.removeAttribute("id");
      return el;
  }
}
const COMPACT = declare({
  id: "storage-open-compact",
  page: "storage",
  opens: "dialog",
  what: "open Compact…: give the database file's free space back to the disk",
});
const BAND_CLEAN = declare({
  id: "storage-band-clean",
  page: "storage",
  opens: "dialog",
  what: "the warning's button: open the delete dialog of the biggest kind",
  shows: "while the space is at or past its warn line",
});
const BUDGET_SAVE = declare({
  id: "storage-budget-save",
  page: "storage",
  opens: "run",
  what: "save the budget and the warn line",
});
const BUDGET_RESET = declare({
  id: "storage-budget-reset",
  page: "storage",
  opens: "run",
  what: "put the budget and the warn line back to their defaults",
});
const TRY_AGAIN = declare({
  id: "storage-try-again",
  page: "storage",
  opens: "view",
  what: "read what failed again",
  shows: "after the storage could not be read",
});
const BUDGET_MB = declareField({
  id: "storage-budget-mb",
  page: "storage",
  what: "the budget in whole megabytes (a warning only: nothing is deleted)",
});
const CLEAN_OLDER = declareField({
  id: "storage-clean-older",
  page: "storage",
  what: "in the delete dialog: only rows at least this old (a number; the unit is beside it)",
});
const CLEAN_FILTER = declareField({
  id: "storage-clean-filter",
  page: "storage",
  row: "<filter name>",
  what: "in the delete dialog: narrow the rows by one of the kind's filters (level, stack, read…)",
});
const WARN_MB = declareField({
  id: "storage-warn-mb",
  page: "storage",
  what: "the warn line in whole megabytes, below the budget",
});

/** Time units for "older than", in seconds. */
const UNITS = [
  ["minutes", 60],
  ["hours", 3600],
  ["days", 86400],
  ["weeks", 604800],
  ["months (30 days)", 2592000],
  ["years (365 days)", 31536000],
];

/**
 * Decimal sizes, the way chassis counts them: 1 MB = 1 000 000 bytes.
 * @param {number} bytes
 */
export function sizeText(bytes) {
  if (!Number.isFinite(bytes) || bytes < 0) return "—";
  if (bytes < 1000) return `${bytes} B`;
  const units = ["kB", "MB", "GB", "TB"];
  let v = bytes / 1000;
  let i = 0;
  while (v >= 1000 && i < units.length - 1) {
    v /= 1000;
    i++;
  }
  return `${v < 10 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
}

/** @param {number} n every digit, grouped by thin spaces */
const count = (n) => String(n).replace(/\B(?=(\d{3})+(?!\d))/g, " ");

/** @param {string | null | undefined} iso */
const moment = (iso) => {
  const t = iso ? Date.parse(iso) / 1000 : NaN;
  return Number.isFinite(t) ? formatDateTime(t) : "—";
};

const LEVEL = /** @type {Record<string, {tone: string, word: string}>} */ ({
  ok: { tone: "success", word: "Within the budget" },
  warn: { tone: "warning", word: "At the warn line" },
  over: { tone: "danger", word: "Past the budget" },
});

/**
 * One cell of a preview's sample row, short enough to read in a table.
 * @param {unknown} v
 */
const cell = (v) => {
  if (v == null) return "—";
  const s = typeof v === "string" ? v : JSON.stringify(v);
  return s.length > 60 ? `${s.slice(0, 59)}…` : s;
};

/** @param {{what: string, why: string, fix?: string}} e */
const sentence = (e) => `${e.what}: ${e.why}${e.fix ? ` ${e.fix}` : ""}`;

/**
 * Read the unit and the number of an "older than" pair.
 * @param {HTMLInputElement} num
 * @param {HTMLSelectElement} unit
 * @returns {number | null} seconds, or null for no age
 */
function olderSeconds(num, unit) {
  const n = Number(num.value);
  if (num.value.trim() === "" || !Number.isFinite(n) || n <= 0) return null;
  return Math.round(n * Number(unit.value));
}

/**
 * A whole-megabytes field.
 * @param {string} id
 * @param {number} v
 * @param {BudgetSetting} s
 */
function numInput(id, v, s) {
  return h("input", {
    id,
    type: "number",
    inputmode: "numeric",
    min: String(s.min_mb),
    max: String(s.max_mb),
    step: "1",
    class: "kp-field__input",
    value: String(v),
    autocomplete: "off",
  });
}

/**
 * The page.
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  ensureStyle("/css/pages/storage.css");
  root.classList.add("st-page");
  const abort = new AbortController();
  /** @type {Overview | null} */
  let data = null;
  /** @type {(() => void)[]} */
  const stops = [];

  // ── header ────────────────────────────────────────────────────────────
  const ago = agoEl("read");
  const compact = drivable(
    button("Compact", {
      cls: "kp-button--secondary",
      title:
        "Give the free space inside the database file back to the disk (nothing is deleted)",
      onClick: () => openCompact(),
    }),
    COMPACT,
  );
  compact.id = "storage-open-compact";
  compact.disabled = true;
  const head = pageHeader({
    title: "Storage",
    desc: "What the dashboard keeps in its own database, how much room it takes, and a careful way to delete what you no longer need. Nothing is deleted on its own, and a full budget only warns.",
    titleMeta: [h("span", { class: "cf-live" }, ago)],
    actions: [compact],
  });

  // ── the three cards, each one stage that is loading from the first frame
  /**
   * @param {string} id @param {string} title @param {string} desc
   * @param {string} words
   */
  const card = (id, title, desc, words) => {
    const inner = h("div", { class: "st-inner" });
    const st = stage(inner);
    setCardState(st, "loading", { words });
    stops.push(attachCardStates(st));
    st.addEventListener(CARD_RETRY_EVENT, () => void load());
    const sec = section({ id, title, desc, body: st });
    return { sec, st, inner };
  };
  const space = card(
    "st-space",
    "Space",
    "What the database takes against the budget you set. The budget is a warning only: when it is reached an Inbox notice says so; nothing is refused or deleted.",
    "Reading the space",
  );
  const kinds = card(
    "st-kinds",
    "What is kept",
    "One row per kind of data the dashboard stores. Clean up reads what a choice would delete first, and asks you to type the kind's name.",
    "Reading the kinds of data",
  );
  const files = card(
    "st-files",
    "Files",
    "The database files on disk, the write-ahead log included. The kit's own file holds sign-in data and is never cleaned from here.",
    "Reading the files",
  );
  root.replaceChildren(head.el, space.sec.el, kinds.sec.el, files.sec.el);

  /** @param {{what: string, why: string, fix?: string | null}} err */
  const fail = (err) => {
    for (const c of [space, kinds, files]) {
      c.inner.replaceChildren(
        failBox(err, { run: () => void load(), drive: { id: TRY_AGAIN } }),
      );
      setCardState(c.st, "ready");
    }
  };

  // ── the budget card ───────────────────────────────────────────────────
  const drawSpace = (/** @type {Overview} */ d) => {
    const b = d.budget;
    const s = b.setting;
    const lv = LEVEL[b.level] ?? LEVEL.ok;
    const pct = b.budget_bytes
      ? Math.min(100, (b.total_bytes * 100) / b.budget_bytes)
      : 0;
    const warnPct = b.budget_bytes
      ? Math.min(100, (b.warn_bytes * 100) / b.budget_bytes)
      : 0;
    const bar = h("div", {
      class: `st-bar st-bar--${b.level}`,
      role: "img",
      "aria-label": `${sizeText(b.total_bytes)} of ${sizeText(b.budget_bytes)} used, ${b.used_percent} %`,
    });
    bar.style.setProperty("--st-fill", `${pct}%`);
    bar.style.setProperty("--st-warn", `${warnPct}%`);
    bar.append(h("span", { class: "st-bar__fill" }));
    bar.append(h("span", { class: "st-bar__warn", "aria-hidden": "true" }));

    const budgetIn = numInput(BUDGET_MB, s.budget_fixed ?? s.budget_mb, s);
    const warnIn = numInput(WARN_MB, s.warn_fixed ?? s.warn_mb, s);
    const lock = !s.can_store;
    budgetIn.disabled = lock || s.budget_fixed != null;
    warnIn.disabled = lock || s.warn_fixed != null;
    const state = h("p", {
      class: "kp-formgrid__value kp-dialog__note st-state",
      role: "status",
    });
    state.textContent = lock
      ? "The budget is set by the service's own configuration and cannot be changed here."
      : s.stored
        ? `Saved here; the default is ${s.default_budget_mb} MB with a warning at ${s.default_warn_mb} MB.`
        : `The default: ${s.default_budget_mb} MB with a warning at ${s.default_warn_mb} MB.`;
    const save = drivable(
      button("Save", {
        cls: "kp-button--primary",
        title: "Keep this budget and warn line",
      }),
      BUDGET_SAVE,
    );
    save.id = "storage-budget-save";
    const reset = drivable(
      button("Reset to default", {
        cls: "kp-button--secondary",
        title: "Back to the default budget and warn line",
      }),
      BUDGET_RESET,
    );
    reset.id = "storage-budget-reset";
    save.disabled = reset.disabled = lock;
    /** @param {Record<string, unknown>} body @param {HTMLButtonElement} by */
    const post = async (body, by) => {
      by.disabled = true;
      const r = await send(
        "POST",
        "/api/kit/storage/budget",
        body,
        "the budget",
      );
      by.disabled = lock;
      if (abort.signal.aborted) return;
      if (!r.ok) {
        state.textContent = sentence(r.error);
        notify(r.error.why, "error");
        return;
      }
      notify("Budget saved", "success");
      await load();
    };
    save.addEventListener("click", () => {
      const bm = Number(budgetIn.value);
      const wm = Number(warnIn.value);
      if (
        !Number.isInteger(bm) ||
        !Number.isInteger(wm) ||
        bm < s.min_mb ||
        wm < s.min_mb ||
        wm >= bm
      ) {
        state.textContent = `Type whole megabytes between ${s.min_mb} and ${s.max_mb}, with the warn line below the budget.`;
        return;
      }
      void post({ budget_mb: bm, warn_mb: wm }, save);
    });
    reset.addEventListener("click", () => void post({ reset: true }, reset));

    const biggest = [...d.kinds].sort((a, b2) => b2.bytes - a.bytes)[0];
    const band =
      b.level !== "ok" && biggest
        ? [
            h(
              "div",
              { class: "kp-attention st-band", role: "status" },
              h(
                "p",
                null,
                `${lv.word}: the database holds ${sizeText(b.total_bytes)} of ${sizeText(b.budget_bytes)}. Nothing was deleted; ${biggest.label} is the biggest kind.`,
              ),
              (() => {
                const bt = button(`Clean up ${biggest.label}`, {
                  cls: "kp-button--secondary",
                  onClick: () => openClean(biggest),
                });
                bt.id = "storage-band-clean";
                return drivable(bt, BAND_CLEAN);
              })(),
            ),
          ]
        : [];

    space.inner.replaceChildren(
      ...band,
      h(
        "div",
        { class: "st-figure" },
        h("span", { class: "st-figure__value" }, sizeText(b.total_bytes)),
        h(
          "span",
          { class: "st-figure__of" },
          `of ${sizeText(b.budget_bytes)} · ${b.used_percent} %`,
        ),
        h("span", { class: `kp-badge kp-badge--${lv.tone}` }, lv.word),
      ),
      bar,
      h(
        "p",
        { class: "st-note" },
        `Warning from ${sizeText(b.warn_bytes)}. Checked ${formatDateTime(b.checked_at)}.`,
      ),
      formGrid([
        gridRow({
          label: "Budget (MB)",
          labelFor: BUDGET_MB,
          control: budgetIn,
          value: "Whole megabytes; the warning is at this size.",
        }),
        gridRow({
          label: "Warn at (MB)",
          labelFor: WARN_MB,
          control: warnIn,
          value: "The Inbox notice appears from here.",
        }),
        gridRow({
          label: "Keep",
          control: buttonGroup([save, reset]),
          buttons: true,
        }),
        gridRow({ control: state, wide: true }),
      ]),
    );
    setCardState(space.st, "ready");
  };

  // ── the kinds table ───────────────────────────────────────────────────
  const slot = tableSlot();
  stops.push(slot.stop);
  const drawKinds = (/** @type {Overview} */ d) => {
    if (!d.has_app_db || d.kinds.length === 0) {
      setCardState(kinds.st, "empty", {
        title: "No kinds of data",
        text: "This service has no application database to clean up.",
      });
      return;
    }
    const tbl = dataTable({
      cls: "st-table",
      remember: "storage-kinds",
      caption: "The kinds of data the dashboard keeps",
      columns: [
        { label: "Kind", sort: "text" },
        { label: "Rows", sort: "number", cls: "kp-text-end" },
        { label: "Size", sort: "number", cls: "kp-text-end" },
        { label: "Oldest", sort: "text" },
        { label: "Newest", sort: "text" },
        { label: "" },
      ],
    });
    tbl.tbody.replaceChildren(
      ...d.kinds.map((k) => {
        const open = h(
          "button",
          {
            type: "button",
            class: "kp-button kp-button--secondary",
            title: `Choose what to delete from ${k.label}; nothing is deleted until you type its name`,
            "aria-label": `Clean up ${k.label}`,
          },
          "Clean up",
        );
        markClean(open, k.id);
        open.addEventListener("click", () => openClean(k));
        return h(
          "tr",
          { "data-kp-row-key": k.id },
          h(
            "td",
            null,
            h("strong", null, k.label),
            h("span", { class: "st-desc" }, k.description),
          ),
          h(
            "td",
            { class: "kp-text-end", "data-kp-sort-value": String(k.rows) },
            `${count(k.rows)} ${k.unit}`,
          ),
          h(
            "td",
            { class: "kp-text-end", "data-kp-sort-value": String(k.bytes) },
            sizeText(k.bytes),
          ),
          h("td", { "data-kp-sort-value": k.oldest ?? "" }, moment(k.oldest)),
          h("td", { "data-kp-sort-value": k.newest ?? "" }, moment(k.newest)),
          h("td", { class: "kp-text-end" }, open),
        );
      }),
    );
    tbl.wrap.setAttribute("data-kp-cards", "");
    for (const tr of /** @type {HTMLTableRowElement[]} */ ([
      ...tbl.tbody.children,
    ])) {
      ["Kind", "Rows", "Size", "Oldest", "Newest", ""].forEach((l, i) => {
        if (l) tr.children[i]?.setAttribute("data-label", l);
      });
    }
    kinds.inner.replaceChildren(tbl.wrap);
    slot.attach(kinds.inner);
    setCardState(kinds.st, "ready");
  };

  // ── the files card ────────────────────────────────────────────────────
  const drawFiles = (/** @type {Overview} */ d) => {
    const tbl = dataTable({
      cls: "st-table",
      remember: "storage-files",
      caption: "The database files",
      columns: [
        { label: "File", sort: "text" },
        { label: "Holds" },
        { label: "Database", sort: "number", cls: "kp-text-end" },
        { label: "Write-ahead log", sort: "number", cls: "kp-text-end" },
        { label: "Total", sort: "number", cls: "kp-text-end" },
      ],
    });
    tbl.tbody.replaceChildren(
      ...d.files.map((f) =>
        h(
          "tr",
          { "data-kp-row-key": f.file },
          h("td", null, h("code", null, f.file)),
          h(
            "td",
            null,
            f.role === "app"
              ? "The dashboard's data: the kinds above."
              : "The kit's own data (sign-in). Never cleaned from here.",
          ),
          h(
            "td",
            { class: "kp-text-end", "data-kp-sort-value": String(f.db_bytes) },
            sizeText(f.db_bytes),
          ),
          h(
            "td",
            { class: "kp-text-end", "data-kp-sort-value": String(f.wal_bytes) },
            sizeText(f.wal_bytes),
          ),
          h(
            "td",
            {
              class: "kp-text-end",
              "data-kp-sort-value": String(f.total_bytes),
            },
            sizeText(f.total_bytes),
          ),
        ),
      ),
    );
    for (const tr of /** @type {HTMLTableRowElement[]} */ ([
      ...tbl.tbody.children,
    ])) {
      ["File", "Holds", "Database", "Write-ahead log", "Total"].forEach(
        (l, i) => tr.children[i]?.setAttribute("data-label", l),
      );
    }
    tbl.wrap.setAttribute("data-kp-cards", "");
    files.inner.replaceChildren(tbl.wrap);
    fileSlot.attach(files.inner);
    setCardState(files.st, "ready");
  };
  const fileSlot = tableSlot();
  stops.push(fileSlot.stop);

  // ── the clean-up dialog ───────────────────────────────────────────────
  /** @param {Kind} k */
  function openClean(k) {
    /** @type {HTMLElement} */
    const body = h("div", { class: "st-clean" });
    const d = openDialog({
      title: `Clean up ${k.label}`,
      description: k.description,
      body: [body],
      id: "storage-clean-dialog",
    });
    dialogControl(d.dialog, "storage-clean-dialog");

    // step 1: choose
    const num = /** @type {HTMLInputElement} */ (
      h("input", {
        id: CLEAN_OLDER,
        type: "number",
        min: "1",
        step: "any",
        class: "kp-field__input",
        placeholder: "e.g. 30",
        autocomplete: "off",
        "aria-describedby": "storage-clean-older-help",
      })
    );
    dialogControl(num, "storage-clean-older");
    const unit = /** @type {HTMLSelectElement} */ (
      h(
        "select",
        { class: "kp-field__input", "aria-label": "Unit of the age" },
        ...UNITS.map(([l, s]) =>
          h("option", { value: String(s), selected: s === 86400 }, l),
        ),
      )
    );
    /** @type {Map<string, HTMLInputElement | HTMLSelectElement>} */
    const filterIn = new Map();
    /** @type {HTMLElement[]} */
    const rows = [];
    if (k.supports_age)
      rows.push(
        gridRow({
          label: "Older than",
          labelFor: "storage-clean-older",
          control: h("span", { class: "st-age" }, num, unit),
          value: h(
            "span",
            { id: "storage-clean-older-help" },
            "Leave empty for every row of this kind.",
          ),
        }),
      );
    for (const f of k.filters) {
      const id = fieldId(CLEAN_FILTER, f.name);
      /** @type {HTMLInputElement | HTMLSelectElement} */
      let inp;
      if (f.type === "text") {
        inp = /** @type {HTMLInputElement} */ (
          h("input", {
            id,
            type: "text",
            class: "kp-field__input",
            autocomplete: "off",
            placeholder: "any",
          })
        );
      } else {
        const opts =
          f.type === "flag"
            ? [
                ["", "Any"],
                ["true", "Yes"],
                ["false", "No"],
              ]
            : [["", "Any"], ...(f.values ?? []).map((v) => [v, v])];
        inp = /** @type {HTMLSelectElement} */ (
          h(
            "select",
            { id, class: "kp-field__input" },
            ...opts.map(([v, l]) => h("option", { value: v }, l)),
          )
        );
      }
      dialogControl(inp, id);
      filterIn.set(f.name, inp);
      rows.push(
        gridRow({ label: f.label, labelFor: id, control: inp, wide: true }),
      );
    }
    const chooseState = h("p", {
      class: "kp-dialog__note st-state",
      role: "status",
    });
    const reset = dialogControl(
      button("Reset", { cls: "kp-button--secondary" }),
      "storage-clean-reset",
    );
    reset.id = "storage-clean-reset";
    reset.addEventListener("click", () => {
      num.value = "";
      for (const i of filterIn.values()) i.value = "";
      chooseState.textContent = "";
    });
    const preview = dialogControl(
      button("Preview", {
        cls: "kp-button--primary",
        title: "Read what this would delete; nothing is deleted yet",
      }),
      "storage-clean-preview",
    );
    preview.id = "storage-clean-preview";
    const close = dialogControl(
      button("Close", { cls: "kp-button--secondary" }),
      "storage-clean-close",
    );
    close.id = "storage-clean-close";
    close.addEventListener("click", () => d.close());

    const selector = () => {
      /** @type {{older_than_secs?: number, filters: Record<string, string>}} */
      const sel = { filters: {} };
      const secs = k.supports_age ? olderSeconds(num, unit) : null;
      if (secs != null) sel.older_than_secs = secs;
      for (const [name, inp] of filterIn)
        if (inp.value.trim() !== "") sel.filters[name] = inp.value.trim();
      return sel;
    };

    const drawChoose = () => {
      chooseState.textContent = "";
      body.replaceChildren(
        formGrid(rows),
        chooseState,
        h("div", { class: "kp-dialog__actions" }, close, reset, preview),
      );
      num.focus();
    };

    preview.addEventListener("click", async () => {
      if (k.supports_age && num.value.trim() !== "") {
        const n = Number(num.value);
        if (!Number.isFinite(n) || n <= 0) {
          chooseState.textContent =
            "The age must be a number above zero, or empty for every row.";
          return;
        }
      }
      const sel = selector();
      preview.disabled = true;
      chooseState.textContent = "Reading what this would delete…";
      const r = await send(
        "POST",
        `/api/kit/storage/${encodeURIComponent(k.id)}/preview`,
        sel,
        "the preview",
      );
      preview.disabled = false;
      if (!r.ok) {
        chooseState.textContent = sentence(r.error);
        return;
      }
      drawPreview(sel, r.body);
    });

    // step 2: the read-only preview and the typed confirmation
    /**
     * @param {{older_than_secs?: number, filters: Record<string, string>}} sel
     * @param {any} p
     */
    const drawPreview = (sel, p) => {
      const back = dialogControl(
        button("Back", { cls: "kp-button--secondary" }),
        "storage-clean-back",
      );
      back.id = "storage-clean-back";
      back.addEventListener("click", drawChoose);
      if (!p.rows) {
        const done = dialogControl(
          button("Close", { cls: "kp-button--secondary" }),
          "storage-clean-done",
        );
        done.id = "storage-clean-done";
        done.addEventListener("click", () => d.close());
        body.replaceChildren(
          h(
            "p",
            { class: "st-none", role: "status" },
            "Nothing matches this choice, so there is nothing to delete.",
          ),
          h("div", { class: "kp-dialog__actions" }, back, done),
        );
        return;
      }
      /** @type {string[]} */
      const cols = [
        ...(p.time_column ? [p.time_column] : []),
        ...(p.columns ?? []).filter(
          (/** @type {string} */ c) => c !== p.time_column,
        ),
      ].slice(0, 4);
      const samples = /** @type {Record<string, unknown>[]} */ (
        p.samples ?? []
      ).slice(0, 5);
      const sample = samples.length
        ? h(
            "div",
            { class: "kp-table-wrap st-sample" },
            h(
              "table",
              { class: "kp-table" },
              h("caption", { class: "kp-sr-only" }, "The first rows"),
              h(
                "thead",
                null,
                h("tr", null, ...cols.map((c) => h("th", null, c))),
              ),
              h(
                "tbody",
                null,
                ...samples.map((s) =>
                  h(
                    "tr",
                    null,
                    ...cols.map((c) => {
                      const v = s[c];
                      return h(
                        "td",
                        { title: typeof v === "string" ? v : "" },
                        c === p.time_column && typeof v === "string"
                          ? moment(v)
                          : cell(v),
                      );
                    }),
                  ),
                ),
              ),
            ),
          )
        : null;
      const del = dialogControl(
        button("Delete", {
          cls: "kp-button--danger",
          title: "Delete these rows, in one transaction",
        }),
        "storage-clean-delete",
      );
      del.id = "storage-clean-delete";
      del.disabled = true;
      del.setAttribute(
        "data-kp-disabled-reason",
        `Type ${p.confirm_with} first`,
      );
      const status = h("p", {
        class: "kp-dialog__note st-state",
        role: "status",
      });
      const confirm = buildTypeConfirm(document, {
        name: String(p.confirm_with),
        id: "storage-clean-confirm-input",
        loses: [
          `${count(p.rows)} ${k.unit} (about ${sizeText(p.bytes)})`,
          ...(p.oldest
            ? [`from ${moment(p.oldest)} to ${moment(p.newest)}`]
            : []),
        ],
        onChange: (matched) => {
          del.disabled = !matched;
          if (matched) del.removeAttribute("data-kp-disabled-reason");
          else
            del.setAttribute(
              "data-kp-disabled-reason",
              `Type ${p.confirm_with} first`,
            );
        },
      });
      confirm.block.id = "storage-clean-confirm";
      dialogControl(confirm.block, "storage-clean-confirm");
      dialogControl(confirm.input, "storage-clean-confirm-field");
      del.addEventListener("click", async () => {
        if (!confirm.matched()) return;
        del.disabled = back.disabled = true;
        status.textContent = "Deleting…";
        const r = await send(
          "POST",
          `/api/kit/storage/${encodeURIComponent(k.id)}/delete`,
          { ...sel, token: p.token, confirm: confirm.input.value.trim() },
          "the delete",
        );
        if (!r.ok) {
          back.disabled = false;
          del.disabled = !confirm.matched();
          status.textContent = sentence(r.error);
          return;
        }
        const n = Number(r.body?.deleted?.deleted ?? r.body?.deleted ?? 0);
        drawDone(n);
        if (!abort.signal.aborted) void load();
      });
      body.replaceChildren(
        h(
          "p",
          { class: "st-sum" },
          h("strong", null, `${count(p.rows)} ${k.unit}`),
          ` would be deleted, about ${sizeText(p.bytes)}`,
          p.oldest ? `, from ${moment(p.oldest)} to ${moment(p.newest)}` : "",
          ". Nothing has been deleted yet.",
        ),
        ...(sample ? [sample] : []),
        confirm.block,
        status,
        h("div", { class: "kp-dialog__actions" }, back, del),
      );
      confirm.input.focus();
    };

    /** @param {number} n */
    const drawDone = (n) => {
      const done = dialogControl(
        button("Done", { cls: "kp-button--primary" }),
        "storage-clean-done",
      );
      done.id = "storage-clean-done";
      done.addEventListener("click", () => d.close());
      body.replaceChildren(
        h(
          "p",
          { class: "st-sum", role: "status" },
          `Deleted ${count(n)} ${k.unit}. The sizes above are read again; the file gives the space back to the disk when you Compact.`,
        ),
        h("div", { class: "kp-dialog__actions" }, done),
      );
      done.focus();
    };

    drawChoose();
  }

  // ── the compact dialog ────────────────────────────────────────────────
  function openCompact() {
    const body = h("div", { class: "st-clean" });
    const d = openDialog({
      title: "Compact the database",
      description:
        "Rewrites the dashboard's database file without its free space, so the disk gets that space back. No row is deleted; it cannot run while a backup holds the state.",
      body: [body],
      id: "storage-compact-dialog",
    });
    dialogControl(d.dialog, "storage-compact-dialog");
    const status = h("p", {
      class: "kp-dialog__note st-state",
      role: "status",
    });
    const cancel = dialogControl(
      button("Cancel", { cls: "kp-button--secondary" }),
      "storage-compact-cancel",
    );
    cancel.addEventListener("click", () => d.close());
    const run = dialogControl(
      button("Compact", { cls: "kp-button--primary" }),
      "storage-compact-run",
    );
    run.disabled = true;
    status.textContent = "Reading what a compact would give back…";
    body.replaceChildren(
      status,
      h("div", { class: "kp-dialog__actions" }, cancel, run),
    );
    void (async () => {
      const r = await send(
        "POST",
        "/api/kit/storage/compact/preview",
        undefined,
        "the compact preview",
      );
      if (!r.ok) {
        status.textContent = sentence(r.error);
        return;
      }
      const p = r.body;
      status.textContent = p.paused
        ? "A backup holds the state right now; try again when it is done."
        : `The file is ${sizeText(p.file_bytes)}; about ${sizeText(p.reclaimable_bytes)} can be given back. It needs ${sizeText(p.free_space_needed_bytes)} of free disk while it runs.`;
      run.disabled = Boolean(p.paused);
    })();
    run.addEventListener("click", async () => {
      run.disabled = cancel.disabled = true;
      status.textContent = "Compacting…";
      const r = await send(
        "POST",
        "/api/kit/storage/compact",
        undefined,
        "the compact",
      );
      cancel.disabled = false;
      if (!r.ok) {
        status.textContent = sentence(r.error);
        return;
      }
      const done = dialogControl(
        button("Done", { cls: "kp-button--primary" }),
        "storage-compact-done",
      );
      done.addEventListener("click", () => d.close());
      body.replaceChildren(
        h("p", { class: "st-sum", role: "status" }, "Compacted."),
        h("div", { class: "kp-dialog__actions" }, done),
      );
      if (!abort.signal.aborted) void load();
    });
  }

  // ── read ──────────────────────────────────────────────────────────────
  async function load() {
    const r = await fetchJson(
      "/api/kit/storage",
      "the storage",
      abort.signal,
    ).catch(() => null);
    if (!r || abort.signal.aborted) return;
    if (!r.ok) {
      fail(r.error);
      return;
    }
    data = /** @type {Overview} */ (r.body);
    compact.disabled = !data.has_app_db;
    setAgo(ago, Math.floor(Date.now() / 1000));
    drawSpace(data);
    drawKinds(data);
    drawFiles(data);
  }
  void load();

  return () => {
    abort.abort();
    for (const s of stops) s();
    root.classList.remove("st-page");
  };
}
