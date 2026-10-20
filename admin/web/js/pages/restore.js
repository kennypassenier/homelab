// redesign-final-h3 (3.71.0's final review, 2026-10-04; FLOWS.md §5, task
// 5): the Restore flow on a page of its own like the Update flow
// (`/backups/restore?stack=…[&app=…][&snapshot=…]`), with the breadcrumb
// Backups / <stack> / Restore. Rebuilt on kp-themes 11.0.0 from the demo
// Kenny approved on 2026-10-19 (app-homelab-backups, feat-kp11demo-8): a kp
// wizard of four steps you can click back to — What (the stack and the
// app), Which night (a dated picker, the newest chosen), Check (the restore
// point, the command the host would run, the safety copy acknowledged, the
// stack's name typed) and Restore (the steps of the job, live). Every
// Restore… (Backups, the stack hub, the palette) lands here; Live view's own
// driven form still opens the action's dialog. The run is the catalog's
// restore action, so the host does what it always did: the safety copy
// first, then the night put back; its log follows below (the job panel).

import { act, catalogReady, onAct, send } from "../act.js";
import { refusalCallout } from "../actui.js";
import { actionForm, previewArgs } from "../actionforms.js";
import { ensureStyle, fetchJson, h } from "../dom.js";
import { declare, declareField, drivable, viaForm } from "../drivable.js";
import { humanBytes } from "../backupsview.js";
import { jobSteps } from "../jobs.js";
import { mountJobPanel } from "../jobpanel.js";
import { previewGate } from "../previewgate.js";
import { restoreHref, restoreNights, whatWillHappen } from "../restoreflow.js";
import { current, rereadStore, subscribe } from "../store.js";
import { openAction } from "../actiondialog.js";
import { pageHeader } from "../ui.js";
import { gridRow, formGrid, buttonGroup } from "../gridrow.js";
import { drawnSelect } from "/static/kp/js/combobox.js";
import {
  BEFORE_STEP_EVENT,
  STEP_EVENT,
  attachWizards,
  wizard,
} from "/static/kp/js/wizard.js";
import {
  DATED_EVENT,
  DATED_RETRY_EVENT,
  attachDatedOptions,
  datedValue,
  setDatedOptions,
  setDatedState,
} from "/static/kp/js/dated-options.js";
import { runMoment } from "/static/kp/js/runs.js";
import { setCode } from "/static/kp/js/code.js";
import {
  attachSteplists,
  buildSteplist,
  setStep,
} from "/static/kp/js/steplist.js";
import { ACK_EVENT, ackGate, attachAckGates } from "/static/kp/js/ack-gate.js";
import {
  TYPE_CONFIRM_EVENT,
  attachTypeConfirms,
} from "/static/kp/js/components.js";
import { attachDisabledReasons } from "/static/kp/js/reason.js";
import {
  CARD_RETRY_EVENT,
  attachCardStates,
  setCardState,
} from "/static/kp/js/card-state.js";

const STACK = declare({
  id: "restore-stack",
  // Backups' Restore picker dialog (its select) before the flow.
  was: ["bk-restore-stack"],
  page: "restore",
  opens: "view",
  row: "<stack>",
  what: "restore from another stack",
});
const APP = declare({
  id: "restore-app",
  // Backups' Restore picker dialog (its select) before the flow.
  was: ["bk-restore-app"],
  page: "restore",
  opens: "view",
  row: "<app>",
  what: "pick the app whose data comes back (step 1)",
});
const NIGHT = declare({
  id: "restore-night",
  // Backups' Restore picker dialog (its select) before the flow.
  was: ["bk-restore-snapshot"],
  page: "restore",
  opens: "view",
  row: "<snapshot>",
  what: "pick the night its data comes back from (step 2)",
  shows: "once an app is picked",
  reach: [{ do: "click", control: "restore-app", row: "*" }],
});
const CONFIRM = declareField({
  id: "restore-confirm",
  page: "restore",
  what: "the stack's name, typed to confirm the restore",
});
const STACK_FIELD = declareField({
  id: "rs-stack",
  page: "restore",
  what: "the stack whose apps the flow lists",
});
const GO = declare({
  id: "restore-go",
  page: "restore",
  opens: "run",
  what: "restore the picked night (the safety copy first); before the app, the night and the typed name it says which is missing",
});
const CANCEL = declare({
  id: "restore-cancel",
  page: "restore",
  opens: "view",
  what: "leave the Restore flow for the stack's Backups tab",
});
const RETRY = declare({
  id: "restore-retry",
  page: "restore",
  opens: "run",
  what: "run the restore again after it stopped",
  shows: "when the restore stopped",
});

const STEPS = /** @type {const} */ ([
  "What",
  "Which night",
  "Check",
  "Restore",
]);

/** The job step states the kp step list knows. @type {Record<string, import("/static/kp/js/steplist.js").StepState>} */
const STEP_STATE = {
  ok: "done",
  bad: "failed",
  run: "running",
  wait: "pending",
  skip: "skipped",
};

/**
 * @param {HTMLElement} root
 * @param {{navigate: (href: string) => void}} ctx
 * @returns {() => void}
 */
export function mount(root, ctx) {
  ensureStyle("/css/pages/restore.css");
  root.classList.add("rs-page");
  const q = new URLSearchParams(location.search);
  const abort = new AbortController();
  /** @type {(() => void)[]} */
  const cleanups = [];
  const names = () => (current().fleet?.stacks ?? []).map((s) => s.name);
  const vmidOf = (/** @type {string} */ n) =>
    (current().fleet?.stacks ?? []).find((s) => s.name === n)?.vmid ?? null;
  const stack = q.get("stack") ?? names()[0] ?? null;
  /** @type {{app: string | null, night: string | null, running: boolean, matched: boolean, acked: boolean, via: boolean}} */
  const S = {
    app: q.get("app"),
    night: q.get("snapshot"),
    running: false,
    matched: false,
    acked: false,
    via: false,
  };
  /** @type {any} */
  let read = null;
  /** @type {"loading" | "ready" | "empty" | "error"} */
  let readState = "loading";
  /** @type {"ready" | "loading" | "empty" | "error"} */
  let nightsState = "ready";
  /** @type {null | (() => void)} */
  let stopJob = null;
  /** @type {null | (() => void)} */
  let stopSteps = null;
  let previewSeq = 0;
  /** @type {{ready: boolean, reason: string}} */
  let gate = { ready: false, reason: "Reading what this would do…" };

  const head = pageHeader({
    title: stack ? `Restore ${stack}` : "Restore",
    desc: "Put one app's data back as it was on a chosen night. Today's data is copied aside first, so a restore can itself be undone.",
  });

  // ── step 1: the stack and the app ────────────────────────────────────
  const stackSel = /** @type {HTMLSelectElement} */ (
    h("select", {
      class: "kp-field__input",
      id: STACK_FIELD,
      "data-kp-select": "",
    })
  );
  const tiles = h("fieldset", {
    class: "kp-fieldset",
    "data-kp-layout": "tiles",
    "aria-labelledby": "rs-tiles-label",
  });
  const facts = h("ul", {
    class: "rs-facts",
    "aria-label": "What this restores",
  });
  const step1Card = h(
    "section",
    {
      class: "kp-card rs-card",
      "data-kp-state": "loading",
      "aria-label": "What to restore",
    },
    h(
      "div",
      { class: "kp-card__stage" },
      h(
        "div",
        { class: "kp-card__data" },
        formGrid([
          gridRow({
            label: "Stack",
            labelFor: "rs-stack",
            control: stackSel,
            value: h(
              "span",
              { class: "rs-hint" },
              "The stack the app lives in.",
            ),
          }),
          gridRow({
            label: "App to bring back",
            labelId: "rs-tiles-label",
            control: tiles,
          }),
          gridRow({
            label: "What it restores",
            control: facts,
          }),
        ]),
      ),
    ),
  );

  // ── step 2: the night ────────────────────────────────────────────────
  const dated = h("div", {
    class: "kp-dated",
    "data-kp-dated": "",
    "aria-label": "Night to restore from",
  });
  const step2Body = formGrid([
    gridRow({
      label: "Night to restore from",
      labelId: "rs-night-label",
      control: dated,
      wide: true,
    }),
  ]);

  // ── step 3: check ────────────────────────────────────────────────────
  const point = h("p", { class: "kp-formgrid__control rs-point" }, "…");
  point.setAttribute("data-kp-formgrid-span", "wide");
  const codeEl = h("figure", { class: "kp-code", "data-kp-code": "" });
  const ack = h(
    "div",
    {
      class: "kp-alert kp-alert--warning",
      "data-kp-ack": "",
      "data-kp-ack-key": "aside",
      "data-kp-ack-label":
        "I understand today's data is copied aside and kept for 7 days",
    },
    h(
      "div",
      { class: "kp-alert__body" },
      h("strong", null, "Today's data is replaced."),
      " A copy of it is set aside first and stays for 7 days, so this step can be undone by restoring that copy.",
    ),
  );
  const typeBox = h("div", { class: "rs-typebox" });
  const go = /** @type {HTMLButtonElement} */ (
    drivable(
      h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--destructive",
          id: "restore-go",
          disabled: "",
          title: "Pick an app and a night first",
        },
        "Restore",
      ),
      GO,
    )
  );
  const err = h("div", { class: "rs-err" });
  const cancel = drivable(
    h(
      "a",
      {
        class: "kp-button",
        href: stack
          ? `/stacks/${encodeURIComponent(stack)}/backups`
          : "/backups",
        title: "Leave without restoring; nothing has run",
      },
      "Cancel",
    ),
    CANCEL,
  );
  const checkBody = h(
    "div",
    { class: "rs-check kp-formgrid" },
    gridRow({
      label: "Restore point",
      control: point,
      wide: true,
    }),
    gridRow({
      label: "Command",
      control: codeEl,
      wide: true,
    }),
    h("div", { "data-kp-formgrid-span": "all" }, ack),
    gridRow({ label: "Confirm", control: typeBox, wide: true }),
    gridRow({
      buttons: true,
      control: buttonGroup([cancel, go]),
    }),
    h("div", { "data-kp-formgrid-span": "all" }, err),
  );

  // ── step 4: the run ──────────────────────────────────────────────────
  const runTitle = h(
    "p",
    { class: "kp-formgrid__control rs-point" },
    "Restoring…",
  );
  runTitle.setAttribute("data-kp-formgrid-span", "wide");
  let steps = h("ol", {
    class: "kp-steplist",
    "aria-label": "Restore progress",
  });
  const stepsBox = h("div", { class: "rs-steps" }, steps);
  const jobBox = h("div", { class: "rs-job" });
  const retryBtn = drivable(
    h(
      "button",
      { type: "button", class: "kp-button kp-button--primary" },
      "Try again",
    ),
    RETRY,
  );
  const backBtn = h(
    "button",
    { type: "button", class: "kp-button" },
    "Back to Check",
  );
  const runActions = gridRow({
    buttons: true,
    control: buttonGroup([retryBtn, backBtn]),
    hidden: true,
  });
  const runBody = h(
    "div",
    { class: "rs-run kp-formgrid" },
    gridRow({ label: "Restoring", control: runTitle, wide: true }),
    gridRow({ label: "Progress", control: stepsBox, wide: true }),
    runActions,
    gridRow({ label: "Log", control: jobBox, wide: true }),
  );

  const wiz = h(
    "div",
    { class: "kp-wizard", "data-kp-wizard": "", "data-kp-navigable": "" },
    h(
      "ol",
      { class: "kp-wizard__steps" },
      ...STEPS.map((s) => h("li", { "data-kp-step-label": "" }, s)),
    ),
    h(
      "section",
      { "data-kp-step": "", "aria-label": "What to restore" },
      h("div", { class: "rs-step" }, step1Card),
    ),
    h(
      "section",
      { "data-kp-step": "", hidden: "", "aria-label": "Which night" },
      h("div", { class: "rs-step" }, step2Body),
    ),
    h(
      "section",
      { "data-kp-step": "", hidden: "", "aria-label": "Check" },
      h("div", { class: "rs-step rs-step--check" }, checkBody),
    ),
    h(
      "section",
      { "data-kp-step": "", hidden: "", "aria-label": "Restoring" },
      h("div", { class: "rs-step" }, runBody),
    ),
    h(
      "div",
      { class: "kp-wizard__actions" },
      h(
        "button",
        { type: "button", class: "kp-button", "data-kp-wizard-back": "" },
        "Back",
      ),
      h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--primary",
          "data-kp-wizard-next": "",
        },
        "Next",
      ),
    ),
  );
  const next = /** @type {HTMLButtonElement} */ (
    wiz.querySelector("[data-kp-wizard-next]")
  );
  const back = /** @type {HTMLButtonElement} */ (
    wiz.querySelector("[data-kp-wizard-back]")
  );

  root.replaceChildren(head.el, wiz);

  if (!stack) {
    // The fleet is the list of stacks: until it has answered, the card is
    // busy; only an answered, empty fleet is "no stack".
    const paintNoStack = () => {
      if (!current().fleet) {
        if (current().fleetRead === "error")
          setCardState(step1Card, "error", {
            title: "The stacks could not be read",
            text: "The dashboard did not get the list of stacks. Nothing was changed.",
          });
        else
          setCardState(step1Card, "loading", { words: "Reading the stacks…" });
        return;
      }
      const first = names()[0];
      if (first) ctx.navigate(restoreHref(first));
      else
        setCardState(step1Card, "empty", {
          title: "No stack to restore",
          text: "The host manages no stack yet; once one has backups, its apps show up here.",
        });
    };
    const detach = attachCardStates(root);
    step1Card.addEventListener(CARD_RETRY_EVENT, () => {
      setCardState(step1Card, "loading", { words: "Reading the stacks…" });
      void rereadStore();
    });
    paintNoStack();
    const off = subscribe(paintNoStack);
    return () => {
      off();
      detach();
      abort.abort();
    };
  }

  cleanups.push(
    attachCardStates(root),
    attachDatedOptions(root),
    attachSteplists(root),
    attachAckGates(root),
    attachDisabledReasons(root),
  );
  const wizards = attachWizards(root, {
    navigableLabels: true,
    beforeStep: () => true,
  });
  cleanups.push(wizards);
  const w = () =>
    /** @type {import("/static/kp/js/wizard.js").WizardHandle} */ (wizard(wiz));

  // ── data ─────────────────────────────────────────────────────────────
  /** The repositories (apps) with at least one snapshot. */
  const repos = () =>
    /** @type {any[]} */ (read?.repos ?? []).filter(
      (r) => (r.snapshots ?? []).length > 0,
    );
  const repoOf = (/** @type {string | null} */ app) =>
    repos().find((r) => r.owner === app) ?? null;
  const nights = () =>
    restoreNights(repoOf(S.app)?.snapshots ?? [], Date.now() / 1000, 30);
  const nightLabel = () =>
    nights().find((n) => n.key === S.night)?.label ?? "the chosen night";
  const snapOf = () =>
    repoOf(S.app)?.snapshots?.find(
      (/** @type {any} */ x) => x.short_id === S.night,
    ) ?? null;

  /** Keep the address in step with the choice (back and reload land here). */
  const keepUrl = () => {
    const href = restoreHref(stack, S.app, S.night);
    if (location.pathname + location.search !== href)
      history.replaceState(history.state, "", href);
  };

  // ── painting ─────────────────────────────────────────────────────────
  const paintStacks = () => {
    const list = names();
    stackSel.replaceChildren(
      ...list.map((n) => {
        const o = h(
          "option",
          { value: n },
          `${n}${vmidOf(n) != null ? ` (CT ${vmidOf(n)})` : ""}`,
        );
        drivable(o, STACK, n);
        o.addEventListener("click", () => pickStack(n));
        return o;
      }),
    );
    stackSel.value = stack;
    drawnSelect(stackSel)?.refresh();
  };
  const pickStack = (/** @type {string} */ n) => {
    if (n !== stack) ctx.navigate(restoreHref(n));
  };
  stackSel.addEventListener("change", () => pickStack(stackSel.value));

  const paintApps = () => {
    const rs = repos();
    if (readState === "ready") {
      setCardState(step1Card, "ready");
    } else if (readState === "empty") {
      setCardState(step1Card, "empty", {
        title: `${stack} has no snapshots yet`,
        text: "Once its first backup has run, its apps and their nights show up here.",
      });
    } else if (readState === "error") {
      setCardState(step1Card, "error", {
        title: "The backups could not be read",
        text: read?.error ?? "The host did not answer. Nothing was changed.",
      });
    } else {
      setCardState(step1Card, "loading", { words: "Reading the backups…" });
    }
    tiles.replaceChildren();
    for (const r of rs) {
      const now = Date.now() / 1000;
      const newest = Math.max(
        ...r.snapshots.map((/** @type {any} */ s) => s.time),
      );
      const label = h(
        "label",
        { class: "kp-field__option" },
        h("input", {
          class: "kp-field__check",
          type: "radio",
          name: "rs-app",
          value: r.owner,
          checked: r.owner === S.app ? "" : null,
        }),
        h("span", { class: "kp-field__option-title" }, r.owner),
        h(
          "span",
          { class: "kp-field__option-hint" },
          `${r.snapshots.length} ${r.snapshots.length === 1 ? "snapshot" : "snapshots"}, newest ${Math.max(0, Math.round((now - newest) / 3600))} h ago`,
        ),
      );
      drivable(label, APP, r.owner);
      tiles.append(label);
    }
    const rep = repoOf(S.app);
    facts.replaceChildren(
      ...(rep
        ? whatWillHappen({
            stack,
            app: /** @type {string} */ (S.app),
            night: S.night ? nightLabel() : "the night you pick next",
            native: read?.native === true,
          }).map((l) => h("li", null, l))
        : [
            h(
              "li",
              null,
              "Pick an app first; this list then says what a restore of it does.",
            ),
          ]),
    );
    gateNext();
  };

  const pending = () => nightsState === "loading";
  const paintNights = () => {
    const list = nights();
    if (readState !== "ready" || !S.app) {
      setDatedOptions(dated, []);
      setDatedState(dated, readState === "loading" ? "loading" : "empty", {
        reason: S.app ? `${S.app} has no snapshot yet` : "Pick an app first",
        action: null,
      });
      gateNext();
      return;
    }
    if (nightsState === "ready") {
      if (!list.some((n) => !n.missed)) {
        const take = h(
          "button",
          { type: "button", class: "kp-button kp-button--primary" },
          "Take a snapshot now",
        );
        take.addEventListener("click", () => void openAction(stack, "backup"));
        setDatedOptions(dated, []);
        setDatedState(dated, "empty", {
          reason: `${S.app} has no snapshot yet`,
          action: take,
        });
      } else {
        setDatedOptions(
          dated,
          list.map((n) =>
            n.missed
              ? {
                  value: n.key,
                  label: n.label,
                  when: Date.parse(`${n.key.slice(7)}T12:00:00Z`),
                  meta: "—",
                  disabled: "no snapshot (missed)",
                }
              : {
                  value: n.key,
                  label: n.label,
                  when: /** @type {any} */ (n.snap).time * 1000,
                  meta:
                    humanBytes(/** @type {any} */ (n.snap).size_bytes) ?? "—",
                },
          ),
          S.night && list.some((n) => n.key === S.night && !n.missed)
            ? S.night
            : null,
        );
        const chosen = datedValue(dated);
        if (chosen && !chosen.startsWith("missed:")) S.night = chosen;
        decorateNights();
      }
    } else {
      setDatedState(dated, nightsState === "empty" ? "empty" : nightsState, {
        title: "The nights could not be read",
        text: "The host did not answer. Nothing was changed.",
        reason: `${S.app} has no snapshot yet`,
      });
    }
    keepUrl();
    gateNext();
  };
  const decorateNights = () => {
    // The package draws few nights as radios and many as a combobox list.
    for (const el of /** @type {HTMLElement[]} */ ([
      ...dated.querySelectorAll("input[type=radio], [role=option][data-value]"),
    ])) {
      const v = el instanceof HTMLInputElement ? el.value : el.dataset.value;
      if (v && !v.startsWith("missed:")) drivable(el, NIGHT, v);
    }
  };

  {
    // The package redraws its night list; mark the radios again each time.
    const mo = new MutationObserver(() => decorateNights());
    mo.observe(dated, { childList: true, subtree: true });
    cleanups.push(() => mo.disconnect());
  }

  /** Next waits for what the step needs, and says what, on hover and focus. */
  function gateNext() {
    const at = w()?.step() ?? 0;
    let reason = "";
    if (at === 0 && (readState !== "ready" || !S.app))
      reason =
        readState === "loading"
          ? "The backups are still being read"
          : readState === "ready"
            ? "Pick the app whose data comes back"
            : "There is nothing to restore yet";
    if (
      at === 1 &&
      (nightsState !== "ready" || !S.night || S.night.startsWith("missed:"))
    )
      reason =
        nightsState === "loading"
          ? "The nights are still being read"
          : nightsState !== "ready"
            ? "There is no night to restore from yet"
            : "Pick a night that has a snapshot";
    next.disabled = reason !== "";
    if (reason) next.setAttribute("data-kp-disabled-reason", reason);
    else next.removeAttribute("data-kp-disabled-reason");
    attachDisabledReasons(root);
  }

  /** The Restore button waits for the preview, the tick and the name. */
  const armGo = () => {
    const ready =
      !S.running && !!S.app && !!S.night && S.matched && S.acked && gate.ready;
    go.disabled = !ready;
    go.setAttribute("aria-disabled", String(!ready));
    const reason =
      !S.app || !S.night
        ? "Pick an app and a night first"
        : !gate.ready
          ? gate.reason
          : !S.acked
            ? `Tick "I understand today's data is copied aside" first`
            : !S.matched
              ? `Type ${stack} exactly first`
              : "";
    if (reason) go.setAttribute("data-kp-disabled-reason", reason);
    else go.removeAttribute("data-kp-disabled-reason");
    go.title = ready
      ? `Restore ${S.app} from ${nightLabel()}, the safety copy first`
      : reason;
    attachDisabledReasons(root);
  };

  /** The Check step: restore point, the command the host would run, the guards. */
  const checkStep = async () => {
    const snap = snapOf();
    point.textContent = snap
      ? `Restore point: ${runMoment(snap.time * 1000, "Europe/Brussels", "en-GB")}. ${S.app} stops for about a minute; the rest of ${stack} keeps running.`
      : "Restore point: pick an app and a night first.";
    go.textContent = `Restore ${S.app ?? ""}`.trim();
    // a fresh guard for every visit to this step
    ackGate(ack)?.set(false);
    S.acked = false;
    S.matched = false;
    typeBox.replaceChildren(h("div", { "data-kp-type-confirm": stack }));
    attachTypeConfirms(typeBox);
    const input = typeBox.querySelector("input");
    if (input) {
      input.id = CONFIRM;
      input.setAttribute("autocomplete", "off");
    }
    armGo();
    // The host's own preview: the command line, or why it cannot run.
    const seq = ++previewSeq;
    gate = { ready: false, reason: "Reading what this would do…" };
    setCode(codeEl, {
      title: "The command the host runs",
      language: "Shell",
      state: "loading",
      loadingWords: "Reading what this would do…",
    });
    armGo();
    const catalog = await catalogReady();
    const kind = read?.native === true ? "restore-native" : "restore";
    const entry = catalog?.actions.find((a) => a.action === kind);
    if (!catalog || !entry || seq !== previewSeq) {
      if (seq === previewSeq) {
        gate = {
          ready: false,
          reason:
            "The dashboard does not know the restore action; reload the page.",
        };
        setCode(codeEl, {
          title: "The command the host runs",
          state: "error",
          message: gate.reason,
        });
        armGo();
      }
      return;
    }
    const form = actionForm(entry, {
      stack,
      selfStack: catalog.self_stack,
      hostTarget: catalog.host_target,
    });
    if (form.refused) {
      gate = { ready: false, reason: form.refused };
      setCode(codeEl, {
        title: "The command the host runs",
        state: "error",
        message: form.refused,
      });
      armGo();
      return;
    }
    const body = previewArgs(
      form,
      /** @type {any} */ (
        read?.native === true
          ? { snapshot: S.night ?? "" }
          : { app: S.app ?? "", snapshot: S.night ?? "" }
      ),
    );
    const r = await send(
      "POST",
      form.previewPath,
      body,
      `the preview of ${form.title}`,
    );
    if (seq !== previewSeq) return;
    if (!r.ok) {
      const g = previewGate(form, null, r.error);
      gate = { ready: false, reason: g.reason };
      setCode(codeEl, {
        title: "The command the host runs",
        state: "error",
        message: g.reason,
      });
    } else {
      const g = previewGate(form, r.body, null);
      gate = { ready: g.ready, reason: g.reason };
      const cli = r.body.cli;
      if (cli)
        setCode(codeEl, {
          title: "The command the host runs",
          language: "Shell",
          text: String(cli),
          copy: true,
        });
      else
        setCode(codeEl, {
          title: "The command the host runs",
          state: "empty",
          message:
            g.notes[0] ?? "The host gave no command line for this restore.",
        });
    }
    armGo();
  };

  // ── the run ──────────────────────────────────────────────────────────
  const stepRows = (
    /** @type {any} */ j,
    /** @type {Map<number, string>} */ names2,
  ) =>
    jobSteps(j, names2).map((row, i) => ({
      id: `s${i}`,
      title: row.title,
      note: row.desc ?? "",
      state: STEP_STATE[row.state] ?? "pending",
    }));

  /** @param {number} jobId */
  const followJob = (jobId) => {
    stopSteps?.();
    /** @type {Map<number, string>} */
    const stepNames = new Map();
    let sig = "";
    const paint = () => {
      const j = act.jobs.find((x) => x.job === jobId);
      if (!j) return;
      if (j.progress) stepNames.set(j.progress.n, j.progress.step);
      const rows = stepRows(j, stepNames);
      const s2 = JSON.stringify(rows);
      if (s2 !== sig) {
        sig = s2;
        const fresh = buildSteplist(rows, { label: `Restoring ${S.app}` });
        steps.replaceWith(fresh);
        steps = fresh;
        attachSteplists(stepsBox);
      }
      const ended = ["done", "failed", "cancelled", "lost", "refused"].includes(
        j.state,
      );
      const snap = snapOf();
      const at = snap
        ? runMoment(snap.time * 1000, "Europe/Brussels", "en-GB")
        : "";
      if (j.state === "done") {
        runTitle.textContent = `${S.app} is back at ${at}.`;
      } else if (ended) {
        runTitle.textContent = `Restoring ${S.app} stopped${j.message ? `: ${j.message}` : ""}. Today's data is safe in the copy set aside.`;
      } else {
        runTitle.textContent = `Restoring ${S.app} to ${at}`;
      }
      runActions.hidden = !(ended && j.state !== "done");
      back.disabled = !ended;
      S.running = !ended;
      if (ended) armGo();
    };
    const off = onAct("jobs", paint);
    paint();
    stopSteps = off;
  };

  const run = async () => {
    if (!S.app || !S.night) return;
    err.replaceChildren();
    const native = read?.native === true;
    const kind = native ? "restore-native" : "restore";
    const catalog = await catalogReady();
    const entry = catalog?.actions.find((a) => a.action === kind);
    if (!catalog || !entry) {
      err.replaceChildren(
        refusalCallout({
          what: "Restore",
          why: "the dashboard does not know the restore action",
          fix: "reload the page",
        }),
      );
      return;
    }
    const form = actionForm(entry, {
      stack,
      selfStack: catalog.self_stack,
      hostTarget: catalog.host_target,
    });
    if (form.refused) {
      err.replaceChildren(
        refusalCallout({ what: form.title, why: form.refused, fix: "" }),
      );
      return;
    }
    S.running = true;
    armGo();
    go.setAttribute("aria-busy", "true");
    const r = await send(
      "POST",
      form.runPath,
      native
        ? { confirm: stack, snapshot: S.night }
        : { confirm: stack, app: S.app, snapshot: S.night },
      form.title,
    );
    go.removeAttribute("aria-busy");
    if (!r.ok) {
      S.running = false;
      err.replaceChildren(refusalCallout(r.error));
      armGo();
      return;
    }
    stopJob?.();
    const panel = mountJobPanel(/** @type {number} */ (r.body.job), {
      compact: true,
      steps: false,
    });
    stopJob = panel.stop;
    jobBox.replaceChildren(panel.element);
    S.via = true;
    await w().goTo(3);
    S.via = false;
    followJob(/** @type {number} */ (r.body.job));
  };
  go.addEventListener("click", () => {
    if (go.disabled || go.getAttribute("aria-disabled") === "true") return;
    void run();
  });
  retryBtn.addEventListener("click", () => void run());
  backBtn.addEventListener("click", () => void w().goTo(2));

  // ── wizard ───────────────────────────────────────────────────────────
  wiz.addEventListener(BEFORE_STEP_EVENT, (e) => {
    const d = /** @type {CustomEvent} */ (e).detail;
    if (S.via) return;
    // The last step is reached by the Restore button only.
    if (d.to === 3 && !S.running) e.preventDefault();
    if (
      d.direction === "forward" &&
      d.from === 0 &&
      (readState !== "ready" || !S.app)
    )
      e.preventDefault();
    if (
      d.direction === "forward" &&
      d.from === 1 &&
      (nightsState !== "ready" || !S.night || S.night.startsWith("missed:"))
    )
      e.preventDefault();
  });
  wiz.addEventListener(STEP_EVENT, (e) => {
    const d = /** @type {CustomEvent} */ (e).detail;
    next.hidden = d.step >= 2;
    back.hidden = d.step === 3 && S.running;
    if (d.step === 0) paintApps();
    if (d.step === 1) paintNights();
    if (d.step === 2) void checkStep();
    gateNext();
  });

  tiles.addEventListener("change", (e) => {
    const input = /** @type {HTMLInputElement} */ (e.target);
    S.app = input.value;
    S.night = null;
    keepUrl();
    paintApps();
  });
  dated.addEventListener(DATED_EVENT, () => {
    const v = datedValue(dated);
    S.night = v && !v.startsWith("missed:") ? v : null;
    keepUrl();
    gateNext();
  });
  dated.addEventListener(DATED_RETRY_EVENT, () => void load());
  root.addEventListener(TYPE_CONFIRM_EVENT, (e) => {
    S.matched = Boolean(/** @type {CustomEvent} */ (e).detail.matched);
    armGo();
  });
  root.addEventListener(ACK_EVENT, (e) => {
    S.acked = Boolean(/** @type {CustomEvent} */ (e).detail.checked);
    armGo();
  });
  step1Card.addEventListener(CARD_RETRY_EVENT, () => void load());

  // ── load ─────────────────────────────────────────────────────────────
  const load = async () => {
    readState = "loading";
    nightsState = "loading";
    paintApps();
    paintNights();
    const enc = encodeURIComponent(stack);
    const b = await fetchJson(
      `/data/backups/${enc}`,
      `${stack}'s backups`,
      abort.signal,
    ).catch(() => null);
    if (abort.signal.aborted) return;
    if (!b || !b.ok) {
      read = {
        error:
          b && !b.ok
            ? b.error.fix
              ? `${b.error.why} — ${b.error.fix}`
              : b.error.why
            : "The host did not answer.",
      };
      readState = "error";
      nightsState = "error";
      paintApps();
      paintNights();
      return;
    }
    read = b.body;
    // An address that names an app without its snapshots, or none: the
    // only app is picked; a snapshot the app does not have is dropped.
    const rs = repos();
    readState = rs.length ? "ready" : "empty";
    nightsState = rs.length ? "ready" : "empty";
    if (!S.app && rs.length === 1) S.app = rs[0].owner;
    if (S.app && !repoOf(S.app)) S.app = null;
    if (S.night && !nights().some((n) => n.key === S.night)) S.night = null;
    paintApps();
    paintNights();
    // An address that carries the choice lands on the step it needs.
    if (S.app && S.night && w().step() === 0) {
      S.via = true;
      await w().goTo(2);
      S.via = false;
    } else if (S.app && w().step() === 0) {
      S.via = true;
      await w().goTo(1);
      S.via = false;
    }
  };
  paintStacks();
  paintApps();
  paintNights();
  next.hidden = false;
  const unsub = subscribe(paintStacks);
  void load();
  return () => {
    abort.abort();
    unsub();
    stopJob?.();
    stopSteps?.();
    for (const c of cleanups) c();
  };
}
