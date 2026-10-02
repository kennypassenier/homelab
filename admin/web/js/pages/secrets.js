// feat-secrets-1/2: which latch secrets/latch_files a stack uses, reveal
// one on explicit click (re-hidden the moment this page is left — nothing
// here keeps a revealed value once `mount`'s cleanup runs), and change one,
// writing through latch the same way the CLI reads it. The write rides
// `ActionKind::ChangeSecret` (stage the new value, then press), so it gets
// the same audit/job trail as every other dashboard write; the value itself
// never becomes a URL, a query param or part of `ActionArgs`.

import { send } from "../act.js";
import { errorBox, fetchJson, h } from "../dom.js";
import { current, subscribe } from "../store.js";

/**
 * @param {any} secret a `CurrentLatchFile` row or an app name
 * @param {"env" | "file"} kind
 * @returns {{kind: "env", app: string} | {kind: "file", from: string, dest: string}}
 */
function refOf(kind, secret) {
  return kind === "env"
    ? { kind: "env", app: secret }
    : { kind: "file", from: secret.from, dest: secret.dest };
}

/**
 * One secret's row: name, a Reveal button that swaps in the value on
 * click and a Hide button that swaps it back out, and a small change form
 * that only appears once "Change…" is pressed.
 * @param {string} stack
 * @param {"env" | "file"} kind
 * @param {any} secret
 */
function secretRow(stack, kind, secret) {
  const ref = refOf(kind, secret);
  const name =
    kind === "env" ? `${secret}/.env` : `${secret.dest} (from ${secret.from})`;
  const valueCell = h("span", { class: "measured" }, "hidden");
  const revealBtn = h(
    "button",
    {
      class: "kp-button kp-button--sm",
      type: "button",
      "data-action": "reveal-secret",
    },
    "Reveal",
  );
  revealBtn.addEventListener("click", async () => {
    if (revealBtn.dataset.revealed === "1") {
      // Hiding again never asks the host a second time — re-hiding is
      // purely a client-side act (feat-secrets-1's rule), the value is
      // simply dropped from this element.
      valueCell.textContent = "hidden";
      revealBtn.dataset.revealed = "";
      revealBtn.textContent = "Reveal";
      return;
    }
    revealBtn.disabled = true;
    revealBtn.textContent = "Reading…";
    const r = await send(
      "POST",
      `/data/secrets/${encodeURIComponent(stack)}/reveal`,
      { secret: ref },
      "reveal the secret",
    );
    revealBtn.disabled = false;
    if (r.ok) {
      valueCell.textContent = r.body.value;
      revealBtn.textContent = "Hide";
      revealBtn.dataset.revealed = "1";
    } else {
      valueCell.textContent = `could not read: ${r.error.why}`;
    }
  });

  const newValue = /** @type {HTMLTextAreaElement} */ (
    h("textarea", {
      class: "kp-input",
      rows: "3",
      placeholder: "the new value",
    })
  );
  const status = h("span", { class: "measured" });
  const saveBtn = h(
    "button",
    {
      class: "kp-button kp-button--sm kp-button--primary",
      type: "button",
    },
    "Save",
  );
  saveBtn.addEventListener("click", async () => {
    const content = newValue.value;
    if (content === "") {
      status.textContent = "type the new value first";
      return;
    }
    status.textContent = "staging…";
    const staged = await send(
      "POST",
      "/data/secrets/stage",
      { content },
      "stage the new value",
    );
    newValue.value = "";
    if (!staged.ok) {
      status.textContent = `could not stage it: ${staged.error.why}`;
      return;
    }
    status.textContent = "writing through latch…";
    const r = await send(
      "POST",
      `/data/actions/${encodeURIComponent(stack)}/change-secret`,
      {
        secret_ref: JSON.stringify(ref),
        stage_token: staged.body.stage_token,
      },
      "change the secret",
    );
    status.textContent = r.ok
      ? "queued — see the Jobs page for the result"
      : `refused: ${r.error.why}`;
  });
  const changeForm = h(
    "div",
    { class: "secret-change", hidden: "" },
    newValue,
    saveBtn,
    h("span", { class: "measured" }, status),
  );
  const changeBtn = h(
    "button",
    {
      class: "kp-button kp-button--sm",
      type: "button",
      "data-action": "change-secret",
    },
    "Change…",
  );
  changeBtn.addEventListener("click", () => {
    changeForm.hidden = !changeForm.hidden;
  });

  return h(
    "tr",
    null,
    h("td", null, name),
    h("td", null, revealBtn, " ", valueCell),
    h("td", null, changeBtn),
    h("td", { colspan: "3" }, changeForm),
  );
}

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const stackSelect = h("select", { class: "kp-input" });
  const body = h("tbody");
  const table = h(
    "table",
    { class: "kp-table" },
    h(
      "thead",
      null,
      h(
        "tr",
        null,
        h("th", null, "Secret"),
        h("th", null, "Value"),
        h("th", null, ""),
      ),
    ),
    body,
  );
  const status = h("div", { class: "measured" });
  root.replaceChildren(
    h("div", { class: "title-row" }, h("h1", null, "Secrets")),
    h(
      "p",
      null,
      "Which latch secrets and latch_files a stack declares. Values are never shown until Reveal is pressed, and are hidden again the moment this page is left.",
    ),
    // fix-206: a bare `<label>` is inline by default, so as a direct
    // top-level child of `#page` it never took part in the page's
    // block-level vertical-spacing rhythm (`main.shell > * + *` in
    // app.css) — its forced margin-block-start had nothing to lay out
    // against. A block wrapper gives it the same section spacing as
    // every other top-level piece of this page.
    h("div", null, h("label", null, "Stack ", stackSelect)),
    status,
    table,
  );

  const abort = new AbortController();

  /** @param {string} stack */
  const loadStack = async (stack) => {
    body.replaceChildren();
    if (!stack) return;
    status.textContent = "Reading what this stack declares…";
    const r = await fetchJson(
      `/data/secrets/${encodeURIComponent(stack)}`,
      `${stack}'s secrets`,
      abort.signal,
    );
    if (!r.ok) {
      status.replaceChildren(errorBox(r.error));
      return;
    }
    const secrets = r.body.secrets ?? [];
    const files = r.body.files ?? [];
    if (secrets.length === 0 && files.length === 0) {
      status.textContent = "This stack declares no latch secret or file.";
      return;
    }
    status.textContent = "";
    body.replaceChildren(
      ...secrets.map((/** @type {any} */ app) => secretRow(stack, "env", app)),
      ...files.map((/** @type {any} */ f) => secretRow(stack, "file", f)),
    );
  };

  const fillStacks = () => {
    const stacks = current().fleet?.stacks ?? [];
    const kept = stackSelect.value;
    const wanted = stacks.some((s) => s.name === kept)
      ? kept
      : (stacks[0]?.name ?? "");
    stackSelect.replaceChildren(
      ...stacks.map((s) => h("option", { value: s.name }, s.name)),
    );
    stackSelect.value = wanted;
    if (wanted) void loadStack(wanted);
  };
  stackSelect.addEventListener("change", () => {
    void loadStack(stackSelect.value);
  });
  const off = subscribe(fillStacks);
  fillStacks();
  return () => {
    abort.abort();
    off();
  };
}
