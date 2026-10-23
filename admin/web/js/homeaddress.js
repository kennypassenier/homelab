// eten-home-ip-1 (Kenny, 2026-10-22: "mee in de volgende release van admin
// dashboard, daar kan dan ergens de optie komen om het adres te updaten bij
// cloudflare"): the Home address section of Settings. The house has no
// static public address; Cloudflare Access lets it in without a login by
// that address (bypass policies). When the provider changes the address the
// house gets login screens and eten refuses it. This section shows the
// house's address now beside the address Cloudflare lets in, says whether
// they match, and has the one button that writes the current address into
// the bypass policies holding the old one: a normal dashboard job
// (`update-home-address`, run on the dashboard's server, shown in the job
// panel, kept in Activity).
//
// The decisions are the server's (`core::homeaddress`); this file only
// draws `GET /data/home-address` and presses the job. The Inbox row that
// points here is `homeAddressRows` in inboxrows.js.
//
// Grid: two address tiles of one height side by side (the house now | what
// Cloudflare lets in), the verdict plate across both, the policies as rows
// of name | where | address | state, then the two buttons. One column on a
// phone. Every part has its final geometry from the first frame.

import { act, send } from "./act.js";
import { refusalCallout } from "./actui.js";
import { agoEl, setAgo } from "./ago.js";
import { fetchJson, h } from "./dom.js";
import { declare, drivable } from "./drivable.js";
import { mountJobPanel } from "./jobpanel.js";
import { jobEnd } from "./pinupdate.js";
import { REFRESH_MS, changeGate } from "./quiet.js";
import { attachDisabledReasons } from "/static/kp/js/reason.js";

const UPDATE = declare({
  id: "home-address-update",
  page: "settings",
  opens: "run",
  what: "write the house's current public address into the Cloudflare Access bypass policies that hold the old one (a job: progress and result show below the button)",
});
const CHECK = declare({
  id: "home-address-check",
  page: "settings",
  opens: "view",
  what: "read the house's address and the Cloudflare policies again now",
});

const TOKEN = declare({
  id: "home-address-token",
  page: "settings",
  opens: "view",
  what: "go to stack admin's secrets to change the Cloudflare API token or account id this section uses (HOMELAB_ADMIN_CLOUDFLARE_ACCESS_TOKEN, HOMELAB_ADMIN_CLOUDFLARE_ACCOUNT_ID in admin.env)",
});

/**
 * @typedef {{name: string, place: string, address: string, current: boolean}}
 *   PolicyRow
 * @typedef {{state: "match" | "differs" | "unknown" | "not_configured" |
 *   "no_policy", current: string | null, held: string[],
 *   policies: PolicyRow[], message: string, configure: string | null,
 *   can_update: boolean, why_not: string | null, lookup_url: string,
 *   read_at: number, token_note?: string | null, fix_steps?: string[]}} HomeAddress
 */

/** How often the section reads again while it is open, in ms. */
const EVERY_MS = REFRESH_MS;

/**
 * The verdict plate's tone and heading for a state.
 * @param {HomeAddress["state"]} state
 * @returns {{tone: "success" | "warning" | "destructive" | "info", title: string}}
 */
export function verdict(state) {
  switch (state) {
    case "match":
      return { tone: "success", title: "The addresses match" };
    case "differs":
      return { tone: "destructive", title: "The addresses differ" };
    case "no_policy":
      return { tone: "warning", title: "Nothing to compare with" };
    case "not_configured":
      return { tone: "warning", title: "Cloudflare is not set up" };
    default:
      return { tone: "info", title: "Not read yet" };
  }
}

/**
 * The section's body.
 * @param {HTMLElement} box
 * @param {(what: string, fail: import("./doctor.js").RouteError | null,
 *   retry: () => void) => void} report the page's one failure alert
 * @returns {() => void} stop
 */
export function mountHomeAddress(box, report) {
  const abort = new AbortController();
  /** @type {(() => void)[]} */
  const stops = [() => abort.abort()];

  const now = h("strong", { class: "ha-addr", id: "ha-now" }, "Reading…");
  const nowNote = h(
    "small",
    { class: "measured", id: "ha-now-note" },
    "Asking the address service",
  );
  const held = h("strong", { class: "ha-addr", id: "ha-held" }, "Reading…");
  const heldNote = h(
    "small",
    { class: "measured", id: "ha-held-note" },
    "Asking Cloudflare",
  );
  const tile = (
    /** @type {string} */ head,
    /** @type {HTMLElement} */ big,
    /** @type {HTMLElement} */ note,
  ) =>
    h(
      "div",
      { class: "ha-tile" },
      h("span", { class: "ha-tile__head" }, head),
      big,
      note,
    );
  const plate = h("div", {
    class: "kp-alert kp-alert--info ha-plate",
    role: "status",
  });
  const list = h("div", {
    class: "ha-rows",
    role: "table",
    "aria-label": "Cloudflare bypass policies that hold a single address",
  });
  const read = agoEl("read", null, { live: true });

  const update = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button",
        id: "ha-update",
        title:
          "Write the house's current address into the Cloudflare policies that hold the old one",
        "data-kp-disabled-reason": "Reading both addresses first",
        disabled: true,
      },
      "Update the home address in Cloudflare",
    ),
    UPDATE,
  );
  const check = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--secondary",
        id: "ha-check",
        title: "Read the house's address and the Cloudflare policies again",
      },
      "Check again",
    ),
    CHECK,
  );
  // eten-home-ip-1 (Kenny, 2026-10-23: "uiteraard moet dat in de settings
  // aangepast kunnen worden"): the token and account id are admin.env
  // secrets, changed where every secret is, with a restart so they load.
  const token = drivable(
    h(
      "a",
      {
        class: "kp-button kp-button--secondary",
        id: "ha-token",
        href: "/stacks/admin/settings?section=secrets",
        title:
          "Change HOMELAB_ADMIN_CLOUDFLARE_ACCESS_TOKEN or HOMELAB_ADMIN_CLOUDFLARE_ACCOUNT_ID in admin.env; tick the restart so the dashboard loads the new value",
      },
      "Change the Cloudflare token",
    ),
    TOKEN,
  );
  const jobSlot = h("div", { class: "ha-job" });
  const result = h("div", { class: "ha-result", role: "status" });

  box.replaceChildren(
    h(
      "div",
      { class: "ha-grid" },
      tile("The house's address now", now, nowNote),
      tile("Cloudflare lets in without a login", held, heldNote),
      plate,
      list,
      h("div", { class: "ha-actions" }, update, check, token, read),
      jobSlot,
      result,
    ),
  );

  const gate = changeGate(["read_at"]);
  /** @type {HomeAddress | null} */
  let last = null;
  let running = false;

  /** @param {HomeAddress} s */
  const paint = (s) => {
    last = s;
    // refresh-one-minute-1: a read with the same content only moves the
    // "read N s ago" line; nothing else is drawn again.
    if (!gate.isNew(s)) {
      setAgo(read, s.read_at);
      return;
    }
    now.textContent = s.current ?? "Unknown";
    nowNote.textContent = s.current
      ? `Asked of ${s.lookup_url}`
      : "The address service did not answer";
    held.textContent = s.held.length ? s.held.join(", ") : "None";
    heldNote.textContent = s.held.length
      ? `${s.policies.length} bypass ${s.policies.length === 1 ? "rule" : "rules"} with a single address`
      : s.state === "not_configured"
        ? "Cloudflare was not asked"
        : "No bypass policy with a single address";
    const v = verdict(s.state);
    plate.className = `kp-alert kp-alert--${v.tone} ha-plate`;
    plate.replaceChildren(
      h(
        "div",
        { class: "kp-alert__body" },
        h("strong", null, v.title),
        h("p", null, s.message),
        s.configure && s.state === "not_configured"
          ? h("p", { class: "measured" }, s.configure)
          : null,
        // eten-home-ip-1: the token's own expiry, told before it bites.
        s.token_note ? h("p", { class: "measured" }, s.token_note) : null,
        // The complete fix when the token is the trouble, one step per row
        // of a two-column grid (number, what to do).
        s.fix_steps?.length
          ? h(
              "ol",
              { class: "ha-steps", "aria-label": "How to fix the token" },
              ...s.fix_steps.map((t, i) =>
                h(
                  "li",
                  { class: "ha-step" },
                  h("span", { class: "ha-step__n" }, String(i + 1)),
                  h("span", { class: "ha-step__t" }, t),
                ),
              ),
            )
          : null,
      ),
    );
    list.replaceChildren(
      h(
        "div",
        { class: "ha-row ha-row--head", role: "row" },
        h("span", { role: "columnheader" }, "Policy"),
        h("span", { role: "columnheader" }, "Where"),
        h("span", { role: "columnheader" }, "Address"),
        h("span", { role: "columnheader" }, "State"),
      ),
      ...s.policies.map((p) =>
        h(
          "div",
          { class: "ha-row", role: "row" },
          h("span", { role: "cell" }, p.name || "Unnamed policy"),
          h("span", { role: "cell" }, p.place),
          h("span", { role: "cell", class: "ha-mono" }, p.address),
          h(
            "span",
            { role: "cell" },
            h(
              "span",
              {
                class: `kp-badge kp-badge--${p.current ? "success" : "destructive"}`,
              },
              p.current ? "Current" : "Old",
            ),
          ),
        ),
      ),
    );
    list.hidden = s.policies.length === 0;
    setAgo(read, s.read_at);
    paintButton();
  };

  const paintButton = () => {
    const b = /** @type {HTMLButtonElement} */ (update);
    const ok = !!last?.can_update && !running;
    b.disabled = !ok;
    if (ok) b.removeAttribute("data-kp-disabled-reason");
    else
      b.setAttribute(
        "data-kp-disabled-reason",
        running
          ? "The update is running"
          : (last?.why_not ?? "Reading both addresses first"),
      );
    stops.push(attachDisabledReasons(b.parentElement ?? box));
  };

  const load = async () => {
    const r = await fetchJson(
      "/data/home-address",
      "the home address",
      abort.signal,
    );
    if (abort.signal.aborted) return;
    if (!r.ok) {
      report("the home address", r.error, () => void load());
      return;
    }
    report("the home address", null, () => void load());
    paint(/** @type {HomeAddress} */ (r.body));
  };

  check.addEventListener("click", () => {
    /** @type {HTMLButtonElement} */ (check).disabled = true;
    void load().finally(() => {
      /** @type {HTMLButtonElement} */ (check).disabled = false;
    });
  });

  update.addEventListener("click", () => {
    if (/** @type {HTMLButtonElement} */ (update).disabled) return;
    running = true;
    paintButton();
    result.replaceChildren();
    void (async () => {
      const r = await send(
        "POST",
        "/data/actions/_host/update-home-address",
        {},
        "the home address update",
      );
      if (!r.ok) {
        running = false;
        paintButton();
        result.replaceChildren(refusalCallout(r.error));
        return;
      }
      const job = /** @type {number} */ (r.body.job);
      const panel = mountJobPanel(job, { compact: true });
      stops.push(panel.stop);
      jobSlot.replaceChildren(panel.element);
      const state = await jobEnd(job);
      running = false;
      const said = act.jobs.find((x) => x.job === job)?.message ?? "";
      result.replaceChildren(
        h(
          "div",
          {
            class: `kp-alert kp-alert--${state === "done" ? "success" : "destructive"}`,
          },
          h(
            "div",
            { class: "kp-alert__body" },
            h(
              "strong",
              null,
              state === "done"
                ? "The home address is updated"
                : "The home address was not updated",
            ),
            h("p", null, said),
          ),
        ),
      );
      await load();
    })().catch(() => {
      running = false;
      paintButton();
    });
  });

  void load();
  const timer = setInterval(() => {
    if (!running && !document.hidden) void load();
  }, EVERY_MS);
  stops.push(() => clearInterval(timer));
  return () => stops.forEach((f) => f());
}
