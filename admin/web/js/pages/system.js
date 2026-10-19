// feat-shell-1 (redesign 3.71.0, FLOWS.md §1 "System"); rebuilt on kp-themes
// 11.0.0 from the demo Kenny approved on 2026-10-19 (app-homelab-backups,
// feat-kp11demo-11): the rarely needed area's landing. A tile per area —
// Host, Backups, Firewall, Metrics, Map, Tools — that says what the area is
// for and what it looks like right now (a live line of its own, read on its
// own, with its own Retry), the problems of the host mirrored on top (no
// room at all when there are none), what is moving (versions as chips that
// say how risky the jump is) and whether look and script of this dashboard
// agree. The settings you choose once (Set up) stay a short list under the
// tiles. Every page's address comes from areas.js's list, so this page and
// the palette never disagree. Its problems surface in the Inbox on their own.

import { SUB_PAGES } from "../areas.js";
import { agoEl, setAgo } from "../ago.js";
import { fetchJson, h, slowRead } from "../dom.js";
import { backupsLastNight } from "../systemview.js";
import { current, subscribe } from "../store.js";
import { SYSTEM_GROUPS, hostLine } from "../systemview.js";
import { fwKpis, fwSets } from "../fwview.js";
import { gridRow, formGrid } from "../gridrow.js";
import {
  attentionBand,
  button,
  dot,
  ensureStyle,
  pageHeader,
  section,
} from "../ui.js";
import {
  STATUS_RETRY_EVENT,
  attachStatusLists,
  buildStatusList,
  setStatus,
} from "/static/kp/js/statusrow.js";
import { attachCardStates, setCardState } from "/static/kp/js/card-state.js";
import {
  attachVersionDeltas,
  setVersionDelta,
  summarizeDeltas,
} from "/static/kp/js/version-delta.js";
import { renderDiagnostics } from "/static/kp/js/diagnostics.js";

/**
 * @typedef {{state: "ok" | "warn" | "failed" | "empty", label: string, detail: string}} Live
 */

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  ensureStyle("/css/pages/system.css");
  const abort = new AbortController();
  /** @type {(() => void)[]} */
  const stops = [];
  const state = h("span", { class: "sy-state" });
  const head = pageHeader({
    title: "System",
    titleMeta: [state],
    desc: "The machine underneath and the settings that apply to every stack. Day to day you rarely need this: problems from here show up in the Inbox on their own.",
  });
  const hrefOf = (/** @type {string} */ id) =>
    SUB_PAGES.find((s) => s.id === id)?.href ?? "/system";

  // ── what needs a look (mirrors the host's problems) ───────────────────
  const attention = attentionBand([]);

  // ── the six tiles ─────────────────────────────────────────────────────
  /** @type {{id: string, title: string, lede: string, open: string, href: string, load: () => Promise<Live>}[]} */
  const AREAS = [
    {
      id: "host",
      title: "Host",
      lede: "The machine everything runs on: disks, the thin pool, the guests and the daily checks.",
      open: "Open Host",
      href: hrefOf("host"),
      load: async () => {
        const r = await fetchJson("/data/ping", "the ping", abort.signal);
        if (!r.ok || !r.body.ok)
          return {
            state: "failed",
            label: "Host reachable",
            detail: r.ok ? `no answer after ${r.body.ms} ms` : r.error.why,
          };
        return {
          state: "ok",
          label: "Host reachable",
          detail: `answered in ${r.body.ms} ms`,
        };
      },
    },
    {
      id: "backups",
      title: "Backups",
      lede: "Every stack against every night, the repositories and the restore wizard.",
      open: "Open Backups",
      href: "/backups",
      load: async () => {
        const names = (current().fleet?.stacks ?? []).map((s) => s.name);
        const r = await slowRead(
          "/data/backup-calendar",
          "the backup nights",
          abort.signal,
        ).catch((e) => ({ ok: false, error: { why: String(e) } }));
        if (!r.ok)
          return {
            state: "failed",
            label: "Last night",
            detail: /** @type {any} */ (r).error?.why ?? "did not answer",
          };
        const x = backupsLastNight(
          /** @type {any} */ (r).body,
          names,
          Math.floor(Date.now() / 1000),
        );
        return {
          state: x.missed.length ? "warn" : "ok",
          label: "Last night",
          detail: x.expected
            ? `${x.covered} of ${x.expected} stacks backed up${x.missed.length ? `; missed: ${x.missed.join(", ")}` : ""}`
            : "no stack keeps data",
        };
      },
    },
    {
      id: "firewall",
      title: "Firewall",
      lede: "Which stack may reach which, and the rule that decides.",
      open: "Open Firewall",
      href: hrefOf("firewall"),
      load: async () => {
        const r = await fetchJson(
          "/data/firewall",
          "the firewalls",
          abort.signal,
        );
        if (!r.ok)
          return { state: "failed", label: "Firewalls", detail: r.error.why };
        const d = /** @type {any} */ (r.body);
        d.stacks = d.stacks ?? [];
        const sets = fwSets(d);
        const bits = [
          ...(sets.none.length
            ? [`${sets.none.length} without a firewall`]
            : []),
          ...(sets.differs.length
            ? [`${sets.differs.length} differ from their files`]
            : []),
        ];
        return {
          state: bits.length ? "warn" : "ok",
          label: "Firewalls",
          detail: bits.length
            ? `${bits.join(", ")}`
            : `every stack's firewall is in force (${fwKpis(d).rules} rules)`,
        };
      },
    },
    {
      id: "metrics",
      title: "Metrics",
      lede: "Charts of the host and the stacks, with the nightly backup marked.",
      open: "Open Metrics",
      href: hrefOf("metrics"),
      load: async () => {
        const r = await fetchJson(
          "/data/charts?range=1h",
          "the charts",
          abort.signal,
        );
        if (!r.ok)
          return {
            state: "failed",
            label: "Scrape targets",
            detail: r.error.why,
          };
        /** @type {any[]} */
        const panels = /** @type {any} */ (r.body).panels ?? [];
        const bad = panels.filter((p) => p.error).length;
        return {
          state: bad ? "warn" : "ok",
          label: "Charts",
          detail: `${panels.length - bad} of ${panels.length} panels answer`,
        };
      },
    },
    {
      id: "map",
      title: "Map",
      lede: "The network drawn as it is: stacks, links and mismatches.",
      open: "Open Map",
      href: hrefOf("fleetview"),
      load: async () => {
        const n = (current().fleet?.stacks ?? []).length;
        const r = await fetchJson(
          "/data/firewall",
          "the firewalls",
          abort.signal,
        );
        const rules = r.ok
          ? /** @type {any} */ (r.body).matrix?.rules?.length
          : null;
        return {
          state: "ok",
          label: "Stacks",
          detail: `${n} stacks${rules != null ? `, ${rules} firewall rules` : ""}`,
        };
      },
    },
    {
      id: "tools",
      title: "Tools",
      lede: "Versions, the look-and-script check and the console of the host.",
      open: "Open the console",
      href: hrefOf("shell"),
      load: async () => {
        const r = await fetchJson(
          "/data/versions",
          "the versions",
          abort.signal,
        );
        if (!r.ok)
          return { state: "failed", label: "Versions", detail: r.error.why };
        const v = /** @type {any} */ (r.body);
        return {
          state: v.update_available ? "warn" : "ok",
          label: "Versions",
          detail: v.update_available
            ? `host ${v.host}: ${v.latest} is available`
            : `host ${v.host ?? "unknown"}, up to date`,
        };
      },
    },
  ];
  /** @type {Map<string, HTMLElement>} */
  const lists = new Map();
  /** @type {Map<string, HTMLElement>} */
  const agos = new Map();
  const tiles = h(
    "ul",
    { class: "kp-tiles sy-tiles" },
    ...AREAS.map((a) => {
      const list = buildStatusList(
        [{ id: a.id, label: a.title, state: "loading", detail: "reading…" }],
        { label: `${a.title} live line`, single: true },
      );
      lists.set(a.id, list);
      const ago = agoEl("read", null);
      agos.set(a.id, ago);
      return h(
        "li",
        { class: "kp-card sy-tile", "data-card": a.id },
        h("h3", { class: "kp-card__title" }, a.title),
        h("p", { class: "kp-lede" }, a.lede),
        h("div", { class: "kp-card__body" }, list),
        h(
          "div",
          { class: "kp-card__footer sy-tile__foot" },
          ago,
          h(
            "a",
            { class: "kp-button kp-button--sm kp-button--ghost", href: a.href },
            a.open,
          ),
        ),
      );
    }),
  );
  const areas = section({
    id: "sy-areas",
    title: "Areas",
    desc: "Open an area, or read how it is doing. Each line is read on its own.",
  });
  const areasCard = h(
    "section",
    {
      class: "kp-card sy-areascard",
      "data-kp-state": "ready",
      "aria-label": "Areas",
    },
    h(
      "div",
      { class: "kp-card__stage" },
      h("div", { class: "kp-card__data" }, tiles),
    ),
  );
  areas.body.append(areasCard);
  const read = async (/** @type {(typeof AREAS)[number]} */ a) => {
    const list = /** @type {HTMLElement} */ (lists.get(a.id));
    setStatus(list, a.id, { state: "loading", detail: "reading…", fix: "" });
    try {
      const x = await a.load();
      if (abort.signal.aborted) return;
      setStatus(list, a.id, {
        label: x.label,
        state: x.state,
        detail: x.detail,
        fix: "",
      });
      setAgo(/** @type {HTMLElement} */ (agos.get(a.id)), Date.now() / 1000);
    } catch (e) {
      if (abort.signal.aborted) return;
      setStatus(list, a.id, {
        label: a.title,
        state: "failed",
        detail: `did not answer: ${String(e)}`,
        fix: "",
      });
    }
  };
  tiles.addEventListener(STATUS_RETRY_EVENT, (e) => {
    const id = /** @type {CustomEvent} */ (e).detail.id;
    const a = AREAS.find((x) => x.id === id);
    if (a) void read(a);
  });

  // ── set up: the settings you choose once ──────────────────────────────
  const setup = SYSTEM_GROUPS.find((g) => g.title === "Set up");
  const setupCard = section({
    id: "sy-setup",
    title: "Set up",
    desc: setup?.desc ?? "Settings you choose once and change rarely.",
  });
  setupCard.body.append(
    h(
      "nav",
      { class: "kp-actions sy-links", "aria-label": "Set up" },
      ...(setup?.cards ?? []).map((c) =>
        h(
          "a",
          {
            class: "kp-action",
            href: c.href ?? hrefOf(c.id),
            "data-card": c.id,
          },
          h("strong", { class: "kp-action__title" }, c.label),
          h("span", { class: "kp-action__mark", "aria-hidden": "true" }),
          h("span", { class: "kp-action__text" }, c.what),
        ),
      ),
      h(
        "a",
        {
          class: "kp-action",
          href: "/data/download/runbook",
          download: "DR_RUNBOOK.md",
          "data-card": "runbook",
        },
        h("strong", { class: "kp-action__title" }, "Disaster runbook"),
        h("span", { class: "kp-action__mark", "aria-hidden": "true" }),
        h(
          "span",
          { class: "kp-action__text" },
          "Download the step-by-step to rebuild everything from backups",
        ),
      ),
    ),
  );

  // ── versions ──────────────────────────────────────────────────────────
  const vsRead = h("p", { class: "kp-formgrid__value", role: "status" });
  const vsList = h("ul", { class: "kp-formgrid sy-vlist", role: "list" });
  const versions = section({
    id: "sy-versions",
    title: "What is moving",
    desc: "The versions of the host and the dashboard. A risky jump is loud; a version that did not move says so quietly.",
  });
  const vsCard = h(
    "section",
    {
      class: "kp-card sy-card",
      "data-kp-state": "loading",
      "aria-label": "Versions",
    },
    h(
      "div",
      { class: "kp-card__stage" },
      h("div", { class: "kp-card__data" }, vsList),
    ),
  );
  versions.body.append(
    formGrid([gridRow({ label: "Summary", control: vsRead, wide: true })]),
    vsCard,
  );
  const diag = h("div", { class: "sy-diag" });
  const looks = section({
    id: "sy-look",
    title: "Look and script",
    desc: "The stylesheet and the script of this dashboard should come from one release.",
  });
  looks.body.append(diag);
  try {
    renderDiagnostics(diag);
  } catch {
    diag.textContent =
      "The versions of the stylesheet and the script could not be read.";
  }
  /** @type {any} */
  let versionsBody = null;
  const paintVersions = () => {
    const v = versionsBody;
    if (!v) return;
    /** @type {{key: string, label: string, from: string, to: string, note: string}[]} */
    const rows = [
      {
        key: "host",
        label: "Host daemon",
        from: v.host ?? "",
        to: v.latest ?? v.host ?? "",
        note: v.update_available ? "an update is available" : "up to date",
      },
    ];
    vsList.replaceChildren(
      ...rows.map((r) =>
        h(
          "li",
          { class: "kp-formgrid__row" },
          h("span", { class: "kp-formgrid__label" }, r.label),
          h(
            "span",
            { class: "kp-formgrid__control" },
            h("span", {
              class: "kp-badge kp-badge--version",
              "data-kp-version-from": r.from,
              "data-kp-version-to": r.to,
              "data-kp-version-show": "both",
            }),
          ),
          h("span", { class: "kp-formgrid__value sy-vnote" }, r.note),
        ),
      ),
    );
    attachVersionDeltas(vsList);
    const s = summarizeDeltas(
      rows.map((r) => ({ from: r.from, to: r.to })),
      {},
    );
    vsRead.textContent = s.text;
    setCardState(vsCard, "ready");
    void setVersionDelta;
  };
  const loadVersions = async () => {
    setCardState(vsCard, "loading", { words: "Reading the versions…" });
    const r = await fetchJson("/data/versions", "the versions", abort.signal);
    if (abort.signal.aborted) return;
    if (!r.ok) {
      setCardState(vsCard, "error", {
        title: "The versions could not be read",
        text: r.error.why,
      });
      return;
    }
    versionsBody = r.body;
    if (!r.body.host) {
      setCardState(vsCard, "empty", {
        title: "No version reported",
        text: "The host has not told which version it runs.",
      });
      return;
    }
    paintVersions();
  };
  vsCard.addEventListener("kp-card-retry", () => void loadVersions());

  root.replaceChildren(
    head.el,
    attention.el,
    areas.el,
    versions.el,
    looks.el,
    setupCard.el,
  );
  stops.push(attachStatusLists(root), attachCardStates(root));

  const paint = () => {
    const s = current();
    const l = hostLine(s.fleet, versionsBody, s.up === false);
    state.replaceChildren(dot(l.tone, l.word));
    const items = [];
    const t = s.fleet?.host.disk_detail?.thin_pool;
    const pool = s.fleet?.host.disk_detail?.thin_pool_size_gb;
    if (t && pool && t.promised_gb > pool)
      items.push({
        key: "pool",
        tone: /** @type {const} */ ("warn"),
        title: `Thin pool promised ${Math.round((t.promised_gb / pool) * 100)} %`,
        text: `${Math.round(t.promised_gb)} GB promised on a ${Math.round(pool)} GB pool.`,
        action: h(
          "a",
          { class: "kp-button kp-button--sm", href: hrefOf("host") },
          "Open Host",
        ),
      });
    if (s.up === false)
      items.push({
        key: "down",
        tone: /** @type {const} */ ("bad"),
        title: "The host does not answer",
        text: "The line to the host is down; what you see may be old.",
        action: h(
          "a",
          { class: "kp-button kp-button--sm", href: hrefOf("host") },
          "Open Host",
        ),
      });
    attention.set(items);
  };
  const off = subscribe(paint);
  paint();
  for (const a of AREAS) void read(a);
  void loadVersions();
  void button;
  return () => {
    abort.abort();
    off();
    for (const s of stops) s();
    areas.stop();
  };
}
