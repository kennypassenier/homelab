// redesign-final-h3 (3.71.0's final review, 2026-10-04; FLOWS.md §5, task
// 5): the Restore flow as the approved flows/restore.html — Which app ·
// Which night · Confirm · Restore and check, on a page of its own like the
// Update flow (`/backups/restore?stack=…[&app=…][&snapshot=…]`), with the
// breadcrumb Backups / <stack> / Restore. Every Restore… (Backups, the
// stack hub, the palette) lands here; Live view's own driven form still
// opens the action's dialog. The run is the catalog's restore action, so
// the host does what it always did: the safety copy first, then the night
// put back; its steps and log follow below (the job panel).

import { catalogReady, send } from "../act.js";
import { refusalCallout } from "../actui.js";
import { actionForm } from "../actionforms.js";
import { fetchJson, h } from "../dom.js";
import { declare, declareField, drivable } from "../drivable.js";
import { humanDuration } from "../format.js";
import { mountJobPanel } from "../jobpanel.js";
import {
  restoreHref,
  restoreNights,
  restoreStep,
  whatWillHappen,
} from "../restoreflow.js";
import { current, subscribe } from "../store.js";
import {
  ensureStyle,
  pageHeader,
  section,
  skeletonLines,
  stepper,
  emptyState,
} from "../ui.js";

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
  const names = () => (current().fleet?.stacks ?? []).map((s) => s.name);
  const stack = q.get("stack") ?? names()[0] ?? null;
  /** @type {{app: string | null, night: string | null, running: boolean}} */
  const S = { app: q.get("app"), night: q.get("snapshot"), running: false };
  /** @type {any} */
  let read = null;
  /** @type {null | (() => void)} */
  let stopJob = null;

  const head = pageHeader({
    title: stack ? `Restore ${stack}` : "Restore",
    desc: "Put one app's data back as it was on a chosen night. Today's data is copied aside first, so a restore can itself be undone.",
  });
  const steps = stepper([
    "Which app",
    "Which night",
    "Confirm",
    "Restore and check",
  ]);
  const stacksRow = h("nav", {
    class: "rs-stacks",
    "aria-label": "Restore from another stack",
  });
  const appsCard = section({
    id: "restore-apps",
    title: "1 · Which app",
    desc: "Only apps that keep data have backups. Pick one.",
  });
  const nightsCard = section({
    id: "restore-nights",
    title: "2 · Which night",
    desc: "Pick the newest night before the problem started. Nights are listed newest first; a missed night is shown, not hidden.",
  });
  const what = h("ol", { class: "rs-what", id: "restore-what" });
  const typed = /** @type {HTMLInputElement} */ (
    h("input", {
      class: "kp-field__input rs-type",
      id: CONFIRM,
      autocomplete: "off",
      spellcheck: "false",
    })
  );
  const go = /** @type {HTMLButtonElement} */ (
    drivable(
      h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--primary",
          id: "restore-go",
          "aria-disabled": "true",
          title: "Type the stack's name first",
        },
        "Restore",
      ),
      GO,
    )
  );
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
  const err = h("div");
  const confirmCard = section({
    id: "restore-confirm-card",
    title: "3 · What will happen",
    desc: "Read, then type the stack's name to confirm.",
  });
  confirmCard.body.append(
    what,
    h(
      "label",
      { class: "rs-confirm", for: CONFIRM },
      h("span", null, "Type ", h("b", null, stack ?? ""), " to confirm"),
      typed,
    ),
    err,
    h(
      "div",
      { class: "rs-foot" },
      h(
        "span",
        { class: "rs-hint", id: "restore-hint" },
        "Nothing has run yet.",
      ),
      h("div", { class: "rs-acts" }, cancel, go),
    ),
  );
  const runCard = section({
    id: "restore-run",
    title: "4 · Restoring",
    desc: "You can leave this page; the running pill in the bar follows the job.",
  });
  runCard.el.hidden = true;

  root.replaceChildren(
    head.el,
    steps.el,
    stacksRow,
    appsCard.el,
    nightsCard.el,
    confirmCard.el,
    runCard.el,
  );

  if (!stack) {
    appsCard.body.replaceChildren(
      emptyState({
        title: "No stack to restore",
        text: "The host manages no stack yet; once one has backups, its apps show up here.",
      }),
    );
    return () => abort.abort();
  }
  appsCard.body.replaceChildren(skeletonLines(2, "Reading the backups"));
  nightsCard.body.replaceChildren(skeletonLines(3, "Reading the nights"));

  /** The repositories (apps) with at least one snapshot. */
  const repos = () =>
    /** @type {any[]} */ (read?.repos ?? []).filter(
      (r) => (r.snapshots ?? []).length > 0,
    );
  const repoOf = (/** @type {string | null} */ app) =>
    repos().find((r) => r.owner === app) ?? null;
  const nights = () =>
    restoreNights(repoOf(S.app)?.snapshots ?? [], Date.now() / 1000);
  const nightLabel = () =>
    nights().find((n) => n.key === S.night)?.label ?? "the chosen night";

  /** Keep the address in step with the choice (back and reload land here). */
  const keepUrl = () => {
    const href = restoreHref(stack, S.app, S.night);
    if (location.pathname + location.search !== href)
      history.replaceState(history.state, "", href);
  };

  const paintStacks = () => {
    const list = names();
    stacksRow.replaceChildren(
      h("span", { class: "rs-hint" }, "Stack"),
      ...list.map((n) =>
        drivable(
          h(
            "a",
            {
              class: `rs-stack${n === stack ? " is-on" : ""}`,
              href: restoreHref(n),
              "aria-current": n === stack ? "page" : null,
              title: `Restore one of ${n}'s apps instead`,
              onclick: (/** @type {MouseEvent} */ e) => {
                e.preventDefault();
                ctx.navigate(restoreHref(n));
              },
            },
            n,
          ),
          STACK,
          n,
        ),
      ),
    );
  };

  /** @param {any} r @param {string} label @param {Node} small @param {boolean} on @param {() => void} pick */
  const card = (r, label, small, on, pick) =>
    h(
      "button",
      {
        type: "button",
        class: `rs-pick${on ? " is-on" : ""}`,
        "aria-pressed": String(on),
        onclick: pick,
        ...r,
      },
      h("strong", null, label),
      h("small", null, small),
    );

  const paint = () => {
    steps.set(restoreStep(S));
    const now = Date.now() / 1000;
    const rs = repos();
    if (!rs.length) {
      appsCard.body.replaceChildren(
        emptyState({
          title: `${stack} has no snapshots yet`,
          text: "Once its first backup has run, its apps and their nights show up here.",
        }),
      );
      nightsCard.body.replaceChildren();
    } else {
      appsCard.body.replaceChildren(
        h(
          "div",
          { class: "rs-grid", role: "group", "aria-label": "Which app" },
          ...rs.map((r) =>
            drivable(
              card(
                { "data-app": r.owner },
                r.owner,
                h(
                  "span",
                  null,
                  `${r.snapshots.length} ${r.snapshots.length === 1 ? "snapshot" : "snapshots"} · newest ${humanDuration(now - Math.max(...r.snapshots.map((/** @type {any} */ s) => s.time)))} ago`,
                ),
                r.owner === S.app,
                () => {
                  if (S.app !== r.owner) S.night = null;
                  S.app = r.owner;
                  keepUrl();
                  paint();
                },
              ),
              APP,
              r.owner,
            ),
          ),
        ),
      );
      nightsCard.body.replaceChildren(
        S.app
          ? h(
              "div",
              { class: "rs-grid", role: "group", "aria-label": "Which night" },
              ...nights().map((n) =>
                n.missed
                  ? h(
                      "div",
                      {
                        class: "rs-pick rs-pick--missed",
                        "aria-disabled": "true",
                      },
                      h("strong", null, n.label),
                      h(
                        "small",
                        null,
                        h("span", { class: "nx-dot nx-dot--bad" }),
                        " no snapshot (missed)",
                      ),
                    )
                  : drivable(
                      card(
                        { "data-night": n.key },
                        n.label,
                        h("span", { class: "mono" }, `snapshot ${n.key}`),
                        n.key === S.night,
                        () => {
                          S.night = n.key;
                          keepUrl();
                          paint();
                        },
                      ),
                      NIGHT,
                      n.key,
                    ),
              ),
            )
          : h("p", { class: "rs-hint" }, "Pick an app first."),
      );
    }
    what.replaceChildren(
      ...(S.app && S.night
        ? whatWillHappen({
            stack,
            app: S.app,
            night: nightLabel(),
            native: read?.native === true,
          }).map((l) => h("li", null, l))
        : [h("li", { class: "rs-hint" }, "Pick an app and a night first.")]),
    );
    const ready =
      !S.running && !!S.app && !!S.night && typed.value.trim() === stack;
    // Held back, never dead: a press before it is ready says what is
    // missing and goes there (and Live view can always press it).
    go.setAttribute("aria-disabled", String(!ready));
    go.title = ready
      ? `Restore ${S.app} from ${nightLabel()}, the safety copy first`
      : !S.app || !S.night
        ? "Pick an app and a night first"
        : `Type ${stack} first`;
  };
  typed.addEventListener("input", paint);
  typed.addEventListener("focus", () => steps.set(Math.max(3, restoreStep(S))));

  go.addEventListener("click", async () => {
    if (go.getAttribute("aria-disabled") === "true") {
      if (S.running) return;
      const hint = /** @type {HTMLElement} */ (
        root.querySelector("#restore-hint")
      );
      hint.textContent = go.title;
      if (!S.app) appsCard.el.scrollIntoView({ block: "nearest" });
      else if (!S.night) nightsCard.el.scrollIntoView({ block: "nearest" });
      else typed.focus();
      return;
    }
    if (!S.app || !S.night) return;
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
    paint();
    err.replaceChildren(h("p", { class: "rs-hint" }, "Sending…"));
    const r = await send(
      "POST",
      form.runPath,
      native
        ? { confirm: stack, snapshot: S.night }
        : { confirm: stack, app: S.app, snapshot: S.night },
      form.title,
    );
    if (!r.ok) {
      S.running = false;
      err.replaceChildren(refusalCallout(r.error));
      paint();
      return;
    }
    err.replaceChildren();
    const panel = mountJobPanel(/** @type {number} */ (r.body.job), {
      compact: true,
      steps: true,
    });
    stopJob = panel.stop;
    runCard.el.hidden = false;
    runCard.body.replaceChildren(panel.element);
    runCard.el.scrollIntoView({ block: "nearest" });
  });

  const load = async () => {
    const enc = encodeURIComponent(stack);
    const b = await fetchJson(
      `/data/backups/${enc}`,
      `${stack}'s backups`,
      abort.signal,
    );
    if (!b.ok) {
      appsCard.body.replaceChildren(
        refusalCallout(b.error, "destructive", "Not read"),
      );
      nightsCard.body.replaceChildren();
      return;
    }
    read = b.body;
    // An address that names an app without its snapshots, or none: the
    // only app is picked; a snapshot the app does not have is dropped.
    const rs = repos();
    if (!S.app && rs.length === 1) S.app = rs[0].owner;
    if (S.app && !repoOf(S.app)) S.app = null;
    if (S.night && !nights().some((n) => n.key === S.night)) S.night = null;
    paint();
  };
  paintStacks();
  const unsub = subscribe(paintStacks);
  void load().catch(() => {});
  return () => {
    abort.abort();
    unsub();
    stopJob?.();
  };
}
