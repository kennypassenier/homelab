// The stack hub's Resources tab (feat-kp11demo-13.1, approved): CPU, memory
// and disk of the stack over the last 24 hours as one kp chart with a mark at
// every deploy ("Deployed by Kenny"), so "did the last deploy make it
// slower" is answered by looking. The chart loads, empties and fails with its
// own states inside a kp card.

import { fetchJson, fetchReport, h } from "./dom.js";
import { resourcesSpec } from "./hubparts.js";
import { whoOf } from "./stackhub.js";
import { stackEntries } from "./stacktabs.js";
import { timeChart } from "./charts.js";
import { current } from "./store.js";
import { section } from "./ui.js";
import { attachCardStates, setCardState } from "/static/kp/js/card-state.js";

/** The window the tab shows, in seconds. */
const WINDOW_S = 24 * 3600;

/**
 * @param {HTMLElement} panel
 * @param {{name: string, signal: AbortSignal}} c
 * @returns {() => void}
 */
export function resourcesTab(panel, c) {
  const { name } = c;
  const chartHost = h("figure", {
    class: "kp-chart hb-chart",
    "data-kp-chart": "",
    "aria-label": "Resources of the stack, last 24 hours",
  });
  const stage = h(
    "div",
    { class: "kp-card hb-stage hb-stage--chart", "data-kp-state": "loading" },
    h(
      "div",
      { class: "kp-card__stage" },
      h("div", { class: "kp-card__data" }, chartHost),
    ),
  );
  const card = section({
    id: "resources",
    title: "Resources",
    desc: "CPU, memory and disk of this stack over the last 24 hours, with a mark at every deploy, so you can see whether the last one made it slower.",
    body: [stage],
    foot: [
      "From Prometheus, in percent of what the stack may use",
      "a mark is one deploy",
    ],
  });
  panel.replaceChildren(card.el);
  const detach = attachCardStates(panel);
  setCardState(stage, "loading", { words: "Reading the resources" });

  const load = async () => {
    const enc = encodeURIComponent(name);
    const now = Math.floor(Date.now() / 1000);
    const [charts, history] = await Promise.all([
      fetchJson(`/data/charts?stack=${enc}&range=24h`, "the charts", c.signal),
      fetchReport(
        `/data/history?since=${now - WINDOW_S}`,
        "the history",
        c.signal,
      ),
    ]);
    if (c.signal.aborted) return;
    if (!charts.ok) {
      setCardState(stage, "error", {
        title: "Could not read the resources",
        text: `${charts.error.what}: ${charts.error.why}`,
      });
      return;
    }
    const deploys = history.ok
      ? stackEntries(history.report?.entries ?? [], name)
          .filter(
            (e) =>
              e.kind === "op" &&
              /deploy/i.test(e.label) &&
              e.start >= now - WINDOW_S,
          )
          .map((e) => ({
            at: e.start,
            label: `Deployed by ${whoOf(e).label}`,
          }))
      : [];
    const ramMax =
      current().fleet?.stacks.find((x) => x.name === name)?.ram_max_mb ?? null;
    const spec = resourcesSpec(
      charts.body,
      deploys,
      { from: now - WINDOW_S, to: now },
      ramMax,
    );
    if (!spec.series.length) {
      setCardState(stage, "empty", {
        title: "No resource readings yet",
        text: "Prometheus has no CPU, memory or disk series for this stack in the last 24 hours.",
      });
      return;
    }
    setCardState(stage, "ready");
    timeChart(chartHost, spec);
  };
  stage.addEventListener("kp-card-retry", () => {
    setCardState(stage, "loading", { words: "Reading the resources" });
    void load().catch(() => {});
  });
  void load().catch(() => {});
  return detach;
}
