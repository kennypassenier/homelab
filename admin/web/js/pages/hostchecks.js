// The Host page's Host checks card (feat-kp11demo-11): the doctor's checks
// as a kp status list, and the "when something goes wrong" list under it.

import { STANDING_STACK } from "../checks.js";
import { doctorRows } from "../doctor.js";
import { declare } from "../drivable.js";
import { h, slowReport } from "../dom.js";
import { humanDuration } from "../format.js";
import { hostChecks } from "../host.js";
import { section } from "../ui.js";
import { mountWhenCheck } from "../whencheck.js";
import {
  attachStatusLists,
  buildStatusList,
  CARD_RETRY_EVENT,
  countStatus,
  setCardState,
} from "/static/kp/dist/kp-themes.min.js";

const CHECK_CONFIRM = declare({
  id: "host-check-confirm",
  page: "host",
  opens: "run",
  row: "<check id>",
  what: "keep today's date as the last time the stack-less check (the restic password chain) was confirmed (answer-check with ok); nothing else changes",
  shows: 'on the Host page\'s "When something goes wrong, check this" list',
});

/**
 * @param {import("./hostenv.js").HostEnv} env
 * @param {{runChecks: HTMLElement}} header `runChecks`: the header's button, disabled during a read
 */
export function createChecks(env, { runChecks }) {
  const { abort } = env;
  // The doctor's checks as a kp status list (feat-kp11demo-11): a mark, the
  // check, its detail and, under a warning or a failure, the remedy the host
  // already computed. The read takes about 30 s: the card lays the theme's
  // busy card over the list with the seconds it has run; no progress is
  // invented, because the host reports none.
  const checksNote = h("span", { role: "status", id: "host-checks-note" });
  let checksList = h("ul", { class: "kp-statuslist", id: "host-checks" });
  const checksKp = h(
    "section",
    {
      class: "kp-card hk-checkscard",
      "data-kp-card": "plain",
      "data-kp-state": "empty",
      "aria-label": "Host checks",
    },
    h(
      "div",
      { class: "kp-card__stage" },
      h("div", { class: "kp-card__data" }, checksList),
    ),
  );
  const checksCard = section({
    id: "host-checks-card",
    title: "Host checks",
    desc: "The doctor's checks about the host itself: disk, state file, clock, certificates.",
    foot: [checksNote],
  });
  // The count line lives in the card's foot and shows only when it has words.
  const setNote = (/** @type {string} */ text) => {
    checksNote.textContent = text;
    checksCard.foot.hidden = !text;
  };
  setNote("");
  checksCard.body.append(checksKp);
  const notRead = () =>
    setCardState(checksKp, "empty", {
      title: "Not read yet",
      text: "Reading takes about 30 s on pve; the result stays here until you leave.",
      action: null,
    });
  notRead();
  /** @param {any} report */
  const paintChecks = (report) => {
    const rows = doctorRows({ ...report, checks: hostChecks(report) });
    const next = buildStatusList(
      rows.map((x) => ({
        id: x.name,
        label: x.name,
        state:
          x.health.tone === "ok"
            ? "ok"
            : x.health.tone === "warn"
              ? "warn"
              : "failed",
        detail: x.detail,
        fix: x.remedy || "",
      })),
      { label: "Host checks", retry: false },
    );
    next.id = "host-checks";
    checksList.replaceWith(next);
    checksList = next;
    attachStatusLists(checksKp);
    const c = countStatus(checksList);
    setNote(
      rows.length
        ? `${c.ok.toLocaleString("en-GB")} of ${c.total.toLocaleString("en-GB")} checks pass, ${c.warn.toLocaleString("en-GB")} with a reservation, ${c.failed.toLocaleString("en-GB")} failed`
        : "",
    );
    setCardState(checksKp, rows.length ? "ready" : "empty", {
      title: "The host has no checks yet",
      text: "The first read after the host starts fills this list.",
    });
    return rows.length;
  };
  let checking = false;
  const loadChecks = async () => {
    if (checking) return;
    checking = true;
    runChecks.setAttribute("disabled", "");
    const began = Date.now();
    let haveRows = false;
    // The theme's own busy card carries the wait; the host reports no
    // progress, so none is drawn and no words are counted out.
    setCardState(checksKp, "loading");
    setNote("");
    try {
      const r = await slowReport(
        "/data/doctor",
        "the doctor",
        abort.signal,
        (b) => {
          if (!hostChecks(b.report).length) return;
          paintChecks(b.report);
          haveRows = true;
        },
      );
      const secs = Math.round((Date.now() - began) / 1000);
      if (!r.ok) {
        setCardState(checksKp, "error", {
          title: "The doctor did not answer",
          text: `After ${humanDuration(secs)}: ${r.error.why}`,
        });
        setNote("");
        return;
      }
      const n = paintChecks(r.report);
      if (checksNote.textContent)
        setNote(`${checksNote.textContent}, read in ${humanDuration(secs)}.`);
      void n;
    } finally {
      checking = false;
      runChecks.removeAttribute("disabled");
    }
  };
  checksKp.addEventListener(
    CARD_RETRY_EVENT,
    () => void loadChecks().catch(() => {}),
  );
  return { card: checksCard, load: loadChecks };
}

/** manual-checks-as-info-1: the stack-less standing check (the restic password chain), information for when something goes wrong. */
export function createWhenWrong(
  /** @type {import("./hostenv.js").HostEnv} */ env,
) {
  const whenWrong = mountWhenCheck({
    stack: STANDING_STACK,
    id: "host-when-wrong",
    drive: CHECK_CONFIRM,
    signal: env.abort.signal,
    emptyText: "No check that only a person can look at is registered yet.",
  });
  env.stops.push(whenWrong.stop);
  return whenWrong;
}
