// Live log (TUI parity, LOG_STREAM and DATA_TRANSFERS): every line the host
// prints for every operation, whoever started it (the CLI, the TUI, the
// nightly round, this dashboard), live; filtered per stack (the TUI's
// source selector), per level and by text; follow the tail or scroll back.
// The transfers' byte counters sit on top while they run.

import { fetchJson, h, progressGroup } from "../dom.js";
import { logLine } from "../jobs.js";
import { lineMatches, transferView, whoText } from "../parity.js";
import { current, listen, subscribe } from "../store.js";
import { setParams } from "../urlstate.js";
import { attachLogs } from "/static/kp/js/log.js";

/** Lines kept in the page, as the server's ring. */
const KEEP = 2000;

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const params = new URLSearchParams(location.search);
  /** @type {{source: string, level: string, q: string}} */
  const filter = {
    source: params.get("source") ?? "",
    level: params.get("level") ?? "",
    q: params.get("q") ?? "",
  };
  const sourceSel = h("select", { class: "kp-field__input", id: "log-source" });
  const levelSel = h(
    "select",
    { class: "kp-field__input", id: "log-level" },
    h("option", { value: "" }, "Every level"),
    h("option", { value: "info" }, "Info and up"),
    h("option", { value: "warn" }, "Warnings and errors"),
    h("option", { value: "error" }, "Errors"),
  );
  levelSel.value = filter.level;
  const text = h("input", {
    class: "kp-field__input",
    id: "log-q",
    type: "search",
    placeholder: "Only lines containing…",
  });
  text.value = filter.q;
  const follow = h("input", {
    class: "kp-field__check",
    type: "checkbox",
    id: "log-follow",
  });
  follow.checked = true;
  const tail = h(
    "button",
    { type: "button", class: "kp-button", id: "log-tail" },
    "Back to the tail",
  );
  const count = h(
    "p",
    { class: "measured", role: "status" },
    "Reading the log…",
  );
  const transfers = h("section", {
    class: "kp-card transfers",
    "aria-label": "Transfers",
    hidden: "",
  });
  const log = h(
    "div",
    {
      class: "kp-log host-log",
      role: "log",
      "aria-live": "off",
      "aria-label": "Every line the host prints",
      id: "host-log",
    },
    h("p", { class: "measured" }, "Reading…"),
  );
  const field = (
    /** @type {string} */ label,
    /** @type {HTMLElement} */ control,
  ) =>
    h(
      "div",
      { class: "kp-field logs-field" },
      h("label", { class: "kp-field__label", for: control.id }, label),
      control,
    );
  root.replaceChildren(
    h("h1", null, "Live log"),
    h(
      "p",
      { class: "measured" },
      "Every line the host prints, whoever started the operation: this dashboard, a CLI or the TUI on a workstation, or the host's own nightly round. Secrets are masked by shape before a line leaves the dashboard.",
    ),
    transfers,
    h(
      "form",
      { class: "logs-controls", role: "search", "aria-label": "Which lines" },
      field("Stack or source", sourceSel),
      field("Level", levelSel),
      field("Lines containing", text),
      h("label", { class: "logs-follow" }, follow, " Follow the tail"),
      tail,
    ),
    count,
    log,
  );

  /** @type {import("../parity.js").HostLine[]} */
  let lines = [];
  /** @type {Set<string>} */
  let sources = new Set();
  let lastSeq = 0;

  const fillSources = () => {
    const fleet = current().fleet?.stacks.map((s) => s.name) ?? [];
    const all = [...new Set([...fleet, ...sources, filter.source])]
      .filter((s) => s)
      .sort();
    const sig = all.join(",");
    if (sourceSel.dataset.sig === sig) return;
    sourceSel.dataset.sig = sig;
    sourceSel.replaceChildren(
      h("option", { value: "" }, "Every stack and source"),
      ...all.map((s) => h("option", { value: s }, s)),
    );
    sourceSel.value = filter.source;
  };

  /** @param {import("../parity.js").HostLine} l */
  const lineEl = (l) => {
    const x = logLine(/** @type {any} */ ({ ...l, job: 0 }));
    return h(
      "p",
      {
        class: "kp-log__line",
        "data-kp-severity": x.severity,
        "data-seq": String(l.seq),
      },
      h("time", { class: "kp-log__time" }, x.time),
      h(
        "span",
        { class: "kp-log__source", "data-kp-source": x.source },
        x.source,
      ),
      h("span", { class: "kp-log__level" }, x.level),
      // Who started it rides in the message: the kp log's grid has four
      // columns (time, source, level, message).
      h(
        "span",
        { class: "kp-log__message" },
        x.msg,
        h("span", { class: "measured host-log__who" }, ` · ${whoText(l)}`),
      ),
    );
  };

  const paintCount = () => {
    const shown = log.childElementCount;
    count.textContent = `${shown} of ${lines.length} line(s) shown${follow.checked ? " · following" : " · scrolled back"}`;
  };
  const toTail = () => {
    log.scrollTop = log.scrollHeight;
  };
  const repaint = () => {
    log.replaceChildren(
      ...lines.filter((l) => lineMatches(l, filter)).map(lineEl),
    );
    attachLogs(log);
    if (follow.checked) toTail();
    paintCount();
  };

  /** @param {import("../parity.js").HostLine} l */
  const add = (l) => {
    if (l.seq <= lastSeq) return;
    lastSeq = l.seq;
    lines.push(l);
    if (lines.length > KEEP) lines = lines.slice(lines.length - KEEP);
    if (!sources.has(l.source)) {
      sources.add(l.source);
      fillSources();
    }
    if (!lineMatches(l, filter)) {
      paintCount();
      return;
    }
    const el = lineEl(l);
    log.append(el);
    while (log.childElementCount > KEEP) log.firstElementChild?.remove();
    attachLogs(el);
    if (follow.checked) toTail();
    paintCount();
  };

  /** @type {Map<string, any>} */
  const live = new Map();
  const paintTransfers = () => {
    const now = Date.now() / 1000;
    for (const [k, t] of live) if (now - t.at > 30) live.delete(k);
    transfers.hidden = live.size === 0;
    transfers.replaceChildren(
      h("h2", null, "Transfers"),
      progressGroup(
        [...live.values()].map((t) => {
          const v = transferView(t);
          return { label: v.label, pct: v.pct ?? 0, value: v.text };
        }),
      ),
    );
  };

  const apply = () => {
    filter.source = sourceSel.value;
    filter.level = levelSel.value;
    filter.q = text.value.trim();
    const search = setParams(location.search, {
      source: filter.source,
      level: filter.level,
      q: filter.q,
    });
    history.replaceState(history.state, "", location.pathname + search);
    repaint();
  };
  sourceSel.addEventListener("change", apply);
  levelSel.addEventListener("change", apply);
  text.addEventListener("input", apply);
  follow.addEventListener("change", () => {
    if (follow.checked) toTail();
    paintCount();
  });
  tail.addEventListener("click", () => {
    follow.checked = true;
    toTail();
    paintCount();
  });
  // Scrolling up is the TUI's SCROLL mode: following stops until the tail.
  log.addEventListener("scroll", () => {
    const atTail = log.scrollHeight - log.scrollTop - log.clientHeight < 40;
    if (follow.checked !== atTail) {
      follow.checked = atTail;
      paintCount();
    }
  });

  const offLine = listen("host_log", (l) => add(l));
  const offTransfer = listen("transfer", (t) => {
    live.set(`${t.op}|${t.label}`, t);
    paintTransfers();
  });
  const unsub = subscribe(fillSources);
  const abort = new AbortController();
  void fetchJson("/data/host-log", "the host's lines", abort.signal).then(
    (r) => {
      if (!r.ok) {
        count.textContent = `The host's lines could not be read: ${r.error.why}`;
        return;
      }
      for (const s of r.body.sources ?? []) sources.add(s);
      fillSources();
      for (const l of r.body.lines ?? []) {
        if (l.seq > lastSeq) {
          lines.push(l);
          lastSeq = l.seq;
        }
      }
      for (const t of r.body.transfers ?? []) live.set(`${t.op}|${t.label}`, t);
      paintTransfers();
      repaint();
    },
  );
  fillSources();
  const timer = setInterval(paintTransfers, 5000);
  return () => {
    abort.abort();
    offLine();
    offTransfer();
    unsub();
    clearInterval(timer);
  };
}
