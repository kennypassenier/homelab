// feat-overview-10: the backup calendar page. Its own module (`../js/backupcalendar.js`
// for the pure grid math) and its own route (`/data/backup-calendar`, a
// dedicated `BackupCalendar` host command) — kept apart from the Backups
// page another helper is building in the same milestone, so the two merge
// without either touching the other's files.

import { calendarDays, calendarWeeks } from "../backupcalendar.js";
import { errorBox, fetchJson, h, slowRead } from "../dom.js";
import { formatDateTime } from "../format.js";

const WINDOW_DAYS = 35;
const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const grid = h("div", {
    class: "backup-cal",
    "aria-label": "Backup calendar",
  });
  const status = h(
    "p",
    { class: "measured", role: "status" },
    "Reading restic through the host…",
  );
  const refresh = h(
    "button",
    { type: "button", class: "kp-button kp-button--primary" },
    "Refresh",
  );
  root.replaceChildren(
    h("div", { class: "title-row" }, h("h1", null, "Backup calendar"), refresh),
    h(
      "p",
      { class: "page-intro" },
      `The last ${WINDOW_DAYS} nights, one cell per day: every stack that keeps data is expected to have at least one restic snapshot that night. Reads restic directly over the network, so this can take a while on a slow link.`,
    ),
    h(
      "div",
      { class: "backup-cal__legend" },
      h("span", { class: "backup-cal__cell backup-cal__cell--ok" }, ""),
      "every stack  ",
      h("span", { class: "backup-cal__cell backup-cal__cell--warn" }, ""),
      "some stacks  ",
      h("span", { class: "backup-cal__cell backup-cal__cell--bad" }, ""),
      "no stack  ",
      h("span", { class: "backup-cal__cell backup-cal__cell--muted" }, ""),
      "no stack keeps data",
    ),
    grid,
    status,
  );
  const abort = new AbortController();

  const load = async () => {
    status.textContent = "Reading restic through the host…";
    grid.replaceChildren();
    const r = await slowRead(
      "/data/backup-calendar",
      "the backup calendar",
      abort.signal,
    );
    if (abort.signal.aborted) return;
    if (!r.ok) {
      grid.replaceChildren(errorBox(r.error));
      status.textContent = "";
      return;
    }
    const stacks = /** @type {Record<string, number[]>} */ (
      r.body.stacks ?? {}
    );
    const expected = Object.keys(stacks).sort();
    const now = r.body.measured_at ?? Math.floor(Date.now() / 1000);
    const days = calendarDays(stacks, expected, WINDOW_DAYS, now);
    const weeks = calendarWeeks(days);
    grid.replaceChildren(
      h(
        "div",
        { class: "backup-cal__grid" },
        ...WEEKDAYS.map((w) => h("div", { class: "backup-cal__head" }, w)),
        ...weeks.flatMap((week) =>
          week.map((d) => {
            if (!d)
              return h("div", {
                class: "backup-cal__cell backup-cal__cell--pad",
              });
            const label = d.expected.length
              ? `${d.date}: ${d.backed_up.length}/${d.expected.length} stacks backed up${d.missing.length ? ` — missing: ${d.missing.join(", ")}` : ""}`
              : `${d.date}: no stack keeps data`;
            return h(
              "div",
              {
                class: `backup-cal__cell backup-cal__cell--${d.tone}`,
                title: label,
                "aria-label": label,
              },
              d.date.slice(8),
            );
          }),
        ),
      ),
    );
    status.textContent = `${expected.length} stacks · measured ${formatDateTime(now)}${r.body.skipped?.length ? ` · skipped: ${r.body.skipped.join("; ")}` : ""}`;
  };

  refresh.addEventListener("click", () => void load().catch(() => {}));
  void load().catch(() => {});
  return () => abort.abort();
}
