// The Host page's Connection card: the pinned TLS line this dashboard holds
// to the host daemon, its last pings as a strip, the reachability strips,
// and the ping button. A ping also sets the header's chip and the
// attention band.

import { fetchJson, h } from "../dom.js";
import { declare, drivable } from "../drivable.js";
import { current } from "../store.js";
import { dot, section } from "../ui.js";
import {
  attachPatterns,
  runFlashButton,
  setUptime,
} from "/static/kp/dist/kp-themes.min.js";
import { createReach } from "./hostreach.js";

/** How many pings the latency strip keeps. */
const PINGS = 12;

const PING = declare({
  id: "ping-host",
  page: "host",
  opens: "run",
  what: "send one ping over the pinned line and time the answer",
});
const COPY_PIN = declare({
  id: "copy-host-pin",
  page: "host",
  opens: "run",
  what: "copy the pinned certificate fingerprint",
});

/**
 * @param {import("./hostenv.js").HostEnv} env
 * @param {{reach: HTMLElement, attention: ReturnType<typeof import("../ui.js").attentionBand>}} header the header parts a ping updates
 */
export function createLine(env, { reach, attention }) {
  const { abort } = env;
  /** @type {(number | null)[]} */
  const pings = [];
  let pingedOnce = false;
  /** @type {any} */
  let lineFacts = null;
  let lineOk = /** @type {boolean | null} */ (null);
  const facts = h("dl", { class: "kp-kv", id: "host-line" });
  const lat = h("div", {
    class: "kp-uptime",
    id: "host-lat",
    "data-kp-uptime": "",
    "data-kp-uptime-count": "12",
    "data-kp-uptime-fit": "scroll",
  });
  const rch = createReach(env);
  const { reach24, reach90, figs: reachFigs } = rch;
  const pingDot = dot("");
  const pingOut = h("span", { id: "host-ping-out" }, "Pinging…");
  const pingBtn = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm",
        title: "Send one ping over the pinned line and time the answer",
        onclick: () => void ping().catch(() => {}),
      },
      "Ping again",
    ),
    PING,
  );
  const pingCount = h("span", null, "last pings");
  const lineCard = section({
    id: "host-connection",
    title: "Connection",
    desc: "The pinned TLS line this dashboard holds to the host daemon.",
    foot: [pingBtn, pingCount],
  });
  // The three strips share one parent, so while they load one busy card
  // stands over the three of them (uptime.js groupHost; since kp-themes
  // 11.2.0 the measure counts that card, kp-gaps/homelab.md line 128).
  lineCard.body.append(
    facts,
    h("div", { class: "hk-strips" }, lat, reach24, reach90),
    reachFigs,
    h("p", { class: "hk-ping-out", role: "status" }, pingDot, pingOut),
  );
  /**
   * The pinned fingerprint, shortened, with kp's copy button for the whole.
   * @param {string} pin
   */
  const pinCell = (pin) => {
    const pinCopy = drivable(
      h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--ghost kp-button--sm kp-copyable__button",
          "data-kp-flash-words": "Copied,Press Ctrl C",
          "aria-label": "Copy the whole fingerprint",
        },
        "Copy",
      ),
      COPY_PIN,
    );
    pinCopy.addEventListener("click", () => {
      void runFlashButton(pinCopy, async () => {
        try {
          await navigator.clipboard.writeText(pin);
          return "Copied";
        } catch {
          return { error: "Press Ctrl C", word: "Press Ctrl C" };
        }
      });
    });
    const cell = h(
      "span",
      { class: "kp-copyable" },
      h(
        "span",
        { class: "kp-copyable__value mono", title: pin },
        `${pin.slice(0, 20)}…`,
      ),
      pinCopy,
    );
    attachPatterns(cell);
    return cell;
  };
  let lastUptime = "";
  const paintLine = () => {
    const f = current().fleet;
    /** @type {[string, Node | string][]} */
    const rows = [];
    if (lineFacts) {
      rows.push([
        "Address",
        h("span", { class: "mono" }, lineFacts.address ?? "not configured"),
      ]);
      rows.push(["Set in", lineFacts.address_source ?? "—"]);
      rows.push([
        "Pinned",
        lineFacts.pin ? pinCell(lineFacts.pin) : "no pin compiled in",
      ]);
    }
    rows.push([
      "Host says",
      lineOk === true
        ? h("span", { class: "hk-status" }, dot("ok"), "matches the pin")
        : lineOk === false
          ? h(
              "span",
              { class: "hk-status" },
              dot("bad"),
              "no answer over the pinned line",
            )
          : f?.host.tls_fingerprint || "not asked yet",
    ]);
    facts.replaceChildren(
      ...rows.map(([k, v]) =>
        h("div", { class: "kp-kv__row" }, h("dt", null, k), h("dd", null, v)),
      ),
    );
    /** @type {import("/static/kp/dist/kp-themes.min.js").UptimeSpec} */
    const uptimeSpec = {
      label: "Line to the host",
      periods: pings.map((ms, i) => ({
        key: `p${i}`,
        label:
          i === pings.length - 1 ? "Now" : `${pings.length - 1 - i} pings ago`,
        state: ms == null ? "down" : ms > 30 ? "degraded" : "up",
        tip:
          ms == null
            ? "no answer"
            : `answered in ${ms} ms${ms > 30 ? " (slow)" : ""}`,
      })),
      from: `${Math.max(pings.length, 1)} pings ago`,
      to: "Now",
      count: PINGS,
      // Every ping unanswered is a failed read, drawn as the package's
      // failed plate; a mix of answers and silences is the strip itself.
      state: pings.length
        ? pings.every((ms) => ms == null)
          ? "error"
          : "ready"
        : pingedOnce
          ? "empty"
          : "loading",
      error: "The host did not answer over the pinned line.",
      open: "info",
    };
    // The strip is redrawn only when what it shows changed: a store event
    // that moves nothing here must not repaint it (and with it a state the
    // package or a reader has put it in).
    const uptimeKey = JSON.stringify(uptimeSpec);
    if (uptimeKey !== lastUptime) {
      lastUptime = uptimeKey;
      setUptime(lat, uptimeSpec);
    }
    pingCount.textContent = pings.length
      ? `last ${pings.length} ping${pings.length === 1 ? "" : "s"}`
      : "no ping yet";
  };
  lat.addEventListener("kp-uptime-retry", () => void ping().catch(() => {}));
  /** @param {boolean} [quiet] a background refresh: no "Pinging…" flash */
  const ping = async (quiet = false) => {
    const silent = quiet && pingedOnce;
    if (!silent) {
      pingBtn.setAttribute("disabled", "");
      pingOut.textContent = "Pinging…";
    }
    const r = await fetchJson("/data/ping", "the ping", abort.signal);
    pingBtn.removeAttribute("disabled");
    pingedOnce = true;
    if (!r.ok) {
      lineOk = false;
      pings.push(null);
      while (pings.length > PINGS) pings.shift();
      pingOut.textContent = `No answer: ${r.error.why}`;
    } else {
      lineFacts = r.body.facts;
      lineOk = !!r.body.ok;
      // redesign-final Low: the dashboard keeps the last twelve round
      // trips it measured, so the strip opens full (the demo's twelve).
      const recent = Array.isArray(r.body.recent) ? r.body.recent : null;
      if (recent && recent.length) pings.splice(0, pings.length, ...recent);
      else pings.push(r.body.ok ? r.body.ms : null);
      while (pings.length > PINGS) pings.shift();
      pingOut.textContent = r.body.ok
        ? `Answered in ${r.body.ms} ms: ${r.body.message}`
        : `No answer after ${r.body.ms} ms: ${r.body.message}`;
    }
    pingDot.dataset.state = lineOk ? "ok" : "down";
    reach.replaceChildren(
      dot(lineOk ? "ok" : "bad"),
      lineOk && r.ok ? `Reachable · ${r.body.ms} ms` : "Not reachable",
    );
    attention.set(
      lineOk
        ? []
        : [
            {
              key: "unreachable",
              tone: "bad",
              title: "The host does not answer over the pinned line",
              text: pingOut.textContent ?? "",
              action: h(
                "button",
                {
                  type: "button",
                  class: "kp-button kp-button--sm",
                  onclick: () => void ping().catch(() => {}),
                },
                "Ping again",
              ),
            },
          ],
    );
    paintLine();
    return lineOk === true;
  };
  const paint = paintLine;
  return { card: lineCard, paint, ping, loadReach: rch.load };
}
