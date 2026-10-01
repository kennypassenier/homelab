// The settings page. The working copy (arch-edit-txn): where it stands
// against the remote, and the push, rebase or drop of commits the remote
// lacks. The host's settings (feat-settings-1): every host.toml key with
// its value, read and written with commands answered to this session only,
// so a TUI with unsaved edits in its settings screen keeps them. arch-self:
// a key that can cut the dashboard off is shown, never changed; one that
// can take its route or the backups down needs its name typed.

import { send } from "../act.js";
import { notify, openDialog, refusalAlarm, refusalCallout } from "../actui.js";
import { agoEl, setAgo } from "../ago.js";
import {
  badgeCell,
  bindTableUrl,
  errorBox,
  fetchJson,
  h,
  tableBlock,
  td,
} from "../dom.js";
import {
  ACCESS,
  editable,
  fieldText,
  hostSettingsBody,
  isSecret,
  parseKey,
  valueText,
} from "../editforms.js";
import { markErrors } from "../editui.js";
import { driven, register } from "../drivehooks.js";
import { formatDateTime } from "../format.js";
import { mountJobPanel } from "../jobpanel.js";
import { listen } from "../store.js";
import { attachDataTables, dataTable } from "/static/kp/js/datatable.js";

/**
 * @typedef {import("../editforms.js").HostField} HostField
 * @typedef {{commit: string, subject: string, at: number}} CommitRef
 */

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const repoBox = h(
    "section",
    { class: "kp-card", "aria-label": "Working copy", id: "repo" },
    h("p", { class: "measured" }, "Reading the working copy…"),
  );
  const hostHead = h("p", { class: "measured" });
  const staged = h("div", {
    class: "staged",
    role: "status",
    "aria-live": "polite",
  });
  const ago = agoEl("read");
  // fix-120 (per-machine tokens, owner decision 2026-10-01): one bearer per
  // machine, named, hashed at rest (host.toml `[[tokens]]`) — issued and
  // revoked here so a lost or retired machine's access ends without
  // touching any other machine's token. The legacy single `token` key still
  // works (shown as "legacy"); the migration note is in OPERATIONS_RUNBOOK.
  const tokensAgo = agoEl("read");
  const tk = tableBlock({
    remember: "tokens",
    caption: "Per-machine tokens",
    search: "Search tokens",
    state: "loading",
    nothing: "No tokens yet: Issue a token adds one.",
    columns: [
      { label: "Name", sort: "text" },
      { label: "Scope", sort: "text", filter: "choice" },
      { label: "Revoke", sort: "text" },
    ],
  });
  const issueBtn = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--primary",
      id: "tokens-issue",
    },
    "Issue token",
  );
  const t = tableBlock({
    remember: "host-settings",
    caption: "host.toml on pve",
    search: "Search settings",
    state: "loading",
    nothing: "host.toml sets nothing.",
    columns: [
      { label: "Group", sort: "text", filter: "choice" },
      { label: "Setting", sort: "text" },
      { label: "Value", sort: "text", cls: "wide" },
      { label: "Takes effect", sort: "text", filter: "choice" },
      {
        label: "Changed",
        sort: "text",
        order:
          "ssh only (secret),ssh only (safety policy),ssh only (can cut the dashboard off),Here with the name typed,Here (secret, write-only),Here",
        filter: "choice",
      },
      { label: "Edit", sort: "text" },
    ],
  });
  root.replaceChildren(
    h("h1", null, "Settings"),
    h("h2", null, "Working copy"),
    repoBox,
    h("h2", null, "Host settings"),
    h(
      "p",
      null,
      "Every key of host.toml. Changes are collected here, checked with the host's start-up validation and written in one go; the TUI's settings screen keeps its own unsaved edits.",
    ),
    hostHead,
    staged,
    t.wrap,
    h("p", null, ago),
    h(
      "div",
      { class: "title-row" },
      h("h2", null, "Per-machine tokens"),
      issueBtn,
    ),
    h(
      "p",
      null,
      "Each machine gets its own bearer, hashed at rest; revoking one never touches another's. The legacy single token (host.toml's bare `token` key, shown as “legacy”) still works until every machine has its own — see the migration note in the runbook.",
    ),
    tk.wrap,
    h("p", null, tokensAgo),
  );
  const detach = attachDataTables(root);
  const table = dataTable(t.wrap);
  const tokensTable = dataTable(tk.wrap);
  const unbind = bindTableUrl(table, "settings");
  const unbindTokens = bindTableUrl(tokensTable, "tokens");
  const abort = new AbortController();

  // ── the working copy ──
  const loadRepo = async () => {
    const r = await fetchJson("/data/repo", "the working copy", abort.signal);
    if (!r.ok) {
      repoBox.replaceChildren(errorBox(r.error));
      return;
    }
    drawRepo(repoBox, r.body, loadRepo);
  };

  // ── host.toml ──
  /** @type {{sha256: string, path: string, fields: HostField[]} | null} */
  let page = null;
  /** @type {Map<string, {field: HostField, value: unknown}>} */
  const changes = new Map();
  /** @type {Set<string>} */
  const confirmed = new Set();

  const paintStaged = () => {
    if (!changes.size) {
      staged.replaceChildren();
      return;
    }
    const review = h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--primary",
        id: "settings-save",
      },
      "Review and save…",
    );
    const drop = h(
      "button",
      { type: "button", class: "kp-button kp-button--ghost" },
      "Discard",
    );
    review.addEventListener("click", () => openReview());
    drop.addEventListener("click", () => {
      changes.clear();
      confirmed.clear();
      paintStaged();
      paintRows();
    });
    staged.replaceChildren(
      h(
        "div",
        { class: "kp-alert kp-alert--info" },
        h(
          "span",
          null,
          `${changes.size} change(s) not saved yet: ${[...changes.keys()].join(", ")}. `,
        ),
        review,
        " ",
        drop,
      ),
    );
  };

  const paintRows = () => {
    if (!page) return;
    t.tbody.replaceChildren(
      ...page.fields.map((f) => {
        const pending = changes.get(f.key);
        const edit = h(
          "button",
          {
            type: "button",
            class: "kp-button kp-button--sm",
            "data-key": f.key,
          },
          pending ? "Change again" : "Edit",
        );
        if (!editable(f)) edit.disabled = true;
        edit.addEventListener("click", () => openKey(f));
        const value = pending
          ? `→ ${pendingText(f, pending.value)}`
          : valueText(f);
        return h(
          "tr",
          {
            "data-kp-row-key": f.key,
            "data-key": f.key,
            class: pending ? "unread" : "",
          },
          td(f.group),
          h(
            "td",
            null,
            h("strong", null, f.label),
            h("br"),
            h("span", { class: "mono measured" }, f.key),
          ),
          td(value, f.kind.type === "table" ? "" : "mono"),
          td(f.apply === "live" ? "at once" : "at the host's next start"),
          badgeCell({
            label: ACCESS[f.access],
            tone: editable(f) ? "ok" : "info",
          }),
          h("td", null, edit),
        );
      }),
    );
    table?.refresh();
  };

  /** @param {HostField} f @param {unknown} v */
  const pendingText = (f, v) =>
    v == null
      ? `remove (default: ${f.default})`
      : f.kind.type === "table"
        ? "new table"
        : Array.isArray(v)
          ? v.join(", ")
          : String(v);

  const loadHost = async () => {
    t.loading({ words: "Reading host.toml from the host…" });
    const r = await fetchJson(
      "/data/host-settings",
      "the host settings",
      abort.signal,
    );
    if (!r.ok) {
      t.failed(r.error);
      return;
    }
    page = r.body.page;
    const unknown = /** @type {string[]} */ (r.body.page.unknown ?? []);
    hostHead.textContent = `${r.body.page.path} · version ${String(r.body.page.sha256).slice(0, 12)}${unknown.length ? ` · keys the host does not read: ${unknown.join(", ")}` : ""}`;
    paintRows();
    t.ready();
    setAgo(ago, r.body.measured_at ?? Date.now() / 1000);
  };

  // ── fix-120: per-machine tokens ──
  const paintTokens = (
    /** @type {{name: string, scope: string}[]} */ tokens,
  ) => {
    tk.tbody.replaceChildren(
      ...tokens.map((v) => {
        const revoke = h(
          "button",
          {
            type: "button",
            class: "kp-button kp-button--sm kp-button--destructive",
            "data-token": v.name,
          },
          "Revoke",
        );
        if (v.name === "legacy") {
          revoke.disabled = true;
          revoke.title =
            "the single `token` key is cleared over ssh (see the migration note)";
        } else {
          revoke.addEventListener("click", () => revokeTokenDialog(v.name));
        }
        return h(
          "tr",
          { "data-kp-row-key": v.name },
          td(v.name),
          td(v.scope),
          h("td", null, revoke),
        );
      }),
    );
    tokensTable?.refresh();
  };
  const loadTokens = async () => {
    tk.loading({ words: "Reading the tokens from the host…" });
    const r = await fetchJson("/data/tokens", "the tokens", abort.signal);
    if (!r.ok) {
      tk.failed(r.error);
      return;
    }
    paintTokens(r.body.tokens ?? []);
    tk.ready();
    setAgo(tokensAgo, Date.now() / 1000);
  };
  const issueTokenDialog = () => {
    const name = h("input", {
      class: "kp-field__input",
      type: "text",
      id: "token-name",
      autocomplete: "off",
      placeholder: "e.g. wsl, ct120-dev",
    });
    const scope = h(
      "select",
      { class: "kp-field__input", id: "token-scope" },
      h("option", { value: "read" }, "read — look only"),
      h("option", { value: "operate" }, "operate — act without destroying"),
      h(
        "option",
        { value: "all" },
        "all — everything, including the dashboard's own",
      ),
    );
    scope.value = "operate";
    const go = h(
      "button",
      { type: "button", class: "kp-button kp-button--primary" },
      "Issue",
    );
    const cancel = h(
      "button",
      { type: "button", class: "kp-button" },
      "Cancel",
    );
    const errBox = h("div");
    const d = openDialog({
      title: "Issue a new token",
      description:
        "What is it for, and what may it do? The plaintext is shown once, right after this — save it there; the host keeps only its SHA-256.",
      id: "token-issue-dialog",
      body: [
        h(
          "div",
          { class: "kp-field" },
          h("label", { class: "kp-field__label", for: "token-name" }, "Name"),
          name,
          h(
            "span",
            { class: "kp-field__help" },
            "What `homelab doctor` and audit.log will call this machine.",
          ),
        ),
        h(
          "div",
          { class: "kp-field" },
          h("label", { class: "kp-field__label", for: "token-scope" }, "Scope"),
          scope,
        ),
        errBox,
        h("div", { class: "kp-dialog__actions" }, cancel, go),
      ],
    });
    const unregister = register("tokens-issue", {
      dialog: d.dialog,
      go: () => go,
      cancel: () => cancel,
      close: () => d.close(),
    });
    d.closed.then(unregister);
    cancel.addEventListener("click", () => d.close());
    go.addEventListener("click", async () => {
      if (driven()) return;
      const n = name.value.trim();
      if (!n) {
        markErrors(new Map([["name", name]]), { name: "A name is needed." });
        return;
      }
      go.disabled = true;
      const r = await send(
        "POST",
        "/data/tokens",
        { name: n, scope: scope.value },
        "a new token",
      );
      go.disabled = false;
      if (!r.ok) {
        errBox.replaceChildren(refusalCallout(r.error));
        return;
      }
      d.close();
      showIssuedToken(r.body.issued);
      void loadTokens();
    });
  };
  /** @param {{name: string, scope: string, token: string}} issued */
  const showIssuedToken = (issued) => {
    const box = h("input", {
      class: "kp-field__input mono",
      type: "text",
      readonly: "",
      id: "issued-token",
    });
    box.value = issued.token;
    const copy = h("button", { type: "button", class: "kp-button" }, "Copy");
    copy.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(issued.token);
        notify("Copied.", "success");
      } catch {
        box.select();
      }
    });
    const done = h(
      "button",
      { type: "button", class: "kp-button kp-button--primary" },
      "Done — I saved it",
    );
    const d = openDialog({
      title: `${issued.name} · shown once`,
      description: `Set it as HOMELAB_TOKEN on that machine (its own .env or ~/.config/homelab/env). It is never shown again, and it is not shared with any other machine's token.`,
      id: "token-issued-dialog",
      body: [
        h("div", { class: "kp-field" }, box, " ", copy),
        h("div", { class: "kp-dialog__actions" }, done),
      ],
    });
    done.addEventListener("click", () => d.close());
  };
  const revokeTokenDialog = (/** @type {string} */ name) => {
    const typed = h("input", {
      class: "kp-field__input",
      type: "text",
      id: "token-revoke-confirm",
      autocomplete: "off",
      placeholder: name,
    });
    const go = h(
      "button",
      { type: "button", class: "kp-button kp-button--destructive" },
      "Revoke",
    );
    const cancel = h(
      "button",
      { type: "button", class: "kp-button" },
      "Cancel",
    );
    const errBox = h("div");
    const d = openDialog({
      title: `Revoke ${name}`,
      description:
        "That machine's token stops working at once; no other machine's token is affected.",
      id: "token-revoke-dialog",
      body: [
        h(
          "div",
          { class: "kp-field" },
          h(
            "label",
            { class: "kp-field__label", for: "token-revoke-confirm" },
            `Type ${name} to confirm`,
          ),
          typed,
        ),
        errBox,
        h("div", { class: "kp-dialog__actions" }, cancel, go),
      ],
    });
    cancel.addEventListener("click", () => d.close());
    go.addEventListener("click", async () => {
      if (driven()) return;
      if (typed.value.trim() !== name) {
        markErrors(new Map([["c", typed]]), { c: `Type ${name} exactly.` });
        return;
      }
      go.disabled = true;
      const r = await send(
        "DELETE",
        `/data/tokens/${encodeURIComponent(name)}`,
        undefined,
        "revoke a token",
      );
      go.disabled = false;
      if (!r.ok) {
        errBox.replaceChildren(refusalCallout(r.error));
        return;
      }
      notify(`${name} revoked.`, "success");
      d.close();
      void loadTokens();
    });
  };

  /** @param {HostField} f */
  const openKey = (f) => {
    const pending = changes.get(f.key);
    const start = pending
      ? f.kind.type === "bool"
        ? pending.value === true
        : String(pending.value ?? "")
      : f.kind.type === "bool"
        ? f.value === true
        : fieldText(f);
    const id = `key-${f.key.replace(/_/g, "-")}`;
    /** @type {HTMLInputElement | HTMLTextAreaElement} */
    let input;
    if (f.kind.type === "bool") {
      const c = h("input", { type: "checkbox", class: "kp-field__check", id });
      c.checked = start === true;
      input = c;
    } else if (f.kind.type === "table") {
      const ta = h("textarea", {
        class: "kp-field__input mono",
        id,
        rows: "10",
        spellcheck: "false",
      });
      ta.value = String(start);
      input = ta;
    } else {
      const i = h("input", {
        class: "kp-field__input",
        // fix-143: a dashboard_secret field is write-only (the host never
        // sends its value back, same as an ssh-only secret) — masked so it
        // is not read over someone's shoulder while it is typed.
        type: isSecret(f) ? "password" : "text",
        id,
        autocomplete: "off",
        spellcheck: "false",
      });
      i.value = String(start);
      input = i;
    }
    const typed = h("input", {
      class: "kp-field__input",
      type: "text",
      id: `${id}-confirm`,
      autocomplete: "off",
      placeholder: f.key,
    });
    const save = h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--primary",
        id: "key-stage",
      },
      "Keep this change",
    );
    const reset = h(
      "button",
      { type: "button", class: "kp-button" },
      "Use the default",
    );
    const cancel = h(
      "button",
      { type: "button", class: "kp-button kp-button--ghost" },
      "Cancel",
    );
    const d = openDialog({
      title: `${f.label} · ${f.key}`,
      description: `${f.help} Default: ${f.default}. Takes effect ${f.apply === "live" ? "at once" : "at the host's next start"}.${isSecret(f) ? ` Currently ${f.set ? "set" : "not set"} — the value itself is never shown.` : ""}`,
      id: "key-dialog",
      body: [
        h(
          "div",
          { class: "kp-field" },
          f.kind.type === "bool"
            ? h("label", { class: "kp-field__label", for: id }, input, " on")
            : h(
                "label",
                { class: "kp-field__label", for: id },
                f.kind.type === "table" ? `${f.key} as TOML` : "Value",
              ),
          ...(f.kind.type === "bool" ? [] : [input]),
          h(
            "span",
            { class: "kp-field__help" },
            f.kind.type === "table"
              ? "Only this key: [[key]] or [key] sections. Empty removes it."
              : isSecret(f)
                ? "Leave empty and save to clear it; Cancel leaves it as it is."
                : "Empty removes the key: the host takes the default.",
          ),
        ),
        ...(f.access === "confirm"
          ? [
              h(
                "div",
                { class: "kp-field field-danger" },
                h(
                  "label",
                  { class: "kp-field__label", for: `${id}-confirm` },
                  `Type ${f.key} to confirm`,
                ),
                typed,
                h(
                  "span",
                  { class: "kp-field__help" },
                  "This key can cut the dashboard's route or stop every backup (arch-self).",
                ),
              ),
            ]
          : []),
        h("div", { class: "kp-dialog__actions" }, cancel, reset, save),
      ],
    });
    const unregisterKey = register("key", {
      dialog: d.dialog,
      key: f.key,
      save: () => save,
      default: () => reset,
      cancel: () => cancel,
      close: () => d.close(),
    });
    d.closed.then(unregisterKey);
    cancel.addEventListener("click", () => d.close());
    const stage = (/** @type {unknown} */ value) => {
      // Live view: the change is kept on the dashboard's server.
      if (driven()) return;
      if (f.access === "confirm") {
        if (typed.value.trim() !== f.key) {
          markErrors(new Map([["confirm", typed]]), {
            confirm: `Type ${f.key} exactly.`,
          });
          return;
        }
        confirmed.add(f.key);
      }
      changes.set(f.key, { field: f, value });
      paintStaged();
      paintRows();
      d.close();
    };
    reset.addEventListener("click", () => stage(null));
    save.addEventListener("click", () => {
      const raw =
        input instanceof HTMLInputElement && input.type === "checkbox"
          ? input.checked
          : input.value;
      const p = parseKey(f, raw);
      if (!p.ok) {
        markErrors(new Map([["value", input]]), { value: p.why });
        return;
      }
      stage(p.value);
    });
  };

  const openReview = () => {
    if (!page) return;
    const body = hostSettingsBody(page.sha256, changes, confirmed);
    // Owner decision 2026-09-30 (item 2): a staged change whose key only
    // takes effect at the host's next start means one press both saves
    // and restarts homelab-host.service, so the change is in force at
    // once instead of waiting for the next unrelated restart.
    const restartNeeded = [...changes.values()].some(
      ({ field }) => field.apply === "restart",
    );
    const list = h(
      "ul",
      { class: "batch-preview" },
      ...[...changes.values()].map(({ field, value }) =>
        h(
          "li",
          null,
          h("strong", null, field.key),
          ` ${valueText(field)} → ${pendingText(field, value)} · ${field.apply === "live" ? "at once" : "at the host's next start"}`,
        ),
      ),
    );
    const runError = h("div");
    const jobBox = h("div");
    const save = h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--primary",
        id: "settings-write",
      },
      restartNeeded ? "Save and restart the host" : "Save",
    );
    const cancel = h("button", { type: "button", class: "kp-button" }, "Back");
    const d = openDialog({
      title: restartNeeded
        ? "Write host.toml and restart the host"
        : "Write host.toml",
      description: restartNeeded
        ? "The host checks the change like a start and refuses it whole if anything is wrong, or if host.toml changed since this page read it. At least one of these keys only takes effect at the host's next start, so this also restarts homelab-host.service (refused while another job runs) and waits for it to answer again."
        : "The host checks the change like a start and refuses it whole if anything is wrong, or if host.toml changed since this page read it.",
      id: "settings-review",
      wide: true,
      body: [
        list,
        runError,
        jobBox,
        h("div", { class: "kp-dialog__actions" }, cancel, save),
      ],
    });
    /** @type {() => void} */
    let stopPanel = () => {};
    const unregisterReview = register("host-review", {
      dialog: d.dialog,
      save: () => save,
      back: () => cancel,
      runError: (/** @type {import("../doctor.js").RouteError | null} */ e) =>
        runError.replaceChildren(...(e ? [refusalCallout(e)] : [])),
      close: () => d.close(),
    });
    d.closed.then(() => {
      unregisterReview();
      stopPanel();
    });
    cancel.addEventListener("click", () => d.close());
    save.addEventListener("click", async () => {
      // Live view: host.toml is written on the dashboard's server, once,
      // and the restart follow-up (below) is queued there too.
      if (driven()) return;
      save.disabled = true;
      const r = await send(
        "PUT",
        "/data/host-settings",
        body,
        "the host settings",
      );
      if (!r.ok) {
        save.disabled = false;
        runError.replaceChildren(refusalCallout(r.error));
        await refusalAlarm(r.error, r.status);
        return;
      }
      saved(r.body);
      if (r.body.follow?.refused) {
        runError.replaceChildren(
          refusalCallout(r.body.follow.refused, "warning", "Not restarted"),
        );
        save.disabled = false;
        return;
      }
      const job = r.body.follow?.job;
      if (typeof job !== "number") {
        d.close();
        return;
      }
      cancel.hidden = true;
      save.hidden = true;
      const panel = mountJobPanel(job, { compact: true });
      stopPanel = panel.stop;
      jobBox.replaceChildren(panel.element);
    });
  };

  /** host.toml was written: say so, and read it again.
   * @param {any} answer */
  const saved = (answer) => {
    const s = answer.saved ?? {};
    const later = /** @type {string[]} */ (s.restart ?? []);
    const restarting = typeof answer.follow?.job === "number";
    notify(
      restarting
        ? "host.toml written; restarting homelab-host.service…"
        : `host.toml written.${later.length ? ` At the host's next start: ${later.join(", ")}.` : " In force now."}`,
      "success",
    );
    changes.clear();
    confirmed.clear();
    paintStaged();
    void loadHost();
  };

  // feat-platform-10: the Live view replay works this very page: the
  // changes Claude kept on the dashboard's server, the key dialog and the
  // review dialog are the ones Edit and Review open.
  let lastSaved = "";
  const unregister = register("host-settings", {
    ready: () => page !== null,
    openKey: (/** @type {string} */ key) => {
      const f = page?.fields.find((x) => x.key === key);
      if (f) openKey(f);
    },
    rowButton: (/** @type {string} */ key) =>
      /** @type {HTMLElement | null} */ (
        t.tbody.querySelector(`button[data-key="${CSS.escape(key)}"]`)
      ),
    reviewButton: () =>
      /** @type {HTMLElement | null} */ (
        staged.querySelector("#settings-save")
      ),
    setStaged: (
      /** @type {Record<string, unknown>} */ want,
      /** @type {string[]} */ typed,
    ) => {
      if (!page) return;
      const same =
        changes.size === Object.keys(want).length &&
        [...changes].every(
          ([k, v]) => JSON.stringify(v.value) === JSON.stringify(want[k]),
        );
      if (same) return;
      changes.clear();
      confirmed.clear();
      for (const [k, v] of Object.entries(want)) {
        const field = page.fields.find((x) => x.key === k);
        if (field) changes.set(k, { field, value: v });
      }
      for (const k of typed) confirmed.add(k);
      paintStaged();
      paintRows();
    },
    openReview: () => openReview(),
    showSaved: (/** @type {any} */ answer) => {
      const key = JSON.stringify(answer);
      if (key === lastSaved) return;
      lastSaved = key;
      saved(answer);
    },
  });

  issueBtn.addEventListener("click", () => issueTokenDialog());

  const offRepo = listen("repo", (v) => drawRepo(repoBox, v, loadRepo));
  const offHost = listen("host_settings", () => void loadHost());
  const offTokens = listen("tokens", () => void loadTokens());
  const retry = () => void loadHost().catch(() => {});
  const retryTokens = () => void loadTokens().catch(() => {});
  root.addEventListener("kp-datatable-retry", retry);
  root.addEventListener("kp-datatable-retry", retryTokens);
  void loadRepo().catch(() => {});
  retry();
  retryTokens();
  return () => {
    unregister();
    abort.abort();
    offRepo();
    offHost();
    offTokens();
    root.removeEventListener("kp-datatable-retry", retry);
    root.removeEventListener("kp-datatable-retry", retryTokens);
    unbind();
    unbindTokens();
    detach();
  };
}

/**
 * The working copy panel.
 * @param {HTMLElement} box
 * @param {{repo: {present: boolean, remote: string, branch: string,
 *   head: CommitRef | null, unpushed: CommitRef[], behind: number,
 *   dirty: string[], key_present: boolean | null, fetched_at: number | null,
 *   error: string | null}, deployed_unpushed: {stack: string, commit: string}[]}} v
 * @param {() => Promise<void>} reload
 */
function drawRepo(box, v, reload) {
  const r = v.repo;
  const facts = h(
    "dl",
    { class: "facts" },
    h("dt", null, "Remote"),
    h("dd", { class: "mono" }, `${r.remote} · ${r.branch}`),
    h("dt", null, "At"),
    h(
      "dd",
      null,
      r.head
        ? `${r.head.commit.slice(0, 10)} · ${r.head.subject}`
        : r.present
          ? "—"
          : "not cloned yet",
    ),
    h("dt", null, "Last fetched"),
    h("dd", null, r.fetched_at ? formatDateTime(r.fetched_at) : "not yet"),
    ...(r.key_present === null
      ? []
      : [
          h("dt", null, "Deploy key"),
          h("dd", null, r.key_present ? "present" : "missing"),
        ]),
  );
  const sync = h(
    "button",
    { type: "button", class: "kp-button", id: "repo-sync" },
    "Fetch now",
  );
  sync.addEventListener("click", async () => {
    sync.disabled = true;
    const x = await send(
      "POST",
      "/data/repo/sync",
      undefined,
      "the working copy",
    );
    sync.disabled = false;
    if (!x.ok) await refusalAlarm(x.error, x.status);
    else notify("The working copy is up to date.", "success");
    void reload();
  });
  const parts = /** @type {Node[]} */ ([facts]);
  if (r.error)
    parts.push(
      h(
        "div",
        { class: "kp-alert kp-alert--warning", role: "status" },
        `The last fetch or clone failed: ${r.error}`,
      ),
    );
  if (r.dirty.length)
    parts.push(
      h(
        "div",
        { class: "kp-alert kp-alert--destructive", role: "alert" },
        `Files differ from the last commit: ${r.dirty.join(", ")}. Edits are refused until that is looked at by hand.`,
      ),
    );
  if (r.unpushed.length) {
    const deployed = v.deployed_unpushed.map((d) => d.stack);
    parts.push(
      h(
        "div",
        { class: "kp-alert kp-alert--warning", role: "status" },
        h(
          "strong",
          null,
          `${r.unpushed.length} commit(s) here are not on the remote. `,
        ),
        deployed.length
          ? `The host runs ${deployed.join(", ")} from one of them: push keeps the repository and the fleet in step.`
          : "Push them, rebase them onto the remote, or drop them.",
        h(
          "ul",
          null,
          ...r.unpushed.map((c) =>
            h(
              "li",
              { class: "mono" },
              `${c.commit.slice(0, 10)} · ${c.subject}`,
            ),
          ),
        ),
        h(
          "div",
          { class: "row-buttons" },
          .../** @type {const} */ (["push", "rebase", "drop"]).map((choice) =>
            resolveButton(choice, reload),
          ),
        ),
      ),
    );
  }
  if (r.behind)
    parts.push(
      h(
        "p",
        { class: "measured" },
        `${r.behind} commit(s) on the remote are not here yet; the next edit or Fetch takes them.`,
      ),
    );
  parts.push(h("div", { class: "row-buttons" }, sync));
  box.replaceChildren(...parts);
}

/**
 * @param {"push" | "rebase" | "drop"} choice
 * @param {() => Promise<void>} reload
 */
function resolveButton(choice, reload) {
  const b = h(
    "button",
    {
      type: "button",
      class: `kp-button${choice === "drop" ? " kp-button--destructive" : ""}`,
      "data-choice": choice,
    },
    { push: "Push", rebase: "Rebase and push", drop: "Drop them…" }[choice],
  );
  b.addEventListener("click", async () => {
    if (choice === "drop") {
      const typed = h("input", {
        class: "kp-field__input",
        type: "text",
        id: "drop-confirm",
        placeholder: "drop",
      });
      const go = h(
        "button",
        { type: "button", class: "kp-button kp-button--destructive" },
        "Drop the commits",
      );
      const d = openDialog({
        title: "Drop the unpushed commits",
        description:
          "The working copy goes back to the remote's branch; those commits are gone from here.",
        id: "drop-dialog",
        body: [
          h(
            "div",
            { class: "kp-field" },
            h(
              "label",
              { class: "kp-field__label", for: "drop-confirm" },
              "Type drop to confirm",
            ),
            typed,
          ),
          h("div", { class: "kp-dialog__actions" }, go),
        ],
      });
      go.addEventListener("click", async () => {
        if (typed.value.trim() !== "drop") {
          markErrors(new Map([["c", typed]]), { c: "Type drop exactly." });
          return;
        }
        d.close();
        await resolve(choice, reload);
      });
      return;
    }
    await resolve(choice, reload);
  });
  return b;
}

/**
 * @param {"push" | "rebase" | "drop"} choice
 * @param {() => Promise<void>} reload
 */
async function resolve(choice, reload) {
  const r = await send(
    "POST",
    "/data/repo/unpushed",
    { choice },
    "the unpushed commits",
  );
  if (!r.ok) await refusalAlarm(r.error, r.status);
  else notify(choice === "drop" ? "Dropped." : "Pushed.", "success");
  void reload();
}
