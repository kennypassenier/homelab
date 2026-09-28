// Shell (TUI parity, the SHELL tab): a line-based shell into one container.
// Each line is the exec action (A6) on the host-wide target: it goes through
// the action queue like a click on "Run a command", shows in Jobs, and the
// host writes it to its audit log. No typed confirmation (Kenny: "we hebben
// genoeg security"); the host refuses unless exec_enabled = true.

import { act, onAct, send } from "../act.js";
import { refusalCallout } from "../actui.js";
import { h } from "../dom.js";
import { shellResult, shellTargets } from "../parity.js";
import { current, subscribe } from "../store.js";
import { setParams } from "../urlstate.js";

/** Lines the Up key recalls. */
const RECALL = 50;

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const params = new URLSearchParams(location.search);
  const target = h("select", { class: "kp-field__input", id: "shell-target" });
  const other = h("input", {
    class: "kp-field__input",
    id: "shell-vmid",
    type: "text",
    inputmode: "numeric",
    pattern: "\\d{3,5}",
    placeholder: "or a vmid",
    "aria-label": "Another container's vmid",
  });
  const line = h("input", {
    class: "kp-field__input mono",
    id: "shell-line",
    type: "text",
    autocomplete: "off",
    spellcheck: "false",
    placeholder: "a command, e.g. df -h",
    "aria-label": "Command line",
  });
  const run = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--destructive",
      id: "shell-run",
    },
    "Run",
  );
  const out = h("div", {
    class: "shell-out",
    id: "shell-out",
    role: "log",
    "aria-live": "polite",
    "aria-label": "What ran and what it printed",
  });
  root.replaceChildren(
    h("h1", null, "Shell"),
    h(
      "p",
      { class: "measured" },
      "One line at a time, run as root inside the container (pct exec). Every line is a job in the queue and a line in the host's audit log; the host refuses unless exec_enabled = true in host.toml. The same as the Run a command form, or homelab exec <vmid> <command>.",
    ),
    h(
      "div",
      { class: "shell-bar" },
      h(
        "div",
        { class: "kp-field" },
        h(
          "label",
          { class: "kp-field__label", for: "shell-target" },
          "Container",
        ),
        target,
      ),
      h(
        "div",
        { class: "kp-field" },
        h(
          "label",
          { class: "kp-field__label", for: "shell-vmid" },
          "Or by number",
        ),
        other,
      ),
    ),
    out,
    h(
      "div",
      { class: "shell-input" },
      h("span", { class: "shell-prompt mono" }, "#"),
      line,
      run,
    ),
  );

  let wanted = params.get("vmid") ?? "";
  const fillTargets = () => {
    const list = shellTargets(current().fleet?.stacks ?? []);
    const sig = list.map((t) => t.value).join(",");
    if (target.dataset.sig === sig) return;
    target.dataset.sig = sig;
    target.replaceChildren(
      h("option", { value: "" }, "Choose a container…"),
      ...list.map((t) => h("option", { value: t.value }, t.label)),
    );
    if (wanted && list.some((t) => t.value === wanted)) target.value = wanted;
    else if (wanted) other.value = wanted;
  };
  fillTargets();
  const unsub = subscribe(fillTargets);
  const vmid = () => other.value.trim() || target.value;
  const remember = () => {
    wanted = vmid();
    history.replaceState(
      history.state,
      "",
      location.pathname + setParams(location.search, { vmid: wanted }),
    );
  };
  target.addEventListener("change", () => {
    other.value = "";
    remember();
  });
  other.addEventListener("change", remember);

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
    const body = /** @type {HTMLElement} */ (el.querySelector(".shell-result"));
    body.replaceChildren(
      r.done
        ? h(
            "pre",
            {
              class: `shell-output mono${r.ok ? "" : " shell-output--failed"}`,
            },
            r.exit == null
              ? r.output
              : `${r.output}${r.output.endsWith("\n") || !r.output ? "" : "\n"}(exit ${r.exit})`,
          )
        : h(
            "p",
            { class: "measured" },
            j.state === "queued"
              ? "queued behind the host's current operation…"
              : "running…",
          ),
    );
    if (r.done) pending.delete(job);
    out.scrollTop = out.scrollHeight;
  };
  const offJobs = onAct("jobs", (id) => {
    if (id == null) for (const j of pending.keys()) paintJob(j);
    else if (pending.has(id)) paintJob(id);
  });

  const submit = async () => {
    const cmd = line.value.trim();
    const n = vmid();
    if (!cmd) return;
    if (!/^\d{3,5}$/.test(n)) {
      target.focus();
      out.append(
        refusalCallout(
          {
            what: "the shell",
            why: "no container is chosen",
            fix: "choose one, or type its vmid",
          },
          "warning",
        ),
      );
      return;
    }
    recall.unshift(cmd);
    recall.splice(RECALL);
    recallAt = -1;
    line.value = "";
    const entry = h(
      "div",
      { class: "shell-entry" },
      h("p", { class: "shell-cmd mono" }, `CT ${n} # ${cmd}`),
      h(
        "div",
        { class: "shell-result" },
        h("p", { class: "measured" }, "sending…"),
      ),
    );
    out.append(entry);
    out.scrollTop = out.scrollHeight;
    const r = await send(
      "POST",
      "/data/actions/_host/exec",
      { vmid: n, command: cmd },
      `exec in CT ${n}`,
    );
    if (!r.ok) {
      /** @type {HTMLElement} */ (
        entry.querySelector(".shell-result")
      ).replaceChildren(refusalCallout(r.error));
      return;
    }
    const job = /** @type {number} */ (r.body.job);
    entry.dataset.job = String(job);
    pending.set(job, entry);
    paintJob(job);
  };
  run.addEventListener("click", () => void submit());
  line.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      void submit();
    } else if (e.key === "ArrowUp" && recall.length) {
      e.preventDefault();
      recallAt = Math.min(recallAt + 1, recall.length - 1);
      line.value = recall[recallAt];
    } else if (e.key === "ArrowDown" && recallAt >= 0) {
      e.preventDefault();
      recallAt -= 1;
      line.value = recallAt >= 0 ? recall[recallAt] : "";
    }
  });
  line.focus();
  return () => {
    unsub();
    offJobs();
  };
}
