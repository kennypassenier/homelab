// feat-pages-1 (chassis-rs 3.1.0, nav-decisions): the kit's own Status
// page, drawn by this app (`kit_pages_in_webapp`) from `GET
// /api/kit/status` instead of the kit's own layout, so it carries this
// app's bar (command palette, notifications, theme switcher) like every
// other page. Read-only besides the project's own section actions, which
// are plain POST/DELETE buttons the server already described
// (`shell::dashboard::Action`) — this page only renders them.

import { send } from "../act.js";
import { errorBox, fetchJson, h } from "../dom.js";

/**
 * @typedef {{label: string, route: string, method: "POST" | "PUT" | "DELETE",
 *   destructive: boolean, confirm: string | null,
 *   busy_label: string | null}} ActionView
 * @typedef {{title: string, explain: string, rows: [string, string][],
 *   html: string | null}} Section
 * @typedef {{section: Section, actions: ActionView[]}} SectionView
 * @typedef {{what: string, why: string, remedy: string}} Problem
 * @typedef {{mode: string, latest: string, last_check: string,
 *   note: string | null}} UpdateView
 * @typedef {{version: string, listen: string, started_at: string,
 *   health: any, sections: SectionView[], problems: Problem[],
 *   update: UpdateView, backup: any}} StatusData
 */

/**
 * One action button, wired to POST (or `a.method`) its route; a 2xx
 * re-reads the page (K29's own rule: the kit's template reloads the whole
 * page too), a refusal shows under the button.
 * @param {ActionView} a
 * @param {() => void} onDone
 */
function actionButton(a, onDone) {
  const note = h("p", { class: "measured" });
  const btn = h(
    "button",
    {
      type: "button",
      class: `kp-button kp-button--sm${a.destructive ? " kp-button--destructive" : ""}`,
    },
    a.label,
  );
  btn.addEventListener("click", async () => {
    if (a.destructive && !confirm(a.confirm ?? "Are you sure?")) return;
    btn.disabled = true;
    const was = btn.textContent;
    if (a.busy_label) btn.textContent = a.busy_label;
    const r = await send(a.method, a.route, undefined, a.label);
    btn.disabled = false;
    btn.textContent = was;
    if (r.ok) {
      note.textContent = "";
      onDone();
    } else {
      note.textContent = `refused: ${r.error.why}`;
    }
  });
  return h("span", { class: "status-action" }, btn, note);
}

/** @param {Section} s @param {ActionView[]} actions @param {() => void} onDone */
function sectionCard(s, actions, onDone) {
  return h(
    "section",
    { class: "kp-card" },
    h("h2", { class: "kp-card__title" }, s.title),
    h("p", { class: "explain" }, s.explain),
    h(
      "dl",
      { class: "status-rows" },
      ...s.rows.flatMap(([label, value]) => [
        h("dt", null, label),
        h("dd", null, value),
      ]),
    ),
    ...(s.html
      ? [
          (() => {
            const d = h("div", null);
            d.innerHTML = s.html ?? "";
            return d;
          })(),
        ]
      : []),
    ...(actions.length
      ? [
          h(
            "div",
            { class: "kp-row status-actions" },
            ...actions.map((a) => actionButton(a, onDone)),
          ),
        ]
      : []),
  );
}

/** @param {Problem} p */
function problemRow(p) {
  return h(
    "li",
    { class: "kp-alert kp-alert--warning" },
    h(
      "div",
      { class: "kp-alert__body" },
      h("strong", null, p.what),
      h("p", null, p.why),
      h("p", null, `Fix: ${p.remedy}`),
    ),
  );
}

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const status = h("p", { class: "measured", role: "status" }, "Reading…");
  const body = h("div", { class: "status-page" });
  root.replaceChildren(h("h1", null, "Status"), status, body);
  const abort = new AbortController();

  const load = async () => {
    const r = await fetchJson(
      "/api/kit/status",
      "the status page",
      abort.signal,
    );
    if (!r.ok) {
      status.textContent = "";
      body.replaceChildren(errorBox(r.error));
      return;
    }
    /** @type {StatusData} */
    const d = r.body;
    status.textContent = "";
    /** @type {Node[]} */
    const out = [];
    out.push(
      h(
        "section",
        { class: "kp-card" },
        h("h2", { class: "kp-card__title" }, "Service"),
        h(
          "dl",
          { class: "status-rows" },
          h("dt", null, "Version"),
          h("dd", null, d.version),
          h("dt", null, "Listening on"),
          h("dd", null, d.listen),
          h("dt", null, "Started"),
          h("dd", null, d.started_at),
          h("dt", null, "Updates"),
          h(
            "dd",
            null,
            `${d.update.mode} · latest ${d.update.latest} · last checked ${d.update.last_check}`,
            ...(d.update.note ? [` · ${d.update.note}`] : []),
          ),
        ),
      ),
    );
    if (d.problems.length)
      out.push(
        h(
          "section",
          { "aria-label": "Problems" },
          h("ul", { class: "status-problems" }, ...d.problems.map(problemRow)),
        ),
      );
    for (const sv of d.sections)
      out.push(sectionCard(sv.section, sv.actions, retry));
    body.replaceChildren(...out);
  };
  const retry = () => void load().catch(() => {});
  retry();
  return () => abort.abort();
}
