// Presets (TUI parity, `homelab presets`): the repository's preset
// catalogue, each preset's size and apps; a new stack starts from one.

import { agoEl, setAgo } from "../ago.js";
import { humanMb } from "../fleet.js";
import { bindTableUrl, fetchJson, h, tableBlock, td } from "../dom.js";
import { openImport } from "../importstack.js";
import { openNewStack } from "../newstack.js";
import { openPresetEditor } from "../presetseditor.js";
import { sortKeys } from "../sortkeys.js";
import {
  attachDataTables,
  compare,
  dataTable,
} from "/static/kp/js/datatable.js";
import { viaForm } from "../drivable.js";

/**
 * The preset's name, as a button that opens the presets editor
 * (feat-preset-1) — editing a preset never touches its YAML by hand.
 * @param {string} name
 * @param {() => void} onChanged
 */
function presetNameCell(name, onChanged) {
  const btn = h(
    "button",
    { type: "button", class: "kp-link-button", "data-preset-edit": name },
    name,
  );
  // fix-239: Live view reaches it as `homelab ui open preset <name>`.
  viaForm(btn, "preset");
  btn.addEventListener("click", () => void openPresetEditor(name, onChanged));
  return btn;
}

/**
 * @param {HTMLElement} root
 * @param {{navigate: (href: string) => void}} ctx
 * @returns {() => void}
 */
export function mount(root, ctx) {
  const keys = sortKeys();
  const note = h("p", { class: "measured", role: "status" });
  const ago = agoEl("read");
  const t = tableBlock({
    remember: "presets",
    caption: "Presets",
    search: "Search presets",
    state: "loading",
    nothing:
      "No presets: the working copy's presets/ directory is empty or missing.",
    columns: [
      { label: "Preset", sort: "text" },
      { label: "Memory", sort: "size" },
      { label: "Cores", sort: "number" },
      { label: "Disk (GB)", sort: "number" },
      { label: "Apps", sort: "text" },
      { label: "What it is", sort: "text", cls: "wide" },
    ],
  });
  const newStack = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--primary",
      id: "presets-new",
    },
    "New stack from a preset…",
  );
  // fix-239: `homelab ui open new-stack`, `open import`, `open new-preset`.
  viaForm(newStack, "new-stack");
  newStack.addEventListener("click", () => void openNewStack(ctx.navigate));
  const importBtn = h(
    "button",
    { type: "button", class: "kp-button", id: "presets-import" },
    "Import a bundle…",
  );
  viaForm(importBtn, "import");
  importBtn.addEventListener("click", () => void openImport(ctx.navigate));
  const newPreset = h(
    "button",
    { type: "button", class: "kp-button", id: "presets-new-preset" },
    "New preset…",
  );
  viaForm(newPreset, "new-preset");
  newPreset.addEventListener("click", () => void openPresetEditor(null, retry));
  root.replaceChildren(
    h(
      "div",
      { class: "title-row" },
      h("h1", null, "Presets"),
      h("span", { class: "actions-row" }, importBtn, newPreset, newStack),
    ),
    h(
      "p",
      { class: "section-head__desc measured" },
      "Ready-made stack templates to start a new stack from.",
    ),
    note,
    t.wrap,
    h("p", null, ago),
  );
  const detach = attachDataTables(root, { compare: keys.compare(compare) });
  const table = dataTable(t.wrap);
  const unbind = bindTableUrl(table, "presets");
  const abort = new AbortController();
  const load = async () => {
    t.loading({ words: "Reading the presets from the working copy…" });
    const r = await fetchJson("/data/presets", "the presets", abort.signal);
    if (!r.ok) {
      t.failed(r.error);
      return;
    }
    const list = r.body.presets ?? [];
    note.textContent = r.body.working_copy
      ? `${list.length} preset(s) in the repository's presets/ directory.`
      : "The dashboard has no working copy yet, so it lists no presets.";
    t.tbody.replaceChildren(
      ...list.map((/** @type {any} */ p) => {
        return h(
          "tr",
          { "data-preset": p.name },
          h("td", null, presetNameCell(p.name, retry)),
          // The memory column sorts by the number behind the words.
          td(keys.note("size", humanMb(p.ram_mb), p.ram_mb), "num"),
          td(String(p.cores), "num"),
          td(String(p.disk_gb), "num"),
          td((p.apps ?? []).join(", ") || "no apps"),
          td(
            `${p.description}${p.gpu ? " · GPU" : ""}${p.vpn ? " · VPN" : ""}`,
          ),
        );
      }),
    );
    t.ready();
    setAgo(ago, Date.now() / 1000);
  };
  const retry = () => void load().catch(() => {});
  root.addEventListener("kp-datatable-retry", retry);
  retry();
  return () => {
    abort.abort();
    root.removeEventListener("kp-datatable-retry", retry);
    unbind();
    detach();
  };
}
