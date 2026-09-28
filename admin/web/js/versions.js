// TUI parity: the version warnings every page carries. The TUI shows an
// update badge when a newer homelab release is out (its u key updates the
// host); the dashboard shows the same, with the button that opens "Update
// the host", and says so when it is itself older than the host it talks to.

import { openAction } from "./actiondialog.js";
import { fetchJson, h } from "./dom.js";
import { versionNotes } from "./parity.js";
import { listen } from "./store.js";

/**
 * @param {HTMLElement} root the banner's place under the navigation
 * @returns {() => void}
 */
export function mountVersions(root) {
  /** @param {any} v */
  const paint = (v) => {
    const n = versionNotes(v);
    root.hidden = !n.update && !n.older;
    const parts = [];
    if (n.update) {
      const b = h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--primary",
          id: "update-host-open",
        },
        "Update the host…",
      );
      b.addEventListener(
        "click",
        () => void openAction("_host", "update-host"),
      );
      parts.push(
        h(
          "div",
          { class: "kp-alert kp-alert--info versions-update", role: "status" },
          h("span", { class: "kp-alert__body" }, n.update, " "),
          b,
        ),
      );
    }
    if (n.older)
      parts.push(
        h(
          "div",
          {
            class: "kp-alert kp-alert--warning versions-older",
            role: "status",
          },
          n.older,
        ),
      );
    root.replaceChildren(...parts);
  };
  const read = async () => {
    const r = await fetchJson("/data/versions", "the versions");
    if (r.ok) paint(r.body);
  };
  const offRelease = listen("release", (v) => paint(v));
  // The host restarts into another version: read the warnings again.
  const offLink = listen("link", (d) => {
    if (d?.up) void read();
  });
  void read();
  return () => {
    offRelease();
    offLink();
  };
}
