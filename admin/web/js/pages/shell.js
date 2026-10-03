// redesign-console (3.71.0, the Console demo Kenny approved on 2026-10-03):
// the host's log and a root shell in one place. Every line the host prints,
// live, with the Source/Level side column (the explorer in log.js), and
// under it a shell bar: one line at a time, run as root inside a container
// (pct exec). Each line is the exec action (A6) on the host-wide target: it
// goes through the action queue like "Run a command", shows in Activity,
// and the host writes it to its audit log; the host refuses unless
// exec_enabled = true. `/shell` and `/log`'s old Console role land here.

import { act, onAct, send } from "../act.js";
import { refusalCallout } from "../actui.js";
import { fetchJson, h } from "../dom.js";
import { declare, drivable } from "../drivable.js";
import { shellResult, shellTargets } from "../parity.js";
import { current, subscribe } from "../store.js";
import { pageHeader } from "../ui.js";
import { setParams } from "../urlstate.js";
import { ensureStyle } from "./activitykit.js";
import { mountHostLog } from "./log.js";

/** Lines the Up key recalls. */
const RECALL = 50;

// Live view (invariant 39): every control on this page that runs
// something; the explorer's own controls are declared for this page too.
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
  what: "jump to the shell bar",
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
  }),
  copy: declare({
    id: "console-copy-line",
    page: "shell",
    opens: "run",
    what: "copy an opened line to the clipboard",
  }),
};

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
        title: "Jump to the shell bar (Ctrl `)",
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
  const liveChip = h("span", { class: "nx-live con-live", role: "status" });
  header.actions.before(liveChip);

  // ── the shell bar ───────────────────────────────────────────────────
  const target = h("select", {
    class: "kp-field__input",
    id: "shell-target",
    "aria-label": "Container",
    title: "The container the command runs in",
  });
  const line = h("input", {
    class: "kp-field__input mono",
    id: "shell-line",
    type: "text",
    autocomplete: "off",
    spellcheck: "false",
    placeholder: "a command, e.g. df -h /appdata",
    "aria-label": "Command",
  });
  const run = drivable(
    h(
      "button",
      {
        type: "submit",
        class: "kp-button kp-button--primary",
        id: "shell-run",
        title:
          "Run as root in the container (pct exec); queued as a job and audited (Enter)",
      },
      "Run",
    ),
    RUN,
  );
  const off = h("p", { class: "con-off", hidden: "" });
  const form = h(
    "form",
    { class: "con-shell", "aria-label": "Run a command" },
    target,
    h(
      "label",
      { class: "con-prompt" },
      h("b", { "aria-hidden": "true" }, "#"),
      line,
    ),
    run,
    off,
  );

  const body = h("div", { class: "con-body" });
  root.replaceChildren(header.el, body);
  const explorer = mountHostLog(body, {
    drive: DRIVE,
    followInBar: false,
    foot: form,
    label: "Host lines and shell",
    onFollow: (on) => paintFollow(on),
  });
  const paintFollow = (/** @type {boolean} */ on) => {
    pause.textContent = on ? "Pause" : "Resume";
    pause.title = on
      ? "Stop following; new lines wait below a marker (Space)"
      : "Follow the newest lines again (Space)";
    liveChip.textContent = on
      ? "following · 2000-line ring"
      : "paused · new lines wait";
    liveChip.dataset.paused = String(!on);
  };
  paintFollow(true);
  pause.addEventListener("click", () =>
    explorer.setFollow(!explorer.following()),
  );
  download.addEventListener("click", () => explorer.download());
  runACommand.addEventListener("click", () => line.focus());

  // Remote exec off on this host: say so under the bar, once known.
  void fetchJson("/data/host-settings", "the host settings").then((r) => {
    if (!r.ok) return;
    /** @type {{key: string, value: unknown, set?: boolean}[]} */
    const fields = r.body?.page?.fields ?? [];
    const exec = fields.find((x) => x.key === "exec_enabled");
    const on = exec ? exec.value === true : null;
    off.hidden = false;
    off.textContent =
      on === false
        ? "Remote exec is off on this host (exec_enabled = false, changed over ssh only): the host refuses commands. ↑ ↓ walk your history."
        : "Runs as root; every line is a job in the queue and a line in the host's audit log. ↑ ↓ walk your history.";
  });

  let wanted = params.get("vmid") ?? "";
  const fillTargets = () => {
    const list = shellTargets(current().fleet?.stacks ?? []);
    if (wanted && !list.some((t) => t.value === wanted))
      list.push({ value: wanted, label: `${wanted} · by number` });
    const sig = list.map((t) => t.value).join(",");
    if (target.dataset.sig === sig) return;
    target.dataset.sig = sig;
    target.replaceChildren(
      h("option", { value: "" }, "Pick a container"),
      ...list.map((t) => h("option", { value: t.value }, t.label)),
    );
    target.value = wanted;
  };
  fillTargets();
  const unsub = subscribe(fillTargets);
  target.addEventListener("change", () => {
    wanted = target.value;
    history.replaceState(
      history.state,
      "",
      location.pathname + setParams(location.search, { vmid: wanted }),
    );
  });

  /** @type {string[]} */
  const recall = [];
  let recallAt = -1;
  /** @type {Map<number, HTMLElement>} */
  const pending = new Map();

  /** @param {number} job */
  const paintJob = (job) => {
    const el = pending.get(job);
    const j = act.jobs.find((x) => x.job === job);
    if (!el || !j) return;
    const r = shellResult(j);
    const res = /** @type {HTMLElement} */ (el.querySelector(".con-exec__out"));
    const state = /** @type {HTMLElement} */ (
      el.querySelector(".con-exec__state")
    );
    state.textContent = r.done
      ? r.ok
        ? `exit ${r.exit ?? 0}`
        : r.exit == null
          ? j.state
          : `exit ${r.exit}`
      : j.state === "queued"
        ? "queued behind the host's current operation…"
        : "running…";
    state.dataset.tone = r.done ? (r.ok ? "ok" : "bad") : "";
    if (r.done) {
      res.textContent = r.output || "(no output)";
      res.hidden = false;
      pending.delete(job);
    }
  };
  const offJobs = onAct("jobs", (id) => {
    if (id == null) for (const j of pending.keys()) paintJob(j);
    else if (pending.has(id)) paintJob(id);
  });

  const submit = async () => {
    const cmd = line.value.trim();
    const n = target.value;
    if (!cmd) return;
    if (!/^\d{3,5}$/.test(n)) {
      target.focus();
      explorer.addBlock(
        h(
          "div",
          { class: "con-exec" },
          refusalCallout(
            {
              what: "the shell",
              why: "no container is chosen",
              fix: "choose one first",
            },
            "warning",
          ),
        ),
      );
      return;
    }
    recall.unshift(cmd);
    recall.splice(RECALL);
    recallAt = -1;
    line.value = "";
    const ct = target.selectedOptions[0]?.textContent?.trim() ?? `CT ${n}`;
    const entry = h(
      "div",
      { class: "con-exec" },
      h(
        "div",
        { class: "con-exec__head" },
        h("span", { class: "mono" }, `# ${cmd}  ·  ${ct}`),
        h("span", { class: "con-exec__state" }, "sending…"),
      ),
      h("pre", { class: "con-exec__out mono", hidden: "" }),
    );
    explorer.addBlock(entry);
    const r = await send(
      "POST",
      "/data/actions/_host/exec",
      { vmid: n, command: cmd },
      `exec in CT ${n}`,
    );
    if (!r.ok) {
      /** @type {HTMLElement} */ (
        entry.querySelector(".con-exec__state")
      ).textContent = "refused";
      entry.append(refusalCallout(r.error));
      return;
    }
    const job = /** @type {number} */ (r.body.job);
    entry.dataset.job = String(job);
    pending.set(job, entry);
    paintJob(job);
  };
  form.addEventListener("submit", (e) => {
    e.preventDefault();
    void submit();
  });
  line.addEventListener("keydown", (e) => {
    if (e.key === "ArrowUp" && recall.length) {
      e.preventDefault();
      recallAt = Math.min(recallAt + 1, recall.length - 1);
      line.value = recall[recallAt];
    } else if (e.key === "ArrowDown" && recallAt >= 0) {
      e.preventDefault();
      recallAt -= 1;
      line.value = recallAt >= 0 ? recall[recallAt] : "";
    }
  });
  /** @param {KeyboardEvent} e */
  const onKey = (e) => {
    if (e.key === "`" && e.ctrlKey) {
      e.preventDefault();
      line.focus();
    }
  };
  document.addEventListener("keydown", onKey);
  return () => {
    explorer.cleanup();
    unsub();
    offJobs();
    document.removeEventListener("keydown", onKey);
    root.classList.remove("con-page");
  };
}
