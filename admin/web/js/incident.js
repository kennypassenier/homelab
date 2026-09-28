// TUI parity (fix-131, `homelab incidents show <name>`): one incident
// bundle in a dialog: the error, the versions and the end of its
// transcript, secrets masked by the dashboard before it arrives.

import { openDialog } from "./actui.js";
import { errorBox, fetchJson, h } from "./dom.js";

/**
 * @param {string} name a bundle's name, as the incidents list shows it
 */
export async function openIncident(name) {
  const body = h(
    "div",
    { class: "incident-body" },
    h("p", { class: "measured" }, "Reading the bundle…"),
  );
  const d = openDialog({
    title: `Incident ${name}`,
    body: [body],
    id: "incident-dialog",
    wide: true,
  });
  const r = await fetchJson(
    `/data/incidents/${encodeURIComponent(name)}`,
    `incident ${name}`,
  );
  if (!r.ok) {
    body.replaceChildren(errorBox(r.error));
    return d;
  }
  body.replaceChildren(h("pre", { class: "incident-text mono" }, r.body.text));
  return d;
}

/**
 * A "Show" button for a row of an incidents table.
 * @param {string} name
 */
export function showButton(name) {
  const b = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--ghost",
      "data-incident": name,
    },
    "Show",
  );
  b.addEventListener("click", () => void openIncident(name));
  return h("td", null, b);
}
