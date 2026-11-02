// The Working copy card: read freshness, versions, the remote and the fetch
// button, with their reads and background refresh.

import { send } from "../../act.js";
import { notify, refusalAlarm } from "../../actui.js";
import { drivable } from "../../drivable.js";
import { fetchJson, h } from "../../dom.js";
import { REFRESH_S, changeGate } from "../../quiet.js";
import {
  REFRESH_EVENT,
  attachAll,
  attachDisabledReasons,
  refreshDone,
  setCardState,
} from "/static/kp/dist/kp-themes.min.js";
import { dataCard } from "./cards.js";
import { FETCH_NOW } from "./controls.js";
import { repoNodes } from "./repodraw.js";

/** The fetch button, drawn into the card by the repo read. */
function buildFetchNow() {
  return drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button",
        id: "repo-sync",
        title: "Fetch the remote now and compare",
        "data-kp-disabled-reason": "The remote is being fetched just now",
      },
      "Fetch now",
    ),
    FETCH_NOW,
  );
}

/** @param {{dashboard: string, host: string}} versions */
function versionNodes(versions) {
  const same = versions.dashboard === versions.host;
  return [
    h("div", { class: "kp-formgrid__label" }, h("span", null, "Versions")),
    h(
      "div",
      {
        class: "kp-formgrid__control st-versions",
        "data-kp-formgrid-span": "wide",
      },
      h(
        "span",
        null,
        same
          ? `Dashboard and host run the same version, ${versions.host}.`
          : `Dashboard ${versions.dashboard} and host ${versions.host}: they differ.`,
      ),
      h("span", {
        class: "kp-badge kp-badge--version",
        "data-kp-version-scheme": "numbers",
        "data-kp-version-from": versions.host,
        "data-kp-version-to": versions.dashboard,
        "data-kp-version-app": "homelab",
        "data-kp-version-href":
          "https://github.com/kennypassenier/homelab/releases",
      }),
    ),
  ];
}

/**
 * The card's frame: the "read from the host" refresh row, the versions row
 * and the rows the repo read fills.
 * @param {HTMLElement} wcBody
 */
function buildFrame(wcBody) {
  const readLine = h("small", { class: "st-working__note" });
  const refreshHost = h("div", {
    class: "kp-refresh",
    "data-kp-refresh": String(REFRESH_S),
    "data-kp-remember": "settings-read",
  });
  const refreshLabel = h("span", null, "Settings read from the host");
  const refreshRow = h(
    "div",
    { class: "kp-formgrid__row" },
    h("div", { class: "kp-formgrid__label" }, refreshLabel, readLine),
    h(
      "div",
      { class: "kp-formgrid__control", "data-kp-formgrid-span": "wide" },
      refreshHost,
    ),
  );
  const versionRow = h("div", { class: "kp-formgrid__row" });
  const repoRows = h("div", { class: "st-wc__rows" });
  wcBody.append(refreshRow, versionRow, repoRows);
  return { readLine, refreshHost, versionRow, repoRows };
}

/**
 * The Working copy card. Returns it before it is wired; `mountWorkingCopy`
 * wires it once the rest of the page exists.
 */
export function buildWorkingCopy() {
  const wcBody = h("div", {
    class: "kp-formgrid st-working",
    "data-kp-formgrid-label": "wide",
    "data-kp-formgrid-lines": "lines",
  });
  const wc = dataCard({
    id: "wc",
    title: "Working copy",
    desc: "The stacks repository the host applies from: where it stands against its remote, how old the read of host.toml is, and which version each side runs.",
    data: [wcBody],
  });
  return { wc, wcBody };
}

/**
 * Wire the card: the reads, the background refresh and the fetch button.
 * Sets `ctx.loadRepo` and `ctx.refreshReadLine`; returns `loadVersions` and
 * `drawRepo` for the first reads and the repo store's pushes.
 * @param {import("./ctx.js").Ctx} ctx
 */
export function mountWorkingCopy(ctx) {
  const repoGate = changeGate();
  const versionsGate = changeGate();
  const fetchNow = buildFetchNow();
  const { readLine, refreshHost, versionRow, repoRows } = buildFrame(
    ctx.wcBody,
  );
  /** @type {any} */
  let repoView = null;
  ctx.refreshReadLine = () => {
    readLine.textContent = ctx.page
      ? `${ctx.page.path}, ${ctx.page.fields.length} keys`
      : "";
  };
  const wcAttach = attachAll(ctx.wcBody);
  ctx.stops.push(() => wcAttach());

  /** @param {boolean} [quiet] */
  const loadVersions = async (quiet = false) => {
    const r = await fetchJson(
      "/data/versions",
      "the versions",
      ctx.abort.signal,
    );
    if (r.ok && quiet && !versionsGate.isNew(r.body)) return true;
    if (r.ok) {
      versionsGate.isNew(r.body);
      versionRow.replaceChildren(
        ...versionNodes({ dashboard: r.body.dashboard, host: r.body.host }),
      );
      void attachAll(versionRow);
    }
    return r.ok;
  };

  /** @param {import("./repodraw.js").RepoView} v */
  const drawRepo = (v) => {
    repoView = v;
    repoRows.replaceChildren(
      ...repoNodes(v, fetchNow, async () => void (await ctx.loadRepo())),
    );
    // The fetch button is drawn here, after the page was wired once.
    void attachDisabledReasons(ctx.wcBody);
  };

  /** @param {boolean} [quiet] */
  ctx.loadRepo = async (quiet = false) => {
    const r = await fetchJson(
      "/data/repo",
      "the working copy",
      ctx.abort.signal,
    );
    if (!r.ok) {
      setCardState(ctx.wc.card, repoView ? "ready" : "error", {
        title: "The working copy could not be read",
        text: r.error.why,
      });
      ctx.failed("the working copy", r.error, () => void ctx.loadRepo());
      return false;
    }
    ctx.failed("the working copy", null, () => void ctx.loadRepo());
    if (!repoGate.isNew(r.body) && quiet && repoView) return true;
    drawRepo(r.body);
    setCardState(ctx.wc.card, "ready");
    return true;
  };

  refreshHost.addEventListener(REFRESH_EVENT, async () => {
    const [a, b] = await Promise.all([
      ctx.loadHost(true),
      ctx.loadRepo(true),
      loadVersions(true),
    ]);
    refreshDone(refreshHost, a && b !== false);
  });
  ctx.wc.card.addEventListener("kp-card-retry", () => {
    setCardState(ctx.wc.card, "loading", {
      words: "Reading the working copy…",
    });
    void ctx.loadRepo();
  });
  ctx.hsCard.card.addEventListener("kp-card-retry", () => {
    setCardState(ctx.hsCard.card, "loading", { words: "Reading host.toml…" });
    void ctx.loadHost();
  });
  fetchNow.addEventListener("click", () => void fetchRemote(ctx, fetchNow));
  return { loadVersions, drawRepo };
}

/**
 * redesign-config-10: the fetch says it runs (it can take seconds on a
 * slow remote), and a page left meanwhile neither toasts nor reads.
 * @param {import("./ctx.js").Ctx} ctx @param {HTMLElement} fetchNow
 */
async function fetchRemote(ctx, fetchNow) {
  const b = /** @type {HTMLButtonElement} */ (fetchNow);
  b.disabled = true;
  b.setAttribute("aria-busy", "true");
  b.textContent = "Fetching the remote…";
  const x = await send(
    "POST",
    "/data/repo/sync",
    undefined,
    "the working copy",
  );
  b.disabled = false;
  b.removeAttribute("aria-busy");
  b.textContent = "Fetch now";
  if (ctx.abort.signal.aborted) return;
  if (!x.ok) await refusalAlarm(x.error, x.status);
  else notify("The working copy is up to date.", "success");
  if (!ctx.abort.signal.aborted) void ctx.loadRepo();
}
