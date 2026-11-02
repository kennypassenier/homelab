// The Save bar: attaching the kit's bar, collecting what changed, Save
// (one PUT for the whole host.toml) and what follows a write.

import { send } from "../../act.js";
import { notify, refusalAlarm, refusalCallout } from "../../actui.js";
import { driven } from "../../drivehooks.js";
import { drivable } from "../../drivable.js";
import { hostSettingsBody } from "../../editforms.js";
import { valueFromMirror } from "../../settingsrows.js";
import {
  attachAll,
  unsaved,
  unsavedSaved,
} from "/static/kp/dist/kp-themes.min.js";
import { CHECK_WRITE, DISCARD, STAGED } from "./controls.js";
import { syncConfirm } from "./groups.js";
import { textOf } from "./rowtext.js";

/** @typedef {import("./ctx.js").HostField} HostField */

/** @param {import("./ctx.js").Ctx} ctx */
async function attachBar(ctx) {
  const off = attachAll(ctx.hostBar);
  await off.ready;
  ctx.detachBar = off;
  const bar = unsaved(ctx.barSlot);
  void bar;
  ctx.saveBtn = ctx.barSlot.querySelector(".kp-unsaved__save");
  const discardBtn = ctx.barSlot.querySelector(".kp-unsaved__discard");
  const chipBtn = ctx.barSlot.querySelector(".kp-unsaved__count");
  if (ctx.saveBtn) drivable(ctx.saveBtn, CHECK_WRITE);
  if (discardBtn) drivable(/** @type {HTMLElement} */ (discardBtn), DISCARD);
  if (chipBtn) drivable(/** @type {HTMLElement} */ (chipBtn), STAGED);
}

/** The field of a row. @param {import("./ctx.js").Ctx} ctx @param {Element} row */
const fieldOfRow = (ctx, row) =>
  ctx.page?.fields.find((f) => f.key === row.getAttribute("data-key"));

/**
 * What the changed rows stage, or why a row is refused.
 * @param {import("./ctx.js").Ctx} ctx
 * @param {{key: string, row: HTMLElement}[]} changes
 */
function collect(ctx, changes) {
  /** @type {Record<string, string>} */
  const refused = {};
  /** @type {Map<string, {field: HostField, value: unknown}>} */
  const staged = new Map();
  const confirmed = new Set();
  for (const c of changes) {
    const f = fieldOfRow(ctx, c.row);
    if (!f) continue;
    const p = valueFromMirror(f, textOf(c.row, f));
    if (!p.ok) {
      refused[c.key] = p.why;
      continue;
    }
    if (f.access === "confirm") {
      if (!ctx.typed.has(f.key)) {
        refused[c.key] = `Type ${f.key} to confirm this change.`;
        continue;
      }
      confirmed.add(f.key);
    }
    staged.set(f.key, { field: f, value: p.value });
  }
  return { refused, staged, confirmed };
}

/**
 * Save: the changes become the values the host takes; a key that needs its
 * name typed waits for it; the host answers once for the whole file.
 * @param {import("./ctx.js").Ctx} ctx
 * @param {CustomEvent} e
 */
async function onSave(ctx, e) {
  const changes = /** @type {{key: string, row: HTMLElement}[]} */ (
    e.detail.changes
  );
  const { refused, staged, confirmed } = collect(ctx, changes);
  if (Object.keys(refused).length) {
    unsavedSaved(ctx.barSlot, { refused });
    return;
  }
  // Live view: host.toml is written on the dashboard's server, once.
  if (driven() || !ctx.page) {
    unsavedSaved(ctx.barSlot, {});
    return;
  }
  const r = await send(
    "PUT",
    "/data/host-settings",
    hostSettingsBody(ctx.page.sha256, staged, confirmed),
    "the host settings",
  );
  if (!r.ok) {
    /** @type {Record<string, string>} */
    const all = {};
    for (const c of changes) all[c.key] = r.error.why;
    unsavedSaved(ctx.barSlot, { refused: all });
    ctx.errBox.replaceChildren(refusalCallout(r.error));
    await refusalAlarm(r.error, r.status);
    return;
  }
  ctx.errBox.replaceChildren();
  unsavedSaved(ctx.barSlot, {});
  ctx.saved(r.body);
}

/**
 * host.toml was written: say so, and read it again.
 * @param {import("./ctx.js").Ctx} ctx @param {any} answer
 */
function saved(ctx, answer) {
  const s = answer.saved ?? {};
  const later = /** @type {string[]} */ (s.restart ?? []);
  const job = answer.follow?.job;
  const restarting = typeof job === "number";
  notify(
    restarting
      ? "host.toml written; restarting homelab-host.service…"
      : `host.toml written.${later.length ? ` At the host's next start: ${later.join(", ")}.` : " In force now."}`,
    "success",
    restarting
      ? {
          label: "Open the job",
          onClick: () => {
            history.pushState(null, "", `/activity?view=running&job=${job}`);
            dispatchEvent(new PopStateEvent("popstate"));
          },
        }
      : undefined,
  );
  if (answer.follow?.refused)
    ctx.errBox.replaceChildren(
      refusalCallout(answer.follow.refused, "warning", "Not restarted"),
    );
  ctx.stale = false;
  void ctx.loadHost();
}

/**
 * Wire the bar and the typed-name confirmations; the Save press and the
 * confirm events are listened to here.
 * @param {import("./ctx.js").Ctx} ctx
 */
export function mountSaveBar(ctx) {
  ctx.attachBar = () => attachBar(ctx);
  ctx.saved = (answer) => saved(ctx, answer);
  const sync = () => syncConfirm(ctx);
  ctx.groupBox.addEventListener("kp-unsaved-change", sync);
  ctx.groupBox.addEventListener("kp-type-confirm", (e) => {
    const d = /** @type {CustomEvent} */ (e).detail;
    const key = /** @type {Element} */ (e.target)
      .closest("[data-key]")
      ?.getAttribute("data-key");
    if (!key) return;
    if (d.matched) ctx.typed.add(key);
    else ctx.typed.delete(key);
  });
  ctx.barSlot.addEventListener("kp-unsaved-change", sync);
  ctx.barSlot.addEventListener(
    "kp-unsaved-save",
    (e) => void onSave(ctx, /** @type {CustomEvent} */ (e)),
  );
}
