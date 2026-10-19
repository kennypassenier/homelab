// The Stacks page (redesign-stacks, release 3.71.0; rebuilt on the approved
// kp-themes 11 demo, feat-kp11demo-12). Top to bottom: the header (live
// status, Deploy all changes with its count, New stack… as the one primary
// action), the host strip (five KPI tiles, each a link), "All stacks" (the
// fleet as a kp datatable grouped by state, or as kp entity cards; one
// selection bar for Deploy, Back up now and Update…; see fleetlist.js),
// "Which stacks use the most" and "Every stack, every night" (fleetglance.js).
// Deploy all changes and New stack open side panels.
//
// Data: the fleet store (live), /data/fleet-trend (the host CPU line),
// /data/drift (which stacks differ from their files, the count on Deploy all
// changes), /data/backup-calendar (each stack's backups and the nights grid),
// /data/capacity (the ranking's CPU and disk), /data/stale-images (newer
// versions) and the Inbox (the verdict and its tile).

import { errorBox, fetchJson, h, slowRead } from "../dom.js";
import { declare, dialogControl, drivable, viaForm } from "../drivable.js";
import { register } from "../drivehooks.js";
import { fleetGlance } from "../fleetglance.js";
import { fleetList } from "../fleetlist.js";
import { openImport } from "../importstack.js";
import { inboxNow, onInbox } from "../inboxlist.js";
import { capacityRows } from "../mapview.js";
import { openNewStack } from "../newstack.js";
import { stackHref } from "../router.js";
import { current, subscribe } from "../store.js";
import {
  deployCount,
  hostStrip,
  newerByStack,
  stackRows,
} from "../stacksview.js";
import { drawer, ensureStyle, kpiStrip, pageHeader } from "../ui.js";
import { setParams } from "../urlstate.js";
import { mount as mountPlan } from "./apply.js";

/** @typedef {{navigate: (href: string) => void}} Ctx */

/**
 * A host tile as ui.js draws it: stacksview's used-of-limit number is the
 * inline meter at the start of the context line, its status dot the
 * context's tone.
 * @param {import("../stacksview.js").HostKpi} t
 * @returns {import("../ui.js").Kpi}
 */
const tileKpi = ({ meter, dot, tone, ...t }) => ({
  ...t,
  tone: tone === "ok" ? "" : tone,
  ctxMeter:
    meter == null ? null : { pct: meter, tone: tone === "ok" ? "" : tone },
  ctxTone: dot ?? null,
});
/** @typedef {import("../stacksview.js").Row} Row */

// Live view (invariant 39): the page's own controls are declared here; the
// list's (view, group, ticks, batch, cards) in fleetlist.js and the glance
// cards' in fleetglance.js.
const DEPLOY_ALL = declare({
  id: "stacks-deploy-all",
  page: "overview",
  opens: "dialog",
  what: "open Deploy all changes: compare every stack with its files, read the plan, then deploy",
});
const NEW_STACK = declare({
  id: "stacks-new",
  page: "overview",
  opens: "dialog",
  what: "open New stack: from a preset, from a bundle or empty",
});
const KPI = declare({
  id: "stacks-kpi",
  page: "overview",
  opens: "view",
  row: "online|newer|cpu|disk",
  what: "open the detail behind one tile of the strip (stacks running, newer versions, host CPU, root disk)",
});
// redesign-final M1: the attention band that duplicated the Inbox tile is
// gone; its Inbox button's id is the strip's Need you tile now.
const INBOX = declare({
  id: "stacks-open-inbox",
  page: "overview",
  opens: "view",
  what: "the strip's Need you tile: open the Inbox",
});

/**
 * @param {HTMLElement} root
 * @param {Ctx} ctx
 * @returns {() => void}
 */
export function mount(root, ctx) {
  ensureStyle("/css/pages/stacks.css");
  root.classList.add("sk-page");
  const abort = new AbortController();
  /** @type {(() => void)[]} */
  const stops = [];
  const q0 = new URLSearchParams(location.search);

  // ── state ────────────────────────────────────────────────────────────
  /** @type {any} */
  let drift = null;
  /** @type {number | null} what the plan itself counted, once read */
  let planCount = null;
  /** @type {any} */
  let calendar = null;
  /** @type {Map<string, number>} */
  let newer = new Map();
  /** @type {any} */
  let trend = null;
  /** @type {Row[]} */
  let rows = [];

  // ── header ───────────────────────────────────────────────────────────
  const deployBadge = h("span", { class: "sk-badge", hidden: "" });
  const deployAll = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button",
        id: "deploy-all",
        title:
          "Make every stack match its files in one confirmed batch; each stack is backed up before it changes",
      },
      "Deploy all changes",
      deployBadge,
    ),
    DEPLOY_ALL,
  );
  deployAll.addEventListener("click", () => openDeployAll());
  const newStack = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--primary",
        id: "new-stack",
        title: "Add a stack from a preset or from an exported bundle",
      },
      "New stack…",
    ),
    NEW_STACK,
  );
  newStack.addEventListener("click", () => openNew());
  const head = pageHeader({
    title: "Stacks",
    desc: "Every stack the host runs, live. Click one to open its hub — its state, logs, backups and settings together — or tick several to act on them at once.",
    live: "updated",
    actions: [deployAll],
    primary: newStack,
  });
  head.el.classList.add("sk-head");

  // ── the strip ────────────────────────────────────────────────────────
  const strip = kpiStrip(
    [
      { key: "online", label: "Stacks running", href: "/stacks" },
      { key: "needs-you", label: "Need you", href: "/inbox" },
      { key: "newer", label: "Newer versions", href: "/update?all=1" },
      { key: "cpu", label: "Host CPU", href: "/host" },
      { key: "disk", label: "Root disk", href: "/host" },
    ],
    { loading: !current().fleet, label: "The host at a glance" },
  );
  strip.el.classList.add("sk-kpis");
  for (const [key, k] of strip.tiles)
    if (key === "needs-you") drivable(k.el, INBOX);
    else drivable(k.el, KPI, key);

  // ── the list and the glance cards ────────────────────────────────────
  const list = fleetList({
    navigate: ctx.navigate,
    openNew: () => openNew(),
    retry: () => readAll(),
  });
  const glance = fleetGlance({
    navigate: ctx.navigate,
    stackHref,
    retry: () => readAll(),
  });
  // One line per source that could not be read (the comparison, the
  // backups, the newer versions, the CPU lines): what, why, what to do.
  const errs = h("div", { class: "sk-errors", "aria-live": "polite" });
  /** @type {Map<string, HTMLElement>} */
  const errBySource = new Map();
  /**
   * @param {string} source
   * @param {import("../doctor.js").RouteError | null} e null: it read fine
   */
  const sourceError = (source, e) => {
    if (e) errBySource.set(source, errorBox(e));
    else errBySource.delete(source);
    errs.replaceChildren(...errBySource.values());
    errs.hidden = errBySource.size === 0;
  };
  errs.hidden = true;

  root.replaceChildren(head.el, strip.el, errs, list.el, ...glance.els);

  // ── painting ─────────────────────────────────────────────────────────
  const repaintHost = () => {
    const f = current().fleet;
    if (!f) return;
    const { items } = inboxNow();
    const tiles = hostStrip(
      f,
      trend,
      {
        count: items.length,
        urgent: items.filter((i) => i.severity === "bad").length,
      },
      newer,
    );
    for (const t of tiles) {
      const k = strip.tiles.get(t.key);
      if (!k) continue;
      k.set({ ...tileKpi(t), loading: false });
    }
    head.live?.set(f.measured_at);
  };

  const recompute = () => {
    const f = current().fleet;
    if (!f) {
      const st = current();
      if (st.up === false) {
        list.state("failed", st.link);
        glance.set({ fleet: "failed" });
      } else {
        list.state("loading");
        glance.set({ fleet: "loading" });
      }
      return;
    }
    rows = stackRows(f, {
      newer,
      drift: drift?.stacks ?? {},
      calendar,
      trend,
      now: Date.now() / 1000,
    });
    list.set({ rows, calendar });
    glance.set({ rows, calendar, fleet: "ready" });
    repaintHost();
    const n = planCount ?? deployCount(drift);
    deployBadge.hidden = n == null || n === 0;
    deployBadge.textContent = n == null ? "" : String(n);
    deployAll.title =
      n == null
        ? "Make every stack match its files in one confirmed batch (not compared yet: the panel compares first)"
        : n === 0
          ? "Every stack matched its files at the last comparison; open to compare again"
          : `${n} ${n === 1 ? "stack differs" : "stacks differ"} from ${n === 1 ? "its" : "their"} files (new and removed ones included); deploy them in one confirmed batch, each backed up first`;
  };

  // ── side panels ──────────────────────────────────────────────────────
  /** @type {ReturnType<typeof drawer> | null} */
  let panel = null;

  /** Deploy all changes: compare, read the plan, deploy (FLOWS.md task 13). */
  const openDeployAll = () => {
    panel?.close();
    const compareNote = h("p", { class: "sk-hint", "aria-live": "polite" });
    const compareBtn = dialogControl(
      h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--sm",
          id: "drift-compare",
          title:
            "Check every stack against its files again now and mark the ones that differ in the list",
        },
        "Compare again",
      ),
      "compare-now",
    );
    /** @type {{pending: number, broken: number} | null} */
    let planRead = null;
    // redesign-flows-4/5: the plan's own steps (apply.js) carry the
    // compare as step 1, done once a compare has run.
    /** @type {Set<() => void>} */
    const compareSubs = new Set();
    const compare = {
      button: h("div", { class: "ap-compare" }, compareBtn, compareNote),
      at: () =>
        planRead != null || deployCount(drift) != null
          ? (drift?.measured_at ?? Date.now() / 1000)
          : null,
      /** @param {() => void} f */
      onChange: (f) => {
        compareSubs.add(f);
        return () => compareSubs.delete(f);
      },
    };
    const paintCompare = () => {
      const n = planRead ? planRead.pending : deployCount(drift);
      const stacks = (/** @type {number} */ k) =>
        `${k} ${k === 1 ? "stack" : "stacks"}`;
      compareNote.textContent =
        n == null
          ? "Comparing every stack with its files…"
          : n === 0
            ? "Every stack matches its files."
            : `${stacks(n)} ${n === 1 ? "differs" : "differ"} from ${n === 1 ? "its" : "their"} files, new and removed ones included · marked in the list.`;
      compareSubs.forEach((f) => f());
    };
    paintCompare();
    compareBtn.addEventListener("click", () => {
      compareBtn.disabled = true;
      compareNote.textContent = "Comparing every stack with its files…";
      void readDrift(true)
        .catch(unlessAborted)
        .finally(() => {
          compareBtn.disabled = false;
          paintCompare();
        });
    });
    const planBody = h("div", { class: "sk-plan", id: "apply-section" });
    let stopPlan = () => {};
    panel = drawer({
      // redesign-final-c1: a wide sheet (stacks.css), the plan's steps,
      // tiles and columns at the approved apply.html's size.
      cls: "sk-drawer sk-drawer--plan",
      title: "Deploy all changes",
      desc: "Make every stack match its files in the repository, in one confirmed batch. Each stack is backed up before it changes.",
      body: [planBody],
      onClose: () => {
        stopPlan();
        panel = null;
        const s = setParams(location.search, { "deploy-all": null });
        if (s !== location.search)
          history.replaceState(history.state, "", location.pathname + s);
      },
    });
    panel.open();
    stopPlan = mountPlan(planBody, {
      compare,
      // The plan's own count is the exact one: it goes on the header
      // button and into the compare line, and the list's flags follow.
      onRead: (read) => {
        planRead = read;
        planCount = read.pending;
        paintCompare();
        recompute();
        void readDrift(false).catch(unlessAborted);
      },
    });
    const s = setParams(location.search, { "deploy-all": "1" });
    if (s !== location.search)
      history.replaceState(history.state, "", location.pathname + s);
  };

  /** New stack: the three routes, each ending on the same review. */
  const openNew = () => {
    panel?.close();
    /**
     * @param {string} title
     * @param {string} text
     * @param {string} form
     * @param {() => void} run
     * @param {string} [name] its Live view name in the panel (the form's
     *   own by default)
     */
    const route = (title, text, form, run, name = form) => {
      const b = viaForm(
        dialogControl(
          h(
            "button",
            { type: "button", class: "nx-action sk-route" },
            h("strong", null, title),
            h("span", null, text),
          ),
          name,
        ),
        form,
      );
      b.addEventListener("click", () => {
        panel?.close();
        run();
      });
      return b;
    };
    panel = drawer({
      cls: "sk-drawer",
      title: "New stack",
      desc: "A stack is one container with the apps it runs. Pick how to start; the wizard asks only what it cannot work out itself.",
      body: [
        h(
          "div",
          { class: "nx-actions sk-routes" },
          route(
            "From a preset",
            "Jellyfin, Vaultwarden, a static site… — tested settings; you pick a name and a disk size",
            "new-stack",
            () => void openNewStack(ctx.navigate),
          ),
          route(
            "From a bundle",
            "A stack exported from another homelab (a .tar.gz from Export bundle)",
            "import",
            () => void openImport(ctx.navigate),
          ),
          route(
            "Empty",
            "Start from a blank stack and add apps yourself, from its hub",
            "new-stack",
            () => void openNewStack(ctx.navigate, { empty: true }),
            "new-empty",
          ),
        ),
        h(
          "p",
          { class: "sk-hint" },
          "Every route ends on the same review: what will be created, which address it gets, and a deploy you confirm. ",
          dialogControl(
            h("a", { href: "/presets" }, "See every preset"),
            "see-presets",
          ),
        ),
      ],
      onClose: () => {
        panel = null;
      },
    });
    panel.open();
  };

  // Live view (owner decision 2026-09-30): `homelab ui select` ticks stacks
  // here exactly as a click would, so a batch action opened right after it
  // acts on the same stacks.
  stops.push(
    register("overview", {
      select: (/** @type {string[]} */ stacks) => list.tick(stacks),
    }),
  );

  // ── reads ────────────────────────────────────────────────────────────
  /** @param {boolean} fresh */
  async function readDrift(fresh) {
    const r = await fetchJson(
      `/data/drift${fresh ? "?fresh=1" : ""}`,
      "the comparison with the files",
      abort.signal,
    );
    sourceError("drift", r.ok ? null : r.error);
    if (r.ok) {
      drift = r.body;
      recompute();
    }
  }
  const readTrend = async () => {
    const r = await fetchJson(
      "/data/fleet-trend",
      "the fleet's last day (the CPU lines)",
      abort.signal,
    );
    sourceError("trend", r.ok ? null : r.error);
    if (r.ok) {
      trend = r.body;
      recompute();
    }
  };
  const readCapacity = async () => {
    glance.set({ cap: "loading" });
    const r = await fetchJson("/data/capacity", "the capacity", abort.signal);
    if (!r.ok) {
      glance.set({ cap: "failed", capWhy: r.error.why, capacity: null });
      return;
    }
    glance.set({
      cap: "ready",
      capacity: new Map(
        capacityRows(r.body.panels ?? []).map((c) => [c.stack, c]),
      ),
    });
  };
  /** An abort (the page closed) is not an error to show. @param {unknown} e */
  const unlessAborted = (e) => {
    if (!abort.signal.aborted) throw e;
  };
  const readCalendar = () => {
    glance.set({ cal: "loading" });
    return slowRead(
      "/data/backup-calendar",
      "the backups",
      abort.signal,
      (last) => {
        calendar = last;
        recompute();
      },
    )
      .then((r) => {
        sourceError("calendar", r.ok ? null : r.error);
        calendar = r.ok ? r.body : { stacks: {}, no_backup: [] };
        glance.set({
          cal: r.ok ? "ready" : "failed",
          calWhy: r.ok ? "" : r.error.why,
        });
        recompute();
      })
      .catch(unlessAborted);
  };
  const readStale = () =>
    slowRead("/data/stale-images", "newer versions", abort.signal, (last) => {
      newer = newerByStack(last.images ?? []);
      recompute();
    })
      .then((r) => {
        sourceError("newer", r.ok ? null : r.error);
        if (r.ok) newer = newerByStack(r.body.images ?? []);
        recompute();
      })
      .catch(unlessAborted);
  function readAll() {
    // FLOWS.md §3 #13: the count on Deploy all changes shows before the
    // click, so the comparison is read with the page (the server reuses a
    // recent one rather than asking latch again).
    void readDrift(true).catch(unlessAborted);
    void readTrend().catch(unlessAborted);
    void readCapacity().catch(unlessAborted);
    void readCalendar();
    void readStale();
  }
  readAll();
  const trendTimer = setInterval(
    () => void readTrend().catch(unlessAborted),
    300_000,
  );

  stops.push(subscribe(recompute));
  stops.push(onInbox(repaintHost));
  recompute();
  // `/stacks?deploy-all=1` (Deploy all changes, `g d`, the old `/apply`)
  // opens its panel at once.
  if (q0.get("deploy-all") === "1") openDeployAll();

  return () => {
    abort.abort();
    clearInterval(trendTimer);
    panel?.close();
    list.stop();
    glance.stop();
    for (const s of stops) s();
    root.classList.remove("sk-page");
  };
}
