// The Connection card's reachability strips (ask-kp11be-2): the host's
// reachability over the last 24 hours (one cell an hour) and 90 days (one a
// day), from the dashboard's own capped history.

import { fetchJson, h } from "../dom.js";
import { periodsOf, shareText, uptimeQuery } from "../uptimeview.js";
import { setUptime } from "/static/kp/dist/kp-themes.min.js";

/** @param {import("./hostenv.js").HostEnv} env */
export function createReach(env) {
  const { abort } = env;
  /** @param {string} id @param {number} count */
  const reachStrip = (id, count) =>
    h("div", {
      class: "kp-uptime",
      id,
      "data-kp-uptime": "",
      "data-kp-uptime-count": String(count),
      "data-kp-uptime-fit": "scroll",
    });
  const reach24 = reachStrip("host-reach-24", 24);
  const reach90 = reachStrip("host-reach-90", 90);
  const reachFigs = h("p", { class: "hk-muted", id: "host-reach-figs" });
  /** @param {HTMLElement} el @param {string} label @param {"day" | "hour"} unit @param {any} entry @param {string | null} why */
  const paintReach = (el, label, unit, entry, why) => {
    const periods = periodsOf(entry, unit, "reachable");
    setUptime(el, {
      label,
      periods,
      from: unit === "day" ? "90 days ago" : "24 hours ago",
      to: unit === "day" ? "Today" : "Now",
      count: unit === "day" ? 90 : 24,
      open: "info",
      state: why
        ? "error"
        : periods.some((p) => p.state !== "none")
          ? "ready"
          : "empty",
      ...(why ? { text: why } : {}),
      message: "No ping measured yet: the watch pings the host once a minute.",
    });
  };
  /** The strips' own busy card while the history is read. */
  const loadingReach = () => {
    setUptime(reach24, {
      label: "Last 24 hours",
      count: 24,
      state: "loading",
    });
    setUptime(reach90, {
      label: "Last 90 days",
      count: 90,
      state: "loading",
    });
  };
  let reachDrawn = false;
  const load = async () => {
    // refresh-one-minute-1: the strips show their loading card only before
    // the first read; a refresh keeps the drawn strips until new ones land.
    if (!reachDrawn) loadingReach();
    const now = Math.floor(Date.now() / 1000);
    const [d, hr] = await Promise.all([
      fetchJson(
        `/data/uptime?${uptimeQuery("day", now)}`,
        "the host's reachability history",
        abort.signal,
      ),
      fetchJson(
        `/data/uptime?${uptimeQuery("hour", now)}`,
        "the host's reachability history",
        abort.signal,
      ),
    ]);
    if (abort.signal.aborted) return true;
    reachDrawn = true;
    const why = !d.ok
      ? `${d.error.what}: ${d.error.why}`
      : !hr.ok
        ? `${hr.error.what}: ${hr.error.why}`
        : null;
    const day = d.ok ? d.body.keys?.["ping:host"] : undefined;
    const hour = hr.ok ? hr.body.keys?.["ping:host"] : undefined;
    paintReach(reach24, "Last 24 hours", "hour", hour, why);
    paintReach(reach90, "Last 90 days", "day", day, why);
    reachFigs.textContent = why
      ? "Reachability history not read."
      : `Reachable: ${shareText(hour?.uptime, "24 hours")} · ${shareText(day?.uptime, "90 days")}. Pinged once a minute; kept 90 days.`;
    return !why;
  };
  reach24.addEventListener("kp-uptime-retry", () => void load());
  reach90.addEventListener("kp-uptime-retry", () => void load());
  return { reach24, reach90, figs: reachFigs, load };
}
