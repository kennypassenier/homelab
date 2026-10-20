// redesign-console (3.71.0, the Console demo Kenny approved on 2026-10-03):
// the host's log and a root shell in one place. Every line the host prints,
// live, with the Source/Level side column (the explorer in log.js), and
// under it a shell bar: one line at a time, run as root inside a container
// (pct exec). Each line is the exec action (A6) on the host-wide target: it
// goes through the action queue like "Run a command", shows in Activity,
// and the host writes it to its audit log; the host refuses unless
// exec_enabled = true. `/shell` and `/log`'s old Console role land here.

import { act, onAct, send } from "../act.js";
import { fetchJson, h } from "../dom.js";
import { gridRow, formGrid, buttonGroup } from "../gridrow.js";
import {
  CONSOLE_RETRY_EVENT,
  attachConsoles,
  clearConsole,
  consoleBlocks,
  runConsoleCommand,
  setConsole,
  setConsoleComplete,
  setConsoleContext,
  setConsoleRun,
} from "/static/kp/js/console.js";
import { setKeys, attachKeys } from "/static/kp/js/keys.js";
import { attachDisabledReasons } from "/static/kp/js/reason.js";
import { declare, declareField, drivable } from "../drivable.js";
import { shellResult, shellTargets } from "../parity.js";
import { current, subscribe } from "../store.js";
import { ensureStyle, pageHeader, toast } from "../ui.js";
import { onPageKey } from "../shortcuts.js";
import { setParams } from "../urlstate.js";
import { mountHostLog } from "./log.js";

/** Words that need a second Enter: what cannot be taken back. */
const CONFIRM_WORDS = ["rm", "reboot", "shutdown", "mkfs", "dd", "kill -9"];
/** Words Tab completes. */
const COMPLETE = [
  "df -h",
  "free -m",
  "uptime",
  "hostname",
  "ls",
  "cat",
  "journalctl -u",
  "systemctl status",
  "docker ps",
  "ps aux",
];

// Live view (invariant 39): every control on this page that runs
// something; the explorer's own controls are declared for this page too.
// review M5: every page field Live view may set is declared (drivable.js
// `declareField`); the client and the dashboard refuse any other.
const SHELL_TARGET = declareField({
  id: "shell-target",
  page: "shell",
  what: "the container a command runs in",
});
const SHELL_LINE = declareField({
  id: "shell-line",
  page: "shell",
  what: "the command to run",
});

const RUN = declare({
  id: "console-run",
  page: "shell",
  opens: "run",
  what: "run the command line as root in the chosen container (a queued, audited job)",
});
const FOCUS = declare({
  id: "console-run-a-command",
  page: "shell",
  opens: "view",
  what: "jump to the shell bar: the command line, focused",
  // redesign-integrate-8: the merge declared the command box itself as a
  // control; a press on a text box does nothing (the sweep said so). Its
  // text is the field shell-line; its press is this jump.
  was: ["console-command"],
});
const DOWNLOAD = declare({
  id: "console-download",
  page: "shell",
  opens: "run",
  what: "save the lines the filter shows as host-log.txt",
});
const DRIVE = {
  follow: declare({
    id: "console-follow",
    page: "shell",
    opens: "view",
    what: "pause or resume following the newest host lines",
  }),
  source: declare({
    id: "console-source",
    page: "shell",
    opens: "view",
    row: "<source>",
    what: "turn one source's lines on or off",
    selects: true,
  }),
  only: declare({
    id: "console-only",
    page: "shell",
    opens: "view",
    row: "<source>",
    what: "show only one source's lines",
  }),
  level: declare({
    id: "console-level",
    page: "shell",
    opens: "view",
    row: "info|warn|error",
    what: "show lines of at least this level",
  }),
  reset: declare({
    id: "console-reset",
    page: "shell",
    opens: "view",
    what: "show every source and level again",
    shows: "while a source or level filter narrows the lines",
    reach: [{ do: "click", control: "console-level", row: "error" }],
  }),
  line: declare({
    id: "console-line",
    page: "shell",
    opens: "view",
    row: "<line number>",
    what: "open or close one line's details",
  }),
  openJob: declare({
    id: "console-open-job",
    page: "shell",
    opens: "dialog",
    row: "<job>",
    what: "open the job a host line belongs to",
    shows: "in an opened line this dashboard's job printed",
    reach: [{ do: "click", control: "console-line", row: "*" }],
  }),
  copy: declare({
    id: "console-copy-line",
    page: "shell",
    opens: "run",
    what: "copy an opened line to the clipboard",
    shows: "in an opened line",
    reach: [{ do: "click", control: "console-line", row: "*" }],
  }),
  everything: declare({
    id: "console-everything",
    page: "shell",
    opens: "view",
    what: "show every source again",
  }),
  onlyLine: declare({
    id: "console-only-line-source",
    page: "shell",
    opens: "view",
    what: "show only the opened line's source",
    shows: "in an opened line",
    reach: [{ do: "click", control: "console-line", row: "*" }],
  }),
  newLines: declare({
    id: "console-back-to-tail",
    page: "shell",
    opens: "view",
    what: "jump back to the newest lines and follow again",
    shows: "while the tail is paused and new lines came",
    reach: [{ do: "click", control: "console-follow" }],
  }),
  search: declare({
    id: "console-search",
    page: "shell",
    opens: "view",
    what: "the Lines containing box",
  }),
  retry: declare({
    id: "console-retry",
    page: "shell",
    opens: "run",
    what: "read the host's lines again after a failed read",
    shows: "after the lines could not be read",
  }),
};
const CONTAINER = declare({
  id: "console-container",
  page: "shell",
  opens: "view",
  what: "the container the next command runs in",
});

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  ensureStyle("/css/pages/activity.css");
  root.classList.add("con-page");
  const params = new URLSearchParams(location.search);

  const pause = drivable(
    h("button", { type: "button", class: "kp-button kp-button--secondary" }),
    DRIVE.follow,
  );
  const download = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--secondary",
        title: "Save the lines the filter shows as a .txt file",
      },
      "Download",
    ),
    DOWNLOAD,
  );
  const runACommand = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--primary",
        title: "Jump to the prompt (:)",
      },
      "Run a command",
    ),
    FOCUS,
  );
  const header = pageHeader({
    title: "Console",
    desc: "Every line the host prints, live, whoever started the work: this dashboard, a CLI or TUI, or the nightly round. Below it, a root shell into any container. Secrets are masked before a line leaves the host.",
    live: false,
    actions: [pause, download],
    primary: runACommand,
  });
  // The live chip sits beside the title, as the demo (senior review,
  // finding 5); on a phone the header reads title + chip, description,
  // "Run a command" across the width, then Pause and Download
  // (activity.css).
  const liveChip = h("span", { class: "nx-live con-live", role: "status" });
  header.title.after(liveChip);
  header.el.classList.add("con-head");

  // ── the console: kp's, in its own pane under the log ────────────────
  // (feat-kp11demo-7) The prompt, the history of the last 50 lines for this
  // tab, the command blocks with their state, timer and exit code, the
  // output folded past twelve lines, Tab to complete, a second Enter for the
  // words that cannot be taken back. The page supplies what a command may do:
  // each line is the exec action on the host-wide target, a queued, audited
  // job; the host refuses unless exec_enabled = true.
  const EXEC_ON =
    "Runs as root; every line is a job in the queue and a line in the host's audit log. ↑ walks your history, Tab completes.";
  const EXEC_OFF =
    "Remote exec is off (exec_enabled = false, changed over ssh only)";
  const consoleEl = h("div", {
    class: "kp-console",
    "data-kp-console": "",
    "data-kp-console-prompt": "#",
    "data-kp-console-context": "select",
    "data-kp-console-history": "50",
    "data-kp-console-remember": "session",
    "data-kp-console-tab": "complete",
    "data-kp-console-status": "exit-code",
    "data-kp-console-max-lines": "12",
    "data-kp-console-follow": "stick",
    "data-kp-console-cancel": "",
    "data-kp-console-confirm": `/^\\s*(${CONFIRM_WORDS.join("|")})\\b/`,
    "data-kp-console-hint": EXEC_ON,
    "data-kp-state": "loading",
    "aria-label": "Host console",
  });
  const consolePane = h("div", { class: "con-pane" }, consoleEl);

  // ── the session tools and the words that need a second press ────────
  const toolRead = h(
    "span",
    { class: "kp-formgrid__value con-read", role: "status" },
    "",
  );
  const mkBtn = (/** @type {string} */ label, /** @type {string} */ title) =>
    h("button", { type: "button", class: "kp-button", title }, label);
  const clearBtn = mkBtn(
    "Clear",
    "Empty the screen; Undo brings every block back",
  );
  const againBtn = mkBtn(
    "Run again",
    "Repeat the last command in the same container",
  );
  const saveBtn = mkBtn("Download the session", "Every block as one text file");
  const chips = h(
    "span",
    { class: "con-chips" },
    ...CONFIRM_WORDS.map((w) => h("span", { class: "kp-badge" }, w)),
  );
  const tools = formGrid(
    [
      gridRow({
        label: "Needs a second press",
        control: [
          chips,
          h(
            "span",
            { class: "con-read" },
            "A line that starts with one of these waits for a second Enter.",
          ),
        ],
        span: "control-value",
      }),
      gridRow({
        label: "Screen",
        control: "Empties the screen; Undo brings it back.",
        span: "control-value",
        actions: clearBtn,
        buttons: true,
      }),
      gridRow({
        label: "Last command",
        control: "Repeats the last command in the same place.",
        span: "control-value",
        actions: againBtn,
        buttons: true,
      }),
      gridRow({
        label: "Whole session",
        control: "Every block as one text file.",
        span: "control-value",
        actions: saveBtn,
        buttons: true,
      }),
      gridRow({
        label: "Keys",
        control: [
          h("span", { class: "kp-keys", "data-con-keys": "" }),
          toolRead,
        ],
        span: "control-value",
      }),
    ],
    "con-tools",
  );

  const body = h("div", { class: "con-body" });
  root.replaceChildren(header.el, body);
  const explorer = mountHostLog(body, {
    drive: DRIVE,
    followInBar: false,
    foot: h("div", { class: "con-foot" }, consolePane, tools),
    label: "Host lines and the console",
    onFollow: (on) => paintFollow(on),
  });
  const paintFollow = (/** @type {boolean} */ on) => {
    pause.textContent = on ? "Pause" : "Resume";
    pause.title = on
      ? "Stop following; new lines wait below a marker (Space)"
      : "Follow the newest lines again (Space)";
    liveChip.textContent = on
      ? "following · 2\u00a0000-line ring"
      : "paused · new lines wait";
    liveChip.dataset.paused = String(!on);
  };
  paintFollow(true);
  pause.addEventListener("click", () =>
    explorer.setFollow(!explorer.following()),
  );
  download.addEventListener("click", () => explorer.download());
  setKeys(/** @type {HTMLElement} */ (root.querySelector("[data-con-keys]")), [
    [":", "jump to the prompt"],
    ["↑", "earlier command"],
    ["Tab", "complete"],
    ["Space", "pause following"],
    ["End", "tail"],
  ]);
  const detach = [
    attachConsoles(root),
    attachKeys(root),
    attachDisabledReasons(root),
  ];

  // The inner parts Live view and the old fields name.
  const decorate = () => {
    const sel = consoleEl.querySelector("select");
    const input = consoleEl.querySelector(".kp-console__input");
    const run = consoleEl.querySelector(".kp-console__run");
    if (sel instanceof HTMLElement) {
      sel.id = SHELL_TARGET;
      drivable(sel, CONTAINER);
    }
    if (input instanceof HTMLElement) input.id = SHELL_LINE;
    if (run instanceof HTMLElement) drivable(run, RUN);
    // The prompt is a package form: a busy submit and a summary that says
    // why a line was refused (kp-themes forms.js).
    const form = consoleEl.querySelector("form");
    if (form instanceof HTMLElement) {
      form.setAttribute("data-kp-form", "");
      form.setAttribute("novalidate", "");
      if (!form.querySelector("[data-kp-form-summary]"))
        form.prepend(
          h("div", {
            class: "kp-form__summary",
            "data-kp-form-summary": "",
            tabindex: "-1",
            hidden: "",
          }),
        );
    }
    if (run instanceof HTMLElement) run.setAttribute("data-kp-submit", "");
  };

  // Remote exec off on this host: the prompt stops and says why.
  const abort = new AbortController();
  let execOff = false;
  const paintExec = () => {
    consoleEl.toggleAttribute("disabled", execOff);
    if (execOff) consoleEl.setAttribute("data-kp-disabled-reason", EXEC_OFF);
    else consoleEl.removeAttribute("data-kp-disabled-reason");
    attachDisabledReasons(root);
  };
  void fetchJson("/data/host-settings", "the host settings", abort.signal).then(
    (r) => {
      if (!r.ok) return;
      /** @type {{key: string, value: unknown, set?: boolean}[]} */
      const fields = r.body?.page?.fields ?? [];
      const exec = fields.find((x) => x.key === "exec_enabled");
      execOff = exec?.value === false;
      paintExec();
    },
  );

  // The containers (the fleet's stacks, by number); ?vmid= or the one
  // picked before stays chosen.
  let wanted = params.get("vmid") ?? "";
  const fillTargets = () => {
    const stacks = current().fleet?.stacks;
    if (!stacks) {
      // The package's own states: the busy card while the host is asked, the
      // failed plate (with its Retry) when it does not answer.
      if (current().up === false)
        setConsole(consoleEl, {
          state: "error",
          message:
            "The host did not answer, so no container can be listed to run a command in.",
        });
      else setConsole(consoleEl, { state: "loading" });
      decorate();
      return;
    }
    const list = shellTargets(stacks);
    if (wanted && !list.some((t) => t.value === wanted))
      list.push({ value: wanted, label: `${wanted} · by number` });
    const sig = list.map((t) => t.value).join(",");
    if (consoleEl.dataset.sig !== sig) {
      consoleEl.dataset.sig = sig;
      setConsoleContext(consoleEl, {
        label: "Run in",
        options: list,
        value: list.some((t) => t.value === wanted)
          ? wanted
          : (list[0]?.value ?? ""),
        state: "ready",
      });
    }
    setConsole(consoleEl, { state: "ready" });
    decorate();
  };
  setConsoleComplete(consoleEl, (prefix) =>
    COMPLETE.filter((w) => w.startsWith(prefix)),
  );
  /** The exec action as the console's job. */
  setConsoleRun(consoleEl, async (command, context) => {
    wanted = context;
    history.replaceState(
      history.state,
      "",
      location.pathname + setParams(location.search, { vmid: wanted }),
    );
    if (!/^\d{3,5}$/.test(context))
      return {
        update: (handler) =>
          handler({
            state: "refused",
            error: "no container is chosen",
            remedy: "Choose a container first.",
          }),
      };
    const r = await send(
      "POST",
      "/data/actions/_host/exec",
      { vmid: context, command },
      `exec in CT ${context}`,
    );
    if (!r.ok)
      return {
        update: (handler) =>
          handler({
            state: "refused",
            error: r.error.why,
            remedy: r.error.fix || undefined,
          }),
      };
    const job = /** @type {number} */ (r.body.job);
    /** @type {(() => void) | null} */
    let stop = null;
    /** @type {import("/static/kp/js/console.js").ConsoleJob} */
    const consoleJob = {
      update: (handler) => {
        const paint = () => {
          const j = act.jobs.find((x) => x.job === job);
          if (!j) return handler({ state: "queued" });
          const res = shellResult(j);
          if (!res.done) {
            // Cancel stays offered while the command runs: a queued job
            // leaves the queue, a running one is stopped on the host (a host
            // before 3.76.0 answers why it cannot, and the toast says it).
            return handler({
              state: j.state === "queued" ? "queued" : "running",
            });
          }
          stop?.();
          if (j.state === "refused" && /^cancelled by /.test(j.message ?? ""))
            return handler({ state: "cancelled" });
          handler({
            state: res.ok ? "ok" : "failed",
            ...(res.exit != null ? { code: res.exit } : {}),
            output: res.output || "",
          });
        };
        stop = onAct("jobs", (id) => {
          if (id == null || id === job) paint();
        });
        paint();
      },
      cancel: () => {
        void send(
          "POST",
          `/data/actions/jobs/${job}/cancel`,
          {},
          "cancel the command",
        ).then((r) => {
          if (!r.ok) toast(`Could not cancel: ${r.error.why}`);
        });
      },
    };
    return consoleJob;
  });
  consoleEl.addEventListener(CONSOLE_RETRY_EVENT, () => fillTargets());
  fillTargets();
  const unsub = subscribe(fillTargets);

  // the session tools
  let last = "";
  consoleEl.addEventListener("kp-console-block", (e) => {
    const b = /** @type {CustomEvent} */ (e).detail.block;
    if (b?.command) last = b.command;
    toolRead.textContent = `${consoleBlocks(consoleEl).length.toLocaleString("en-GB")} commands in this tab.`;
  });
  clearBtn.addEventListener("click", () => {
    clearConsole(consoleEl, { undo: true });
    toolRead.textContent =
      "Screen cleared. Undo is in the toast for a few seconds; ↑ still recalls your commands.";
  });
  againBtn.addEventListener("click", () => {
    if (!last) {
      toolRead.textContent = "No command ran in this tab yet.";
      return;
    }
    void runConsoleCommand(consoleEl, last);
    toolRead.textContent = `Run again: ${last}`;
  });
  saveBtn.addEventListener("click", () => {
    const blocks = consoleBlocks(consoleEl);
    const text = blocks
      .map(
        (b) =>
          `# ${b.context}\n$ ${b.command}\n${b.output}\n[${b.state}${b.code === undefined ? "" : `, exit ${b.code}`}]`,
      )
      .join("\n\n");
    const url = URL.createObjectURL(new Blob([text], { type: "text/plain" }));
    const a = h("a", { href: url, download: "host-console.txt" });
    document.body.append(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 0);
    toolRead.textContent = `Downloaded host-console.txt: ${blocks.length.toLocaleString("en-GB")} blocks, ${text.length.toLocaleString("en-GB")} characters.`;
  });
  const focusPrompt = () =>
    /** @type {HTMLElement | null} */ (
      consoleEl.querySelector(".kp-console__input")
    )?.focus();
  runACommand.addEventListener("click", focusPrompt);
  // fix-kp11-7 / feat-kp11demo-14: ":" focuses the prompt while this page is
  // mounted (shortcuts.js shows it dimmed, with why, on every other page).
  const stopKeys = onPageKey("prompt", focusPrompt);
  return () => {
    abort.abort();
    explorer.cleanup();
    unsub();
    for (const d of detach) d();
    stopKeys();
    root.classList.remove("con-page");
  };
}
