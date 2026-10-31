// Secrets (redesign 3.71, Kenny's approved demo `secrets.html`, 2026-10-03;
// rebuilt on the approved kp-themes 11 demo `app-homelab-settings`,
// feat-kp11demo-6, Kenny 2026-10-19). What a stack declares as latch secrets
// and latch files: each a masked field (kp-themes' `secret`: Reveal reads it
// from the host and shows it for 30 s with a draining rule, Copy copies it
// without showing it), the audit sentence the host wrote for the read, the
// restart order and which file feeds which app, and the change drawer: paste,
// stage, write, with a 5 s Undo that survives leaving the page.
// feat-secrets-1/2 still hold: a value is fetched only on a press, lives only
// in this page's memory while it is shown, hides itself after 30 seconds (and
// at once on Hide all, Esc or leaving the page), and every reveal and copy is
// written to the host's audit trail with who asked. The write rides
// `ActionKind::ChangeSecret` (stage the value, then press), so it gets the same
// job trail as every other dashboard write; the value never becomes a URL, a
// query parameter or part of `ActionArgs`.
//
// The fleet-wide page (stack list, keys, drawer on one grid: kp-themes'
// master-detail) has no address since 3.71.0; each stack's Settings shows its
// own (`opts.stack`), without the list.

import { send } from "../act.js";
import { fetchJson, h } from "../dom.js";
import { declare, declareField, drivable } from "../drivable.js";
import { formatDateTime } from "../format.js";
import { stackHref } from "../router.js";
import {
  secretRows,
  splitReason,
  stackCount,
  stackKind,
  stackLine,
} from "../secretsview.js";
import { registerShortcuts } from "/static/kp/dist/kp-themes.min.js";
import {
  GROUP_SECRETS,
  noDialog,
  onPageKey,
  unlessDialog,
} from "../shortcuts.js";
import { current, listen, subscribe } from "../store.js";
import { esc } from "../settingsform.js";
import { liveStatus, pageHeader, toast } from "../ui.js";
import { attachAll } from "/static/kp/dist/kp-themes.min.js";
import { attachDisabledReasons } from "/static/kp/dist/kp-themes.min.js";
import { setCardState } from "/static/kp/dist/kp-themes.min.js";
import { deplist } from "/static/kp/dist/kp-themes.min.js";
import { setMatrix } from "/static/kp/dist/kp-themes.min.js";
import { setPaneState } from "/static/kp/dist/kp-themes.min.js";
import { hideSecret } from "/static/kp/dist/kp-themes.min.js";
import { undoToast } from "/static/kp/dist/kp-themes.min.js";

// review M5: every page field Live view may set is declared (drivable.js
// `declareField`); the client and the dashboard refuse any other.
const SECRET_VALUE = declareField({
  id: "secret-value",
  page: "secrets",
  what: "the new value of a secret (Change drawer)",
});
const SECRET_RESTART = declareField({
  id: "secret-restart",
  page: "secrets",
  what: "restart the stack after writing (Change drawer)",
});

// fix-239 / invariant 39: Live view reaches every control on this page —
// `homelab ui click <id> [row]`, the row being `<stack>/<app>/.env` (or
// `<stack>/<from>` for a latch file); the value and the restart box are the
// page's own fields (`homelab ui type secret-value …`, `ui check
// secret-restart on|off`). The final write is also the catalog form
// `homelab ui open change-secret <stack>`.
/**
 * feat-shell-1 (FLOWS.md §2): since 3.71.0 the secrets live in each stack
 * hub's Settings; a control on a stack's row is found there.
 * @param {string | null} row
 */
const secretsAt = (row) =>
  row
    ? `/stacks/${encodeURIComponent(row.split("/")[0])}/settings?section=secrets`
    : null;
const REVEAL_SECRET = declare({
  id: "reveal-secret",
  page: "secrets",
  opens: "run",
  row: "<stack>/<secret>",
  at: secretsAt,
  what: "show one secret's value for 30 seconds (once more hides it again); audited",
});
const COPY_SECRET = declare({
  id: "copy-secret",
  page: "secrets",
  opens: "run",
  row: "<stack>/<secret>",
  at: secretsAt,
  what: "copy one secret's value to the clipboard without showing it; audited",
});
const EDIT_SECRET = declare({
  id: "edit-secret",
  page: "secrets",
  opens: "run",
  row: "<stack>/<secret>",
  at: secretsAt,
  what: "open the change drawer for one secret",
});
const CHANGE_A_SECRET = declare({
  id: "change-a-secret",
  page: "secrets",
  opens: "run",
  what: "open the change drawer for the first secret of the stack on screen",
});
const HIDE_ALL = declare({
  id: "hide-all-secrets",
  page: "secrets",
  opens: "run",
  what: "hide every revealed value now",
});
const STAGE_SECRET = declare({
  id: "stage-secret",
  page: "secrets",
  opens: "run",
  what: "stage the typed value (nothing is written yet)",
  shows: "in the change drawer, once a value is typed",
  reach: [
    { do: "click", control: "change-a-secret" },
    { do: "type", field: "secret-value", text: "DEMO=1" },
  ],
});
const WRITE_SECRET = declare({
  id: "write-secret",
  page: "secrets",
  opens: "run",
  what: "write the staged value through latch after a 5 s Undo window, then restart when ticked",
  shows: "in the change drawer, once the value is staged",
  reach: [
    { do: "click", control: "change-a-secret" },
    { do: "type", field: "secret-value", text: "DEMO=1" },
    { do: "click", control: "stage-secret" },
  ],
});
const EDIT_STAGED = declare({
  id: "edit-staged-secret",
  page: "secrets",
  opens: "run",
  what: "go back from the staged value to editing it",
  shows: "in the change drawer, once the value is staged",
  reach: [
    { do: "click", control: "change-a-secret" },
    { do: "type", field: "secret-value", text: "DEMO=1" },
    { do: "click", control: "stage-secret" },
  ],
});
const CLOSE_DRAWER = declare({
  id: "close-secret-change",
  page: "secrets",
  opens: "run",
  what: "close the change drawer without writing",
  shows: "while the change drawer is open",
  reach: [{ do: "click", control: "change-a-secret" }],
  // review M6: its x and its Cancel, each closing the drawer.
  twins: true,
});
const UNDO_WRITE = declare({
  id: "undo-secret-write",
  page: "secrets",
  opens: "run",
  what: "cancel a write in its 5 s Undo window, before it runs",
  shows: "for 5 s after Write",
  reach: [
    { do: "click", control: "change-a-secret" },
    { do: "type", field: "secret-value", text: "DEMO=1" },
    { do: "click", control: "stage-secret" },
    { do: "click", control: "write-secret" },
  ],
});
const RETRY_STACK = declare({
  id: "secrets-try-again",
  page: "secrets",
  opens: "run",
  what: "read an unreadable stack's file again",
  shows: "on a stack whose secrets file could not be read",
});

/** How long Write waits for an Undo before it sends anything. */
const UNDO_MS = 5000;

/**
 * A plain message at the foot of the screen (ui.js `toast`, drawn the
 * page's way); a newer one replaces it.
 * @param {string} msg
 * @param {number} [ms]
 */
const say = (msg, ms = 5000) => void toast(msg, { ms });

/**
 * Ask the host for one value, recorded as a reveal or a copy. `driven` is
 * true for a click Live view made (the server believes it only while Live
 * view drives), so Activity names Claude instead of the person.
 * @param {string} stack
 * @param {import("../secretsview.js").SecretRef} ref
 * @param {"reveal" | "copy"} purpose
 * @param {boolean} driven
 */
async function readValue(stack, ref, purpose, driven) {
  return send(
    "POST",
    `/data/secrets/${encodeURIComponent(stack)}/reveal`,
    { secret: ref, purpose, driven },
    purpose === "copy" ? "copy the secret" : "reveal the secret",
  );
}

/**
 * The restart plan of a stack: what each file restarts when it changes, the
 * files of the unit that restarts first at the top (the stack's restart
 * order, edited on its Settings tab).
 * @param {string} stack
 * @param {import("../secretsview.js").Declared} d
 * @returns {{from: string, to: string, state: "unknown"}[]}
 */
export function restartItems(stack, d) {
  const rank = (/** @type {string} */ u) => {
    const i = (d.restart_order ?? []).indexOf(u);
    return i < 0 ? Number.MAX_SAFE_INTEGER : i;
  };
  return d.files
    .filter((f) => !!f.restarts)
    .map((f) => ({
      from: f.from,
      to: /** @type {string} */ (f.restarts),
      state: /** @type {const} */ ("unknown"),
    }))
    .sort((a, b) => rank(a.to) - rank(b.to));
}

/**
 * The units a file change restarts, in their restart order, as one
 * sentence ("paperless restarts postgres first, then redis"); empty when
 * fewer than two units restart, because one has no order.
 * @param {string} stack
 * @param {import("../secretsview.js").Declared} d
 */
export function restartSentence(stack, d) {
  const units = [...new Set(restartItems(stack, d).map((i) => i.to))];
  if (units.length < 2) return "";
  return `${stack} restarts ${units[0]} first, then ${units.slice(1).join(", then ")}.`;
}

/**
 * Which file feeds which app: a stack's `.env` secrets feed the app they
 * are named for; a file feeds what it restarts. Files that feed nothing are
 * drawn as unused.
 * @param {string} stack
 * @param {import("../secretsview.js").Declared} d
 */
export function usesOf(stack, d) {
  /** @type {{key: string, label: string, sub: string, ghost?: boolean}[]} */
  const files = [];
  /** @type {Map<string, string[]>} */
  const uses = new Map();
  const apps = new Set();
  for (const app of d.secrets) {
    const key = `${app}/.env`;
    files.push({ key, label: key, sub: "latch secret" });
    uses.set(key, [app]);
    apps.add(app);
  }
  for (const f of d.files) {
    files.push({
      key: f.from,
      label: f.from,
      sub: `written to ${f.dest}`,
      ghost: !f.restarts,
    });
    uses.set(f.from, f.restarts ? [f.restarts] : []);
    if (f.restarts) apps.add(f.restarts);
  }
  void stack;
  return { files, apps: [...apps], uses };
}

/**
 * The fleet's Secrets page, or with `opts.stack` one stack's secrets
 * without the stack list and page header (the stack hub's Settings,
 * `?section=secrets`).
 * @param {HTMLElement} root
 * @param {{stack?: string}} [opts]
 * @returns {() => void}
 */
export function mount(root, opts = {}) {
  const one = opts.stack ?? null;
  const abort = new AbortController();
  /** @type {(() => void)[]} */
  const stops = [];

  /** @type {{stack: string | null, reading: boolean,
   *   summary: Record<string, import("../secretsview.js").Declared> | null,
   *   failed: import("../doctor.js").RouteError | null,
   *   change: import("../secretsview.js").SecretRow | null,
   *   stage: 0 | 1, value: string, token: string | null, restart: boolean,
   *   staging: boolean, stageError: string | null}} */
  const S = {
    stack: one ?? new URLSearchParams(location.search).get("stack"),
    reading: false,
    summary: null,
    failed: null,
    change: null,
    stage: 0,
    value: "",
    token: null,
    restart: true,
    staging: false,
    stageError: null,
  };

  /** The stacks the list pane lists: the host's fleet, else the repository's. */
  const fleet = () => {
    const f = current().fleet?.stacks?.map((s) => s.name);
    if (f && f.length) return f;
    return S.summary ? Object.keys(S.summary) : [];
  };
  const entry = (/** @type {string} */ s) => S.summary?.[s];

  // ── the controls the header carries ──────────────────────────────────
  const hideAllBtn = drivable(
    h(
      "button",
      {
        class: "kp-button",
        type: "button",
        title: "Hide every revealed value now (Esc)",
      },
      "Hide all",
    ),
    HIDE_ALL,
  );
  const hideAll = () => {
    for (const f of root.querySelectorAll("[data-kp-secret]")) hideSecret(f);
  };
  hideAllBtn.addEventListener("click", hideAll);
  const changeBtn = drivable(
    h(
      "button",
      {
        class: "kp-button kp-button--primary",
        type: "button",
        title: "Stage a new value for the first secret of this stack",
      },
      "Change a secret",
    ),
    CHANGE_A_SECRET,
  );
  changeBtn.addEventListener("click", () => {
    const d = S.stack ? entry(S.stack) : null;
    const first = d && S.stack ? secretRows(S.stack, d)[0] : null;
    if (first) openChange(first);
    else say(`${S.stack ?? "This stack"} declares no secret to change`);
  });

  // ── the cards: keys (detail) and the change drawer ───────────────────
  /** @param {string} id @param {string | null} title @param {string} desc */
  const card = (id, title, desc) => {
    const data = h("div", { class: "kp-card__data" });
    // Inside a stack hub the hub's own section carries the heading: the pane
    // says what it is for in its sentence and names itself for a screen reader.
    const titleEl = h(
      title == null ? "span" : "h2",
      title == null
        ? { class: "kp-sr-only", id: `${id}-t` }
        : { class: "kp-card__title", id: `${id}-t` },
      title ?? "Secrets",
    );
    const lede = h("p", { class: "kp-lede" }, desc);
    const tools = h("div", { class: "kp-card__actions" });
    const el = h(
      "section",
      {
        class: "kp-card nx-card sx-card",
        id,
        "data-kp-state": "loading",
        "aria-labelledby": `${id}-t`,
      },
      h("div", { class: "kp-card__header" }, titleEl, tools),
      lede,
      h("div", { class: "kp-card__stage" }, data),
    );
    return { el, data, titleEl, lede, tools };
  };
  const audit = "Every reveal and change is written to the host's audit log";
  const detailCard = card(
    "sd",
    one != null ? null : "Secrets",
    one != null
      ? `${audit}. Esc hides every value.`
      : `What this stack's lxc-compose.yml lists under latch_secrets and latch_files. ${audit}.`,
  );
  const drawerCard = card(
    "ch",
    "Change a secret",
    "Pick Change… on a secret to stage a new value here. Nothing is written until you press Write.",
  );
  drawerCard.el.classList.add("sx-drawer");
  const closeBtn = drivable(
    h(
      "button",
      {
        class: "kp-button kp-button--sm",
        type: "button",
        "aria-label": "Close",
        title: "Close without writing (Esc)",
        onclick: () => closeChange(),
      },
      "×",
    ),
    CLOSE_DRAWER,
  );
  drawerCard.tools.append(closeBtn);
  const openStack = h(
    "a",
    { class: "kp-button kp-button--sm", title: "Open this stack's page" },
    "Open stack",
  );
  if (one == null) detailCard.tools.append(openStack);
  const detail = detailCard.data;
  const drawer = drawerCard.data;

  const head = pageHeader({
    title: "Secrets",
    desc: "The latch secrets and files each stack declares. Values stay hidden until you reveal one; it hides again after 30 seconds or when you leave. A change is written through latch, the same way the CLI reads it.",
    split: true,
    live: "read",
    actions: [hideAllBtn],
    primary: changeBtn,
  });
  const live = /** @type {ReturnType<typeof liveStatus>} */ (head.live);
  live.set(null, "reading…");

  /** The stack list pane (fleet page only). */
  const listEl = h("ul", {
    class: "kp-pane__list",
    role: "listbox",
    "aria-label": "Stacks",
  });
  const readsLine = h("p", { class: "kp-lede", "aria-live": "polite" });
  if (one != null) {
    // In the hub the hub carries the header; Hide all and Change a secret
    // stay as this section's own row.
    root.replaceChildren(
      h(
        "div",
        { class: "sx sx--one" },
        h(
          "div",
          { class: "sx-actions kp-formgrid__buttons" },
          hideAllBtn,
          changeBtn,
        ),
        h("div", { class: "sx-md sx-md--one" }, detailCard.el, drawerCard.el),
      ),
    );
  } else {
    root.replaceChildren(
      h(
        "div",
        { class: "sx" },
        head.el,
        h(
          "div",
          {
            class: "kp-split kp-split--panes sx-md",
            "data-kp-master-detail": "",
            "data-kp-pane-widths": "15rem 1fr 22rem",
            "data-kp-pane-history": "off",
            id: "sx-panes",
          },
          h(
            "section",
            {
              class: "kp-split__pane",
              "data-kp-pane": "list",
              "aria-label": "Stacks",
            },
            readsLine,
            listEl,
          ),
          h(
            "section",
            {
              class: "kp-split__pane",
              "data-kp-pane": "detail",
              "aria-label": "Secrets of the stack",
            },
            h("h3", { "data-kp-pane-heading": "", tabindex: "-1" }),
            h("div", { class: "kp-pane__body" }, detailCard.el),
          ),
          h(
            "aside",
            {
              class: "kp-split__pane",
              "data-kp-pane": "aside",
              "aria-label": "Change a secret",
            },
            drawerCard.el,
          ),
        ),
      ),
    );
  }
  const attached = attachAll(root);
  stops.push(() => attached());
  setCardState(detailCard.el, "loading", { words: "Reading the secrets…" });
  // The drawer stages a change to what the list shows: while the list is
  // read or cannot be read, the drawer is in the same state (its own busy
  // card and plate), not a form with nothing to change.
  let drawerSeen = "";
  const mirrorDrawer = () => {
    const st = detailCard.el.dataset.kpState ?? "loading";
    if (st === drawerSeen) return;
    drawerSeen = st;
    if (st === "loading")
      setCardState(drawerCard.el, "loading", { words: "Reading the secrets…" });
    else if (st === "error")
      setCardState(drawerCard.el, "error", {
        title: "Nothing to change yet",
        text: "The secrets could not be read, so there is nothing to stage. Try again on the list.",
        retry: false,
      });
    else setCardState(drawerCard.el, "ready");
  };
  new MutationObserver(mirrorDrawer).observe(detailCard.el, {
    attributes: true,
    attributeFilter: ["data-kp-state"],
  });
  mirrorDrawer();

  // ── the list pane ─────────────────────────────────────────────────────
  function paintList() {
    if (one != null) return;
    const names = fleet();
    const read = names.filter(
      (n) => entry(n) && stackKind(entry(n)) !== "unreadable",
    ).length;
    const bad = names.filter(
      (n) => entry(n) && stackKind(entry(n)) === "unreadable",
    ).length;
    readsLine.textContent = S.summary
      ? `${read} of ${names.length} stacks read${bad ? `, ${bad} failed` : ""}`
      : "Reading the stacks…";
    listEl.replaceChildren(
      ...names.map((s) => {
        const d = entry(s);
        const k = stackKind(d);
        return h(
          "li",
          {
            role: "option",
            "data-kp-pane-key": s,
            "aria-selected": String(s === S.stack),
            title: stackLine(d),
          },
          h(
            "span",
            { class: "sx-pane-item" },
            h("span", {
              class: "kp-idmark kp-idmark--sm",
              "data-kp-idmark": s,
              "aria-hidden": "true",
            }),
            h("b", null, s),
            h(
              "span",
              {
                class: `kp-badge${k === "unreadable" ? " kp-badge--warning" : ""}`,
              },
              k === "unreadable" ? "unreadable" : String(stackCount(d)),
            ),
          ),
        );
      }),
    );
    void attachAll(listEl);
  }

  // ── the keys of the stack on screen ───────────────────────────────────
  /** The audit sentence of a read, in the row's value track. @param {HTMLElement} rowEl @param {string} what @param {string} name */
  const written = (rowEl, what, name) => {
    const p = rowEl.querySelector("[data-audit]");
    if (p)
      p.textContent = `You ${what} ${name} on ${formatDateTime(Date.now() / 1000)}. The host wrote it to its audit log.`;
  };

  /** @param {string} s @param {import("../secretsview.js").Declared} d */
  function keysHtml(s, d) {
    const rows = secretRows(s, d);
    return `<div class="kp-formgrid sx-keys st-lines" data-kp-anchors="rows" data-kp-formgrid-label="wide" data-kp-formgrid-lines="lines" style="--kp-secret-min: 9rem">${rows
      .map(
        (r, i) =>
          `<div class="kp-formgrid__row" data-sx-row="${i}" data-key="${esc(r.key)}" data-kp-anchor="${esc(r.key)}" data-kp-anchor-label="${esc(r.name)}"><div class="kp-formgrid__label"><span class="sx-what"><span class="kp-idmark kp-idmark--sm" data-kp-idmark="${esc(r.name)}" aria-hidden="true"></span><b>${esc(r.name)}</b></span><small>${esc(r.note)}</small></div><div class="kp-formgrid__control"><div class="kp-secret" data-kp-secret data-kp-secret-ttl="30" data-kp-secret-clear="30" data-kp-secret-mask="12" data-kp-secret-label="${esc(s)}/${esc(r.name)}" data-sx-i="${i}"></div></div><p class="kp-formgrid__value" data-audit></p><div class="kp-formgrid__actions"><button type="button" class="kp-button kp-button--sm" data-sx-change="${i}" title="Stage a new value for this secret">Change…</button></div></div>`,
      )
      .join("")}</div>`;
  }

  /** @param {string} s @param {import("../secretsview.js").Declared} d */
  function drawKeys(s, d) {
    const rows = secretRows(s, d);
    detail.innerHTML = keysHtml(s, d);
    const parts = [];
    const restarts = restartItems(s, d);
    if (restarts.length) {
      parts.push(
        `<section class="st-sub" aria-labelledby="sx-restarts-h"><h3 id="sx-restarts-h">What restarts when a file changes</h3><p class="kp-lede">A changed file restarts what reads it, so nothing is restarted blind.${restartSentence(s, d) ? ` ${esc(restartSentence(s, d))} The order is set on the stack's Settings tab.` : ""}</p><div class="kp-deplist" data-kp-deplist id="sx-restarts" data-kp-dep-order="none" data-kp-dep-direction="from-reaches-to" data-kp-dep-heading="{n} restarts when ${esc(s)}'s files change" data-kp-dep-empty="Nothing restarts when a file changes."></div></section>`,
      );
    }
    const u = usesOf(s, d);
    if (u.files.length) {
      parts.push(
        `<section class="st-sub" aria-labelledby="sx-uses-h"><h3 id="sx-uses-h">Which file feeds which app</h3><p class="kp-lede">If you change a file, the apps on its lines notice.</p><div class="kp-matrix sx-uses" data-kp-matrix data-kp-view="lines" data-kp-matrix-owned aria-label="Which file feeds which app" id="sx-uses"></div></section>`,
      );
    }
    detail.insertAdjacentHTML("beforeend", parts.join(""));
    const off = attachAll(detail);
    void off.ready.then(() => {
      for (const field of detail.querySelectorAll("[data-kp-secret]")) {
        const i = Number(field.getAttribute("data-sx-i"));
        const row = rows[i];
        const reveal = field.querySelector(".kp-secret__reveal");
        const copy = field.querySelector(".kp-secret__copy");
        if (reveal)
          drivable(/** @type {HTMLElement} */ (reveal), REVEAL_SECRET, row.key);
        if (copy)
          drivable(/** @type {HTMLElement} */ (copy), COPY_SECRET, row.key);
        field.addEventListener(
          "click",
          (e) => {
            /** @type {any} */ (field).__trusted = e.isTrusted;
          },
          true,
        );
      }
      for (const b of detail.querySelectorAll("[data-sx-change]")) {
        const row = rows[Number(b.getAttribute("data-sx-change"))];
        drivable(/** @type {HTMLElement} */ (b), EDIT_SECRET, row.key);
        b.addEventListener("click", () => openChange(row));
      }
      const dep = detail.querySelector("#sx-restarts");
      if (dep) {
        const handle = deplist(dep);
        handle?.set(restarts);
        handle?.setState("ready");
      }
      const uses = /** @type {HTMLElement | null} */ (
        detail.querySelector("#sx-uses")
      );
      if (uses) {
        setMatrix(uses, {
          view: "lines",
          corner: "file ↓ app →",
          rows: u.files.map((f) => ({
            key: f.key,
            label: f.label,
            sub: f.sub,
            glyph: "text",
            ghost: f.ghost,
            dashed: f.ghost,
          })),
          columns: u.apps.map((a) => ({ key: a, label: a })),
          cell: (r, c) =>
            u.uses.get(r)?.includes(c)
              ? { tone: "ok", label: "reads it" }
              : { tone: "none" },
          countLabel: (n) => `${n} ${n === 1 ? "app" : "apps"}`,
          sourcesLabel: "Files",
          usersLabel: "Apps",
        });
      }
    });
    stops.push(() => off());
  }

  /** Every secret field asks for its value through this one listener. */
  detail.addEventListener("kp-secret-request", (e) => {
    const s = S.stack;
    const field = /** @type {HTMLElement} */ (e.target);
    const d = s ? entry(s) : null;
    if (!s || !d) return;
    const row = secretRows(s, d)[Number(field.getAttribute("data-sx-i"))];
    const { respond, fail, purpose } = /** @type {CustomEvent} */ (e).detail;
    e.preventDefault();
    const driven = /** @type {any} */ (field).__trusted === false;
    void readValue(s, row.ref, purpose, driven).then((r) => {
      if (!r.ok) {
        fail(new Error(r.error.why));
        return;
      }
      respond(String(r.body?.value ?? ""));
      const rowEl = /** @type {HTMLElement} */ (field.closest("[data-sx-row]"));
      written(
        rowEl,
        purpose === "copy" ? "copied" : "revealed",
        `${s}/${row.name}`,
      );
    });
  });

  function paintDetail() {
    const s = S.stack;
    if (!s) {
      setCardState(detailCard.el, S.summary ? "empty" : "loading", {
        title: "Pick a stack",
        text: "Choose one in the list to read its keys.",
        words: "Reading the secrets…",
      });
      openStack.hidden = true;
      return;
    }
    const d = entry(s);
    const k = stackKind(d);
    if (one == null) detailCard.titleEl.textContent = s;
    openStack.hidden = false;
    openStack.setAttribute("href", stackHref(s));
    if (S.reading || !S.summary) {
      setCardState(detailCard.el, "loading", { words: "Reading the secrets…" });
      return;
    }
    if (k === "ok" && d) {
      drawKeys(s, d);
      setCardState(detailCard.el, "ready");
      return;
    }
    detail.replaceChildren();
    if (k === "unreadable" && d?.unreadable) {
      const { why, fix } = splitReason(d.unreadable);
      setCardState(detailCard.el, "error", {
        title: `${s}'s secrets cannot be read`,
        text: `${why}${fix ? `. ${fix}` : ""}`.replace(/\.\.$/, "."),
      });
      return;
    }
    const open = h(
      "a",
      { class: "kp-button", href: `${stackHref(s, "settings")}?section=files` },
      "Open its stack file",
    );
    setCardState(detailCard.el, "empty", {
      title: `${s} declares no secrets`,
      text: "Add latch_secrets or latch_files to its lxc-compose.yml to keep a value out of the repository.",
      action: open,
    });
  }
  detailCard.el.addEventListener("kp-card-retry", () => {
    if (S.stack) void pick(S.stack);
    else void load();
  });
  // The unreadable state's Try again carries the Live view control.
  new MutationObserver(() => {
    const b = detailCard.el.querySelector(".kp-card__retry");
    if (b && !b.hasAttribute("data-drive"))
      drivable(/** @type {HTMLElement} */ (b), RETRY_STACK);
  }).observe(detailCard.el, { childList: true, subtree: true });

  // ── the change drawer: paste → stage → write ─────────────────────────
  /** @param {import("../secretsview.js").SecretRow} row */
  function openChange(row) {
    S.change = row;
    S.stage = 0;
    S.value = "";
    S.token = null;
    S.restart = true;
    S.stageError = null;
    paintDrawer();
    setTimeout(() => drawer.querySelector("textarea")?.focus(), 0);
  }
  function closeChange() {
    S.change = null;
    S.value = "";
    S.token = null;
    paintDrawer();
  }

  /** @param {("done" | "running" | "pending")[]} st @param {string} app */
  const steps = (st, app) =>
    `<ol class="kp-steplist" aria-label="Writing the secret">${[
      ["paste", "Paste", "The new value, into the box below"],
      ["stage", "Stage", "Kept in the dashboard until you write"],
      ["write", "Write", `Through latch, then ${app} restarts when ticked`],
    ]
      .map(
        ([id, t, n], i) =>
          `<li class="kp-steplist__item" data-state="${st[i]}" data-kp-step-id="${id}"><span class="kp-steplist__mark" aria-hidden="true"></span><span class="kp-steplist__body"><span class="kp-steplist__title">${t}</span><span class="kp-steplist__note">${esc(n)}</span></span></li>`,
      )
      .join("")}</ol>`;

  function paintDrawer() {
    closeBtn.hidden = !S.change;
    if (!S.change) {
      drawerCard.titleEl.textContent = "Change a secret";
      drawerCard.lede.textContent =
        "Pick Change… on a secret to stage a new value here. Nothing is written until you press Write.";
      drawer.innerHTML = steps(["pending", "pending", "pending"], "the app");
      void attachAll(drawer);
      return;
    }
    const row = S.change;
    const stack = /** @type {string} */ (S.stack);
    const app = row.ref.kind === "env" ? row.ref.app : stack;
    drawerCard.titleEl.textContent = `Change ${row.name}`;
    drawerCard.lede.textContent =
      "The new value never enters a URL or the job's arguments; latch stores it.";
    drawer.innerHTML = `${steps(S.stage ? ["done", "running", "pending"] : ["running", "pending", "pending"], app)}<div class="kp-formgrid sx-change" data-kp-formgrid-label="wide" data-kp-formgrid-lines="none"><div class="kp-formgrid__row"><label class="kp-formgrid__label" for="${SECRET_VALUE}">New value<small>KEY=value, one per line</small></label><div class="kp-formgrid__control"><textarea class="kp-field__input" id="${SECRET_VALUE}" name="secret-value" rows="5" placeholder="KEY=value" autocomplete="off" spellcheck="false"${S.stage ? ' disabled data-kp-disabled-reason="Staged for writing: press Edit value to change it"' : ""}></textarea></div></div><div class="kp-formgrid__row"><div class="kp-formgrid__label">After writing</div><div class="kp-formgrid__control"><label class="kp-switch" title="After the write, deploy ${esc(stack)} so ${esc(app)} reads the new value"><input class="kp-switch__input" type="checkbox" role="switch" id="${SECRET_RESTART}" name="secret-restart"${S.restart ? " checked" : ""} /><span>Restart ${esc(app)}</span></label></div></div>${S.stageError ? `<div class="kp-formgrid__row"><p class="kp-formgrid__control" data-kp-formgrid-span="all" role="alert">Could not stage it: ${esc(S.stageError)}</p></div>` : ""}<div class="kp-formgrid__row kp-formgrid__row--buttons"><div class="kp-formgrid__control" data-kp-formgrid-span="control-value"><div class="kp-formgrid__buttons"><button type="button" class="kp-button" data-sx-back>${S.stage ? "Edit value" : "Cancel"}</button><button type="button" class="kp-button kp-button--primary" data-sx-next>${S.stage ? "Write and restart" : S.staging ? "Staging…" : "Stage"}</button></div></div></div></div>`;
    const ta = /** @type {HTMLTextAreaElement} */ (
      drawer.querySelector("textarea")
    );
    ta.value = S.value;
    const next = /** @type {HTMLButtonElement} */ (
      drawer.querySelector("[data-sx-next]")
    );
    const back = /** @type {HTMLElement} */ (
      drawer.querySelector("[data-sx-back]")
    );
    const restart = /** @type {HTMLInputElement} */ (
      drawer.querySelector(`#${SECRET_RESTART}`)
    );
    drivable(next, S.stage ? WRITE_SECRET : STAGE_SECRET);
    if (S.stage) next.dataset.action = "change-secret";
    drivable(back, S.stage ? EDIT_STAGED : CLOSE_DRAWER);
    const sync = () => {
      const off = S.staging || (!S.stage && !S.value.trim());
      next.disabled = off;
      if (off && !S.staging && !S.stage)
        next.setAttribute(
          "data-kp-disabled-reason",
          "Paste or type the new value first",
        );
      else next.removeAttribute("data-kp-disabled-reason");
      // A reason that arrives after the drawer was wired is wired now.
      void attachDisabledReasons(drawer);
    };
    sync();
    ta.addEventListener("input", () => {
      S.value = ta.value;
      sync();
    });
    restart.addEventListener("change", () => (S.restart = restart.checked));
    next.addEventListener("click", () => (S.stage ? write() : void stage()));
    back.addEventListener("click", () => {
      if (!S.stage) return closeChange();
      S.stage = 0;
      S.token = null;
      paintDrawer();
    });
    void attachAll(drawer);
  }

  /** Step 2: the value goes to the dashboard's memory; nothing is written. */
  async function stage() {
    if (!S.value.trim() || S.staging) return;
    S.staging = true;
    S.stageError = null;
    paintDrawer();
    const r = await send(
      "POST",
      "/data/secrets/stage",
      { content: S.value },
      "stage the new value",
    );
    S.staging = false;
    if (r.ok) {
      S.token = String(r.body?.stage_token ?? "");
      S.stage = 1;
    } else S.stageError = r.error.why;
    paintDrawer();
  }

  /**
   * Step 3: after a 5 s Undo window, the write job; then the restart. Leaving
   * the page inside the window writes it (leave: "commit"), never loses it.
   */
  function write() {
    const row = S.change;
    const stack = S.stack;
    const token = S.token;
    const restart = S.restart;
    if (!row || !stack || !token) return;
    const was = { stage: S.stage, value: S.value };
    closeChange();
    const t = undoToast(`Writing ${row.name} through latch`, {
      mode: "defer",
      ms: UNDO_MS,
      clock: "ring",
      key: "Mod+Z",
      leave: "commit",
      failed: "Not written: the host refused the change.",
      commit: async () => {
        const r = await send(
          "POST",
          `/data/actions/${encodeURIComponent(stack)}/change-secret`,
          // bundle-secret-restart: the write and the deploy are one job on
          // the server, so leaving the page never loses the deploy.
          {
            secret_ref: JSON.stringify(row.ref),
            stage_token: token,
            ...(restart ? { then_deploy: true } : {}),
          },
          "change the secret",
        );
        if (!r.ok) throw new Error(r.error.why);
        say(
          restart
            ? `Writing ${row.name} through latch, then deploying ${stack} · one job queued`
            : `Writing ${row.name} through latch · job queued`,
          6000,
        );
      },
      undo: () => say("Write cancelled before it ran"),
      // A refused write brings the staged value back.
      restore: () => {
        if (S.stack !== stack) return;
        S.change = row;
        S.stage = /** @type {0 | 1} */ (was.stage);
        S.value = was.value;
        S.token = token;
        paintDrawer();
      },
    });
    const undo = t.toast.querySelector("[data-kp-undo-button]");
    if (undo) drivable(/** @type {HTMLElement} */ (undo), UNDO_WRITE);
  }

  // ── data ──────────────────────────────────────────────────────────────
  /** @param {string} s */
  async function pick(s) {
    chosen = true;
    S.stack = s;
    hideAll();
    if (S.change) S.change = null;
    S.reading = true;
    if (one == null) {
      const url = new URL(location.href);
      url.searchParams.set("stack", s);
      history.replaceState(history.state, "", url.pathname + url.search);
    }
    paintAll();
    const r = await fetchJson(
      `/data/secrets/${encodeURIComponent(s)}`,
      `${s}'s secrets`,
      abort.signal,
    ).catch(() => null);
    if (!r || S.stack !== s) return;
    S.reading = false;
    if (S.summary)
      S.summary[s] = r.ok
        ? r.body
        : {
            secrets: [],
            files: [],
            unreadable: `${r.error.why} :: ${r.error.fix || "read again"}`,
          };
    paintAll();
  }

  async function load() {
    S.failed = null;
    const r = await fetchJson(
      "/data/secrets",
      "the stacks' secrets",
      abort.signal,
    ).catch(() => null);
    if (!r) return;
    if (!r.ok) {
      S.failed = r.error;
      live.fail("not read");
      setCardState(detailCard.el, "error", {
        title: "The stacks' secrets could not be read",
        text: `${r.error.why}${r.error.fix ? `. ${r.error.fix}` : ""}`,
      });
      return;
    }
    S.summary = r.body.stacks ?? {};
    live.set(Date.now() / 1000);
    choose();
    paintAll();
  }

  /** The stack shown when none was asked for: the first that declares
   * something, in the fleet's order — chosen again once the fleet arrives,
   * until a stack is picked. */
  let chosen = S.stack != null;
  function choose() {
    if (one != null) return;
    const names = fleet();
    if (chosen && S.stack && names.includes(S.stack)) return;
    const first =
      names.find((n) => stackKind(entry(n)) === "ok") ?? names[0] ?? null;
    if (first === S.stack) return;
    S.stack = first;
  }

  function paintAll() {
    paintList();
    paintDetail();
    paintDrawer();
  }

  // The master-detail's own selection reads the stack.
  root.addEventListener("kp-pane-select", (e) => {
    const key = /** @type {CustomEvent} */ (e).detail?.key;
    if (key && key !== S.stack) void pick(key);
  });
  root.addEventListener("kp-pane-retry", () => {
    if (S.stack) void pick(S.stack);
  });
  void setPaneState;

  // r reveals the chosen secret (the first, or the row the focus is in).
  stops.push(
    onPageKey("reveal", () => {
      const focused = document.activeElement?.closest?.("[data-sx-row]");
      const field = (
        focused ?? detail.querySelector("[data-sx-row]")
      )?.querySelector(".kp-secret__reveal");
      /** @type {HTMLElement | null} */ (field)?.click();
    }),
  );

  // Esc hides every revealed secret and closes the change form, in a field
  // too (kp 11.2.0 `inFields`).
  stops.push(
    registerShortcuts("page:secrets", [
      {
        keys: "Escape",
        inFields: true,
        what: "Hide every secret and close the change form",
        group: GROUP_SECRETS,
        run: unlessDialog(() => {
          hideAll();
          if (S.change) closeChange();
        }),
      },
    ]),
  );
  const off = subscribe(() => {
    if (!S.summary) return;
    if (one == null && !chosen && current().fleet) {
      chosen = true;
      S.stack = null;
      choose();
      paintDetail();
    }
    paintList();
  });

  paintAll();
  void load();
  // fix-wipe-1: a finished change-secret job changes what this page lists;
  // read it again rather than wait for a refresh.
  const offJobs = listen("action", (j) => {
    if (j?.action !== "change-secret") return;
    if (j.state !== "done" && j.state !== "failed") return;
    if (S.stack === j.stack && !S.change) void pick(j.stack);
    else void load();
  });
  return () => {
    abort.abort();
    hideAll();
    off();
    offJobs();
    for (const s of stops) s();
  };
}
