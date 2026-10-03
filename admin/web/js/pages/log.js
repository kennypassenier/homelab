// redesign-console (3.71.0, the Console demo Kenny approved on 2026-10-03):
// the host-log explorer. Every line the host prints for every operation,
// whoever started it (this dashboard, a CLI or TUI on a workstation, the
// nightly round), live. Console shows it with the shell under it; Activity
// shows it as its Host log view (`/activity?view=host-log`; the old `/log`
// redirects there).
//
// A log explorer, so the filters are a side column (DESIGN_LANGUAGE §12):
// Source (each a plain-click toggle with its count, "only" beside it) and
// Level. The toolbar above the lines keeps its three zones (fix for
// Kenny's "alle items lijken gewoon links aligned naast elkaar gezet"):
// "Lines containing" at its own width on the left, the count, the follow
// switch and the key hints against the right edge. The lines scroll inside
// their own region; the page never grows with them.

import { act } from "../act.js";
import { ownerOf } from "../activityview.js";
import { fetchJson, h } from "../dom.js";
import { drivable } from "../drivable.js";
import {
  LEVELS,
  counts,
  countText,
  keyAction,
  lineShown,
  logFilterFromParams,
  logFilterToParams,
  mergeLines,
  narrowed,
  sourceLabel,
  sourceList,
  sourceOn,
  toggleSource,
} from "../hostlogview.js";
import { logLine } from "../jobs.js";
import { openJobDialog } from "../jobpanel.js";
import { transferView } from "../parity.js";
import { current, listen, subscribe } from "../store.js";
import { emptyState, kbd, stackMark, toolbar } from "../ui.js";
import { setParams } from "../urlstate.js";

/** Lines kept in the page, as the server's ring. */
const KEEP = 2000;

/**
 * @typedef {{follow: string, source: string, only: string, level: string,
 *   reset: string, line: string, openJob: string, copy: string,
 *   everything: string, onlyLine: string, newLines: string,
 *   search: string, retry: string, download?: string}} DriveIds the Live
 *   view controls the host page
 *   declared for this explorer (drivable.js)
 * @typedef {{drive: DriveIds, followInBar: boolean, foot?: HTMLElement,
 *   onFollow?: (on: boolean) => void, label: string}} ExplorerOpts
 */

/**
 * @param {HTMLElement} root
 * @param {ExplorerOpts} opts
 * @returns {{cleanup: () => void, setFollow: (on: boolean) => void,
 *   following: () => boolean, download: () => void,
 *   addBlock: (el: HTMLElement) => void, focusSearch: () => void}}
 */
export function mountHostLog(root, opts) {
  const params = new URLSearchParams(location.search);
  let f = logFilterFromParams(params);
  /** @type {import("../parity.js").HostLine[]} */
  let lines = [];
  /** @type {Set<string>} */
  const seen = new Set();
  let lastSeq = 0;
  /** Live lines that arrived while the snapshot was being read. */
  /** @type {import("../parity.js").HostLine[]} */
  let early = [];
  let loaded = false;
  /** @type {string | null} */
  let failure = null;
  let follow = true;
  let unseen = 0;
  /** @type {number | null} */
  let open = null;

  const side = h("aside", {
    class: "hl-side",
    "aria-label": "Which lines",
  });
  const countEl = h("span", { class: "hl-count", role: "status" });
  const followBtn = drivable(
    h("button", {
      type: "button",
      class: "kp-button kp-button--ghost kp-button--sm hl-follow",
      title: "Stop following: new lines wait below a marker (Space)",
    }),
    opts.drive.follow,
  );
  followBtn.addEventListener("click", () => setFollow(!follow));
  const hints = h(
    "span",
    { class: "hl-hints" },
    kbd("Space"),
    " pause · ",
    kbd("End"),
    " tail",
  );
  const tb = toolbar({
    search: {
      placeholder: "Lines containing…",
      label: "Lines containing",
      value: f.q,
      onInput: (q) => {
        f = { ...f, q };
        changed();
      },
    },
    groups: [],
    state: [countEl, ...(opts.followInBar ? [followBtn] : []), hints],
  });
  tb.el.classList.add("hl-bar");
  if (tb.search) drivable(tb.search, opts.drive.search);
  const transfers = h("div", { class: "hl-xfers" });
  const list = h("div", { class: "hl-lines" });
  const blocks = h("div", { class: "hl-blocks" });
  const newPill = h("button", {
    type: "button",
    class: "kp-button kp-button--sm kp-button--primary hl-newpill",
    hidden: "",
  });
  drivable(newPill, opts.drive.newLines);
  newPill.addEventListener("click", () => setFollow(true));
  const out = h(
    "div",
    {
      class: "hl-out",
      role: "log",
      "aria-live": "polite",
      "aria-label": opts.label,
      tabindex: "0",
      id: "host-log",
    },
    transfers,
    list,
    blocks,
  );
  const term = h(
    "section",
    { class: "hl-term", "aria-label": opts.label },
    tb.el,
    h("div", { class: "hl-outwrap" }, out, newPill),
    ...(opts.foot ? [opts.foot] : []),
  );
  root.replaceChildren(h("div", { class: "hl" }, side, term));

  const fleetNames = () => (current().fleet?.stacks ?? []).map((s) => s.name);
  const allSources = () => sourceList(fleetNames(), seen);

  const toUrl = () =>
    history.replaceState(
      history.state,
      "",
      location.pathname + setParams(location.search, logFilterToParams(f)),
    );

  /** @param {string} s */
  const swatch = (s) =>
    s === "HOST" || !fleetNames().includes(s)
      ? h("span", { class: "hl-sw", "aria-hidden": "true" })
      : stackMark(s, 12);

  // The side column is built once and updated in place (senior review,
  // finding 3): rebuilding it for every arriving line took keyboard focus
  // away and made the "only" link flicker. A source seen for the first
  // time gets its row inserted at its place; nothing else moves.
  const everythingIn = /** @type {HTMLInputElement} */ (
    drivable(h("input", { type: "checkbox" }), opts.drive.everything)
  );
  everythingIn.addEventListener("change", () => {
    f = { ...f, off: new Set(), only: null };
    changed();
  });
  const everythingCount = h("small");
  const srcOpts = h(
    "div",
    { class: "hl-opts" },
    h(
      "label",
      { class: "hl-opt", title: "Show every source again" },
      everythingIn,
      h("span"),
      h("span", null, "Everything"),
      everythingCount,
    ),
  );
  /** @type {Map<string, {label: HTMLElement, input: HTMLInputElement,
   *   count: HTMLElement}>} */
  const srcRows = new Map();
  /** @param {string} s */
  const sourceRow = (s) => {
    const input = /** @type {HTMLInputElement} */ (
      drivable(
        h("input", {
          type: "checkbox",
          "aria-label": `Show ${sourceLabel(s)}`,
        }),
        opts.drive.source,
        s,
      )
    );
    input.addEventListener("change", () => {
      f = toggleSource(f, s, allSources());
      changed();
    });
    const only = drivable(
      h(
        "button",
        {
          type: "button",
          class: "hl-only",
          title: `Show only ${sourceLabel(s)}`,
        },
        "only",
      ),
      opts.drive.only,
      s,
    );
    only.addEventListener("click", (e) => {
      e.preventDefault();
      f = { ...f, only: s, off: new Set() };
      changed();
    });
    const count = h("small");
    const label = h(
      "label",
      {
        class: "hl-opt",
        title: `Show or hide ${s === "HOST" ? "the host's own" : `${s}'s`} lines; each click turns it on or off`,
      },
      input,
      swatch(s),
      h("span", { class: "hl-opt__name" }, sourceLabel(s), " ", only),
      count,
    );
    return { label, input, count };
  };
  const levelRows = LEVELS.map((lv) => {
    const input = /** @type {HTMLInputElement} */ (
      drivable(
        h("input", { type: "radio", name: `hl-level-${opts.drive.level}` }),
        opts.drive.level,
        lv.value || "info",
      )
    );
    input.addEventListener("change", () => {
      f = { ...f, level: /** @type {any} */ (lv.value) };
      changed();
    });
    const count = h("small");
    const label = h(
      "label",
      { class: "hl-opt", title: `Show ${lv.label.toLowerCase()}` },
      input,
      h("span", { class: `hl-dot hl-dot--${lv.dot}`, "aria-hidden": "true" }),
      h("span", null, lv.label),
      count,
    );
    return { value: lv.value, input, count, label };
  });
  const reset = drivable(
    h(
      "button",
      { type: "button", class: "kp-button kp-button--ghost kp-button--sm" },
      "Reset the filters",
    ),
    opts.drive.reset,
  );
  reset.addEventListener("click", () => {
    f = { off: new Set(), only: null, level: "", q: "" };
    if (tb.search) tb.search.value = "";
    changed();
  });
  // The demo's words: the hint while nothing is narrowed, the reset
  // while something is (senior review, finding 16).
  const hint = h("p", { class: "hl-hint" }, "Hover a source for “only”.");
  side.replaceChildren(
    h("div", { class: "hl-group" }, h("h3", null, "Source"), srcOpts),
    h(
      "div",
      { class: "hl-group" },
      h("h3", null, "Level"),
      h("div", { class: "hl-opts" }, ...levelRows.map((r) => r.label)),
    ),
    h("div", { class: "hl-sidefoot" }, reset, hint),
  );

  const paintSide = () => {
    const c = counts(lines);
    const srcs = allSources();
    srcs.forEach((s, i) => {
      if (srcRows.has(s)) return;
      const row = sourceRow(s);
      srcRows.set(s, row);
      // After the "Everything" row, before the next source already drawn.
      const next = srcs.slice(i + 1).find((n) => srcRows.has(n) && n !== s);
      srcOpts.insertBefore(
        row.label,
        next ? (srcRows.get(next)?.label ?? null) : null,
      );
    });
    everythingIn.checked = f.only == null && f.off.size === 0;
    everythingCount.textContent = String(lines.length);
    for (const [s, row] of srcRows) {
      const on = sourceOn(s, f);
      row.input.checked = on;
      row.label.classList.toggle("hl-opt--off", !on);
      row.count.textContent = String(c.bySource.get(s) ?? 0);
    }
    for (const r of levelRows) {
      r.input.checked = f.level === r.value;
      r.count.textContent = String(c.byLevel[r.value]);
    }
    const n = narrowed(f);
    reset.hidden = !n;
    hint.hidden = n;
  };

  /** @param {import("../parity.js").HostLine} l */
  const lineEls = (l, fresh = false) => {
    const x = logLine(/** @type {any} */ ({ ...l, job: 0 }));
    const isOpen = open === l.seq;
    const tone =
      x.severity === "error" ? "error" : x.severity === "warning" ? "warn" : "";
    const row = drivable(
      h(
        "div",
        {
          class: `hl-ln${tone ? ` hl-ln--${tone}` : ""}${fresh ? " is-new" : ""}`,
          role: "button",
          tabindex: "0",
          "aria-expanded": String(isOpen),
          title: "Click for when, who and the request it belongs to",
          "data-seq": String(l.seq),
        },
        h("time", null, x.time),
        h("span", { class: `hl-lv hl-lv--${tone || "info"}` }, x.level),
        h("span", { class: "hl-src" }, swatch(l.source), sourceLabel(l.source)),
        h(
          "span",
          { class: "hl-msg" },
          ...mark(l.msg),
          l.by
            ? h(
                "em",
                null,
                `  · ${l.by === "admin" ? "this dashboard" : ownerOf(l.by)}`,
              )
            : "",
        ),
      ),
      opts.drive.line,
      String(l.seq),
    );
    const toggle = () => {
      open = isOpen ? null : l.seq;
      paintLines();
      // The repainted row keeps focus, so the next key reaches it.
      /** @type {HTMLElement | null} */ (
        list.querySelector(`.hl-ln[data-seq="${l.seq}"]`)
      )?.focus({ preventScroll: true });
    };
    row.addEventListener("click", toggle);
    row.addEventListener("keydown", (e) => {
      // A role=button row answers Space like Enter (finding 12).
      if (e.target === row && (e.key === "Enter" || e.key === " ")) {
        e.preventDefault();
        toggle();
      }
    });
    if (!isOpen) return [row];
    const job =
      l.req == null
        ? null
        : act.jobs.find((j) => j.reqs?.includes(l.req ?? -1));
    const onlyBtn = h(
      "button",
      { type: "button", class: "kp-button kp-button--sm" },
      `Only ${sourceLabel(l.source)}`,
    );
    drivable(onlyBtn, opts.drive.onlyLine);
    onlyBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      f = { ...f, only: l.source, off: new Set() };
      open = null;
      changed();
    });
    const copy = drivable(
      h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--sm kp-button--ghost",
          title: "Copy this line to the clipboard",
        },
        "Copy line",
      ),
      opts.drive.copy,
    );
    copy.addEventListener("click", (e) => {
      e.stopPropagation();
      void navigator.clipboard
        ?.writeText(`${x.time} ${x.level} ${l.source} ${l.msg}`)
        .then(() => (copy.textContent = "Copied"))
        .catch(() => (copy.textContent = "Press Ctrl C"))
        // Back to its own name after a moment (finding 14).
        .finally(() =>
          setTimeout(() => {
            if (copy.isConnected) copy.textContent = "Copy line";
          }, 1500),
        );
    });
    const more = h(
      "div",
      { class: "hl-more" },
      h(
        "dl",
        { class: "hl-kv" },
        h("dt", null, "When"),
        h("dd", null, new Date(l.ts * 1000).toString().slice(0, 24)),
        h("dt", null, "Started by"),
        h(
          "dd",
          null,
          l.by
            ? l.by === "admin"
              ? "Kenny, from this dashboard"
              : `${ownerOf(l.by)} (the token “${l.by}”)`
            : "the host (the nightly round or its own work)",
        ),
        h("dt", null, "Line"),
        h(
          "dd",
          { class: "mono" },
          `#${l.seq}${l.req != null ? ` · request ${l.req}` : ""}`,
        ),
      ),
      h(
        "div",
        { class: "hl-more__acts" },
        onlyBtn,
        ...(job
          ? [
              (() => {
                const b = drivable(
                  h(
                    "button",
                    {
                      type: "button",
                      class: "kp-button kp-button--sm kp-button--ghost",
                      title: "Open the job this line belongs to",
                    },
                    "Open the job",
                  ),
                  opts.drive.openJob,
                  String(job.job),
                );
                b.addEventListener("click", (e) => {
                  e.stopPropagation();
                  openJobDialog(job.job);
                });
                return b;
              })(),
            ]
          : []),
        copy,
      ),
    );
    return [row, more];
  };

  /** @param {string} text @returns {(string | Node)[]} */
  const mark = (text) => {
    const q = f.q.trim().toLowerCase();
    if (!q) return [text];
    const i = text.toLowerCase().indexOf(q);
    if (i < 0) return [text];
    return [
      text.slice(0, i),
      h("mark", null, text.slice(i, i + q.length)),
      text.slice(i + q.length),
    ];
  };

  const shownLines = () => lines.filter((l) => lineShown(l, f));

  const paintCount = () => {
    countEl.textContent = loaded
      ? countText(shownLines().length, lines.length)
      : "Reading the lines…";
    followBtn.textContent = follow ? "Pause" : "Resume";
    followBtn.title = follow
      ? "Stop following: new lines wait below a marker (Space)"
      : "Follow the newest lines again (Space)";
    followBtn.setAttribute("aria-pressed", String(!follow));
    newPill.hidden = follow || unseen === 0;
    newPill.textContent = `${unseen} new ${unseen === 1 ? "line" : "lines"} · back to the tail`;
    root.dataset.following = String(follow);
  };

  const toTail = () => {
    out.scrollTop = out.scrollHeight;
  };

  const paintLines = () => {
    if (!loaded) {
      list.replaceChildren(
        ...Array.from({ length: 12 }, () =>
          h(
            "div",
            { class: "hl-ln hl-ln--sk", "data-kp-state": "loading" },
            ...[80, 60, 80, 70].map((w) =>
              h("span", { class: "kp-skeleton", style: `inline-size:${w}%` }),
            ),
          ),
        ),
      );
    } else if (failure) {
      const again = h(
        "button",
        { type: "button", class: "kp-button kp-button--sm" },
        "Try again",
      );
      drivable(again, opts.drive.retry);
      again.addEventListener("click", () => void read());
      list.replaceChildren(
        h(
          "div",
          { class: "kp-alert kp-alert--destructive hl-error", role: "alert" },
          h(
            "span",
            { class: "kp-alert__body" },
            `The host's lines could not be read: ${failure}`,
          ),
          again,
        ),
      );
    } else {
      const v = shownLines();
      list.replaceChildren(
        ...(v.length
          ? v.flatMap((l) => lineEls(l))
          : [
              emptyState(
                lines.length
                  ? {
                      title: "No line matches",
                      text: "Lines that match the filter appear here as they arrive.",
                    }
                  : {
                      title: "No lines yet",
                      text: "The host has printed nothing since the dashboard started; its lines appear here as it works.",
                    },
              ),
            ]),
      );
    }
    paintCount();
    if (follow) toTail();
  };

  let sideQueued = false;
  const queueSide = () => {
    if (sideQueued) return;
    sideQueued = true;
    requestAnimationFrame(() => {
      sideQueued = false;
      paintSide();
    });
  };

  const changed = () => {
    toUrl();
    paintSide();
    paintLines();
  };

  /** @param {boolean} on @param {boolean} [quiet] */
  const setFollow = (on, quiet = false) => {
    follow = on;
    if (on) {
      unseen = 0;
      toTail();
    }
    paintCount();
    opts.onFollow?.(on);
    if (!quiet && on) toTail();
  };

  /** @param {import("../parity.js").HostLine} l */
  const add = (l) => {
    // Until the snapshot is in, a live line waits: the two merge by number
    // when it lands (finding 1).
    if (!loaded) {
      early.push(l);
      if (early.length > KEEP) early = early.slice(early.length - KEEP);
      return;
    }
    if (l.seq <= lastSeq) return;
    lastSeq = l.seq;
    lines.push(l);
    if (lines.length > KEEP) lines = lines.slice(lines.length - KEEP);
    seen.add(l.source);
    queueSide();
    if (!loaded || !lineShown(l, f)) {
      paintCount();
      return;
    }
    if (!follow) unseen += 1;
    if (list.querySelector(".nx-empty")) list.replaceChildren();
    list.append(...lineEls(l, true));
    while (list.childElementCount > KEEP) list.firstElementChild?.remove();
    if (follow) toTail();
    paintCount();
  };

  /** @type {Map<string, any>} */
  const live = new Map();
  const paintTransfers = () => {
    const now = Date.now() / 1000;
    for (const [k, t] of live) if (now - t.at > 30) live.delete(k);
    transfers.replaceChildren(
      ...[...live.values()].map((t) => {
        const v = transferView(t);
        return h(
          "div",
          { class: "hl-xfer" },
          h("span", null, h("strong", null, t.op), ` · ${t.label}`),
          h(
            "span",
            {
              class: "hl-meter",
              role: "progressbar",
              "aria-label": v.label,
              ...(v.pct == null ? {} : { "aria-valuenow": String(v.pct) }),
            },
            h("span", { style: `inline-size:${v.pct ?? 100}%` }),
          ),
          h("span", { class: "hl-hint" }, v.text),
        );
      }),
    );
  };

  // Scrolling up is the TUI's SCROLL mode: following stops until the tail.
  out.addEventListener("scroll", () => {
    const atTail = out.scrollHeight - out.scrollTop - out.clientHeight < 24;
    if (atTail && !follow) setFollow(true, true);
    else if (!atTail && follow && loaded) {
      follow = false;
      paintCount();
      opts.onFollow?.(false);
    }
  });

  /** @param {KeyboardEvent} e */
  const onKey = (e) => {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    // A line row already answered this key (Space or Enter opens it).
    if (e.defaultPrevented) return;
    if (document.querySelector("dialog[open]")) return;
    const a = document.activeElement;
    const what = keyAction(
      e.key,
      a?.tagName ?? "",
      a?.getAttribute("role") ?? null,
    );
    if (!what) return;
    e.preventDefault();
    if (what === "search") tb.search?.focus();
    else if (what === "pause") setFollow(!follow);
    else setFollow(true);
  };
  document.addEventListener("keydown", onKey);

  const abort = new AbortController();
  const read = async () => {
    loaded = false;
    failure = null;
    paintLines();
    const r = await fetchJson(
      "/data/host-log",
      "the host's lines",
      abort.signal,
    );
    loaded = true;
    // The page's own lines, the snapshot and the live lines that came
    // while it was read: merged by number, capped at KEEP (finding 1).
    lines = mergeLines(
      mergeLines(lines, r.ok ? (r.body.lines ?? []) : [], KEEP),
      early,
      KEEP,
    );
    early = [];
    if (!r.ok) {
      failure = r.error.why;
      paintLines();
      return;
    }
    for (const s of r.body.sources ?? []) seen.add(s);
    for (const l of lines) seen.add(l.source);
    lastSeq = lines.length ? lines[lines.length - 1].seq : lastSeq;
    for (const t of r.body.transfers ?? []) live.set(`${t.op}|${t.label}`, t);
    paintTransfers();
    paintSide();
    paintLines();
  };

  const offLine = listen("host_log", (l) => add(l));
  const offTransfer = listen("transfer", (t) => {
    live.set(`${t.op}|${t.label}`, t);
    paintTransfers();
  });
  const unsub = subscribe(queueSide);
  paintSide();
  paintLines();
  void read();
  const timer = setInterval(paintTransfers, 5000);

  const download = () => {
    const text = shownLines()
      .map((l) => {
        const x = logLine(/** @type {any} */ ({ ...l, job: 0 }));
        return `${new Date(l.ts * 1000).toISOString()}  ${x.level.toUpperCase().padEnd(5)}  ${l.source.padEnd(12)}  ${l.msg}`;
      })
      .join("\n");
    const url = URL.createObjectURL(
      new Blob([`${text}\n`], { type: "text/plain" }),
    );
    const a = h("a", { href: url, download: "host-log.txt" });
    document.body.append(a);
    a.click();
    a.remove();
    // The browser has the file once the click ran (finding 14).
    setTimeout(() => URL.revokeObjectURL(url), 0);
  };

  return {
    cleanup: () => {
      abort.abort();
      offLine();
      offTransfer();
      unsub();
      clearInterval(timer);
      document.removeEventListener("keydown", onKey);
    },
    setFollow,
    following: () => follow,
    download,
    addBlock: (el) => {
      blocks.append(el);
      setFollow(true);
    },
    focusSearch: () => tb.search?.focus(),
  };
}
