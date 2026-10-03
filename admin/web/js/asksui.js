// feat-ops-2: the host's open questions, each with Allow / Stop, what each
// answer sets in motion and how long the host still waits. Since 3.71.0
// (feat-shell-4, decision "Inbox") they are the Inbox's top rows instead of
// a strip under the bar; the bar's Inbox counter and a toast make a new one
// visible from every page.

import { answerBody, askView, openAsks } from "./asks.js";
import { h } from "./dom.js";
import { current, setAsks, subscribe } from "./store.js";

/**
 * Draw the open questions into `region`, kept current; empty (hidden) when
 * the host asks nothing.
 * @param {HTMLElement} region
 * @returns {() => void} stop
 */
export function mountAsks(region) {
  /** @type {Map<string, {box: HTMLElement, left: HTMLElement}>} */
  const shownAsks = new Map();

  /** @param {import("./asks.js").Ask} a @param {boolean} allow @param {HTMLElement} box */
  const answer = async (a, allow, box) => {
    box.querySelectorAll("button").forEach((b) => (b.disabled = true));
    const note = /** @type {HTMLElement} */ (box.querySelector(".ask-note"));
    note.textContent = allow ? "Sending: allow…" : "Sending: stop…";
    /** @type {any} */
    let body = null;
    let ok = false;
    try {
      const r = await fetch("/data/asks/answer", {
        method: "POST",
        headers: {
          "content-type": "application/json",
          accept: "application/json",
        },
        body: JSON.stringify(answerBody(a, allow)),
      });
      ok = r.ok;
      body = await r.json().catch(() => null);
    } catch {
      body = { why: "the dashboard did not answer" };
    }
    if (ok) {
      note.textContent = allow
        ? "Allowed. The operation goes on."
        : "Stopped. The operation stops here.";
      setAsks(
        current().asks.filter((x) => !(x.id === a.id && x.boot === a.boot)),
      );
      return;
    }
    note.textContent = `Not sent: ${body?.why ?? "unknown error"}${body?.fix ? `. ${body.fix}` : ""}`;
    box.querySelectorAll("button").forEach((b) => (b.disabled = false));
  };

  const render = () => {
    const now = Date.now() / 1000;
    const open = openAsks(current().asks, now);
    const keys = new Set(open.map((a) => askView(a, now).key));
    for (const [k, v] of shownAsks)
      if (!keys.has(k)) {
        v.box.remove();
        shownAsks.delete(k);
      }
    for (const a of open) {
      const v = askView(a, now);
      const known = shownAsks.get(v.key);
      if (known) {
        known.left.textContent = v.left;
        known.box.dataset.urgent = String(v.urgent);
        continue;
      }
      const left = h("p", { class: "measured ask-left" }, v.left);
      const allow = h(
        "button",
        { type: "button", class: "kp-button" },
        "Allow",
      );
      const stop = h(
        "button",
        { type: "button", class: "kp-button kp-button--destructive" },
        "Stop",
      );
      const box = h(
        "div",
        {
          class: "kp-alert kp-alert--warning ask",
          role: "alert",
          id: `ask-${v.key}`,
          "data-ask": v.key,
        },
        h("strong", null, v.title),
        h("p", null, v.what),
        h(
          "dl",
          { class: "facts ask-consequences" },
          h("dt", null, "Allow"),
          h("dd", null, v.ifAllowed),
          h("dt", null, "Stop"),
          h("dd", null, v.ifStopped),
        ),
        h("div", { class: "ask-buttons" }, allow, stop),
        left,
        h("p", { class: "ask-note", role: "status", "aria-live": "polite" }),
      );
      allow.addEventListener("click", () => void answer(a, true, box));
      stop.addEventListener("click", () => void answer(a, false, box));
      region.append(box);
      shownAsks.set(v.key, { box, left });
    }
    region.hidden = region.childElementCount === 0;
  };
  const off = subscribe(render);
  const timer = setInterval(render, 1000);
  render();
  return () => {
    off();
    clearInterval(timer);
  };
}
