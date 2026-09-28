// Milestone follow (feat-platform-10): Claude drives this dashboard with
// `homelab ui <step>`, and this tab plays each step when its "Watch Claude"
// toggle is on: the page changes, the dialog opens, the text appears letter
// by letter, the button shows its press. The final press runs on the
// dashboard's server, once; this tab only shows the job it started.
//
// Off (the default, remembered per tab): nothing here ever touches the
// page; the badge says what Claude is working on and offers to follow.
// On, while Claude drives: the viewer's own clicks and keys are refused
// with a visible note rather than mixed in.

import { openAction } from "./actiondialog.js";
import { notify } from "./actui.js";
import { fetchJson, h } from "./dom.js";
import {
  FOLLOW_KEY,
  badgeText,
  isActive,
  letterDelay,
  localOf,
  plan,
  readFollow,
} from "./driveview.js";
import { listen } from "./store.js";

/** @param {number} ms */
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const now = () => Date.now() / 1000;

/** sessionStorage: per tab, kept over a reload; absent in a private window. */
function storage() {
  try {
    return window.sessionStorage;
  } catch {
    return null;
  }
}

/**
 * The toggle and the badge at the top of every page, and the replay.
 * @param {HTMLElement} region
 * @param {{navigate: (href: string) => void}} ctx
 */
export function mountFollow(region, ctx) {
  let following = readFollow(storage());
  /** @type {import("./driveview.js").DriveState | null} */
  let state = null;
  /** @type {import("./driveview.js").Local} */
  let local = { seq: -1, page: location.pathname, form: null };
  /** @type {import("./actiondialog.js").ActionController | null} */
  let ctl = null;
  let queue = Promise.resolve();

  const toggle = h("input", {
    class: "kp-switch__input",
    type: "checkbox",
    role: "switch",
    id: "watch-claude",
    "aria-describedby": "watch-claude-hint",
  });
  toggle.checked = following;
  const badge = h("span", {
    class: "kp-badge kp-badge--info follow-badge",
    role: "status",
    "aria-live": "polite",
    hidden: "",
  });
  const watchBtn = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--ghost follow-watch",
      hidden: "",
    },
    "Watch",
  );
  region.replaceChildren(
    h(
      "label",
      { class: "kp-switch follow-switch" },
      toggle,
      h("span", { class: "kp-switch__state", "aria-hidden": "true" }),
      h("span", null, "Watch Claude"),
    ),
    h(
      "span",
      { class: "measured follow-hint", id: "watch-claude-hint" },
      "this tab plays Claude's steps live",
    ),
    badge,
    watchBtn,
  );

  const paint = () => {
    const text = badgeText(state, now());
    badge.hidden = !text;
    badge.textContent = text ?? "";
    watchBtn.hidden = !text || following;
    region.dataset.driving = String(!!text);
    region.dataset.following = String(following);
    banner();
  };

  /** A strip inside a driven dialog: a modal dialog makes the page's own
   * toggle unreachable, so the way out is in the dialog itself. */
  const banner = () => {
    const d = ctl?.dialog;
    if (!d) return;
    let b = /** @type {HTMLElement | null} */ (
      d.querySelector(".drive-banner")
    );
    if (!b) {
      const stop = h(
        "button",
        { type: "button", class: "kp-button kp-button--ghost" },
        "Stop watching",
      );
      stop.addEventListener("click", () => setFollowing(false));
      b = h(
        "div",
        { class: "kp-alert kp-alert--info drive-banner", role: "status" },
        h("span", { class: "kp-alert__body drive-banner__text" }),
        stop,
      );
      d.querySelector(".kp-dialog__body")?.prepend(b);
    }
    const t = /** @type {HTMLElement} */ (
      b.querySelector(".drive-banner__text")
    );
    t.textContent = `${badgeText(state, now()) ?? "Claude drove this form"}. Your own input waits until Claude is done.`;
  };

  // ── the operations ───────────────────────────────────────────────────

  /** @param {HTMLElement | null} el @param {string} cls @param {number} ms */
  const flash = async (el, cls, ms) => {
    el?.classList.add(cls);
    await sleep(ms);
    el?.classList.remove(cls);
  };

  /**
   * @param {import("./driveview.js").Op} op
   * @param {import("./driveview.js").DriveState} s
   */
  const run = async (op, s) => {
    switch (op.op) {
      case "goto":
        ctx.navigate(op.path);
        await sleep(250);
        return;
      case "open": {
        ctl?.close();
        const c = await openAction(op.stack, op.action, { driven: true });
        ctl = c;
        c?.closed.then(() => {
          if (ctl === c) ctl = null;
        });
        banner();
        await sleep(300);
        return;
      }
      case "type": {
        const input = ctl?.input(op.name);
        if (!ctl || !input) return;
        const wrap = /** @type {HTMLElement | null} */ (
          input.closest(".kp-field")
        );
        wrap?.classList.add("drive-focus");
        ctl.set(op.name, "");
        const ms = letterDelay(op.text);
        for (let i = 1; i <= op.text.length; i += 1) {
          ctl.set(op.name, op.text.slice(0, i));
          await sleep(ms);
        }
        wrap?.classList.remove("drive-focus");
        return;
      }
      case "set": {
        const input = ctl?.input(op.name);
        if (!ctl || !input) return;
        ctl.set(op.name, op.value);
        await flash(
          /** @type {HTMLElement | null} */ (input.closest(".kp-field")),
          "drive-focus",
          200,
        );
        return;
      }
      case "press":
        await flash(ctl?.button(op.button) ?? null, "drive-press", 420);
        return;
      case "sync": {
        const f = s.form;
        if (!ctl || !f) return;
        if (ctl.step() !== f.step_index) await ctl.goTo(f.step_index);
        ctl.errors(f.errors);
        ctl.runError(f.run_error);
        if (f.job) ctl.showJob(f.job.job);
        return;
      }
      case "close":
        ctl?.close();
        ctl = null;
        return;
      case "note":
        notify(`Claude's step was refused: ${op.refusal.why}.`, "warning");
        return;
    }
  };

  /** @param {import("./driveview.js").DriveEvent} ev */
  const play = async (ev) => {
    local.page = location.pathname;
    const p = plan(local, ev, following);
    local = p.local;
    for (const op of p.ops) {
      if (!following) return;
      await run(op, ev.state);
    }
  };

  /** Catch up with the state as the server has it now. */
  const catchUpNow = async () => {
    const r = await fetchJson("/data/drive", "what Claude is doing");
    if (!r.ok) return;
    /** @type {import("./driveview.js").DriveState} */
    const s = r.body.state;
    state = s;
    paint();
    if (!following || !isActive(s, now())) return;
    local.page = location.pathname;
    const ev = {
      seq: s.seq,
      step: { do: "state" },
      applied: true,
      refusal: null,
      state: s,
    };
    // A seq that cannot follow on forces a catch-up.
    queue = queue.then(() => play({ ...ev, seq: -2 }));
  };

  /** @param {boolean} on */
  const setFollowing = (on) => {
    following = on;
    toggle.checked = on;
    try {
      storage()?.setItem(FOLLOW_KEY, on ? "on" : "off");
    } catch {
      // A private window keeps it for this page only.
    }
    if (on) void catchUpNow();
    else {
      // The driven dialog was Claude's, never the viewer's input.
      ctl?.close();
      ctl = null;
      local = { seq: -1, page: location.pathname, form: null };
    }
    paint();
  };
  toggle.addEventListener("change", () => setFollowing(toggle.checked));
  watchBtn.addEventListener("click", () => setFollowing(true));

  listen("drive", (/** @type {import("./driveview.js").DriveEvent} */ ev) => {
    state = ev.state;
    paint();
    queue = queue.then(() => play(ev)).catch(() => {});
  });
  // The press's job moves on without a step: its state comes with the
  // queue's own `action` events.
  listen("action", (/** @type {import("./jobs.js").Job} */ j) => {
    const mine = state?.form?.job;
    if (!mine || mine.job !== j.job) return;
    mine.state = j.state;
    mine.message = j.message;
    paint();
  });

  // ── the viewer's own input, while Claude drives a tab that follows ───
  let lastNote = 0;
  /** @param {Event} e */
  const refuse = (e) => {
    if (!e.isTrusted || !following || !isActive(state, now())) return;
    const t = /** @type {Element | null} */ (e.target);
    if (t?.closest?.("#follow, .drive-banner")) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    if (Date.now() - lastNote > 3000) {
      lastNote = Date.now();
      notify(
        "Claude is driving this tab: your input was not used. Turn off Watch Claude to use it yourself.",
        "info",
      );
    }
  };
  for (const type of [
    "pointerdown",
    "mousedown",
    "click",
    "keydown",
    "beforeinput",
    "paste",
    "submit",
  ])
    document.addEventListener(type, refuse, { capture: true });

  setInterval(paint, 5000);
  paint();
  void catchUpNow();
}
