// Milestone follow (feat-platform-10): Claude drives this dashboard with
// `homelab ui <step>`, and this tab plays each step when its "Live view"
// switch is on: the page changes, the dialog opens, the text appears letter
// by letter, the button shows its press. The final press runs on the
// dashboard's server, once; this tab only shows the job it started. The
// edit forms (a stack's settings and firewall, a new stack, the host
// settings, the batch and roll back dialogs) are played on their own
// dialogs through editdrive.js.
//
// Off (the default, remembered per tab): nothing here ever touches the
// page; the badge says what Claude is working on and offers to follow.
// On, while Claude drives: the viewer's own clicks and keys are refused
// with a visible note rather than mixed in.
//
// Live view (Kenny, 2026-09-29): before each step that changes the screen,
// a tab that follows shows "Next: <step>" with the server's countdown,
// marks the step's target, and offers Pause, Continue and Stop; Claude's
// plan, when it sent one, is listed beside the page (driveannounce.js).
// A simulated "Claude" cursor glides to that target during the countdown,
// clicks at 0 and sits in a field while Claude types (drivecursor.js).

import { send } from "./act.js";
import { openAction } from "./actiondialog.js";
import { notify } from "./actui.js";
import { fetchJson, h } from "./dom.js";
import { handle, setDriven } from "./drivehooks.js";
import {
  announceView,
  countdown,
  findTarget,
  makeBar,
  makePlan,
  mark,
  unmark,
} from "./driveannounce.js";
import { makeCursor } from "./drivecursor.js";
import { REDUCED_TYPE_MS, typingDelays } from "./drivepace.js";
import { openDriven } from "./editdrive.js";
import { takeStep, topDialog } from "./pagedrive.js";
import { TAB } from "./tabname.js";
import {
  FOLLOW_KEY,
  badgeText,
  isActive,
  localOf,
  plan,
  readFollow,
  stillDriving,
} from "./driveview.js";
import { listen } from "./store.js";
import { attachSwitches } from "/static/kp/js/forms.js";

/** @param {number} ms */
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** @returns {boolean} */
const reducedMotion = () =>
  !!globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
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
  /** @type {import("./actiondialog.js").ActionController |
   *   import("./editdrive.js").DrivenControl | null} */
  let ctl = null;
  /** fix-239: the page-level dialog this tab opened for Claude, if any. */
  /** @type {HTMLDialogElement | null} */
  let pageDlg = null;
  const closePageDlg = () => {
    pageDlg?.close();
    pageDlg = null;
  };
  let queue = Promise.resolve();
  /**
   * review (the 15 s no-answer): the page-control steps the dashboard
   * stopped waiting for; one still queued here, or still being looked for,
   * is dropped at once rather than clicked late. Bounded: the newest 50.
   * @type {number[]}
   */
  const closed = [];

  const toggle = h("input", {
    class: "kp-switch__input",
    type: "checkbox",
    role: "switch",
    id: "live-view",
    "aria-describedby": "live-view-hint",
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
    "Live view",
  );
  region.replaceChildren(
    h(
      "label",
      { class: "kp-switch follow-switch" },
      toggle,
      h("span", { class: "kp-switch__state", "aria-hidden": "true" }),
      h("span", null, "Live view"),
    ),
    h(
      "span",
      { class: "measured follow-hint", id: "live-view-hint" },
      "this tab plays Claude's steps live",
    ),
    badge,
    watchBtn,
  );
  // The switch's On/Off words (they were never filled here before).
  attachSwitches(region);

  // Live view: the bar over the page and the plan beside it; inside a
  // driven dialog the bar is the dialog's banner (`banner`).
  const pageBar = makeBar(false);
  const planBox = makePlan();
  document.body.append(pageBar.el, planBox.el);
  /** @type {import("./driveannounce.js").Bar | null} */
  let dialogBar = null;
  const clock = countdown();
  const cursor = makeCursor();

  const idleText = () =>
    `${badgeText(state, now()) ?? "Claude drove this form"}. Your own input waits until Claude is done.`;

  const paintLive = () => {
    const on = following && !!state && stillDriving(state, now());
    const v = on ? announceView(state, clock.left(), on) : null;
    const d = ctl?.dialog ?? pageDlg;
    const inDialog = !!d && !!dialogBar && d.contains(dialogBar.el);
    pageBar.paint(inDialog ? null : v, "");
    if (inDialog) dialogBar?.paint(v, idleText());
    planBox.paint(state, on);
    if (!v || !state?.announce) unmark();
  };

  const paint = () => {
    const text = badgeText(state, now());
    badge.hidden = !text;
    badge.textContent = text ?? "";
    watchBtn.hidden = !text || following;
    region.dataset.driving = String(!!text);
    region.dataset.following = String(following);
    setDriven(following && !!text);
    // From the first announcement (Claude may not be driving yet) until
    // done, Stop or Live view off.
    cursor.show(
      following && (!!text || !!state?.announce) && !state?.stopped_by,
    );
    banner();
    paintLive();
  };

  /** A strip inside a driven dialog: a modal dialog makes the page's own
   * toggle unreachable, so the way out is in the dialog itself. */
  const banner = () => {
    const d = ctl?.dialog ?? pageDlg;
    if (!d) return;
    let b = /** @type {HTMLElement | null} */ (
      d.querySelector(".drive-banner")
    );
    if (!b || !dialogBar || !d.contains(dialogBar.el)) {
      const stop = h(
        "button",
        { type: "button", class: "kp-button kp-button--ghost" },
        "Leave live view",
      );
      stop.addEventListener("click", () => setFollowing(false));
      // Live view's bar is the banner: the announcement, its countdown and
      // Pause/Continue/Stop keep their place whether or not a step is
      // announced, so the dialog never moves.
      dialogBar = makeBar(true, [stop]);
      b?.remove();
      b = dialogBar.el;
      d.querySelector(".kp-dialog__body")?.prepend(b);
    }
    dialogBar.paint(
      following && state
        ? announceView(state, clock.left(), stillDriving(state, now()))
        : null,
      idleText(),
    );
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
        // Kenny, 2026-10-01: Live view went to the next page while the
        // last dialog was still open on screen. A page change always
        // closes the dialog first, the way a person would.
        ctl?.close();
        closePageDlg();
        ctx.navigate(op.path);
        await sleep(250);
        return;
      case "open": {
        ctl?.close();
        const c =
          op.edit && s.form
            ? await openDriven(s.form, ctx)
            : await openAction(op.stack, op.action, { driven: true });
        ctl = c;
        c?.closed.then(() => {
          if (ctl === c) ctl = null;
        });
        banner();
        await sleep(300);
        return;
      }
      case "type": {
        const input = ctl?.input(op.name, op.id);
        if (!ctl || !input) return;
        const wrap = /** @type {HTMLElement | null} */ (
          input.closest(".kp-field")
        );
        wrap?.classList.add("drive-focus");
        cursor.sit(wrap ?? input);
        ctl.set(op.name, "", op.id);
        // As a person types (drivepace.js): uneven, resting after words and
        // punctuation; reduced motion sets the whole text at once.
        const delays = typingDelays(op.text, Math.random, reducedMotion());
        if (delays.length === 0) {
          ctl.set(op.name, op.text, op.id);
          await sleep(REDUCED_TYPE_MS);
        }
        const letters = [...op.text];
        for (let i = 0; i < delays.length; i += 1) {
          if (!following) break;
          ctl.set(op.name, letters.slice(0, i + 1).join(""), op.id);
          await sleep(delays[i]);
        }
        ctl.set(op.name, op.text, op.id);
        wrap?.classList.remove("drive-focus");
        return;
      }
      case "pick": {
        const input = ctl?.input(op.name, op.id);
        if (!ctl || !input) return;
        const c = ctl;
        const commit = () => c.set(op.name, op.value, op.id);
        // A dropdown is opened, its options passed over and the choice
        // clicked (drivecursor.js); any other control is set as before.
        if (input instanceof HTMLSelectElement && following)
          await cursor.pick(input, op.value, commit);
        else {
          commit();
          await flash(
            /** @type {HTMLElement | null} */ (input.closest(".kp-field")),
            "drive-focus",
            200,
          );
        }
        return;
      }
      case "row": {
        const c = ctl && "row" in ctl ? ctl : null;
        await flash(c?.row(op.row, op.target) ?? null, "drive-press", 420);
        return;
      }
      case "select": {
        // Owner decision 2026-09-30: ticks the Overview table's own
        // multiselect, exactly as a click would, so a batch action opened
        // right after acts on the same rows a viewer would see ticked.
        // redesign-stacks: the Stacks list (cards or table) is marked
        // `data-drive-list="stacks"`, whichever view is on.
        handle("overview")?.select?.(op.stacks);
        await flash(
          /** @type {HTMLElement | null} */ (
            document.querySelector('[data-drive-list="stacks"]')
          ),
          "drive-focus",
          250,
        );
        return;
      }
      case "set": {
        const input = ctl?.input(op.name, op.id);
        if (!ctl || !input) return;
        ctl.set(op.name, op.value, op.id);
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
        if ("sync" in ctl) {
          await ctl.sync(f);
          banner();
          return;
        }
        if (ctl.step() !== f.step_index) await ctl.goTo(f.step_index);
        ctl.errors(f.errors);
        ctl.runError(f.run_error);
        if (f.job) ctl.showJob(f.job.job);
        return;
      }
      case "close":
        ctl?.close();
        ctl = null;
        closePageDlg();
        return;
      case "tab": {
        // fix-239: a page control (drivable.js). Only the tab the server
        // lets claim this step takes it, so a click that starts work (a
        // stale image's backup, commit and deploy) happens exactly once
        // however many tabs follow; the others only mark it.
        if (closed.includes(s.seq)) return;
        const claim = await send(
          "POST",
          "/data/drive/claim",
          { seq: s.seq, tab: TAB },
          "claiming Claude's step",
        );
        if (!claim.ok) {
          await flash(findTarget(op.step, ctl, s), "drive-press", 420);
          return;
        }
        const a = await takeStep(
          /** @type {import("./pagedrive.js").TabStep} */ (op.step),
          ctx.navigate,
          async (el) => {
            cursor.sit(el);
            // redesign-integrate-8: a person's pace unless the demo host's
            // sweep asked for less (`press_ms`).
            await flash(el, "drive-press", s.press_ms ?? 420);
          },
          () => closed.includes(s.seq),
        );
        if (closed.includes(s.seq)) return;
        const opened = a.dialog ? topDialog() : null;
        if (opened && opened !== pageDlg) {
          opened.addEventListener("close", () => {
            if (pageDlg === opened) pageDlg = null;
          });
        }
        pageDlg = opened;
        // Pause and Stop inside it: a modal dialog covers the page's bar.
        banner();
        await send(
          "POST",
          "/data/drive/taken",
          { seq: s.seq, tab: TAB, ...a },
          "answering Claude's step",
        );
        return;
      }
      case "note":
        notify(`Claude's step was refused: ${op.refusal.why}.`, "warning");
        return;
      case "highlight":
        // Only while the step is still announced: a step already taken
        // leaves no mark behind.
        if (state?.announce) {
          const a = state.announce;
          const el = mark(findTarget(op.step, ctl, state));
          if (a.countdown)
            cursor.glide(el, a.id, a.total_ms, clock.left, () =>
              Boolean(state?.paused_by),
            );
          else cursor.sit(el);
        }
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

  /** Catch up with the state as the server has it now; false when it
   * could not be read. */
  const catchUpNow = async () => {
    const r = await fetchJson("/data/drive", "what Claude is doing");
    if (!r.ok) return false;
    /** @type {import("./driveview.js").DriveState} */
    const s = r.body.state;
    state = s;
    clock.set(s);
    paint();
    if (!following || !isActive(s, now())) return true;
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
    return true;
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
    // fix-185 (`homelab ui reload`): every tab takes the dashboard's
    // current page at once, whether or not Live view follows here — this
    // is precisely the step that brings a stale tab current, so it never
    // waits for the viewer's own toggle the way a driven step does.
    if (ev.kind === "reload") {
      location.reload();
      return;
    }
    // review (the 15 s no-answer): the dashboard gave up on this step.
    if (ev.kind === "closed") {
      closed.push(ev.seq);
      if (closed.length > 50) closed.shift();
      return;
    }
    state = ev.state;
    clock.set(ev.state);
    if ((ev.kind ?? "step") === "step") {
      if (following && ev.applied) cursor.stepped();
      unmark();
    }
    paint();
    queue = queue.then(() => play(ev)).catch(() => {});
  });
  // The live channel came back (the dashboard restarted, e.g. during its
  // own install-native, 2026-09-29 18:15): the new dashboard's drive state
  // is the truth, so a "Claude is driving" badge, the input lock and the
  // cursor from before the restart must not outlive it. Read it again.
  // A dashboard still starting may not answer yet: a few tries.
  const reread = async () => {
    for (let i = 0; i < 5; i += 1) {
      if (await catchUpNow()) return;
      await sleep(1500);
    }
  };
  listen("reopened", () => void reread());
  listen("resync", () => void reread());
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
    if (t?.closest?.("#follow, .drive-banner, .drive-announce, .drive-plan"))
      return;
    e.preventDefault();
    e.stopImmediatePropagation();
    if (Date.now() - lastNote > 3000) {
      lastNote = Date.now();
      notify(
        "Claude is driving this tab: your input was not used. Turn off Live view to use it yourself.",
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
  // The countdown's digit and bar; a no-op while nothing is announced.
  setInterval(() => {
    if (state?.announce) paintLive();
  }, 100);
  paint();
  void catchUpNow();
}
