// feat-kp11demo-6 (Passkeys and machine tokens, on kp-themes 11): the
// Sign-in card of Settings. Passkeys are kp-themes' list of ways in
// (`passkeys`: when each was added and last used in words, the old ones
// flagged, a confirmation before one is removed, the warning when the page is
// not on https, loading, empty and failed filling the list); registering is a
// `data-kp-form` form and a QR that opens this page on a phone; machine
// tokens are rows of the same form grid, with the scope of each, and a token
// is shown once, when it is made, in kp's secret field.
//
// Registering one's own passkey uses the kit's `/static/passkeys.js` (the two
// WebAuthn ceremonies against `/passkeys/register/start|finish`).

import { send } from "./act.js";
import { openDialog, refusalCallout, notify } from "./actui.js";
import { fetchJson, h } from "./dom.js";
import { declare, declareField, drivable } from "./drivable.js";
import { driven, register } from "./drivehooks.js";
import { listen } from "./store.js";
import { markErrors } from "./editui.js";
import { esc } from "./settingsform.js";
// @ts-ignore — a kit asset, not part of this app's own module graph.
import { registerPasskey } from "/static/passkeys.js";
import { attachAll } from "/static/kp/js/auto.js";
import { setCardState } from "/static/kp/js/card-state.js";
import { form as kpForm } from "/static/kp/js/forms.js";
import { hideSecret, revealSecret } from "/static/kp/js/patterns.js";
import {
  PASSKEYS_REGISTER_EVENT,
  PASSKEYS_RETRY_EVENT,
  setPasskeys,
} from "/static/kp/js/passkeys.js";
import { setQr } from "/static/kp/js/qr.js";

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
  shows: "when the dashboard is reached over HTTPS",
});
const DELETE = declare({
  id: "delete-passkey",
  page: "passkeys",
  opens: "run",
  row: "<passkey label>",
  what: "delete one passkey after a confirmation",
  shows: "once a passkey is registered",
});
const LABEL_FIELD = declareField({
  id: "passkey-label",
  page: "passkeys",
  what: "the name a new passkey is kept under",
});
const REGISTER_GO = declare({
  id: "register-passkey-go",
  page: "passkeys",
  opens: "run",
  what: "ask the browser for the passkey and keep it under the typed name",
  shows: "in the passkey form (Register this device)",
  reach: [{ do: "click", control: "register-passkey" }],
});
const REGISTER_CANCEL = declare({
  id: "register-passkey-cancel",
  page: "passkeys",
  opens: "view",
  what: "fold the passkey form away without registering",
  shows: "in the passkey form (Register this device)",
  reach: [{ do: "click", control: "register-passkey" }],
});
const ISSUE_TOKEN = declare({
  id: "issue-token",
  page: "settings",
  opens: "dialog",
  what: "open the Issue a new token dialog",
});
const REVOKE_TOKEN = declare({
  id: "revoke-token",
  page: "settings",
  opens: "dialog",
  row: "<token name>",
  what: "ask to revoke one machine's token",
});
const TOKEN_NAME = declareField({
  id: "token-name",
  page: "settings",
  what: "the name of a token to issue (Issue token dialog)",
});
const TOKEN_SCOPE = declareField({
  id: "token-scope",
  page: "settings",
  what: "the scope of a token to issue (Issue token dialog)",
});
const ISSUED_TOKEN = declareField({
  id: "issued-token",
  page: "settings",
  what: "the token just issued, to copy (read only)",
});
const TOKEN_REVOKE_CONFIRM = declareField({
  id: "token-revoke-confirm",
  page: "settings",
  what: "type the token name to confirm revoking it",
});

/**
 * @param {string} iso RFC 3339 text from the kit
 * @returns {number | null} ms
 */
export const msOf = (iso) => {
  const ms = Date.parse(iso);
  return Number.isFinite(ms) ? ms : null;
};

/**
 * The passkeys as kp's list takes them.
 * @param {PasskeyView[]} list
 * @returns {import("/static/kp/js/passkeys.js").PasskeyRow[]}
 */
export function passkeyRows(list) {
  return list.map((p) => ({
    key: p.id,
    label: p.label || "unnamed",
    kind: /** @type {const} */ ("passkey"),
    createdAt: msOf(p.created_at) ?? undefined,
    lastUsedAt: p.last_used_at ? msOf(p.last_used_at) : null,
  }));
}

/**
 * The Sign-in card's body: passkeys and the QR on the left, machine tokens on
 * the right.
 * @param {HTMLElement} box
 * @param {(what: string, fail: import("./doctor.js").RouteError | null,
 *   retry: () => void) => void} report the page's one failure alert
 * @returns {() => void} stop
 */
export function mountSignin(box, report) {
  const abort = new AbortController();
  /** @type {(() => void)[]} */
  const stops = [() => abort.abort()];

  // ── passkeys ──────────────────────────────────────────────────────────
  const registerBtn = drivable(
    h(
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
  const labelInput = /** @type {HTMLInputElement} */ (
    h("input", {
      class: "kp-field__input",
      type: "text",
      id: LABEL_FIELD,
      name: "label",
      placeholder: "this laptop",
      "aria-label": "What to call this passkey",
    })
  );
  const go = drivable(
    h(
      "button",
      {
        type: "submit",
        class: "kp-button kp-button--primary",
        "data-kp-submit": "",
        title: "Ask the browser for the passkey and keep it under this name",
      },
      "Register",
    ),
    REGISTER_GO,
  );
  const cancel = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button",
        title: "Keep things as they are",
      },
      "Cancel",
    ),
    REGISTER_CANCEL,
  );
  const status = h("p", { class: "kp-formgrid__value", role: "status" });
  const regForm = /** @type {HTMLFormElement} */ (
    h(
      "form",
      {
        class: "kp-formgrid si-register",
        "data-kp-form": "",
        "data-kp-formgrid-lines": "none",
        novalidate: "",
        hidden: "",
      },
      h("div", {
        class: "kp-form__summary",
        "data-kp-form-summary": "",
        tabindex: "-1",
        hidden: "",
        "data-kp-formgrid-span": "all",
      }),
      h(
        "div",
        { class: "kp-formgrid__row" },
        h(
          "label",
          { class: "kp-formgrid__label", for: LABEL_FIELD },
          "Name it",
          h("small", null, "What you will call this passkey in the list."),
        ),
        h("div", { class: "kp-formgrid__control" }, labelInput),
        status,
      ),
      h(
        "div",
        { class: "kp-formgrid__row kp-formgrid__row--buttons" },
        h(
          "div",
          {
            class: "kp-formgrid__control",
            "data-kp-formgrid-span": "control-value",
          },
          h("div", { class: "kp-formgrid__buttons" }, go, cancel),
        ),
      ),
    )
  );
  const list = h("div", { class: "kp-passkeys", "data-kp-passkeys": "" });
  const qr = h("div", {
    class: "kp-qr",
    "data-kp-qr": "",
    "data-kp-qr-label":
      "Link to this page, to sign in and register on your phone",
    "data-kp-qr-caption": "link",
  });
  const qrCard = h(
    "section",
    { class: "st-sub", "aria-labelledby": "si-qr-h" },
    h("h3", { id: "si-qr-h" }, "Register from your phone"),
    h(
      "p",
      { class: "kp-lede" },
      "Scan the code with the phone you want to sign in with: it opens this page there; nothing is sent to a third party.",
    ),
    qr,
  );
  const passkeyCol = h(
    "div",
    { class: "si-col" },
    h("div", { class: "si-col__head" }, h("h3", null, "Passkeys"), registerBtn),
    h(
      "p",
      { class: "kp-lede" },
      "The browsers and keys that sign in without the token; old ones are flagged so you can remove them.",
    ),
    regForm,
    list,
    qrCard,
  );

  /** @type {PasskeysData | null} */
  let data = null;
  const reload = () => void loadPasskeys().catch(() => {});
  /** @param {import("/static/kp/js/passkeys.js").PasskeysSpec} extra */
  const draw = (extra) => {
    setPasskeys(list, {
      register: false,
      retry: true,
      // A pasted token is the documented fallback sign-in, so the last
      // passkey may go (kp's default would refuse).
      protectLast: "off",
      confirm: "dialog",
      neverUsedAfter: 90,
      markCurrent: false,
      remove: async (row) => {
        const r = await send(
          "DELETE",
          `/api/passkeys/${row.key}`,
          undefined,
          "delete the passkey",
        );
        if (!r.ok) throw new Error(r.error.why);
        reload();
      },
      // The kit cannot rename a passkey yet (kp-gaps/homelab.md): say so.
      rename: async () => {
        throw new Error("Renaming a passkey is not supported by the host yet.");
      },
      ...extra,
    });
  };
  draw({ state: "loading" });
  list.addEventListener(PASSKEYS_RETRY_EVENT, () => {
    draw({ state: "loading" });
    reload();
  });
  list.addEventListener(PASSKEYS_REGISTER_EVENT, () => openRegister());
  // Live view's Delete needs the row's button to carry its label.
  const markRows = () => {
    for (const li of list.querySelectorAll("[data-kp-passkey]")) {
      const label = li.querySelector(".kp-passkeys__label")?.textContent ?? "";
      const btn = li.querySelector('[data-kp-passkeys-act="remove"]');
      if (btn && !btn.hasAttribute("data-drive"))
        drivable(/** @type {HTMLElement} */ (btn), DELETE, label);
    }
  };
  new MutationObserver(markRows).observe(list, {
    childList: true,
    subtree: true,
  });
  stops.push(() => {});

  const openRegister = () => {
    regForm.hidden = false;
    labelInput.focus();
  };
  registerBtn.addEventListener("click", () => {
    if (regForm.hidden) openRegister();
    else regForm.hidden = true;
  });
  cancel.addEventListener("click", () => {
    regForm.hidden = true;
    status.textContent = "";
  });
  regForm.addEventListener("kp-form-valid", async (e) => {
    const done = /** @type {CustomEvent} */ (e).detail?.done;
    try {
      await registerPasskey(labelInput.value, status);
      regForm.hidden = true;
      labelInput.value = "";
      status.textContent = "";
      reload();
    } catch (err) {
      kpForm(regForm)?.errors({
        label: err instanceof Error ? err.message : String(err),
      });
    } finally {
      done?.();
    }
  });

  const loadPasskeys = async () => {
    const r = await fetchJson(
      "/api/kit/passkeys",
      "the passkeys",
      abort.signal,
    );
    if (!r.ok) {
      draw({
        state: "error",
        message: r.error.why,
      });
      report("the passkeys", r.error, reload);
      return;
    }
    report("the passkeys", null, reload);
    data = r.body;
    const d = /** @type {PasskeysData} */ (r.body);
    /** @type {HTMLButtonElement} */ (registerBtn).disabled = !d.https;
    if (!d.https)
      registerBtn.setAttribute(
        "data-kp-disabled-reason",
        "Passkeys need https: this dashboard is not reached over https, or its public address is not set.",
      );
    else registerBtn.removeAttribute("data-kp-disabled-reason");
    draw({
      rows: passkeyRows(d.passkeys),
      state: d.passkeys.length ? "ready" : "empty",
      insecure: !d.https,
      register: d.https,
      emptyTitle: "No passkey registered yet",
      emptyText:
        "Register one to sign in with this device instead of pasting the token.",
    });
    const url = d.public_url
      ? `${d.public_url.replace(/\/$/, "")}/settings?section=sign-in`
      : null;
    if (url) setQr(qr, { value: url, state: "ready" });
    else qrCard.hidden = true;
  };

  // ── machine tokens ───────────────────────────────────────────────────
  const issueBtn = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm",
        id: "tokens-issue",
        title: "Issue a bearer token for one machine; it is shown once",
      },
      "Issue token…",
    ),
    ISSUE_TOKEN,
  );
  const tokenGrid = h("div", {
    class: "kp-formgrid si-tokens",
    "data-kp-formgrid-lines": "lines",
    "data-kp-formgrid-stack": "never",
  });
  const madeBox = h("div", { class: "st-made", hidden: "" });
  const tokenCard = /** @type {HTMLElement} */ (
    h(
      "section",
      {
        class: "kp-card st-subcard",
        "data-kp-state": "loading",
        "aria-label": "Machine tokens",
      },
      h(
        "div",
        { class: "kp-card__stage" },
        h("div", { class: "kp-card__data" }, tokenGrid),
      ),
    )
  );
  const tokenCol = h(
    "div",
    { class: "si-col" },
    h(
      "div",
      { class: "si-col__head" },
      h("h3", null, "Machine tokens"),
      issueBtn,
    ),
    h(
      "p",
      { class: "kp-lede" },
      "One bearer token per machine for the CLI and TUI; each has its scope, and its value is shown once, when it is made.",
    ),
    madeBox,
    tokenCard,
  );
  box.replaceChildren(passkeyCol, tokenCol);
  const attached = attachAll(box);
  stops.push(() => attached());
  setCardState(tokenCard, "loading", { words: "Reading the tokens…" });
  tokenCard.addEventListener("kp-card-retry", () => {
    setCardState(tokenCard, "loading", { words: "Reading the tokens…" });
    void loadTokens();
  });

  /** @param {{name: string, scope: string}[]} tokens */
  const paintTokens = (tokens) => {
    if (!tokens.length) {
      tokenGrid.replaceChildren();
      setCardState(tokenCard, "empty", {
        title: "No token yet",
        text: "Issue token… adds one for a machine.",
      });
      return;
    }
    tokenGrid.replaceChildren(
      ...tokens.map((v) => {
        const legacy = v.name === "legacy";
        const revoke = /** @type {HTMLButtonElement} */ (
          drivable(
            h(
              "button",
              {
                type: "button",
                class: "kp-button kp-button--destructive kp-button--sm",
                "data-token": v.name,
                title: `Revoke ${v.name}'s token: that machine can no longer reach the host`,
              },
              "Revoke",
            ),
            REVOKE_TOKEN,
            v.name,
          )
        );
        if (legacy) {
          // A removal the page may not do: held off, with the reason.
          revoke.setAttribute("aria-disabled", "true");
          revoke.setAttribute(
            "data-kp-disabled-reason",
            "Cleared over ssh only: the single `token` key is not the dashboard's to remove.",
          );
        } else
          revoke.addEventListener("click", () => revokeTokenDialog(v.name));
        return h(
          "div",
          { class: "kp-formgrid__row" },
          h(
            "div",
            { class: "kp-formgrid__label" },
            h("span", null, v.name),
            h(
              "small",
              null,
              legacy
                ? "the single host.toml token every machine shared"
                : "its own bearer, hashed at rest",
            ),
          ),
          h(
            "div",
            { class: "kp-formgrid__control" },
            h("span", { class: "kp-badge st-scope" }, v.scope),
          ),
          h("div", { class: "kp-formgrid__actions" }, revoke),
        );
      }),
    );
    setCardState(tokenCard, "ready");
    void attachAll(tokenGrid);
  };
  const loadTokens = async () => {
    const r = await fetchJson("/data/tokens", "the tokens", abort.signal);
    if (!r.ok) {
      setCardState(tokenCard, "error", {
        title: "The tokens could not be read",
        text: r.error.why,
      });
      report("the tokens", r.error, () => void loadTokens());
      return;
    }
    report("the tokens", null, () => void loadTokens());
    paintTokens(r.body.tokens ?? []);
  };

  const issueTokenDialog = () => {
    const name = /** @type {HTMLInputElement} */ (
      h("input", {
        class: "kp-field__input",
        type: "text",
        id: TOKEN_NAME,
        name: "name",
        required: "",
        autocomplete: "off",
        placeholder: "e.g. wsl, ct120-dev",
      })
    );
    const scope = /** @type {HTMLSelectElement} */ (
      h(
        "select",
        { class: "kp-field__input", id: TOKEN_SCOPE, name: "scope" },
        h("option", { value: "read" }, "read: look only"),
        h("option", { value: "operate" }, "operate: act without destroying"),
        h(
          "option",
          { value: "all" },
          "all: everything, including the dashboard's own",
        ),
      )
    );
    scope.value = "operate";
    const goBtn = /** @type {HTMLButtonElement} */ (
      h(
        "button",
        {
          type: "submit",
          class: "kp-button kp-button--primary",
          "data-kp-submit": "",
        },
        "Issue",
      )
    );
    const cancelBtn = h(
      "button",
      { type: "button", class: "kp-button" },
      "Cancel",
    );
    const errBox = h("div");
    const f = /** @type {HTMLFormElement} */ (
      h(
        "form",
        {
          class: "kp-formgrid",
          "data-kp-form": "",
          "data-kp-formgrid-label": "wide",
          novalidate: "",
        },
        h("div", {
          class: "kp-form__summary",
          "data-kp-form-summary": "",
          tabindex: "-1",
          hidden: "",
          "data-kp-formgrid-span": "all",
        }),
        h(
          "div",
          { class: "kp-formgrid__row" },
          h(
            "label",
            { class: "kp-formgrid__label", for: TOKEN_NAME },
            "Name",
            h(
              "small",
              null,
              "What `homelab doctor` and audit.log will call this machine.",
            ),
          ),
          h("div", { class: "kp-formgrid__control" }, name),
        ),
        h(
          "div",
          { class: "kp-formgrid__row" },
          h(
            "label",
            { class: "kp-formgrid__label", for: TOKEN_SCOPE },
            "Scope",
          ),
          h("div", { class: "kp-formgrid__control" }, scope),
        ),
        h(
          "div",
          { class: "kp-formgrid__row" },
          h(
            "div",
            { class: "kp-formgrid__control", "data-kp-formgrid-span": "all" },
            errBox,
          ),
        ),
        h(
          "div",
          { class: "kp-formgrid__row kp-formgrid__row--buttons" },
          h(
            "div",
            {
              class: "kp-formgrid__control",
              "data-kp-formgrid-span": "control-value",
            },
            h("div", { class: "kp-formgrid__buttons" }, cancelBtn, goBtn),
          ),
        ),
      )
    );
    const d = openDialog({
      title: "Issue a new token",
      description:
        "What is it for, and what may it do? The plaintext is shown once, right after this; the host keeps only its SHA-256.",
      id: "token-issue-dialog",
      body: [f],
    });
    void attachAll(f);
    const unregister = register("tokens-issue", {
      dialog: d.dialog,
      go: () => goBtn,
      cancel: () => cancelBtn,
      close: () => d.close(),
    });
    d.closed.then(unregister);
    cancelBtn.addEventListener("click", () => d.close());
    f.addEventListener("kp-form-valid", async (e) => {
      const done = /** @type {CustomEvent} */ (e).detail?.done;
      if (driven()) return done?.();
      const n = name.value.trim();
      if (!n) {
        markErrors(new Map([["name", name]]), { name: "A name is needed." });
        return done?.();
      }
      const r = await send(
        "POST",
        "/data/tokens",
        { name: n, scope: scope.value },
        "a new token",
      );
      done?.();
      if (!r.ok) {
        errBox.replaceChildren(refusalCallout(r.error));
        return;
      }
      d.close();
      showIssued(r.body.issued);
      void loadTokens();
    });
  };

  /**
   * The token just made: in kp's secret field, shown until it is hidden (no
   * countdown), with Copy, and a line saying it will not be shown again.
   * @param {{name: string, scope: string, token: string}} issued
   */
  const showIssued = (issued) => {
    madeBox.hidden = false;
    madeBox.innerHTML = `<div class="kp-formgrid" data-kp-formgrid-label="wide"><div class="kp-formgrid__group"><h4 class="kp-formgrid__heading">New token for ${esc(issued.name)}</h4><div class="kp-formgrid__row"><div class="kp-formgrid__label">Token value<small>Set it as HOMELAB_TOKEN on that machine (its own .env or ~/.config/homelab/env).</small></div><div class="kp-formgrid__control"><div class="kp-secret" data-kp-secret data-kp-secret-ttl="0" data-kp-secret-label="Token for ${esc(issued.name)}" id="${ISSUED_TOKEN}"></div></div><p class="kp-formgrid__value">Copy it now: it is not shown again, and it is not shared with any other machine's token. The host keeps only its SHA-256.</p></div><div class="kp-formgrid__row kp-formgrid__row--buttons"><div class="kp-formgrid__control" data-kp-formgrid-span="control-value"><div class="kp-formgrid__buttons"><button type="button" class="kp-button kp-button--primary" data-st-done>Done, I saved it</button></div></div></div></div></div>`;
    const field = /** @type {HTMLElement} */ (
      madeBox.querySelector("[data-kp-secret]")
    );
    field.setAttribute("data-kp-secret-value", issued.token);
    const off = attachAll(madeBox);
    void off.ready.then(() => revealSecret(field));
    madeBox.querySelector("[data-st-done]")?.addEventListener("click", () => {
      hideSecret(field);
      madeBox.replaceChildren();
      madeBox.hidden = true;
      off();
    });
    madeBox.scrollIntoView({ block: "nearest" });
  };

  /** @param {string} name */
  const revokeTokenDialog = (name) => {
    void import("/static/kp/js/components.js").then(({ openConfirmation }) => {
      const btn = /** @type {HTMLElement} */ (
        tokenGrid.querySelector(`[data-token="${CSS.escape(name)}"]`)
      );
      openConfirmation(btn, {
        phrase: `Revoke ${name}?`,
        typeName: name,
        loses: [
          "That machine's token stops working at once.",
          "No other machine's token is affected.",
        ],
        acceptLabel: "Revoke",
        onAccept: async () => {
          if (driven()) return;
          const r = await send(
            "DELETE",
            `/data/tokens/${encodeURIComponent(name)}?confirm=${encodeURIComponent(name)}`,
            undefined,
            "revoke a token",
          );
          if (!r.ok) {
            notify(`Not revoked: ${r.error.why}`, "error");
            return;
          }
          notify(`${name} revoked.`, "success");
          void loadTokens();
        },
      });
      // Live view types the name into the confirmation field by its id.
      queueMicrotask(() => {
        const input = document.querySelector(
          "dialog[open] .kp-type-confirm__input",
        );
        if (input) input.id = TOKEN_REVOKE_CONFIRM;
      });
    });
  };
  issueBtn.addEventListener("click", () => issueTokenDialog());

  stops.push(listen("tokens", () => void loadTokens()));
  reload();
  void loadTokens().catch(() => {});
  void data;
  return () => {
    for (const s of stops) s();
  };
}
