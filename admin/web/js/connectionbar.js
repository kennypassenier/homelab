// feat-kp11demo-14 (the connection banner): the dashboard says it when it
// loses the host, says why, counts the jobs the host holds queued and keeps
// trying. kp-themes' banner (js/connection.js) does the drawing, the grace
// period (a drop shorter than 3 s shows nothing), the countdown to the next
// try and the "Connected again" that leaves by itself; this module only
// reports what the live channel says. It replaces the 11 px `link` text the
// bar carried (index.html `#link`).
//
// What feeds it:
//   · the live channel dropped          → offline, with its reconnect attempts
//   · a new channel is being opened     → reconnecting, with the attempt
//   · it is open and the host link is
//     down (`link.up === false`)         → refused, with the host's own error
//   · it is open again                  → online ("Connected again", then gone)

import { act, onAct } from "./act.js";
import {
  current,
  onStream,
  retryStream,
  streamNow,
  subscribe,
} from "./store.js";
import { setStrings } from "/static/kp/dist/kp-themes.min.js";
import {
  CONNECTION_RETRY_EVENT,
  attachConnection,
  connectionOf,
  setConnection,
} from "/static/kp/dist/kp-themes.min.js";

/**
 * The stack whose Traefik and cloudflared carry the dashboard's own
 * connection: while one of its jobs runs (a backup pauses it, a deploy
 * restarts it) the live channel drops for longer than the banner's grace.
 */
export const GATEWAY_STACK = "gateway";

/**
 * The running job of the gateway stack, if any (offline-flash-1).
 * @param {{stack: string, action: string, state: string}[]} jobs
 * @returns {string | null} the job's action, or null
 */
export function gatewayJob(jobs) {
  const j = jobs.find(
    (x) =>
      x.stack === GATEWAY_STACK &&
      (x.state === "running" || x.state === "queued"),
  );
  return j ? j.action : null;
}

/**
 * The banner's words for a drop. While a gateway job runs the page says
 * that, instead of a bare "You are offline".
 * @param {string | null} action
 * @returns {{title: string, why: string}}
 */
export function offlineWords(action) {
  if (!action)
    return {
      title: "You are offline",
      why: "The page keeps showing what it last read. The host keeps running what it already holds.",
    };
  const doing = /backup/.test(action)
    ? "backed up"
    : /deploy|update|apply|restart/.test(action)
      ? "deployed"
      : "worked on";
  return {
    title: `The gateway is being ${doing}`,
    why: "The dashboard reaches you through the gateway, so the page lets go for a moment. It reconnects by itself when the gateway is back.",
  };
}

/**
 * What the banner shows for what the live channel and the host link say.
 * @param {{stream: import("./store.js").Stream, up: boolean | null,
 *   link: string, queued: number}} s
 * @returns {{state: import("/static/kp/dist/kp-themes.min.js").ConnectionState,
 *   detail: import("/static/kp/dist/kp-themes.min.js").ConnectionDetail}}
 */
export function connectionFor(s) {
  const pending = Math.max(0, s.queued);
  if (s.stream.state === "down")
    return { state: "offline", detail: { pending } };
  if (s.stream.state === "trying")
    return {
      state: "reconnecting",
      detail: { pending, attempt: s.stream.attempt },
    };
  if (s.stream.state === "open" && s.up === false)
    return { state: "refused", detail: { pending, why: s.link } };
  return { state: "online", detail: { pending } };
}

/** The jobs the host holds queued, as far as this page last heard. */
const queuedJobs = () => act.jobs.filter((j) => j.state === "queued").length;

/**
 * The words the package would say about "changes" are about an outbox this
 * dashboard does not have: nothing is kept in the browser. What is true is
 * that the page keeps showing what it last read and that the host keeps
 * running what it already holds.
 */
function sayItAsHomelab() {
  // The type stub lists only the strings other pages use.
  setStrings(
    /** @type {any} */ ({
      connOfflineWhy:
        "The page keeps showing what it last read. The host keeps running what it already holds.",
      connPending: (/** @type {number} */ n) =>
        `${n} ${n === 1 ? "job is" : "jobs are"} queued on the host`,
      connBackWhy: "Everything is up to date.",
      connBackSending: (/** @type {number} */ n) =>
        `${n} ${n === 1 ? "job is" : "jobs are"} queued on the host`,
      connRefusedWhy:
        "You are online, but the host did not answer the dashboard. Nothing was changed; try again in a moment.",
    }),
  );
}

/**
 * Wire the banner in `el` (`<div class="kp-connection" data-kp-connection>`).
 * @param {HTMLElement} el
 * @returns {() => void} stop
 */
export function mountConnectionBar(el) {
  sayItAsHomelab();
  const detach = attachConnection(el.parentElement ?? document);
  // One outage is reported once. The package waits 3 s before it shows an
  // offline banner (a Wi-Fi hiccup shows nothing), so a try that starts
  // inside that wait neither shows it early nor starts the wait again.
  let outage = false;
  const report = () => {
    const c = connectionFor({
      stream: streamNow(),
      up: current().up,
      link: current().link,
      queued: queuedJobs(),
    });
    const words = offlineWords(gatewayJob(act.jobs));
    setStrings(
      /** @type {any} */ ({
        connOffline: words.title,
        connOfflineWhy: words.why,
      }),
    );
    const shown = connectionOf(el)?.state() !== "online";
    if (c.state === "offline") {
      if (outage && !shown) return;
      outage = true;
    } else if (c.state === "reconnecting") {
      if (!shown) return;
    } else if (c.state === "online") outage = false;
    setConnection(el, c.state, c.detail);
  };
  const stops = [onStream(report), subscribe(report), onAct("jobs", report)];
  // The banner's own retry (automatic or Retry now) opens the channel again;
  // the stream's answer then moves the banner on.
  el.addEventListener(CONNECTION_RETRY_EVENT, () => retryStream());
  report();
  return () => {
    for (const s of stops) s();
    detach();
  };
}
