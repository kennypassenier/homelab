// TUI parity: the version warnings every page carries. The TUI shows an
// update badge when a newer homelab release is out (its u key updates the
// host); the dashboard shows the same, with the button that opens "Update
// the host", and says so when it is itself older than the host it talks to.

import { send } from "./act.js";
import { openAction } from "./actiondialog.js";
import { fetchJson, h } from "./dom.js";
import { outdatedPage, versionNotes } from "./parity.js";
import { listen } from "./store.js";

/**
 * @param {HTMLElement} root the banner's place under the navigation
 * @returns {() => void}
 */
export function mountVersions(root) {
  /** The dashboard version that served this page (its first read). */
  /** @type {string | null} */
  let loadedAs = null;
  /** @type {HTMLElement | null} */
  let outdated = null;
  /** @type {any} */
  let last = null;
  /** @param {any} v */
  const paint = (v) => {
    last = v;
    const n = versionNotes(v);
    root.hidden = !n.update && !n.older && !outdated;
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
    root.replaceChildren(...(outdated ? [outdated] : []), ...parts);
  };
  /** @param {string | null | undefined} serving */
  const checkOutdated = (serving) => {
    if (loadedAs == null) {
      loadedAs = serving ?? null;
      return;
    }
    const words = outdatedPage(loadedAs, serving);
    if (!words) return;
    // Nothing of the viewer's is lost: load the new release at once.
    const typing = document.activeElement?.matches(
      "input:not([type=checkbox]):not([type=radio]), textarea, select",
    );
    if (!document.querySelector("dialog[open]") && !typing) {
      location.reload();
      return;
    }
    // A dialog or a field in use: say so, and reload on the viewer's word —
    // or by itself once the dialog has closed and nothing is being typed
    // (Kenny, 2026-10-01: after Live view installed a new dashboard from a
    // dialog, the banner stayed up although the dialog was long closed).
    if (outdated) return;
    const retry = setInterval(() => {
      const busy = document.activeElement?.matches(
        "input:not([type=checkbox]):not([type=radio]), textarea, select",
      );
      if (!document.querySelector("dialog[open]") && !busy) {
        clearInterval(retry);
        location.reload();
      }
    }, 2000);
    const b = h(
      "button",
      { type: "button", class: "kp-button kp-button--primary" },
      "Reload",
    );
    b.addEventListener("click", () => location.reload());
    outdated = h(
      "div",
      {
        class: "kp-alert kp-alert--warning versions-outdated",
        role: "status",
      },
      h("span", { class: "kp-alert__body" }, words, " "),
      b,
    );
    paint(last);
  };
  const read = async () => {
    const r = await fetchJson("/data/versions", "the versions");
    if (!r.ok) return;
    paint(r.body);
    checkOutdated(r.body.dashboard);
    // fix-185: tell the driver which version this page actually runs now —
    // on every load and reconnect, not only while Live view follows, so
    // `homelab ui` can refuse a step before it lands on a stale page rather
    // than finding out from the dashboard's own guard afterwards. Best
    // effort: a page that cannot reach this route is no worse off than
    // before it existed.
    if (r.body.dashboard)
      void send(
        "POST",
        "/data/drive/attach",
        { page_version: r.body.dashboard },
        "reporting this page's version",
      );
  };
  const offRelease = listen("release", (v) => paint(v));
  // The host restarts into another version: read the warnings again.
  const offLink = listen("link", (d) => {
    if (d?.up) void read();
  });
  // The dashboard itself restarted (a release installed): the live channel
  // reconnects with a resync; the versions say whether this page is old.
  const offResync = listen("resync", () => void read());
  const offReopened = listen("reopened", () => void read());
  void read();
  return () => {
    offRelease();
    offLink();
    offResync();
    offReopened();
  };
}
