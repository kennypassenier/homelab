// The Presets page (redesign-presets, release 3.71.0; rebuilt on the approved
// kp-themes 11 demo, feat-kp11demo-12.5). The repository's presets/ directory
// as a gallery of kp entity cards (RAM, cores and disk as figures, GPU and VPN
// as flags, Use and Edit as buttons; the app-less preset is the "Empty stack"
// card at the end), a kp toolbar whose search understands what you mean
// ("gpu media") with removable tags, and a second card that compares two or
// three presets and costs one against the host's memory (presetscompare.js).
//
// "Use" opens the existing New stack wizard with that preset chosen
// (feat-stacks-3, replayed by Live view), "Edit" the presets editor
// (feat-preset-1).

import { agoEl, setAgo } from "../ago.js";
import { fetchJson, h } from "../dom.js";
import { declare, declareField, drivable, viaForm } from "../drivable.js";
import { openImport } from "../importstack.js";
import { openNewStack } from "../newstack.js";
import { openPresetEditor } from "../presetseditor.js";
import { presetsCompare } from "../presetscompare.js";
import { countText, emptyPreset, ramText, sets } from "../presetsview.js";
import { ensureStyle, failBox, pageHeader, segSwitch } from "../ui.js";
import { setParams } from "../urlstate.js";
import {
  ENTITY_ACTION_EVENT,
  ENTITY_GRID_RETRY_EVENT,
  ENTITY_NEW_EVENT,
  setEntityGrid,
  setEntityGridState,
} from "/static/kp/js/entity-card.js";
import { matchIntent } from "/static/kp/js/intent.js";
import {
  SEGMENTED_EVENT,
  attachSegmented,
  getSegmentedValues,
  setSegmentedValues,
} from "/static/kp/js/segmented.js";
import {
  TOOLBAR_CLEAR_EVENT,
  TOOLBAR_REMOVE_EVENT,
  attachToolbars,
  setToolbarActive,
} from "/static/kp/js/toolbar.js";

/** @typedef {import("../presetsview.js").PresetView} PresetView */

// Live view (invariant 39): every control on the page has its own id; the
// buttons that open a dialog are also the server-modelled forms they open
// (`homelab ui open new-stack | import | new-preset | preset <name>`).
/**
 * @param {string} id @param {string} what @param {string} [row]
 * @param {{shows?: string}} [more]
 */
const ctl = (id, what, row, more = {}) =>
  declare({
    id,
    page: "presets",
    opens: /^presets-(card|use|edit|start|import|new|empty)/.test(id)
      ? "dialog"
      : "view",
    what,
    ...(row ? { row } : {}),
    ...more,
  });
const TRY_AGAIN = declare({
  id: "presets-try-again",
  page: "presets",
  opens: "view",
  what: "read what failed again",
  shows: "after the presets could not be read",
});
const IMPORT = ctl("presets-import", "open the Import a bundle dialog");
const NEW_PRESET = ctl(
  "presets-new-preset",
  "open the presets editor for a new preset",
);
const NEW_STACK = ctl("presets-new-stack", "open the New stack wizard");
const NONE_IMPORT = ctl(
  "presets-import-first",
  "the empty page's Import a bundle…",
  undefined,
  { shows: "while there is no preset" },
);
const NONE_NEW_PRESET = ctl(
  "presets-new-preset-first",
  "the empty page's New preset…",
  undefined,
  { shows: "while there is no preset" },
);
const NONE_NEW_STACK = ctl(
  "presets-new-stack-first",
  "the empty page's New stack…",
  undefined,
  { shows: "while there is no preset" },
);
const SEARCH = ctl(
  "presets-search",
  "the gallery's search box (name, app or purpose)",
);
const SEARCH_FIELD = declareField({
  id: "ps-search",
  page: "presets",
  what: "the preset search: a name, an app or a purpose",
});
const USE = ctl(
  "presets-use",
  "Use this preset: the New stack wizard on that preset",
  "<preset>",
);
const EDIT = ctl(
  "presets-edit",
  "open the presets editor on one preset",
  "<preset>",
);
const START_EMPTY = ctl(
  "presets-start-empty",
  "the Empty stack card: the New stack wizard with no apps",
);

const MODE = declare({
  id: "presets-search-mode",
  page: "presets",
  opens: "view",
  row: "intent|text",
  what: "search the presets by meaning (gpu media finds the media preset) or by the letters of their name",
});
const NEEDS = declare({
  id: "presets-needs",
  page: "presets",
  opens: "view",
  row: "all|gpu|vpn",
  what: "keep only the presets that need a GPU or a VPN (both on: those that need both)",
});
const CLEAR = declare({
  id: "presets-clear-search",
  page: "presets",
  opens: "view",
  what: "clear the gallery's search and filters",
  shows: "when the search matches no preset",
});

const MODE_KEY = "homelab.presets.search";

/**
 * @param {HTMLElement} root
 * @param {{navigate: (href: string) => void}} ctx
 * @returns {() => void}
 */
export function mount(root, ctx) {
  ensureStyle("/css/pages/config.css");
  ensureStyle("/css/pages/presets.css");
  root.classList.add("cf-page", "ps-page");
  const abort = new AbortController();
  const q0 = new URLSearchParams(location.search);
  /** @type {"intent" | "text"} */
  let mode = "intent";
  try {
    if (localStorage.getItem(MODE_KEY) === "text") mode = "text";
  } catch {
    // the choice lasts for this visit
  }
  /** @type {{presets: PresetView[], suggest_vmid: number | null, working_copy: boolean, sync_error: string | null, swap: {divisor: number, min_mb: number, max_mb: number} | null} | null} */
  let data = null;
  /** @type {"loading" | "failed" | "ready"} */
  let status = "loading";
  let failWhy = "";

  /** @param {string} [preset] */
  const useStack = (preset) =>
    void openNewStack(ctx.navigate, preset ? { preset } : {});

  // ── header ────────────────────────────────────────────────────────────
  const ago = agoEl("read");
  /** @param {string} label @param {string} title @param {string} form @param {string} drive @param {() => void} go @param {string} [cls] */
  const action = (
    label,
    title,
    form,
    drive,
    go,
    cls = "kp-button--secondary",
  ) =>
    drivable(
      viaForm(
        h(
          "button",
          { type: "button", class: `kp-button ${cls}`, title, onclick: go },
          label,
        ),
        form,
      ),
      drive,
    );
  const head = pageHeader({
    title: "Presets",
    desc: "Ready-made stacks from the repository's presets/ directory. Pick one to start a new stack with its apps and sizes filled in; you choose the name and the container number.",
    titleMeta: [h("span", { class: "cf-live" }, ago)],
    actions: [
      action(
        "Import a bundle…",
        "Add a stack from a bundle someone shared (an export of another homelab)",
        "import",
        IMPORT,
        () => void openImport(ctx.navigate),
      ),
      action(
        "New preset…",
        "Write a new preset into presets/: its size, apps and files",
        "new-preset",
        NEW_PRESET,
        () => void openPresetEditor(null, retry),
      ),
    ],
    primary: action(
      "New stack…",
      "Start a new stack; choose a preset in the first step",
      "new-stack",
      NEW_STACK,
      () => useStack(),
      "kp-button--primary",
    ),
  });

  // ── toolbar (the package's) ───────────────────────────────────────────
  const search = /** @type {HTMLInputElement} */ (
    h("input", {
      type: "search",
      class: "kp-field__input kp-toolbar__input",
      placeholder: "Find a preset by name, app or purpose",
      "aria-label": "Find a preset",
      id: SEARCH_FIELD,
    })
  );
  search.value = q0.get("q") ?? "";
  drivable(search, SEARCH);
  const needs = h(
    "div",
    {
      class: "kp-segmented kp-segmented--chips",
      "data-kp-segmented": "multi",
      role: "group",
      "aria-label": "Needs",
      id: "ps-needs",
    },
    ...[
      ["all", "All"],
      ["gpu", "GPU"],
      ["vpn", "VPN"],
    ].map(([v, l]) =>
      drivable(
        h(
          "button",
          {
            type: "button",
            class: "kp-segmented__item",
            "data-value": v,
            ...(v === "all" ? { "data-kp-segmented-all": "" } : {}),
            "aria-pressed": v === "all" ? "true" : "false",
          },
          h("span", { class: "kp-segmented__label" }, l),
        ),
        NEEDS,
        v,
      ),
    ),
  );
  const modeSeg = segSwitch({
    label: "Search by",
    value: mode,
    items: [
      {
        value: "intent",
        label: "By meaning",
        hint: "Every word you type must start a word of the preset's name, purpose or apps, in any order",
      },
      {
        value: "text",
        label: "Letters in the name",
        hint: "The letters you type must be inside the preset's name",
      },
    ],
    onChange: (v) => {
      mode = v === "text" ? "text" : "intent";
      try {
        localStorage.setItem(MODE_KEY, mode);
      } catch {
        // kept for this visit only
      }
      paint();
    },
    mark: (b, v) => void drivable(b, MODE, v),
  });
  const count = h("div", { class: "kp-toolbar__state", "aria-live": "polite" });
  const tb = h(
    "div",
    {
      class: "kp-toolbar ps-tb",
      "data-kp-toolbar": "",
      role: "group",
      "aria-label": "Presets",
    },
    h("div", { class: "kp-toolbar__search" }, search),
    h(
      "div",
      { class: "kp-toolbar__filters" },
      h(
        "div",
        {
          class: "kp-toolbar__group",
          role: "group",
          "aria-labelledby": "ps-needs-l",
        },
        h("b", { class: "kp-toolbar__label", id: "ps-needs-l" }, "Needs"),
        needs,
      ),
      h(
        "div",
        {
          class: "kp-toolbar__group",
          role: "group",
          "aria-labelledby": "ps-mode-l",
        },
        h("b", { class: "kp-toolbar__label", id: "ps-mode-l" }, "Search by"),
        modeSeg.el,
      ),
    ),
    count,
    h("div", { class: "kp-toolbar__active", hidden: "" }),
  );
  const grid = h("div", {
    class: "kp-entity-grid ps-gallery",
    "data-kp-entity-grid": "",
    "data-kp-entity-min": "18rem",
    "data-kp-entity-meters": "0",
    "aria-label": "Presets",
  });
  const below = h("div", { class: "ps-below" });
  const compare = presetsCompare({
    swap: () => data?.swap ?? null,
    retry: () => retry(),
  });
  root.replaceChildren(head.el, tb, grid, below, compare.el);
  const detachSeg = attachSegmented(root);
  const detachTb = attachToolbars(root);

  const wanted = () =>
    getSegmentedValues(needs).filter((v) => v === "gpu" || v === "vpn");
  const keepUrl = () =>
    history.replaceState(
      history.state,
      "",
      `${location.pathname}${setParams(location.search, { q: search.value.trim().toLowerCase() })}`,
    );

  /** @param {PresetView} p */
  const match = (p) => {
    const q = search.value.trim();
    const words = [p.name, p.description, ...(p.apps ?? [])].join(" ");
    const okText =
      !q ||
      (mode === "intent"
        ? matchIntent(words, q, { typo: 1 })
        : p.name.toLowerCase().includes(q.toLowerCase()));
    return okText && wanted().every((n) => (n === "gpu" ? p.gpu : p.vpn));
  };

  /** @param {PresetView} p @returns {import("/static/kp/js/entity-card.js").EntityData} */
  const entity = (p) => {
    const has = sets(p);
    const empty = (p.apps ?? []).length === 0;
    return {
      key: p.name,
      name: p.name,
      state: { label: empty ? "empty stack" : "preset", tone: "info" },
      figures: [
        { value: ramText(p.ram_mb), label: "RAM" },
        {
          value: has.cores ? String(p.cores) : `${p.cores} (default)`,
          label: "Cores",
        },
        {
          value: has.disk ? `${p.disk_gb} GB` : `${p.disk_gb} GB (default)`,
          label: "Disk",
        },
      ],
      flags: [
        ...(p.gpu
          ? [{ label: "GPU", tone: /** @type {const} */ ("info") }]
          : []),
        ...(p.vpn
          ? [{ label: "VPN", tone: /** @type {const} */ ("info") }]
          : []),
      ],
      actions: [
        { id: "use", label: empty ? "Start empty" : "Use" },
        { id: "edit", label: "Edit" },
      ],
    };
  };

  /** Mark what the cards drew so Live view reaches it. */
  const decorate = () => {
    for (const card of /** @type {HTMLElement[]} */ ([
      ...grid.querySelectorAll(":scope > .kp-card--entity"),
    ])) {
      const key = card.getAttribute("data-kp-entity-key") ?? "";
      const p = data?.presets.find((x) => x.name === key);
      const buttons = /** @type {HTMLElement[]} */ ([
        ...card.querySelectorAll("button.kp-entity__action"),
      ]);
      const [use, edit] = buttons;
      if (use && !use.dataset.drive) {
        viaForm(use, "new-stack");
        if ((p?.apps ?? []).length === 0) drivable(use, START_EMPTY);
        else drivable(use, USE, key);
      }
      if (edit && !edit.dataset.drive) {
        viaForm(edit, "preset");
        drivable(edit, EDIT, key);
      }
    }
  };
  new MutationObserver(decorate).observe(grid, {
    childList: true,
    subtree: true,
  });

  const paint = () => {
    modeSeg.set(mode);
    const tags = [
      ...wanted().map((n) => ({ id: n, label: `Needs: ${n.toUpperCase()}` })),
      ...(search.value.trim()
        ? [{ id: "q", label: `Search: ${search.value.trim()}` }]
        : []),
    ];
    setToolbarActive(tb, tags);
    below.replaceChildren();
    if (status === "loading") {
      count.textContent = "";
      setEntityGridState(grid, "loading", { count: 6 });
      return;
    }
    if (status === "failed") {
      count.textContent = "";
      setEntityGridState(grid, "error", { message: failWhy });
      return;
    }
    if (!data) return;
    const all = data.presets;
    if (!all.length) {
      count.textContent = "";
      setEntityGridState(grid, "empty", {
        title: "No presets in this working copy",
        message:
          data.working_copy === false
            ? "The dashboard has no working copy yet, so it lists no presets."
            : "The working copy's presets/ directory is empty or missing.",
        actionLabel: "New preset…",
      });
      below.replaceChildren(noPresets());
      return;
    }
    const empty = emptyPreset(all);
    const list = all.filter(match).sort((a, b) => {
      // The app-less preset stays last.
      if (a === empty) return 1;
      if (b === empty) return -1;
      return a.name.localeCompare(b.name);
    });
    const listed = all.filter((p) => p !== empty);
    count.textContent = countText(
      list.filter((p) => p !== empty).length,
      listed.length,
      tags.length ? "x" : "",
      data.suggest_vmid,
    );
    if (!list.length) {
      setEntityGridState(grid, "empty", {
        title: "No preset matches",
        message: search.value.trim()
          ? `Nothing matches “${search.value.trim()}”. Fewer words or another spelling may.`
          : "No preset needs all of that. Clear the filters to see every preset.",
        actionLabel: "Clear the search",
      });
      return;
    }
    setEntityGrid(grid, list.map(entity));
    decorate();
  };

  search.addEventListener("input", () => {
    keepUrl();
    paint();
  });
  needs.addEventListener(SEGMENTED_EVENT, paint);
  tb.addEventListener(TOOLBAR_REMOVE_EVENT, (e) => {
    const id = /** @type {CustomEvent} */ (e).detail.id;
    if (id === "q") search.value = "";
    else
      setSegmentedValues(
        needs,
        wanted().filter((n) => n !== id),
      );
    keepUrl();
    paint();
  });
  tb.addEventListener(TOOLBAR_CLEAR_EVENT, () => {
    search.value = "";
    setSegmentedValues(needs, []);
    keepUrl();
    paint();
  });
  grid.addEventListener(ENTITY_NEW_EVENT, () => {
    if (!data?.presets.length) {
      void openPresetEditor(null, retry);
      return;
    }
    search.value = "";
    setSegmentedValues(needs, []);
    keepUrl();
    paint();
  });
  grid.addEventListener(ENTITY_GRID_RETRY_EVENT, () => retry());
  grid.addEventListener(ENTITY_ACTION_EVENT, (e) => {
    const { key, id } = /** @type {CustomEvent} */ (e).detail;
    if (id === "use") useStack(key);
    else void openPresetEditor(key, retry);
  });
  // The empty plate's button is a Live view control too.
  new MutationObserver(() => {
    const b = grid.querySelector(".kp-stateplate button");
    if (b instanceof HTMLElement && !b.dataset.drive && data?.presets.length)
      drivable(b, CLEAR);
  }).observe(grid, { childList: true, subtree: true });

  /** The page with no presets: why, and the three ways on. */
  const noPresets = () =>
    h(
      "section",
      { class: "kp-card nx-card ps-none", "aria-labelledby": "ps-none-h" },
      h(
        "div",
        { class: "nx-card__head" },
        h("h2", { id: "ps-none-h" }, "No presets in this working copy"),
        h(
          "p",
          { class: "section-head__desc" },
          data?.working_copy === false
            ? "The dashboard has no working copy yet, so it lists no presets. The Settings page shows the working copy's state."
            : "The working copy's presets/ directory is empty or missing. A preset is one folder with a preset.yml and its apps.",
        ),
      ),
      h(
        "div",
        { class: "ps-next" },
        h(
          "div",
          null,
          h("strong", null, "Write one"),
          "New preset… asks for a name, sizes and apps and commits the folder.",
          action(
            "New preset…",
            "Write a new preset into presets/",
            "new-preset",
            NONE_NEW_PRESET,
            () => void openPresetEditor(null, retry),
            "kp-button--sm",
          ),
        ),
        h(
          "div",
          null,
          h("strong", null, "Import one"),
          "A bundle from another homelab or a backup of your own.",
          action(
            "Import a bundle…",
            "Add a stack from a bundle",
            "import",
            NONE_IMPORT,
            () => void openImport(ctx.navigate),
            "kp-button--sm",
          ),
        ),
        h(
          "div",
          null,
          h("strong", null, "Or start a stack"),
          "The New stack wizard says what it needs before anything is written.",
          action(
            "New stack…",
            "Start a new stack",
            "new-stack",
            NONE_NEW_STACK,
            () => useStack(),
            "kp-button--sm",
          ),
        ),
      ),
    );

  const load = async () => {
    status = "loading";
    compare.set({ status: "loading" });
    paint();
    const r = await fetchJson("/data/presets", "the presets", abort.signal);
    if (!r.ok) {
      status = "failed";
      failWhy = `${r.error.what}: ${r.error.why}`;
      paint();
      compare.set({ status: "failed", why: failWhy });
      return;
    }
    data = {
      presets: r.body.presets ?? [],
      suggest_vmid: r.body.suggest_vmid ?? null,
      working_copy: r.body.working_copy !== false,
      sync_error: r.body.sync_error ?? null,
      swap: r.body.swap ?? null,
    };
    status = "ready";
    setAgo(ago, Date.now() / 1000);
    paint();
    compare.set({ presets: data.presets, status: "ready" });
  };
  const retry = () => void load().catch(() => {});
  paint();
  retry();
  const hostWatch = setInterval(() => compare.host(), 5000);
  return () => {
    abort.abort();
    clearInterval(hostWatch);
    detachSeg();
    detachTb();
    compare.stop();
    root.classList.remove("cf-page", "ps-page");
  };
}
