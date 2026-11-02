// Reading host.toml: the first read, background refreshes that draw nothing
// when the file is the same, and a read that arrives while changes are staged.

import { setAgo } from "../../ago.js";
import { fetchJson } from "../../dom.js";
import { changeGate } from "../../quiet.js";
import {
  setCardState,
  setProblems,
  unsaved,
} from "/static/kp/dist/kp-themes.min.js";
import { drawGroups, sectionJumper } from "./groups.js";

/**
 * @param {import("./ctx.js").Ctx} ctx
 * @param {() => void} paintSide
 * @returns {(quiet?: boolean) => Promise<boolean>}
 */
export function makeLoadHost(ctx, paintSide) {
  const hostGate = changeGate();
  const goToSection = sectionJumper(ctx);
  /** The bar and the groups' package parts are attached before the page is called ready. */
  const settle = async () => {
    if (!ctx.detachBar) await ctx.attachBar();
  };
  /**
   * @param {boolean} [quiet] a background refresh: a read that brings the
   *   same file draws nothing again
   * @returns {Promise<boolean>}
   */
  const loadHost = async (quiet = false) => {
    // A read that arrives while changes wait must not wipe them.
    const pending = ctx.groupBox.querySelectorAll("[data-kp-changed]").length;
    if (ctx.page && pending > 0 && ctx.groupBox.children.length) {
      ctx.stale = true;
      setProblems(ctx.band, [
        {
          key: "settings-stale",
          severity: "warning",
          what: "host.toml changed on the host while you were editing",
          why: "Your changes are kept. Save will be refused, because the file is not the one this page read.",
          remedy:
            "Discard the changes to read the new file, or copy them down.",
        },
      ]);
      return true;
    }
    const r = await fetchJson(
      "/data/host-settings",
      "the host settings",
      ctx.abort.signal,
    );
    if (!r.ok) {
      setCardState(ctx.hsCard.card, ctx.page ? "ready" : "error", {
        title: "host.toml could not be read",
        text: r.error.why,
      });
      ctx.failed("the host settings", r.error, () => void loadHost());
      return false;
    }
    ctx.failed("the host settings", null, () => void loadHost());
    const same = !hostGate.isNew(r.body.page);
    if (quiet && same && ctx.page) {
      setAgo(ctx.ago, r.body.measured_at ?? Date.now() / 1000);
      return true;
    }
    const first = ctx.page === null;
    ctx.page = r.body.page;
    paintSide();
    await drawGroups(ctx);
    await settle();
    setCardState(ctx.hsCard.card, "ready");
    // The groups just drawn are the saved state, except for what the bar
    // gave back from this tab (kp keeps staged changes for the tab and puts
    // them back as the bar attaches): marking those clean would drop them.
    if (!ctx.groupBox.querySelector("[data-kp-changed]"))
      unsaved(ctx.barSlot)?.markClean();
    setAgo(ctx.ago, r.body.measured_at ?? Date.now() / 1000);
    ctx.refreshReadLine();
    if (first) goToSection();
    return true;
  };
  return loadHost;
}
