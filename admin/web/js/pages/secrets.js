// Secrets (redesign 3.71, Kenny's approved demo `secrets.html`, 2026-10-03,
// plus "Alle drie"). Three panes: every stack with its exact count (a
// warning chip, with the reason and the fix, on a stack whose file does not
// read), the chosen stack's secrets with a masked value, Reveal, Copy and
// Change…, and the change drawer in three steps (paste → stage → write and
// restart, with Undo). feat-secrets-1/2 still hold: a value is fetched only
// on a press, lives only in this page's memory, hides itself after 30
// seconds (and at once on Hide all, Esc or leaving the page), and every
// reveal and copy is written to the host's audit trail with who asked —
// Activity reads "Kenny revealed gateway/traefik/.env". The write rides
// `ActionKind::ChangeSecret` (stage the value, then press), so it gets the
// same job trail as every other dashboard write; the value never becomes a
// URL, a query parameter or part of `ActionArgs`.
//
// Self-contained on purpose (the redesign's shared shell is built beside
// it): the hover card, the toast and the stack colour are small local
// helpers below, marked LOCAL, for the foundation's components to replace.

import { act, onAct, send } from "../act.js";
import { agoEl, setAgo } from "../ago.js";
import { fetchJson, h } from "../dom.js";
import { declare, drivable } from "../drivable.js";
import { stackHref } from "../router.js";
import {
  createReveals,
  secretRows,
  splitReason,
  stackCount,
  stackKind,
  stackLine,
} from "../secretsview.js";
import { current, subscribe } from "../store.js";

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
const PICK_STACK = declare({
  id: "secrets-stack",
  page: "secrets",
  opens: "run",
  row: "<stack>",
  // feat-shell-1: the secrets live in each stack hub's Settings.
  at: secretsAt,
  what: "show one stack's secrets (re-reads what it declares)",
});
const REVEAL_SECRET = declare({
  id: "reveal-secret",
  page: "secrets",
  opens: "run",
  row: "<stack>/<secret>",
  // feat-shell-1: the secrets live in each stack hub's Settings.
  at: secretsAt,
  what: "show one secret's value for 30 seconds (once more hides it again); audited",
});
const COPY_SECRET = declare({
  id: "copy-secret",
  page: "secrets",
  opens: "run",
  row: "<stack>/<secret>",
  // feat-shell-1: the secrets live in each stack hub's Settings.
  at: secretsAt,
  what: "copy one secret's value to the clipboard without showing it; audited",
});
const EDIT_SECRET = declare({
  id: "edit-secret",
  page: "secrets",
  opens: "run",
  row: "<stack>/<secret>",
  // feat-shell-1: the secrets live in each stack hub's Settings.
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
});
const WRITE_SECRET = declare({
  id: "write-secret",
  page: "secrets",
  opens: "run",
  what: "write the staged value through latch after a 5 s Undo window, then restart when ticked",
});
const EDIT_STAGED = declare({
  id: "edit-staged-secret",
  page: "secrets",
  opens: "run",
  what: "go back from the staged value to editing it",
});
const CLOSE_DRAWER = declare({
  id: "close-secret-change",
  page: "secrets",
  opens: "run",
  what: "close the change drawer without writing",
});
const UNDO_WRITE = declare({
  id: "undo-secret-write",
  page: "secrets",
  opens: "run",
  what: "cancel a write in its 5 s Undo window, before it runs",
});
const RETRY_STACK = declare({
  id: "secrets-try-again",
  page: "secrets",
  opens: "run",
  what: "read an unreadable stack's file again",
});

/** How long Write waits for an Undo before it sends anything. */
const UNDO_MS = 5000;
/** The clipboard is cleared this long after a copy. */
const CLIPBOARD_MS = 30_000;

/** The page's stylesheet, added once. */
function ensureStyles() {
  if (document.querySelector('link[data-page-css="secrets"]')) return;
  const l = document.createElement("link");
  l.rel = "stylesheet";
  l.href = "/css/secrets.css";
  l.dataset.pageCss = "secrets";
  document.head.append(l);
}

/** LOCAL: one hue per stack, in the fleet's sorted order. @param {string} s @param {string[]} all */
const stackColour = (s, all) =>
  `var(--chart-${(Math.max(0, [...all].sort().indexOf(s)) % 5) + 1})`;

/** @param {HTMLElement} el @param {Record<string, string>} props */
function styled(el, props) {
  for (const [k, v] of Object.entries(props)) el.style.setProperty(k, v);
  return el;
}

/** LOCAL: the colour swatch. @param {string} s @param {string[]} all */
const swatch = (s, all) =>
  styled(h("span", { class: "sw", "aria-hidden": "true" }), {
    "--c": stackColour(s, all),
  });

/** A skeleton bar. @param {string} w */
const sk = (w) => styled(h("span", { class: "sk" }, "·"), { "--w": w });

/**
 * LOCAL: a short message at the bottom of the screen, with an optional
 * action (Undo). Returns a function that removes it.
 * @param {string} msg
 * @param {{label: string, drive?: string, onClick: () => void}} [action]
 * @param {number} [ms]
 */
function toast(msg, action, ms = 5000) {
  const t = h(
    "div",
    { class: "sx-toast", role: "status" },
    h("span", null, msg),
  );
  if (action) {
    const b = h(
      "button",
      { class: "kp-button kp-button--sm kp-button--ghost", type: "button" },
      action.label,
    );
    if (action.drive) drivable(b, action.drive);
    b.addEventListener("click", () => {
      t.remove();
      action.onClick();
    });
    t.append(b);
  }
  // One stack of toasts: a plain message replaces the plain one before it,
  // never a toast still offering Undo.
  let box = document.querySelector(".sx-toasts");
  if (!box) {
    box = h("div", { class: "sx-toasts" });
    document.body.append(box);
  }
  box
    .querySelectorAll(".sx-toast:not([data-action])")
    .forEach((x) => x.remove());
  if (action) t.dataset.action = "";
  box.append(t);
  const timer = setTimeout(() => t.remove(), ms);
  return () => {
    clearTimeout(timer);
    t.remove();
  };
}

/**
 * LOCAL: one floating card for hover AND keyboard focus, under the element
 * (above when there is no room), never under the pointer.
 * @param {HTMLElement} tip
 * @param {HTMLElement} el
 * @param {() => Node[]} fill
 */
function tipOn(tip, el, fill) {
  const show = () => {
    tip.replaceChildren(...fill());
    tip.hidden = false;
    const r = el.getBoundingClientRect();
    const tw = tip.offsetWidth;
    const th = tip.offsetHeight;
    const left = Math.min(
      Math.max(8, r.left + r.width / 2 - tw / 2),
      innerWidth - tw - 8,
    );
    let top = r.bottom + 8;
    if (top + th > innerHeight - 8) top = r.top - th - 8;
    tip.style.left = `${left + scrollX}px`;
    tip.style.top = `${top + scrollY}px`;
  };
  const hide = () => (tip.hidden = true);
  el.addEventListener("pointerenter", show);
  el.addEventListener("pointerleave", hide);
  el.addEventListener("focus", show);
  el.addEventListener("blur", hide);
}

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
 * After a write's job is done, deploy the stack so the app reads the new
 * value ("restart"). Lives outside the page: it still happens when the page
 * was left meanwhile. A failed write deploys nothing.
 * @param {number} job
 * @param {string} stack
 */
function deployAfter(job, stack) {
  const off = onAct("jobs", () => {
    const j = act.jobs.find((x) => x.job === job);
    if (
      !j ||
      !["done", "failed", "refused", "deferred", "unknown"].includes(j.state)
    )
      return;
    off();
    if (j.state !== "done") {
      toast(
        `The write ${j.state}; ${stack} was not restarted`,
        undefined,
        8000,
      );
      return;
    }
    void send(
      "POST",
      `/data/actions/${encodeURIComponent(stack)}/deploy`,
      {},
      `deploy ${stack}`,
    ).then((r) =>
      toast(
        r.ok
          ? `Restarting ${stack}: deploy queued · see Activity`
          : `Could not restart ${stack}: ${r.error.why}`,
        undefined,
        8000,
      ),
    );
  });
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
  ensureStyles();
  const abort = new AbortController();
  const tip = h("div", { class: "sx-tip", role: "tooltip", hidden: "" });
  document.body.append(tip);

  /** @type {{stack: string | null, reading: boolean,
   *   summary: Record<string, import("../secretsview.js").Declared> | null,
   *   failed: import("../doctor.js").RouteError | null,
   *   busy: Set<string>, errors: Map<string, string>,
   *   change: import("../secretsview.js").SecretRow | null,
   *   stage: 0 | 1, value: string, token: string | null, restart: boolean,
   *   staging: boolean, stageError: string | null}} */
  const S = {
    stack: one ?? new URLSearchParams(location.search).get("stack"),
    reading: false,
    summary: null,
    failed: null,
    busy: new Set(),
    errors: new Map(),
    change: null,
    stage: 0,
    value: "",
    token: null,
    restart: true,
    staging: false,
    stageError: null,
  };
  const reveals = createReveals({ onChange: () => paintDetail() });

  /** The stacks the left pane lists: the host's fleet, else the repository's. */
  const fleet = () => {
    const f = current().fleet?.stacks?.map((s) => s.name);
    if (f && f.length) return f;
    return S.summary ? Object.keys(S.summary) : [];
  };
  const entry = (/** @type {string} */ s) => S.summary?.[s];

  const live = agoEl("read");
  const liveWrap = h(
    "span",
    { class: "nx-live", "data-state": "loading" },
    live,
  );
  live.className = "";
  live.textContent = "reading…";

  const hideAllBtn = drivable(
    h(
      "button",
      {
        class: "kp-button kp-button--secondary",
        type: "button",
        title: "Hide every revealed value now (Esc)",
      },
      "Hide all",
    ),
    HIDE_ALL,
  );
  hideAllBtn.addEventListener("click", () => reveals.hideAll());
  const changeBtn = drivable(
    h(
      "button",
      {
        class: "kp-button kp-button--primary",
        type: "button",
        title: "Stage a new value for the first secret of this stack",
      },
      "Change a secret…",
    ),
    CHANGE_A_SECRET,
  );
  changeBtn.addEventListener("click", () => {
    const d = S.stack ? entry(S.stack) : null;
    const first = d && S.stack ? secretRows(S.stack, d)[0] : null;
    if (first) openChange(first);
    else toast(`${S.stack ?? "This stack"} declares no secret to change`);
  });

  const list = h("nav", { class: "nx-card sx-stacks", "aria-label": "Stacks" });
  const detail = h("section", { class: "nx-card", "aria-labelledby": "sd-h" });
  const drawer = h("aside", {
    class: "nx-card sx-drawer",
    "aria-labelledby": "ch-h",
  });

  if (one != null) {
    // In the hub the hub carries the header; Hide all and Change a secret
    // stay as this section's own row.
    root.replaceChildren(
      h(
        "div",
        { class: "sx sx--one" },
        h("div", { class: "nx-header__actions" }, hideAllBtn, changeBtn),
        h("div", { class: "sx-md sx-md--one" }, detail, drawer),
      ),
    );
  } else
    root.replaceChildren(
      h(
        "div",
        { class: "sx" },
        h(
          "header",
          { class: "nx-header" },
          h(
            "div",
            { class: "nx-header__title" },
            h("h1", null, "Secrets"),
            liveWrap,
          ),
          h(
            "p",
            { class: "nx-header__desc" },
            "The latch secrets and files each stack declares. Values stay hidden until you reveal one; it hides again after 30 seconds or when you leave. A change is written through latch, the same way the CLI reads it.",
          ),
          h("div", { class: "nx-header__actions" }, hideAllBtn, changeBtn),
        ),
        h("div", { class: "sx-md" }, list, detail, drawer),
      ),
    );

  // ---- left pane -------------------------------------------------------
  function paintList() {
    const names = fleet();
    if (!S.summary || names.length === 0) {
      list.replaceChildren(
        ...Array.from({ length: Math.max(names.length, 4) }, () =>
          h(
            "div",
            { class: "sx-skrow", "aria-hidden": "true" },
            sk("10px"),
            h("span", null, sk("60%"), h("small", null, sk("80%"))),
            sk("18px"),
          ),
        ),
      );
      if (S.failed)
        list.prepend(
          h(
            "div",
            { class: "kp-alert kp-alert--destructive", role: "alert" },
            h(
              "div",
              { class: "kp-alert__body" },
              h("strong", null, "Could not read the stacks' secrets"),
              h("p", null, S.failed.why),
              S.failed.fix ? h("p", null, `Fix: ${S.failed.fix}`) : "",
            ),
          ),
        );
      return;
    }
    list.replaceChildren(
      ...names.map((s) => {
        const d = entry(s);
        const k = stackKind(d);
        const chip =
          k === "unreadable"
            ? h(
                "span",
                {
                  class: "nx-chip nx-chip--warn",
                  "aria-label": "unreadable",
                },
                "!",
              )
            : h("span", { class: "nx-chip" }, String(stackCount(d)));
        const b = drivable(
          h(
            "button",
            {
              type: "button",
              "aria-current": String(s === S.stack),
              "aria-label": `${s}: ${stackLine(d)}`,
            },
            swatch(s, names),
            h("span", null, s, h("small", null, stackLine(d))),
            chip,
          ),
          PICK_STACK,
          s,
        );
        b.addEventListener("click", () => pick(s));
        tipOn(tip, b, () => {
          const e = entry(s);
          const kk = stackKind(e);
          return [
            h("b", null, s),
            kk === "ok" && e
              ? h(
                  "span",
                  null,
                  secretRows(s, e)
                    .map((r) => r.name)
                    .join(", "),
                )
              : kk === "unreadable" && e?.unreadable
                ? h(
                    "span",
                    { class: "nx-dot nx-dot--warn" },
                    splitReason(e.unreadable).why,
                  )
                : h("span", null, "No latch_secrets or latch_files declared."),
            h(
              "span",
              { class: "hint" },
              "Click to open · ↑ ↓ move between stacks",
            ),
          ];
        });
        return b;
      }),
    );
  }

  // ---- middle pane -----------------------------------------------------
  /** @param {import("../secretsview.js").SecretRow} row */
  function secRow(row) {
    const s = /** @type {string} */ (S.stack);
    const value = reveals.value(row.key);
    const shown = value != null;
    const err = S.errors.get(row.key);
    const busy = S.busy.has(row.key);
    const left = Math.ceil(reveals.left(row.key) / 1000);
    const mask = h(
      "div",
      {
        class: `sx-mask${shown ? " sx-mask--shown" : ""}${err && !shown ? " sx-mask--error" : ""}`,
        "aria-live": "polite",
      },
      h(
        "span",
        { "data-secret-value": row.key },
        shown
          ? value
          : err
            ? `could not read: ${err}`
            : busy
              ? "reading…"
              : "••••••••••••••••••••",
      ),
      h(
        "span",
        { class: "hint", "data-left": shown ? row.key : "" },
        shown ? `hides in ${left} s` : err ? "" : "hidden",
      ),
    );
    if (shown) {
      // The bar drains over the full 30 s; a repaint continues it where it
      // was instead of starting over.
      const bar = h("i", { class: "sx-mask__timer", "aria-hidden": "true" });
      styled(bar, {
        "--sx-ttl": `${reveals.ttl}ms`,
        "animation-delay": `-${reveals.ttl - reveals.left(row.key)}ms`,
      });
      mask.append(bar);
    }
    const reveal = drivable(
      h(
        "button",
        {
          class: "kp-button kp-button--sm",
          type: "button",
          "aria-pressed": String(shown),
          title: shown ? "Hide it again now" : "Show the value for 30 seconds",
        },
        shown ? "Hide" : busy ? "Reading…" : "Reveal",
      ),
      REVEAL_SECRET,
      row.key,
    );
    if (busy) reveal.setAttribute("disabled", "");
    reveal.addEventListener("click", async (e) => {
      if (reveals.value(row.key) != null) {
        // Hiding never asks the host: the value is dropped from memory.
        reveals.hide(row.key);
        return;
      }
      S.busy.add(row.key);
      S.errors.delete(row.key);
      paintDetail();
      const r = await readValue(s, row.ref, "reveal", !e.isTrusted);
      S.busy.delete(row.key);
      if (r.ok) reveals.reveal(row.key, String(r.body?.value ?? ""));
      else {
        S.errors.set(row.key, r.error.why);
        paintDetail();
      }
    });
    const copy = drivable(
      h(
        "button",
        {
          class: "kp-button kp-button--sm kp-button--ghost",
          type: "button",
          title: "Copy the value to the clipboard without showing it",
        },
        "Copy",
      ),
      COPY_SECRET,
      row.key,
    );
    copy.addEventListener("click", (e) => copyValue(row, !e.isTrusted));
    const change = drivable(
      h(
        "button",
        {
          class: "kp-button kp-button--sm",
          type: "button",
          title: "Stage a new value for this secret",
        },
        "Change…",
      ),
      EDIT_SECRET,
      row.key,
    );
    change.addEventListener("click", () => openChange(row));
    return h(
      "div",
      { class: "sx-sec", "aria-current": String(S.change?.key === row.key) },
      h(
        "div",
        { class: "sx-sec__name" },
        h("strong", null, row.name),
        h("small", null, row.note),
      ),
      mask,
      h("div", { class: "sx-sec__btns" }, reveal, copy, change),
    );
  }

  /**
   * Copy: the clipboard write starts inside the click (a ClipboardItem
   * whose content is the host's answer, so the browser still counts the
   * click), and the value is never put on screen. Where the clipboard is
   * refused, the value is revealed and selected so Ctrl C copies it.
   * @param {import("../secretsview.js").SecretRow} row
   * @param {boolean} driven
   */
  function copyValue(row, driven) {
    const s = /** @type {string} */ (S.stack);
    /** @type {string | null} */
    let failure = null;
    /** @type {string | null} */
    let got = null;
    const asked = readValue(s, row.ref, "copy", driven).then((r) => {
      if (!r.ok) {
        failure = r.error.why;
        throw new Error(failure);
      }
      got = String(r.body?.value ?? "");
      return got;
    });
    const done = () => {
      S.errors.delete(row.key);
      paintDetail();
      toast(`Copied ${row.name} · the clipboard clears in 30 s`);
      setTimeout(() => {
        if (document.hasFocus())
          navigator.clipboard?.writeText("").catch(() => {});
      }, CLIPBOARD_MS);
    };
    const fallback = async () => {
      const v = got ?? (await asked.catch(() => null));
      if (v == null) {
        S.errors.set(row.key, failure ?? "the host did not answer");
        paintDetail();
        return;
      }
      try {
        await navigator.clipboard.writeText(v);
        done();
        return;
      } catch {
        // The browser refused the clipboard: show the value and select it.
      }
      reveals.reveal(row.key, v);
      const span = detail.querySelector(
        `[data-secret-value="${CSS.escape(row.key)}"]`,
      );
      const sel = getSelection();
      if (span && sel) {
        const range = document.createRange();
        range.selectNodeContents(span);
        sel.removeAllRanges();
        sel.addRange(range);
      }
      toast(`Press Ctrl C to copy ${row.name} · it hides in 30 s`);
    };
    if (typeof ClipboardItem !== "undefined" && navigator.clipboard?.write) {
      const blob = asked.then((v) => new Blob([v], { type: "text/plain" }));
      navigator.clipboard
        .write([new ClipboardItem({ "text/plain": blob })])
        .then(done, fallback);
    } else void fallback();
  }

  function paintDetail() {
    const s = S.stack;
    const names = fleet();
    if (!s) {
      detail.replaceChildren(
        h(
          "div",
          { class: "nx-card__head" },
          h("h2", { id: "sd-h" }, sk("8rem")),
          h(
            "p",
            null,
            "What this stack's lxc-compose.yml lists under latch_secrets and latch_files.",
          ),
        ),
        h("div", null, ...skRows()),
        foot(),
      );
      return;
    }
    const d = entry(s);
    const k = stackKind(d);
    /** @type {(Node | string)[]} */
    let body;
    if (S.reading || !S.summary) body = skRows();
    else if (k === "ok" && d)
      body = [
        ...secretRows(s, d).map(secRow),
        d.files.length
          ? ""
          : h("p", { class: "hint" }, `${s} declares no latch_files.`),
      ];
    else if (k === "unreadable" && d?.unreadable) {
      const { why, fix } = splitReason(d.unreadable);
      const again = drivable(
        h(
          "button",
          { class: "kp-button kp-button--sm", type: "button" },
          "Try again",
        ),
        RETRY_STACK,
      );
      again.addEventListener("click", () => pick(s));
      body = [
        h(
          "div",
          { class: "kp-alert kp-alert--warning", role: "alert" },
          h(
            "div",
            { class: "kp-alert__body" },
            h("strong", null, `${s}'s secrets cannot be read`),
            h("p", null, `${why}.`.replace(/\.\.$/, ".")),
            fix ? h("p", null, `Fix: ${fix}`.replace(/\.?$/, ".")) : "",
            again,
          ),
        ),
      ];
    } else
      body = [
        h(
          "div",
          { class: "nx-empty" },
          h("strong", null, `${s} declares no secrets`),
          h(
            "span",
            null,
            "Add latch_secrets or latch_files to its lxc-compose.yml to keep a value out of the repository.",
          ),
          h(
            "a",
            { class: "sx-linkbtn", href: stackHref(s, "settings") },
            "Open its stack file",
          ),
        ),
      ];
    const hintP = body.find(
      (x) => x instanceof HTMLElement && x.classList.contains("hint"),
    );
    if (hintP instanceof HTMLElement) hintP.style.margin = "12px 0 0";
    detail.replaceChildren(
      h(
        "div",
        { class: "nx-card__head" },
        h(
          "h2",
          { id: "sd-h" },
          h("span", { class: "nx-row" }, swatch(s, names), s),
        ),
        h(
          "p",
          null,
          "What this stack's lxc-compose.yml lists under latch_secrets and latch_files.",
        ),
        h(
          "div",
          { class: "nx-card__tools" },
          h(
            "a",
            {
              class: "kp-button kp-button--sm kp-button--ghost",
              href: stackHref(s),
              title: `Open ${s}'s page`,
            },
            "Open stack",
          ),
        ),
      ),
      h("div", null, ...body),
      foot(),
    );
  }
  const skRows = () =>
    [1, 2].map(() =>
      h(
        "div",
        { class: "sx-sec", "aria-hidden": "true" },
        h("div", { class: "sx-sec__name" }, sk("60%"), sk("80%")),
        h("div", { class: "sx-mask" }, sk("90%"), h("span")),
        h("div", { class: "sx-sec__btns" }, sk("12rem")),
      ),
    );
  const foot = () =>
    h(
      "div",
      { class: "nx-card__foot" },
      h(
        "span",
        null,
        "Every reveal and change is written to the host's audit log",
      ),
      h(
        "span",
        null,
        h("kbd", { class: "nx-kbd" }, "↑ ↓"),
        " stack · ",
        h("kbd", { class: "nx-kbd" }, "Esc"),
        " hide all",
      ),
    );

  // ---- right pane: the change drawer -------------------------------------
  /** @param {import("../secretsview.js").SecretRow} row */
  function openChange(row) {
    S.change = row;
    S.stage = 0;
    S.value = "";
    S.token = null;
    S.restart = true;
    S.stageError = null;
    paintDetail();
    paintDrawer();
    setTimeout(() => drawer.querySelector("textarea")?.focus(), 0);
  }
  function closeChange() {
    S.change = null;
    S.value = "";
    S.token = null;
    paintDetail();
    paintDrawer();
  }

  function paintDrawer() {
    if (!S.change) {
      drawer.replaceChildren(
        h(
          "div",
          { class: "nx-card__head" },
          h("h2", { id: "ch-h" }, "Change a secret"),
          h(
            "p",
            null,
            "Pick Change… on a secret to stage a new value here. Nothing is written until you press Write.",
          ),
        ),
        h(
          "ol",
          { class: "sx-steps" },
          h("li", null, "Paste the new value"),
          h("li", null, "Stage it: nothing is written yet"),
          h("li", null, "Write it through latch, then restart the app"),
        ),
      );
      return;
    }
    const row = S.change;
    const stack = /** @type {string} */ (S.stack);
    const app = row.ref.kind === "env" ? row.ref.app : stack;
    const ta = /** @type {HTMLTextAreaElement} */ (
      h("textarea", {
        class: "kp-field__input",
        id: "secret-value",
        name: "secret-value",
        placeholder: "KEY=value, one per line",
        "aria-label": "New value",
        autocomplete: "off",
        spellcheck: "false",
      })
    );
    ta.value = S.value;
    if (S.stage > 0) ta.disabled = true;
    const next = drivable(
      h(
        "button",
        {
          class: "kp-button kp-button--primary",
          type: "button",
          title: S.stage
            ? "Write the staged value through latch, then restart the app"
            : "Keep the value ready; nothing is written yet",
        },
        S.stage ? "Write and restart" : S.staging ? "Staging…" : "Stage",
      ),
      S.stage ? WRITE_SECRET : STAGE_SECRET,
    );
    if (S.stage) next.dataset.action = "change-secret";
    next.disabled = S.staging || (!S.stage && !S.value.trim());
    ta.addEventListener("input", () => {
      S.value = ta.value;
      next.disabled = !S.value.trim();
    });
    next.addEventListener("click", () => (S.stage ? write() : stage()));
    const restart = /** @type {HTMLInputElement} */ (
      h("input", {
        type: "checkbox",
        id: "secret-restart",
        name: "secret-restart",
      })
    );
    restart.checked = S.restart;
    restart.addEventListener("change", () => (S.restart = restart.checked));
    const close = drivable(
      h(
        "button",
        {
          class: "kp-button kp-button--sm kp-button--ghost",
          type: "button",
          "aria-label": "Close",
          title: "Close without writing (Esc)",
        },
        "×",
      ),
      CLOSE_DRAWER,
    );
    close.addEventListener("click", closeChange);
    const back = S.stage
      ? drivable(
          h(
            "button",
            {
              class: "kp-button kp-button--secondary",
              type: "button",
              title: "Back to editing the value",
            },
            "Edit value",
          ),
          EDIT_STAGED,
        )
      : drivable(
          h(
            "button",
            {
              class: "kp-button kp-button--secondary",
              type: "button",
              title: "Close without writing",
            },
            "Cancel",
          ),
          CLOSE_DRAWER,
        );
    back.addEventListener("click", () => {
      if (!S.stage) return closeChange();
      S.stage = 0;
      S.token = null;
      paintDrawer();
    });
    drawer.replaceChildren(
      h(
        "div",
        { class: "nx-card__head" },
        h("h2", { id: "ch-h" }, `Change ${row.name}`),
        h(
          "p",
          null,
          "The new value never enters a URL or the job's arguments; latch stores it.",
        ),
        h("div", { class: "nx-card__tools" }, close),
      ),
      h(
        "ol",
        { class: "sx-steps" },
        h("li", { class: S.stage ? "done" : "on" }, "Paste the new value"),
        h(
          "li",
          { class: S.stage === 1 ? "on" : "" },
          "Staged: nothing is written yet",
        ),
        h("li", null, `Write through latch and restart ${app}`),
      ),
      h(
        "label",
        { class: "kp-field" },
        h("span", { class: "kp-field__label" }, "New value"),
        ta,
      ),
      h(
        "label",
        {
          class: "kp-field kp-field--check",
          title: `After the write, deploy ${stack} so ${app} reads the new value`,
        },
        restart,
        ` Restart ${app} after writing`,
      ),
      S.stageError
        ? h(
            "p",
            { class: "hint", role: "alert" },
            `Could not stage it: ${S.stageError}`,
          )
        : "",
      h("div", { class: "nx-row sx-drawer__btns" }, back, next),
    );
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

  /** Step 3: after a 5 s Undo window, the write job; then the restart. */
  function write() {
    const row = S.change;
    const stack = S.stack;
    const token = S.token;
    const restart = S.restart;
    if (!row || !stack || !token) return;
    closeChange();
    const timer = setTimeout(async () => {
      dismiss();
      const r = await send(
        "POST",
        `/data/actions/${encodeURIComponent(stack)}/change-secret`,
        { secret_ref: JSON.stringify(row.ref), stage_token: token },
        "change the secret",
      );
      if (!r.ok) {
        toast(`Could not write ${row.name}: ${r.error.why}`, undefined, 10000);
        return;
      }
      toast(`Writing ${row.name} through latch · job queued`, undefined, 6000);
      if (restart && typeof r.body?.job === "number")
        deployAfter(r.body.job, stack);
    }, UNDO_MS);
    const dismiss = toast(
      `Writing ${row.name} through latch in 5 s`,
      {
        label: "Undo",
        drive: UNDO_WRITE,
        onClick: () => {
          clearTimeout(timer);
          toast("Write cancelled before it ran");
        },
      },
      UNDO_MS,
    );
  }

  // ---- data --------------------------------------------------------------
  /** @param {string} s */
  async function pick(s) {
    chosen = true;
    S.stack = s;
    reveals.hideAll();
    S.errors.clear();
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
    const r = await fetchJson(
      "/data/secrets",
      "the stacks' secrets",
      abort.signal,
    ).catch(() => null);
    if (!r) return;
    if (!r.ok) {
      S.failed = r.error;
      liveWrap.dataset.state = "failed";
      live.textContent = "not read";
      paintAll();
      return;
    }
    S.summary = r.body.stacks ?? {};
    S.failed = null;
    liveWrap.dataset.state = "";
    live.className = "";
    setAgo(live, Date.now() / 1000);
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
    if (first && S.summary) paintAll();
  }

  function paintAll() {
    paintList();
    paintDetail();
    paintDrawer();
  }

  // The "hides in N s" words tick each second, without a repaint.
  const ticker = setInterval(() => {
    detail.querySelectorAll("[data-left]").forEach((e) => {
      const key = /** @type {HTMLElement} */ (e).dataset.left;
      if (key)
        e.textContent = `hides in ${Math.ceil(reveals.left(key) / 1000)} s`;
    });
  }, 1000);

  /** @param {KeyboardEvent} e */
  const onKey = (e) => {
    if (document.querySelector("dialog[open]")) return;
    const tag =
      /** @type {HTMLElement | null} */ (document.activeElement)?.tagName ?? "";
    if (/INPUT|TEXTAREA|SELECT/.test(tag)) {
      if (e.key === "Escape" && S.change) closeChange();
      return;
    }
    if (e.key === "Escape") {
      reveals.hideAll();
      if (S.change) closeChange();
    }
    if (
      one == null &&
      (e.key === "ArrowDown" || e.key === "ArrowUp") &&
      S.summary
    ) {
      const names = fleet();
      if (!names.length) return;
      e.preventDefault();
      const i = names.indexOf(S.stack ?? "") + (e.key === "ArrowDown" ? 1 : -1);
      void pick(names[(i + names.length) % names.length]);
    }
  };
  document.addEventListener("keydown", onKey);
  const off = subscribe(() => {
    if (!S.summary) return;
    if (one == null && !chosen && current().fleet) {
      chosen = true;
      S.stack = null;
      choose();
    }
    paintList();
  });

  paintAll();
  void load();
  return () => {
    abort.abort();
    reveals.clear();
    clearInterval(ticker);
    document.removeEventListener("keydown", onKey);
    off();
    tip.remove();
  };
}
