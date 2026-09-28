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
  parseKey,
  valueText,
} from "../editforms.js";
import { markErrors } from "../editui.js";
import { formatTime } from "../format.js";
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
  const hostErr = h("div");
  const hostHead = h("p", { class: "measured" });
  const staged = h("div", {
    class: "staged",
    role: "status",
    "aria-live": "polite",
  });
  const ago = agoEl("read");
  const t = tableBlock({
    remember: "host-settings",
    caption: "host.toml on pve",
    search: "Search settings",
    state: "loading",
    columns: [
      { label: "Group", sort: "text", filter: "choice" },
      { label: "Setting", sort: "text" },
      { label: "Value", sort: "text", cls: "wide" },
      { label: "Takes effect", sort: "text", filter: "choice" },
      {
        label: "Changed",
        sort: "text",
        order:
          "ssh only (secret),ssh only (safety policy),ssh only (can cut the dashboard off),Here with the name typed,Here",
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
    hostErr,
    staged,
    t.wrap,
    h("p", null, ago),
  );
  const detach = attachDataTables(root);
  const table = dataTable(t.wrap);
  const unbind = bindTableUrl(table, "settings");
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
    table?.state("loading");
    const r = await fetchJson(
      "/data/host-settings",
      "the host settings",
      abort.signal,
    );
    if (!r.ok) {
      hostErr.replaceChildren(errorBox(r.error));
      table?.state("failed");
      return;
    }
    hostErr.replaceChildren();
    page = r.body.page;
    const unknown = /** @type {string[]} */ (r.body.page.unknown ?? []);
    hostHead.textContent = `${r.body.page.path} · version ${String(r.body.page.sha256).slice(0, 12)}${unknown.length ? ` · keys the host does not read: ${unknown.join(", ")}` : ""}`;
    paintRows();
    table?.state("ready");
    setAgo(ago, r.body.measured_at ?? Date.now() / 1000);
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
        type: "text",
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
      description: `${f.help} Default: ${f.default}. Takes effect ${f.apply === "live" ? "at once" : "at the host's next start"}.`,
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
    cancel.addEventListener("click", () => d.close());
    const stage = (/** @type {unknown} */ value) => {
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
    const save = h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--primary",
        id: "settings-write",
      },
      "Write host.toml",
    );
    const cancel = h("button", { type: "button", class: "kp-button" }, "Back");
    const d = openDialog({
      title: "Write host.toml",
      description:
        "The host checks the change like a start and refuses it whole if anything is wrong, or if host.toml changed since this page read it.",
      id: "settings-review",
      wide: true,
      body: [
        list,
        runError,
        h("div", { class: "kp-dialog__actions" }, cancel, save),
      ],
    });
    cancel.addEventListener("click", () => d.close());
    save.addEventListener("click", async () => {
      save.disabled = true;
      const r = await send(
        "PUT",
        "/data/host-settings",
        body,
        "the host settings",
      );
      save.disabled = false;
      if (!r.ok) {
        runError.replaceChildren(refusalCallout(r.error));
        await refusalAlarm(r.error, r.status);
        return;
      }
      const s = r.body.saved ?? {};
      const later = /** @type {string[]} */ (s.restart ?? []);
      notify(
        `host.toml written.${later.length ? ` At the host's next start: ${later.join(", ")}.` : " In force now."}`,
        "success",
      );
      changes.clear();
      confirmed.clear();
      paintStaged();
      d.close();
      void loadHost();
    });
  };

  const offRepo = listen("repo", (v) => drawRepo(repoBox, v, loadRepo));
  const offHost = listen("host_settings", () => void loadHost());
  const retry = () => void loadHost().catch(() => {});
  root.addEventListener("kp-datatable-retry", retry);
  void loadRepo().catch(() => {});
  retry();
  return () => {
    abort.abort();
    offRepo();
    offHost();
    root.removeEventListener("kp-datatable-retry", retry);
    unbind();
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
    h("dd", null, r.fetched_at ? formatTime(r.fetched_at) : "not yet"),
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
