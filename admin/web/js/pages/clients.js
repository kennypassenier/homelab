// feat-pages-1 (chassis-rs 3.1.0, nav-decisions): the kit's own Clients
// page (K12-K16), drawn by this app from `GET /api/kit/clients` so it
// carries this app's bar. Issue a token, reveal/copy it, see its last
// requests, send a test request, re-issue, revoke or delete — every
// action is the kit's own route (`/api/clients…`); this page only renders
// the data and wires the buttons.

import { send } from "../act.js";
import { errorBox, fetchJson, h, tableBlock } from "../dom.js";
import { attachDataTables } from "/static/kp/js/datatable.js";

/**
 * @typedef {{id: string, name: string, active: boolean, issued_at: string,
 *   revoked_at: string | null, last_used_at: string | null, uses: number,
 *   fields: Record<string, string>, declared: string[], extra: string[],
 *   actions: {label: string, route: string, method: "POST" | "PUT" | "DELETE",
 *     destructive: boolean, confirm: string | null,
 *     busy_label: string | null}[]}} ClientRow
 * @typedef {{clients: ClientRow[], declared_columns: {title: string}[],
 *   extra_columns: {title: string}[],
 *   form_fields: ({name: string, label: string, kind: "text",
 *     placeholder: string} | {name: string, label: string, kind: "select",
 *     options: {value: string, label: string}[]})[],
 *   reveal_seconds: number, capture_body_bytes: number,
 *   capture_ttl_minutes: number, has_test_route: boolean}} ClientsData
 */

/** @param {ClientsData["form_fields"][number]} f */
function fieldInput(f) {
  if (f.kind === "select") {
    const sel = /** @type {HTMLSelectElement} */ (
      h(
        "select",
        { class: "kp-field__input", name: f.name, required: "" },
        h("option", { value: "", disabled: "", selected: "" }, "— choose —"),
        ...f.options.map((o) => h("option", { value: o.value }, o.label)),
      )
    );
    return sel;
  }
  return h("input", {
    class: "kp-field__input",
    type: "text",
    name: f.name,
    placeholder: f.placeholder,
    required: "",
  });
}

/**
 * @param {ClientsData} data
 * @param {() => void} onIssued
 */
function issueForm(data, onIssued) {
  const name = /** @type {HTMLInputElement} */ (
    h("input", {
      class: "kp-field__input",
      type: "text",
      name: "name",
      placeholder: "home-assistant",
      required: "",
      pattern: "[A-Za-z0-9._-]{1,64}",
    })
  );
  const fields = data.form_fields.map(fieldInput);
  const status = h("p", { class: "measured kp-w-full kp-m-0" });
  const btn = h(
    "button",
    { type: "submit", class: "kp-button kp-button--primary" },
    "Issue token",
  );
  const form = h(
    "form",
    { class: "kp-row kp-row--end kp-gap-md" },
    h(
      "div",
      { class: "kp-field" },
      h("label", { class: "kp-field__label" }, "Name"),
      name,
    ),
    ...data.form_fields.map((f, i) =>
      h(
        "div",
        { class: "kp-field" },
        h("label", { class: "kp-field__label" }, f.label),
        fields[i],
      ),
    ),
    btn,
    status,
  );
  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    btn.disabled = true;
    status.textContent = "issuing…";
    const extra = Object.fromEntries(
      data.form_fields.map((f, i) => [
        f.name,
        /** @type {any} */ (fields[i]).value,
      ]),
    );
    const r = await send(
      "POST",
      "/api/clients",
      { name: name.value, ...extra },
      "issue a token",
    );
    btn.disabled = false;
    if (r.ok) {
      name.value = "";
      status.textContent = "issued — reveal it below.";
      onIssued();
    } else {
      status.textContent = `refused: ${r.error.why}`;
    }
  });
  return h(
    "section",
    { class: "kp-card kp-mb-lg" },
    h("h2", { class: "kp-card__title kp-mt-0 kp-fs-md" }, "Add a client"),
    form,
  );
}

/**
 * The manage block for one row: reveal/copy, requests, the project's
 * actions and the danger zone, all behind a disclosure so the table stays
 * short.
 * @param {ClientRow} c
 * @param {ClientsData} data
 * @param {() => void} onChanged
 */
function manageBlock(c, data, onChanged) {
  const tokenLine = h("p", { class: "mono" }, "••••••••••••");
  const revealBtn = h(
    "button",
    { type: "button", class: "kp-button kp-button--sm" },
    "Reveal",
  );
  const requests = h("div", { class: "measured" }, "not read yet");
  const note = h("p", { class: "measured" });

  revealBtn.addEventListener("click", async () => {
    const r = await fetchJson(`/api/clients/${c.id}/token`, "reveal the token");
    if (r.ok) {
      tokenLine.textContent = r.body.token;
      window.setTimeout(() => {
        tokenLine.textContent = "••••••••••••";
      }, data.reveal_seconds * 1000);
    } else {
      note.textContent = `could not reveal it: ${r.error.why}`;
    }
  });

  const requestsBtn = h(
    "button",
    { type: "button", class: "kp-button kp-button--sm" },
    "Last requests",
  );
  requestsBtn.addEventListener("click", async () => {
    const r = await fetchJson(
      `/api/clients/${c.id}/requests`,
      "the last requests",
    );
    if (!r.ok) {
      requests.textContent = `could not read them: ${r.error.why}`;
      return;
    }
    /** @type {any[]} */
    const list = r.body;
    requests.replaceChildren(
      list.length
        ? h(
            "ul",
            null,
            ...list.map((cap) =>
              h(
                "li",
                null,
                `${cap.at} · ${cap.method} ${cap.path} · ${cap.status}${cap.truncated ? " (truncated)" : ""}`,
              ),
            ),
          )
        : h("span", null, "none yet"),
    );
  });

  /** @type {Node[]} */
  const row = [
    h("h4", null, "Token"),
    h("div", { class: "kp-row" }, tokenLine, revealBtn),
    h("h4", null, "Requests"),
    h("div", { class: "kp-row" }, requestsBtn),
    requests,
  ];

  if (data.has_test_route) {
    const testBtn = h(
      "button",
      { type: "button", class: "kp-button kp-button--sm" },
      "Send test",
    );
    testBtn.addEventListener("click", async () => {
      testBtn.disabled = true;
      const r = await send(
        "POST",
        `/api/clients/${c.id}/test`,
        undefined,
        "send the test request",
      );
      testBtn.disabled = false;
      note.textContent = r.ok
        ? `test sent: HTTP ${r.body.status}`
        : `refused: ${r.error.why}`;
    });
    row.push(testBtn);
  }

  if (c.actions.length) {
    row.push(
      h("h4", null, "Actions"),
      h(
        "div",
        { class: "kp-row" },
        ...c.actions.map((a) => {
          const b = h(
            "button",
            {
              type: "button",
              class: `kp-button kp-button--sm${a.destructive ? " kp-button--destructive" : ""}`,
            },
            a.label,
          );
          b.addEventListener("click", async () => {
            if (a.destructive && !confirm(a.confirm ?? "Are you sure?")) return;
            const r = await send(a.method, a.route, undefined, a.label);
            if (r.ok) onChanged();
            else note.textContent = `refused: ${r.error.why}`;
          });
          return b;
        }),
      ),
    );
  }

  row.push(h("h4", null, "Danger zone"));
  /** @type {Node[]} */
  const danger = [];
  if (c.active) {
    const reissue = h(
      "button",
      { type: "button", class: "kp-button kp-button--sm" },
      "Re-issue",
    );
    reissue.addEventListener("click", async () => {
      if (!confirm("Re-issue? The current token stops working at once."))
        return;
      const r = await send(
        "POST",
        `/api/clients/${c.id}/reissue`,
        undefined,
        "re-issue the token",
      );
      if (r.ok) onChanged();
      else note.textContent = `refused: ${r.error.why}`;
    });
    const revoke = h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm kp-button--destructive",
      },
      "Revoke",
    );
    revoke.addEventListener("click", async () => {
      if (!confirm("Revoke this token? This client is locked out immediately."))
        return;
      const r = await send(
        "POST",
        `/api/clients/${c.id}/revoke`,
        undefined,
        "revoke the token",
      );
      if (r.ok) onChanged();
      else note.textContent = `refused: ${r.error.why}`;
    });
    danger.push(reissue, revoke);
  }
  const del = h(
    "button",
    { type: "button", class: "kp-button kp-button--sm kp-button--destructive" },
    "Delete",
  );
  del.addEventListener("click", async () => {
    if (!confirm("Delete this client and its history?")) return;
    const r = await send(
      "DELETE",
      `/api/clients/${c.id}`,
      undefined,
      "delete the client",
    );
    if (r.ok) onChanged();
    else note.textContent = `refused: ${r.error.why}`;
  });
  danger.push(del);
  row.push(h("div", { class: "kp-row" }, ...danger), note);

  return h("div", { class: "client-manage" }, ...row);
}

/**
 * @param {ClientRow} c
 * @param {ClientsData} data
 * @param {() => void} onChanged
 */
function clientRow(c, data, onChanged) {
  const details = h(
    "details",
    null,
    h("summary", null, "Manage"),
    manageBlock(c, data, onChanged),
  );
  return h(
    "tr",
    { "data-kp-row-key": c.id },
    h("td", { class: "kp-fw-medium" }, c.name),
    h("td", null, c.issued_at),
    h("td", null, c.last_used_at ? `${c.last_used_at} (${c.uses}×)` : "never"),
    h(
      "td",
      null,
      c.active
        ? h("span", { class: "kp-badge kp-badge--success" }, "active")
        : h("span", { class: "kp-badge" }, `revoked ${c.revoked_at ?? ""}`),
    ),
    ...c.declared.map((v) => h("td", null, v)),
    ...c.extra.map((v) => {
      const td = h("td", null);
      td.innerHTML = v;
      return td;
    }),
    h("td", null, details),
  );
}

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const intro = h(
    "p",
    null,
    "Every client listed here is one program that calls this service, with its own token. Give each one its own so you can switch a single one off without touching the others.",
  );
  const formSlot = h("div", { class: "measured" }, "Reading…");
  const table = tableBlock({
    remember: "clients",
    caption: "Registered clients",
    search: "Search clients",
    state: "loading",
    nothing: "No clients yet. Add one above.",
    columns: [
      { label: "Name", sort: "text" },
      { label: "Issued", sort: "text" },
      { label: "Last used", sort: "text" },
      { label: "State", sort: "text", filter: "choice" },
      { label: "", sort: "none" },
    ],
  });
  root.replaceChildren(h("h1", null, "Clients"), intro, formSlot, table.wrap);
  const detach = attachDataTables(root);
  const abort = new AbortController();

  const load = async () => {
    table.loading({ words: "Reading the clients…" });
    const r = await fetchJson(
      "/api/kit/clients",
      "the clients page",
      abort.signal,
    );
    if (!r.ok) {
      formSlot.replaceChildren(errorBox(r.error));
      table.loading({ words: "" });
      table.ready();
      return;
    }
    /** @type {ClientsData} */
    const data = r.body;
    formSlot.replaceChildren(issueForm(data, retry));
    table.tbody.replaceChildren(
      ...data.clients.map((c) => clientRow(c, data, retry)),
    );
    table.ready();
  };
  const retry = () => void load().catch(() => {});
  root.addEventListener("kp-datatable-retry", retry);
  retry();
  return () => {
    abort.abort();
    root.removeEventListener("kp-datatable-retry", retry);
    detach();
  };
}
