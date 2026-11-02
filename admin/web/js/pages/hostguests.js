// The Host page's Containers card (feat-kp11demo-11): the demo's three
// views of the same guests (a table you can sort, filter and open, cards of
// one height with two meters, and the pressure list that puts the guest
// closest to its memory limit first), read from /data/host/guests.

import { agoEl, setAgo } from "../ago.js";
import { fetchJson, h } from "../dom.js";
import { declare } from "../drivable.js";
import { formGrid, gridRow } from "../gridrow.js";
import { guestTable } from "../host.js";
import { setEntityGridQuiet } from "../inplace.js";
import { REFRESH_S, changeGate } from "../quiet.js";
import { stackHref } from "../router.js";
import { current } from "../store.js";
import { section, toggleGroup } from "../ui.js";
import { setParams } from "../urlstate.js";
import {
  attachDataTables,
  BULLETS_PICK_EVENT,
  BULLETS_RETRY_EVENT,
  dataTable as tableOf,
  ENTITY_ACTION_EVENT,
  ENTITY_GRID_RETRY_EVENT,
  SEGMENTED_EVENT,
} from "/static/kp/dist/kp-themes.min.js";
import { guestCard, guestDetail } from "./hostguestrows.js";
import { paintPending, paintPressure, paintRows } from "./hostguestpaint.js";
import { guestViews } from "./hostguesttable.js";

const GUEST_FILTER = declare({
  id: "container-filter",
  page: "host",
  opens: "run",
  row: "all|running|stopped",
  what: "turn one status of the Containers table on or off (all: every row)",
});

/**
 * @param {import("./hostenv.js").HostEnv} env
 * @param {{onLoaded: () => void}} hooks `onLoaded`: new guests arrived (the KPI strip repaints)
 */
export function createGuests(env, hooks) {
  const { abort, ctx } = env;
  /** @type {import("../host.js").Guest[] | null} */
  let guests = null;
  /** @type {string | null} */
  let guestsWhy = null;
  const q0 = new URLSearchParams(location.search);
  /** @type {"table" | "cards" | "pressure"} */
  let gView = ["table", "cards", "pressure"].includes(
    q0.get("containers") ?? "",
  )
    ? /** @type {any} */ (q0.get("containers"))
    : "table";
  let gShow = new Set();
  const gFilter = toggleGroup({
    label: "Status",
    all: { label: "All", hint: "Every container, running or not" },
    chips: [
      {
        value: "running",
        label: "Running",
        hint: "Click to show or hide running containers",
      },
      {
        value: "stopped",
        label: "Stopped",
        hint: "Click to show or hide stopped containers",
      },
    ],
    onChange: (on) => {
      gShow = on;
      gDt()?.filter(3, on.size ? [...on] : null);
    },
    drive: { id: GUEST_FILTER },
  });
  const gViewItem = (
    /** @type {"table" | "cards" | "pressure"} */ v,
    /** @type {string} */ label,
  ) =>
    h(
      "button",
      {
        type: "button",
        class: "kp-segmented__item",
        role: "radio",
        "aria-checked": String(gView === v),
        "data-value": v,
      },
      label,
    );
  const gViewSeg = h(
    "div",
    {
      class: "kp-segmented",
      "data-kp-segmented": "single",
      role: "radiogroup",
      "aria-label": "View of the containers",
    },
    gViewItem("table", "Table"),
    gViewItem("cards", "Cards"),
    gViewItem("pressure", "Pressure"),
  );
  const {
    read: gRead,
    tbody: gTbody,
    table: gTableEl,
    cards: gCards,
    bullets: gBullets,
    views: gViews,
  } = guestViews();
  const gDt = () => tableOf(gTableEl);
  const guestsAgo = agoEl("read");
  const guestsCard = section({
    id: "host-containers",
    title: "Containers",
    desc: "Every container and VM on the host, managed by this orchestrator or not: as a table, as cards with meters, or as a pressure list.",
    foot: [`Read with pct list every ${REFRESH_S} s`, guestsAgo],
  });
  guestsCard.body.append(
    formGrid(
      [
        gridRow({
          label: "View",
          hideLabel: true,
          control: gViewSeg,
          value: gRead,
        }),
        gridRow({ label: "Status", hideLabel: true, control: gFilter.el }),
      ],
      "hk-toolbar",
    ),
    gViews,
  );

  /** @param {typeof gView} v */
  const showFor = (v) => {
    gTableEl.hidden = v !== "table";
    gCards.hidden = v !== "cards";
    gBullets.hidden = v !== "pressure";
  };
  const paintTable = (/** @type {ReturnType<typeof guestTable>} */ view) => {
    const dt = gDt();
    paintRows({ dt, tbody: gTbody, table: gTableEl }, view);
    if (!guestsReady) {
      dt?.busy(null);
      dt?.state("ready");
      guestsReady = true;
    }
  };
  // The table leaves its loading state once; later refreshes keep it ready.
  let guestsReady = false;
  const paint = () => {
    showFor(gView);
    if (!guests) {
      guestsReady = false;
      return paintPending(
        { dt: gDt(), cards: gCards, bullets: gBullets },
        guestsWhy,
      );
    }
    const view = guestTable(guests, current().fleet);
    const running = view.filter((r) => r.running).length;
    gFilter.counts({
      all: view.length,
      running,
      stopped: view.length - running,
    });
    paintTable(view);
    setEntityGridQuiet(gCards, view.map(guestCard));
    const withRam = paintPressure(gBullets, view);
    gRead.textContent =
      gView === "pressure"
        ? `Memory share of ${withRam.toLocaleString("en-GB")} running guests, worst first; amber from 75 %.`
        : `${view.length.toLocaleString("en-GB")} guests: ${running.toLocaleString("en-GB")} running.`;
  };
  gViewSeg.addEventListener(SEGMENTED_EVENT, (e) => {
    const v = /** @type {CustomEvent} */ (e).detail.value;
    gView = v === "cards" || v === "pressure" ? v : "table";
    history.replaceState(
      history.state,
      "",
      location.pathname +
        setParams(location.search, {
          containers: gView === "table" ? null : gView,
        }),
    );
    paint();
  });
  gViews.addEventListener(ENTITY_ACTION_EVENT, (e) => {
    const d = /** @type {CustomEvent} */ (e).detail;
    const g =
      guests &&
      guestTable(guests, current().fleet).find(
        (x) => String(x.vmid) === String(d.key),
      );
    if (!g) return;
    if (d.id === "stack" && g.stack) ctx.navigate(stackHref(g.stack));
    else ctx.navigate("/activity?view=host-log");
  });
  gViews.addEventListener(BULLETS_PICK_EVENT, (e) => {
    const key = /** @type {CustomEvent} */ (e).detail.key;
    gViewSeg
      .querySelector('[data-value="table"]')
      ?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    gDt()?.expand([String(key)]);
  });
  for (const ev of [
    "kp-datatable-retry",
    ENTITY_GRID_RETRY_EVENT,
    BULLETS_RETRY_EVENT,
  ])
    gViews.addEventListener(ev, () => void load().catch(() => {}));
  const guestsGate = changeGate();
  const load = async () => {
    const r = await fetchJson(
      "/data/host/guests",
      "the host's containers",
      abort.signal,
    );
    if (!r.ok) {
      guestsWhy = `${r.error.why}${r.error.fix ? ` — ${r.error.fix}` : ""}`;
      if (!guests) paint();
      return false;
    }
    guestsWhy = null;
    setAgo(guestsAgo, r.body.measured_at);
    // refresh-one-minute-1: the same containers draw nothing again.
    if (!guestsGate.isNew(r.body.guests ?? []) && guests) return true;
    guests = r.body.guests ?? [];
    paint();
    hooks.onLoaded();
    return true;
  };

  /** Opens the sentence of an opened row. @param {HTMLElement} row */
  const detail = (row) => {
    const g = guests
      ? guestTable(guests, current().fleet).find(
          (x) => String(x.vmid) === row.dataset.kpRowKey,
        )
      : null;
    return g ? guestDetail(g) : null;
  };
  /** Wires the package's data table to the card; returns its cleanup. */
  const attach = () => attachDataTables(guestsCard.body, { detail });
  return {
    card: guestsCard,
    paint,
    load,
    attach,
    resetFilter: () => gFilter.reset(),
  };
}
