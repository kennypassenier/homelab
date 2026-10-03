// feat-pages-1 (chassis-rs 3.1.0, nav-decisions): the kit's own Passkeys
// page, drawn by this app from `GET /api/kit/passkeys`. Registration
// reuses the kit's own `/static/passkeys.js` (the two WebAuthn
// ceremonies against `/passkeys/register/start|finish`) instead of
// re-encoding challenges here.

import { send } from "../act.js";
import { errorBox, fetchJson, h } from "../dom.js";
// @ts-ignore — a kit asset, not part of this app's own module graph.
import { registerPasskey } from "/static/passkeys.js";

/**
 * @typedef {{id: string, label: string, created_at: string,
 *   last_used_at: string | null}} PasskeyView
 * @typedef {{https: boolean, public_url: string | null,
 *   passkeys: PasskeyView[]}} PasskeysData
 */

/**
 * @param {PasskeyView} p
 * @param {() => void} onChanged
 */
function passkeyRow(p, onChanged) {
  const note = h("span", { class: "measured" });
  const del = h(
    "button",
    { type: "button", class: "kp-button kp-button--sm kp-button--destructive" },
    "Delete",
  );
  del.addEventListener("click", async () => {
    if (!confirm(`Delete the passkey "${p.label}"?`)) return;
    del.disabled = true;
    const r = await send(
      "DELETE",
      `/api/passkeys/${p.id}`,
      undefined,
      "delete the passkey",
    );
    del.disabled = false;
    if (r.ok) onChanged();
    else note.textContent = `refused: ${r.error.why}`;
  });
  return h(
    "li",
    { class: "kp-card" },
    h("strong", null, p.label),
    h(
      "p",
      { class: "measured" },
      `created ${p.created_at}`,
      p.last_used_at ? ` · last used ${p.last_used_at}` : " · never used",
    ),
    h("div", { class: "kp-row" }, del, note),
  );
}

/**
 * @param {() => void} onRegistered
 */
function registerForm(onRegistered) {
  const label = h("input", {
    class: "kp-field__input",
    type: "text",
    placeholder: "this laptop",
  });
  const status = h("p", { class: "measured" });
  const btn = h(
    "button",
    { type: "button", class: "kp-button kp-button--primary" },
    "Register a passkey",
  );
  btn.addEventListener("click", async () => {
    btn.disabled = true;
    try {
      await registerPasskey(
        /** @type {HTMLInputElement} */ (label).value,
        status,
      );
      onRegistered();
    } catch (e) {
      status.textContent = e instanceof Error ? e.message : String(e);
    } finally {
      btn.disabled = false;
    }
  });
  return h(
    "section",
    { class: "kp-card kp-mb-lg" },
    h("h2", { class: "kp-card__title kp-mt-0 kp-fs-md" }, "Add a passkey"),
    h(
      "div",
      { class: "kp-row kp-row--end kp-gap-md" },
      h(
        "div",
        { class: "kp-field" },
        h("label", { class: "kp-field__label" }, "Label"),
        label,
      ),
      btn,
    ),
    status,
  );
}

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const status = h("p", { class: "measured", role: "status" }, "Reading…");
  const body = h("div", { class: "passkeys-page" });
  root.replaceChildren(
    h("h1", null, "Sign-in"),
    h(
      "p",
      { class: "section-head__desc measured" },
      "The passkeys that can sign in to this dashboard; add or remove one here.",
    ),
    status,
    body,
  );
  const abort = new AbortController();

  const load = async () => {
    const r = await fetchJson(
      "/api/kit/passkeys",
      "the passkeys page",
      abort.signal,
    );
    if (!r.ok) {
      status.textContent = "";
      body.replaceChildren(errorBox(r.error));
      return;
    }
    /** @type {PasskeysData} */
    const d = r.body;
    status.textContent = "";
    /** @type {Node[]} */
    const out = [];
    if (!d.https) {
      out.push(
        h(
          "div",
          { class: "kp-alert kp-alert--warning" },
          h(
            "div",
            { class: "kp-alert__body" },
            "This dashboard is not reached over HTTPS (or the public URL is not set), so the browser's passkey API is unavailable here.",
          ),
        ),
      );
    } else {
      out.push(registerForm(retry));
    }
    out.push(
      h(
        "ul",
        { class: "passkeys-list" },
        ...(d.passkeys.length
          ? d.passkeys.map((p) => passkeyRow(p, retry))
          : [h("li", { class: "measured" }, "No passkeys registered yet.")]),
      ),
    );
    body.replaceChildren(...out);
  };
  const retry = () => void load().catch(() => {});
  retry();
  return () => abort.abort();
}
