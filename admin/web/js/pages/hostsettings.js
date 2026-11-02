// The Host page's Host settings card: host.toml as the host reads it,
// read-only here (Settings changes it), as the dashboard's data table.

import { countOf, section, segSwitch } from "../ui.js";
import { fetchJson, h, labelCells, tableBlock } from "../dom.js";
import { declare, drivable } from "../drivable.js";
import { hostSettingsView } from "../host.js";
import { attachDataTables } from "/static/kp/dist/kp-themes.min.js";

const SETTINGS_VIEW = declare({
  id: "host-settings-view",
  page: "host",
  opens: "run",
  row: "changed|all",
  what: "show only the host settings host.toml changes, or all of them",
});
const SORT_SETTINGS = declare({
  id: "sort-host-settings",
  page: "host",
  opens: "view",
  row: "setting|group|value|default",
  what: "sort the host settings table: each header clicked is a further sort key (again reverses it, a third time takes it out)",
});

/** @param {import("./hostenv.js").HostEnv} env */
export function createSettings(env) {
  const { abort } = env;
  /** @type {import("../host.js").SettingRow[]} */
  let sRows = [];
  let sAll = false;
  const sDesc = h(
    "span",
    null,
    "host.toml as the host reads it. Read-only here; Settings changes them.",
  );
  const sView = segSwitch({
    label: "Which settings",
    value: "changed",
    items: [
      {
        value: "changed",
        label: "Changed",
        hint: "Only the settings that differ from their default",
      },
      { value: "all", label: "All", hint: "Every setting host.toml knows" },
    ],
    onChange: (v) => {
      sAll = v === "all";
      paintSettings();
    },
    drive: { id: SETTINGS_VIEW },
  });
  // The settings are the dashboard's data table: its own search, its own
  // sort keys (Live view reaches each header) and the package's loading,
  // empty and failed states.
  const sBlock = tableBlock({
    remember: "host-settings",
    caption: "Host settings as host.toml sets them",
    captionHidden: true,
    search: "Search settings",
    state: "loading",
    nothing: "host.toml changes nothing: every setting is at its default.",
    columns: [
      { label: "Setting", sort: "text" },
      { label: "Group", sort: "text" },
      { label: "Value", sort: "text" },
      { label: "Default", sort: "text" },
    ],
  });
  const sBody = sBlock.tbody;
  sBody.id = "host-settings";
  [...sBlock.wrap.querySelectorAll("thead th")].forEach((th, i) =>
    drivable(
      /** @type {HTMLElement} */ (th),
      SORT_SETTINGS,
      ["setting", "group", "value", "default"][i] ?? "setting",
    ),
  );
  const settingsCard = section({
    id: "host-settings-card",
    title: "Host settings",
    desc: "",
    collapsible: true,
    open: true,
    tools: [
      h(
        "a",
        {
          class: "kp-button kp-button--sm",
          href: "/settings",
          title: "Change host.toml on the Settings page",
        },
        "Edit in Settings",
      ),
    ],
  });
  /** @type {HTMLElement} */ (
    settingsCard.el.querySelector(".section-head__desc")
  ).replaceChildren(sDesc);
  settingsCard.body.append(
    h("div", { class: "hk-filters" }, sView.el),
    sBlock.wrap,
  );
  const paintSettings = () => {
    const shown = sRows.filter((x) => sAll || x.set);
    sBody.replaceChildren(
      ...shown.map((x) =>
        h(
          "tr",
          {
            class: x.set ? "hk-set-row" : null,
            "data-key": x.key,
            "data-set": x.set ? "1" : "0",
          },
          h(
            "td",
            null,
            h(
              "div",
              { class: "hk-name" },
              h("span", null, x.label),
              h("small", { class: "mono" }, x.key),
            ),
          ),
          h("td", { class: "hk-muted" }, x.group),
          h("td", { class: x.set ? "mono" : "mono hk-muted" }, x.value),
          h("td", { class: "mono hk-muted" }, x.def),
        ),
      ),
    );
    labelCells(sBody, () => ["Setting", "Group", "Value", "Default"]);
    sBlock.ready();
  };
  const loadSettings = async () => {
    sBlock.loading({ words: "Reading host.toml from the host…" });
    const r = await fetchJson(
      "/data/host-settings",
      "the host settings",
      abort.signal,
    );
    if (!r.ok) {
      sBlock.failed(r.error);
      return;
    }
    const v = hostSettingsView(r.body.page);
    sRows = v.rows;
    sDesc.textContent = `host.toml as the host reads it: ${v.changed} of ${v.rows.length} settings changed from their default. Read-only here; Settings changes them.`;
    paintSettings();
    // redesign-final (exact counts): the changed settings are the table's
    // rows marked changed.
    countOf(sDesc, sBody, ':scope > tr[data-set="1"]');
    sView.counts({ changed: v.changed, all: v.rows.length });
  };
  /** Wires the package's data table to the card; returns its cleanup. */
  const attach = () => attachDataTables(settingsCard.body, {});
  return { card: settingsCard, load: loadSettings, attach };
}
