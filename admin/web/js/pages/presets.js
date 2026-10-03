// The Presets page (redesign-presets, release 3.71.0; Kenny approved the
// demo 2026-10-03: ~/.local/share/homelab/redesign-3.71/presets.html,
// implemented exactly). The repository's presets/ directory as a gallery:
// one card per preset with its mark, purpose, apps and size (RAM, cores,
// disk — "default" where the preset leaves it to the fleet), "Use this
// preset" and "Edit"; the app-less preset (presets/custom) is the dashed
// "Empty stack" card at the end. A search and an A–Z / Largest first
// switch sit above it; with no presets at all the page says why and offers
// the three ways on (write one, import one, start empty).
//
// "Use this preset" opens the existing New stack wizard with that preset
// chosen (feat-stacks-3: name and number, size, data, plan and commit,
// replayed by Live view), "Edit" the presets editor (feat-preset-1).

import { agoEl, setAgo } from "../ago.js";
import { fetchJson, h } from "../dom.js";
import { declare, drivable, viaForm } from "../drivable.js";
import { openImport } from "../importstack.js";
import { openNewStack } from "../newstack.js";
import { openPresetEditor } from "../presetseditor.js";
import {
  countText,
  emptyPreset,
  galleryCards,
  ramText,
  sets,
} from "../presetsview.js";
import {
  chip,
  ensureStyle,
  failBox,
  highlight,
  pageHeader,
  pageTip,
  segSwitch,
  skeleton,
  toolbar,
} from "../ui.js";
import { setParams } from "../urlstate.js";

/** @typedef {import("../presetsview.js").PresetView} PresetView */

// Live view (invariant 39): every control on the page has its own id; the
// buttons that open a dialog are also the server-modelled forms they open
// (`homelab ui open new-stack | import | new-preset | preset <name>`).
/** @param {string} id @param {string} what @param {string} [row] */
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
const SORT = ctl(
  "presets-sort",
  "sort the gallery A–Z or largest first",
  "name|ram",
);
const CARD = ctl(
  "presets-card",
  "a preset's card: opens the New stack wizard on it",
  "<preset>",
);
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
const EMPTY_CARD = ctl(
  "presets-empty-card",
  "the Empty stack card itself: the New stack wizard with no apps",
);
const CLEAR = ctl(
  "presets-clear-search",
  "clear the gallery's search",
  undefined,
  {
    shows: "when the search matches no preset",
  },
);

const HUES = [
  "var(--chart-1)",
  "var(--chart-2)",
  "var(--chart-3)",
  "var(--chart-4)",
  "var(--chart-5)",
];

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
  const S = {
    q: (q0.get("q") ?? "").toLowerCase(),
    /** @type {"name" | "ram"} */
    sort: q0.get("sort") === "ram" ? "ram" : "name",
  };
  const keepUrl = () =>
    history.replaceState(
      history.state,
      "",
      `${location.pathname}${setParams(location.search, { q: S.q, sort: S.sort === "ram" ? "ram" : null })}`,
    );
  /** @type {{presets: PresetView[], suggest_vmid: number | null, working_copy: boolean, sync_error: string | null} | null} */
  let data = null;

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

  // ── toolbar ───────────────────────────────────────────────────────────
  const sortSeg = segSwitch({
    label: "Sort",
    items: [
      { value: "name", label: "A–Z", hint: "Sort the presets by name" },
      {
        value: "ram",
        label: "Largest first",
        hint: "Sort the presets by memory, largest first",
      },
    ],
    value: S.sort,
    mark: (b, v) => void drivable(b, SORT, v),
    onChange: (v) => {
      S.sort = v === "ram" ? "ram" : "name";
      keepUrl();
      paint();
    },
  });
  const count = h("span", { class: "cf-count" });
  const tb = toolbar({
    search: {
      placeholder: "Find a preset by name, app or purpose",
      label: "Find a preset",
      value: S.q,
      onInput: (v) => {
        S.q = v.trim().toLowerCase();
        keepUrl();
        paint();
      },
    },
    groups: [],
    state: [sortSeg.el, count],
  });
  tb.el.classList.add("ps-tb");
  if (tb.search) drivable(tb.search, SEARCH);

  const gallery = h("section", {
    class: "ps-gallery",
    "aria-label": "Presets",
  });
  const below = h("div", { class: "ps-below" });
  root.replaceChildren(head.el, tb.el, gallery, below);

  // ── cards ─────────────────────────────────────────────────────────────
  /** @param {PresetView} p @param {number} i */
  const card = (p, i) => {
    const has = sets(p);
    const use = viaForm(
      h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--secondary",
          title: `New stack from ${p.name}: name and container number next`,
          "data-preset-use": p.name,
          onclick: (/** @type {Event} */ e) => {
            e.stopPropagation();
            useStack(p.name);
          },
        },
        "Use this preset",
      ),
      "new-stack",
    );
    drivable(use, USE, p.name);
    const edit = viaForm(
      h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--ghost",
          title: `Edit presets/${p.name}/preset.yml and its apps`,
          "data-preset-edit": p.name,
          onclick: (/** @type {Event} */ e) => {
            e.stopPropagation();
            void openPresetEditor(p.name, retry);
          },
        },
        "Edit",
      ),
      "preset",
    );
    drivable(edit, EDIT, p.name);
    const gpu = p.gpu ? chip("GPU", { tone: "info" }) : null;
    const vpn = p.vpn ? chip("VPN", { tone: "info" }) : null;
    /** @param {string} label @param {string} value @param {boolean} set */
    const spec = (label, value, set) => {
      const b = h(
        "b",
        { class: set ? null : "ps-def" },
        set ? value : "default",
      );
      if (!set)
        pageTip().attach(b, () => [
          h("b", null, "Not set by the preset"),
          h(
            "span",
            null,
            `The new-stack form fills in the fleet default (${value}); you can change it there.`,
          ),
        ]);
      return h("div", null, h("span", null, label), b);
    };
    const c = h(
      "article",
      {
        class: "ps-card",
        tabindex: "0",
        "data-preset": p.name,
        title: `Start a new stack from ${p.name} (Enter)`,
        onclick: () => useStack(p.name),
        onkeydown: (/** @type {KeyboardEvent} */ e) => {
          if (e.key === "Enter" && e.target === c) useStack(p.name);
        },
      },
      h(
        "div",
        { class: "ps-card__top" },
        h(
          "span",
          { class: "ps-card__mono", "aria-hidden": "true" },
          p.name.slice(0, 2),
        ),
        h("h3", null, highlight(p.name, S.q)),
        h("span", { class: "cf-row" }, gpu, vpn),
      ),
      h("p", null, highlight(p.description, S.q)),
      h(
        "div",
        { class: "ps-apps" },
        (p.apps ?? []).map((a) => chip(highlight(a, S.q))),
      ),
      h(
        "div",
        { class: "ps-spec" },
        spec("RAM", ramText(p.ram_mb), true),
        spec("Cores", String(p.cores), has.cores),
        spec("Disk", `${p.disk_gb} GB`, has.disk),
      ),
      h("div", { class: "ps-card__foot" }, use, edit),
    );
    c.style.setProperty("--c", HUES[i % HUES.length]);
    if (gpu)
      pageTip().attach(gpu, () => [
        h("b", null, "GPU passthrough"),
        h(
          "span",
          null,
          "The host's /dev/dri is passed into the container for VAAPI transcoding.",
        ),
      ]);
    if (vpn)
      pageTip().attach(vpn, () => [
        h("b", null, "Tunnel device"),
        h("span", null, "The container gets /dev/net/tun for a VPN client."),
      ]);
    drivable(c, CARD, p.name);
    return c;
  };

  /** @param {PresetView} p */
  const emptyCard = (p) => {
    const start = viaForm(
      h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--secondary",
          title: `A new stack with no apps (from presets/${p.name})`,
          "data-preset-use": p.name,
          onclick: (/** @type {Event} */ e) => {
            e.stopPropagation();
            useStack(p.name);
          },
        },
        "Start empty",
      ),
      "new-stack",
    );
    drivable(start, START_EMPTY);
    const c = h(
      "article",
      {
        class: "ps-card ps-card--empty",
        tabindex: "0",
        title: "Start a new stack with no apps (Enter)",
        onclick: () => useStack(p.name),
        onkeydown: (/** @type {KeyboardEvent} */ e) => {
          if (e.key === "Enter" && e.target === c) useStack(p.name);
        },
      },
      h(
        "div",
        { class: "ps-card__top" },
        h("span", { class: "ps-card__mono", "aria-hidden": "true" }, "+"),
        h("h3", null, "Empty stack"),
        h("span"),
      ),
      h(
        "p",
        null,
        "No apps yet: a container with the default size. Add apps from the stack's own page afterwards.",
      ),
      h("span"),
      h("span"),
      h("div", { class: "ps-card__foot" }, start),
    );
    c.style.setProperty("--c", "var(--muted-foreground)");
    drivable(c, EMPTY_CARD);
    return c;
  };

  const skeletonCard = () =>
    h(
      "article",
      { class: "ps-card ps-card--sk", "aria-hidden": "true" },
      h(
        "div",
        { class: "ps-card__top" },
        h("span", { class: "ps-card__mono" }),
        h("h3", null, skeleton("60%")),
        h("span"),
      ),
      h("p", null, skeleton("90%")),
      h("div", { class: "ps-apps" }, skeleton("40%")),
      h(
        "div",
        { class: "ps-spec" },
        ...["RAM", "Cores", "Disk"].map((l) =>
          h("div", null, h("span", null, l), h("b", null, skeleton("3ch"))),
        ),
      ),
      h("div", { class: "ps-card__foot" }, skeleton("100%"), skeleton("100%")),
    );

  const paint = () => {
    if (!data) return;
    const all = data.presets;
    const empty = emptyPreset(all);
    const listed = all.filter((p) => p !== empty);
    if (!all.length) {
      tb.el.hidden = false;
      count.textContent = "";
      gallery.hidden = true;
      below.replaceChildren(noPresets());
      return;
    }
    gallery.hidden = false;
    below.replaceChildren();
    const cards = galleryCards(all, S.q, S.sort);
    count.textContent = countText(
      cards.length,
      listed.length,
      S.q,
      data.suggest_vmid,
    );
    gallery.replaceChildren(
      ...cards.map(card),
      ...(S.q && !cards.length
        ? [
            h(
              "div",
              { class: "cf-empty ps-nomatch" },
              h("strong", null, `No preset matches “${S.q}”`),
              drivable(
                h(
                  "button",
                  {
                    type: "button",
                    class: "cf-linkbtn",
                    onclick: () => {
                      S.q = "";
                      if (tb.search) tb.search.value = "";
                      keepUrl();
                      paint();
                    },
                  },
                  "Clear the search",
                ),
                CLEAR,
              ),
            ),
          ]
        : []),
      ...(empty ? [emptyCard(empty)] : []),
    );
  };

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

  gallery.replaceChildren(...Array.from({ length: 8 }, skeletonCard));
  count.replaceChildren(skeleton("14ch"));

  const load = async () => {
    const r = await fetchJson("/data/presets", "the presets", abort.signal);
    if (!r.ok) {
      gallery.hidden = true;
      below.replaceChildren(
        failBox(r.error, { run: retry, drive: { id: TRY_AGAIN } }),
      );
      count.textContent = "";
      return;
    }
    data = {
      presets: r.body.presets ?? [],
      suggest_vmid: r.body.suggest_vmid ?? null,
      working_copy: r.body.working_copy !== false,
      sync_error: r.body.sync_error ?? null,
    };
    setAgo(ago, Date.now() / 1000);
    paint();
  };
  const retry = () => void load().catch(() => {});
  retry();
  return () => {
    abort.abort();
    root.classList.remove("cf-page", "ps-page");
  };
}
