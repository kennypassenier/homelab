// The host's slow reads (Today, the fleet check, the doctor) as the pages
// see them (shell/slow.rs). The dashboard runs each read once and answers
// every request within 20 s; a 202 `{running, run}` says it is still on its
// way. Since slow-reads (Kenny, 2026-09-29, form "Trage pagina's"): the
// dashboard keeps the last answer, so opening a page shows it at once
// (`read_at`, `refreshing: {run}`) while one new run reads again; a
// finished run is announced on the live channel (`slow_read`) and fetched by
// its id, which starts nothing. Pure apart from the fetch it is handed, so
// the node tests drive it.

/**
 * @typedef {{ok: true, body: any} | {ok: false, error: any}} Answer
 * @typedef {(url: string, what: string, signal?: AbortSignal) => Promise<Answer>} Fetch
 */

/**
 * @param {string} url
 * @param {number} run
 */
export function runUrl(url, run) {
  return `${url}${url.includes("?") ? "&" : "?"}run=${encodeURIComponent(run)}`;
}

/**
 * What one answer means for the page.
 * @param {Answer} r
 * @returns {{kind: "done"} | {kind: "wait", run: number} | {kind: "last", run: number}}
 *   done: the answer to show; wait: still running, ask after `run`; last:
 *   the previous answer, shown now, while `run` reads again
 */
export function step(r) {
  if (!r.ok) return { kind: "done" };
  if (r.body?.running === true)
    return { kind: "wait", run: Number(r.body.run) };
  const again = r.body?.refreshing?.run;
  if (again != null) return { kind: "last", run: Number(again) };
  return { kind: "done" };
}

/**
 * Read `url` to its fresh answer. When the dashboard holds an earlier one,
 * `onLast(body)` gets it first, at once.
 * @param {Fetch} fetchJson
 * @param {string} url
 * @param {string} what
 * @param {AbortSignal} [signal]
 * @param {(body: any) => void} [onLast]
 * @returns {Promise<Answer>}
 */
export async function freshRead(fetchJson, url, what, signal, onLast) {
  let r = await fetchJson(url, what, signal);
  for (;;) {
    const s = step(r);
    if (s.kind === "done") return r;
    if (s.kind === "last" && r.ok) onLast?.(r.body);
    r = await fetchJson(runUrl(url, s.run), what, signal);
  }
}

/**
 * Whether a `slow_read` event is one this page should fetch: its read, a
 * run that answered and that the page has not shown, and the page not
 * already waiting for it.
 * @param {{read?: string, run?: number, ok?: boolean} | null} ev
 * @param {string} key
 * @param {{shown: number | null, reading: boolean}} page
 */
export function fetchAnnounced(ev, key, page) {
  return (
    ev != null &&
    ev.read === key &&
    typeof ev.run === "number" &&
    // A run that failed is its own page's answer; the others keep theirs.
    ev.ok !== false &&
    !page.reading &&
    ev.run !== page.shown
  );
}

// ── Within one tab: going back to a page paints at once ────────────────
/** @type {Map<string, any>} */
const kept = new Map();

/**
 * @param {string} url
 * @param {any} body a finished answer
 */
export function keepRead(url, body) {
  kept.set(url, body);
}

/** @param {string} url @returns {any} the last answer this tab saw, or undefined */
export function keptRead(url) {
  return kept.get(url);
}
