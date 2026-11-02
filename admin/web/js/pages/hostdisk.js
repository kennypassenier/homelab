// The Host page's Disk card (feat-kp11demo-11): two kp stack bars, what
// fills the root volume, and the thin pool: what the guests wrote and what
// they were promised on top of it; plus the growth line in the card's foot.

import { agoEl, setAgo } from "../ago.js";
import { fetchJson, h } from "../dom.js";
import { diskBreakdown, rootGrowth, rootVolumeLine } from "../host.js";
import { current } from "../store.js";
import { section } from "../ui.js";
import { setStackbar } from "/static/kp/dist/kp-themes.min.js";

/** @param {import("./hostenv.js").HostEnv} env */
export function createDisk(env) {
  const { abort } = env;
  // Two kp stack bars (feat-kp11demo-11): what fills the root volume, and
  // the thin pool: what the guests wrote, and what they were promised on
  // top of it (an amber part, so an over-promise runs the bar past the end).
  const headRow = (
    /** @type {string} */ title,
    /** @type {string} */ sentence,
    /** @type {HTMLElement} */ read,
  ) =>
    h(
      "div",
      { class: "kp-formgrid__row hk-disk__head" },
      h("h3", { class: "kp-formgrid__label" }, title),
      h("p", { class: "kp-formgrid__control" }, sentence),
      read,
    );
  const rootRead = h("p", {
    class: "kp-formgrid__value",
    id: "host-disk-read",
    role: "status",
  });
  const poolRead = h("p", {
    class: "kp-formgrid__value",
    id: "host-pool-read",
    role: "status",
  });
  const rootBar = h("figure", { class: "kp-stackbar kp-stackbar--lg" });
  const poolBar = h("figure", { class: "kp-stackbar kp-stackbar--lg" });
  const diskBody = h(
    "div",
    { id: "host-disk", class: "kp-formgrid hk-disk" },
    headRow("Root volume", "What fills the host's own disk.", rootRead),
    rootBar,
    headRow(
      "Thin pool for the guests",
      "Written by the guests, against what all guests were promised.",
      poolRead,
    ),
    poolBar,
  );
  const growthLine = h("span", { id: "host-growth" }, "Growth: reading");
  const diskAgo = agoEl("read");
  const diskCard = section({
    id: "host-disk-card",
    title: "Disk",
    desc: "What fills the root disk, and the pool every container's own disk is carved from.",
    foot: [growthLine, diskAgo],
  });
  diskCard.body.append(diskBody);
  const gbText = (/** @type {number} */ gb) => `${Math.round(gb)} GB`;
  const paint = () => {
    const f = current().fleet;
    const d = f ? diskBreakdown(f) : null;
    if (!f) {
      for (const bar of [rootBar, poolBar])
        setStackbar(bar, {
          title: "Reading the disks",
          parts: [],
          state: "loading",
        });
      return;
    }
    if (!d) {
      for (const bar of [rootBar, poolBar])
        setStackbar(bar, {
          title: "Disk",
          parts: [],
          state: "empty",
          message: "The host has not read its disks yet.",
        });
      return;
    }
    const used = d.dirs.reduce((a, x) => a + x.gb, 0);
    setStackbar(rootBar, {
      title: "Root volume",
      total: d.rootGb,
      format: gbText,
      restLabel: "Free",
      marks: [{ at: 0.8, label: "80 % warning level" }],
      parts: d.dirs.map((x) => ({ key: x.path, label: x.path, value: x.gb })),
      state: "ready",
      label: "What fills the root volume",
    });
    rootRead.textContent = `${rootVolumeLine(f)}: ${gbText(used)} of ${gbText(d.rootGb)} in the biggest folders, ${d.usedPct}% used.`;
    const t = f.host.disk_detail?.thin_pool;
    if (!d.poolGb || !t) {
      setStackbar(poolBar, {
        title: "Thin pool",
        parts: [],
        state: "empty",
        message: "This host version does not report its thin pool.",
      });
      poolRead.textContent = "Not reported by this host version.";
    } else {
      const written = (d.poolGb * t.data_pct) / 100;
      const unwritten = Math.max(0, t.promised_gb - written);
      setStackbar(poolBar, {
        title: "Thin pool",
        total: d.poolGb,
        format: gbText,
        restLabel: "Free in the pool",
        marks: [],
        parts: [
          { key: "written", label: "Written by the guests", value: written },
          ...(unwritten > 0
            ? [
                {
                  key: "promised",
                  label: "Promised, not written yet",
                  value: unwritten,
                  tone: /** @type {const} */ ("warning"),
                },
              ]
            : []),
        ],
        state: "ready",
        label: "Thin pool: written and promised",
      });
      const over = t.promised_gb - d.poolGb;
      poolRead.textContent = `${gbText(written)} written of ${gbText(d.poolGb)}; ${gbText(t.promised_gb)} promised${over > 0 ? `, ${gbText(over)} more than the pool holds` : ""}.`;
    }
    setAgo(diskAgo, f.host.disk_detail?.measured_at ?? null);
  };
  /** The last /data/disk-growth answer; null until it arrives. */
  /** @type {Awaited<ReturnType<typeof fetchJson>> | null} */
  let growthRes = null;
  /**
   * The growth line needs two answers, the fit and the root volume's size
   * (the fleet read). Whichever arrives last paints it: with the size still
   * unknown, 0 GB a week would read "Root is not growing", which is wrong.
   */
  const paintGrowth = () => {
    const r = growthRes;
    if (!r) return;
    const rootGb = current().fleet?.host.disk_detail?.root_lv_size_gb ?? 0;
    if (r.ok && !(rootGb > 0)) return;
    const line = r.ok ? rootGrowth(r.body.rows ?? [], rootGb) : null;
    growthLine.textContent =
      line ??
      (r.ok
        ? "Growth: not enough history yet to say how fast root fills"
        : `Growth not measured: ${r.error.why}`);
  };
  const loadGrowth = async () => {
    const r = await fetchJson("/data/disk-growth", "disk growth", abort.signal);
    if (abort.signal.aborted) return;
    growthRes = r;
    paintGrowth();
  };
  return { card: diskCard, paint, paintGrowth, loadGrowth };
}
