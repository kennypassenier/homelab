// feat-pages-1 (chassis-rs 3.1.0, nav-decisions): the kit's own Passkeys,
// drawn by this app from `GET /api/kit/passkeys`. Registration reuses the
// kit's own `/static/passkeys.js` (the two WebAuthn ceremonies against
// `/passkeys/register/start|finish`) instead of re-encoding challenges
// here.
//
// redesign-settings (3.71.0, Kenny's approved demo settings.html): the
// passkeys are the left half of Settings' Sign-in card, beside the
// machine tokens. `/settings?section=sign-in` (where `/passkeys` goes,
// router.js) mounts this module (main.js VIEWS), which draws the whole
// Settings page opened at Sign-in; Settings itself draws the passkeys
// with `mountPasskeys`.

import { send } from "../act.js";
import { fetchJson } from "../dom.js";
import { declare, drivable } from "../drivable.js";
// @ts-ignore — a kit asset, not part of this app's own module graph.
import { registerPasskey } from "/static/passkeys.js";
import { el, failBox, skel } from "./configkit.js";
import { mount as mountSettings } from "./settings.js";

/**
 * @typedef {{id: string, label: string, created_at: string,
 *   last_used_at: string | null}} PasskeyView
 * @typedef {{https: boolean, public_url: string | null,
 *   passkeys: PasskeyView[]}} PasskeysData
 */

// Live view (invariant 39): Sign-in lives at /settings?section=sign-in.
const REGISTER = declare({
  id: "register-passkey",
  page: "passkeys",
  opens: "run",
  what: "register this device as a passkey (the browser asks for Touch ID, Windows Hello or a security key)",
});
const DELETE = declare({
  id: "delete-passkey",
  page: "passkeys",
  opens: "run",
  row: "<passkey label>",
  what: "delete one passkey after a confirmation",
});

const LABEL = declare({
  id: "passkey-label",
  page: "passkeys",
  opens: "view",
  what: "the name a new passkey is kept under",
});
const REGISTER_GO = declare({
  id: "register-passkey-go",
  page: "passkeys",
  opens: "run",
  what: "ask the browser for the passkey and keep it under the typed name",
});
const REGISTER_CANCEL = declare({
  id: "register-passkey-cancel",
  page: "passkeys",
  opens: "view",
  what: "fold the passkey form away without registering",
});

/**
 * `/settings?section=sign-in`: the Settings page, opened at Sign-in.
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  return mountSettings(root, { section: "sign-in" });
}

/**
 * The Passkeys half of the Sign-in card: a heading with Register a
 * passkey, then the list (or why there is none).
 * @param {HTMLElement} box
 * @returns {() => void}
 */
export function mountPasskeys(box) {
  const abort = new AbortController();
  const register = drivable(
    el(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm",
        title: "Register this device (Touch ID, Windows Hello, a security key)",
      },
      "Register a passkey",
    ),
    REGISTER,
  );
  const label = /** @type {HTMLInputElement} */ (
    el("input", {
      class: "kp-field__input",
      type: "text",
      id: "passkey-label",
      placeholder: "this laptop",
      "aria-label": "What to call this passkey",
    })
  );
  drivable(label, LABEL);
  const status = el("p", { class: "cf-hint", role: "status" });
  const go = el(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--sm kp-button--primary",
      title: "Ask the browser for the passkey and keep it under this name",
    },
    "Register",
  );
  drivable(go, REGISTER_GO);
  const cancel = el(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--sm kp-button--ghost",
      title: "Keep things as they are",
    },
    "Cancel",
  );
  drivable(cancel, REGISTER_CANCEL);
  const form = el(
    "div",
    { class: "si-register", hidden: true },
    el(
      "label",
      { class: "kp-field" },
      el("span", { class: "kp-field__label" }, "Name it"),
      label,
    ),
    el("div", { class: "cf-row" }, go, cancel),
    status,
  );
  const list = el("div", { class: "si-list" }, skel("60%"), skel("40%"));
  box.replaceChildren(
    el("div", { class: "si-col__head" }, el("h3", null, "Passkeys"), register),
    form,
    list,
  );
  register.addEventListener("click", () => {
    form.hidden = !form.hidden;
    if (!form.hidden) label.focus();
  });
  cancel.addEventListener("click", () => {
    form.hidden = true;
    status.textContent = "";
  });
  go.addEventListener("click", async () => {
    /** @type {HTMLButtonElement} */ (go).disabled = true;
    try {
      await registerPasskey(label.value, status);
      form.hidden = true;
      label.value = "";
      retry();
    } catch (e) {
      status.textContent = e instanceof Error ? e.message : String(e);
    } finally {
      /** @type {HTMLButtonElement} */ (go).disabled = false;
    }
  });

  /** @param {PasskeyView} p */
  const row = (p) => {
    const note = el("span", { class: "cf-hint" });
    const del = drivable(
      el(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--sm kp-button--ghost si-danger",
          title: `Delete the passkey "${p.label}"; it can no longer sign in`,
        },
        "Delete",
      ),
      DELETE,
      p.label,
    );
    del.addEventListener("click", async () => {
      if (!confirm(`Delete the passkey "${p.label}"?`)) return;
      /** @type {HTMLButtonElement} */ (del).disabled = true;
      const r = await send(
        "DELETE",
        `/api/passkeys/${p.id}`,
        undefined,
        "delete the passkey",
      );
      /** @type {HTMLButtonElement} */ (del).disabled = false;
      if (r.ok) retry();
      else note.textContent = `refused: ${r.error.why}`;
    });
    return el(
      "li",
      { class: "si-item" },
      el(
        "div",
        { class: "si-item__id" },
        el("strong", null, p.label || "unnamed"),
        el(
          "span",
          { class: "cf-hint" },
          `created ${p.created_at}${p.last_used_at ? ` · last used ${p.last_used_at}` : " · never used"}`,
        ),
        note,
      ),
      del,
    );
  };

  const load = async () => {
    const r = await fetchJson(
      "/api/kit/passkeys",
      "the passkeys",
      abort.signal,
    );
    if (!r.ok) {
      list.replaceChildren(failBox(r.error, retry));
      return;
    }
    /** @type {PasskeysData} */
    const d = r.body;
    /** @type {HTMLButtonElement} */ (register).disabled = !d.https;
    if (!d.https)
      register.title =
        "Passkeys need HTTPS: this dashboard is not reached over HTTPS (or its public URL is not set)";
    list.replaceChildren(
      ...(d.https
        ? []
        : [
            el(
              "div",
              { class: "kp-alert kp-alert--warning" },
              el(
                "div",
                { class: "kp-alert__body" },
                "This dashboard is not reached over HTTPS (or the public URL is not set), so the browser's passkey API is unavailable here.",
              ),
            ),
          ]),
      d.passkeys.length
        ? el("ul", { class: "si-items" }, ...d.passkeys.map(row))
        : el(
            "div",
            { class: "cf-empty" },
            el("strong", null, "No passkey yet"),
            el(
              "span",
              null,
              "Register one to sign in with this device instead of pasting the token.",
            ),
          ),
    );
  };
  const retry = () => void load().catch(() => {});
  retry();
  return () => abort.abort();
}
