// Milestone edit: every edit form described once, as data, like
// actionforms.js. The stack settings form (feat-stacks-2), the firewall
// rule form and the firewall model (feat-firewall-1), the new-stack wizard
// (feat-stacks-3) and the host settings fields (feat-settings-1). Each
// field has a stable id and each step a name, so milestone `follow`
// (feat-platform-10) can replay a wizard step by step; the `*Body`
// functions are the one place typed values become the request the server
// takes.
//
// Pure: no DOM, no fetch, no clock.

/**
 * @typedef {"number" | "check" | "text" | "textarea" | "choice" | "typed"} FieldKind
 * @typedef {{name: string, id: string, kind: FieldKind, label: string,
 *   help: string, required?: boolean, min?: number, max?: number,
 *   unit?: string, pattern?: string, placeholder?: string,
 *   choices?: {value: string, label: string}[], expect?: string,
 *   current?: string | boolean}} EditField
 * @typedef {{id: string, label: string, fields: EditField[]}} EditStep
 * @typedef {Record<string, string | boolean>} Values
 * @typedef {{cores: number, memory_mb: number, swap_mb: number,
 *   disk_gb: number, storage?: string}} Resources
 * @typedef {{vmid: number, hostname: string, ip: string,
 *   resources: Resources, boot: {onboot: boolean, order?: number | null},
 *   protection: boolean, apps: string[], natives: string[],
 *   firewall: FirewallSpec | null}} ManifestView
 * @typedef {{dir: "in" | "out", action: "ACCEPT" | "DROP" | "REJECT",
 *   source?: string | null, dest?: string | null,
 *   proto?: "tcp" | "udp" | "icmp" | null, dport?: string | null,
 *   comment?: string | null, note?: string | null}} Rule
 * @typedef {{enabled: boolean, comment?: string | null,
 *   policy_in: "ACCEPT" | "DROP" | "REJECT",
 *   policy_out: "ACCEPT" | "DROP" | "REJECT",
 *   management_open?: string | null, rules: Rule[]}} FirewallSpec
 * @typedef {{origin: number | null, rule: Rule}} RuleEdit
 * @typedef {{enabled: boolean, comment: string, policy_in: string,
 *   policy_out: string, management_open: string, rules: RuleEdit[]}} FirewallModel
 */

/** The step every edit ends on: the plan, then the commit. */
export const PLAN = /** @type {const} */ ("plan");

/** What can follow a commit, in the order the plan offers it. */
export const FOLLOW = /** @type {const} */ ({
  none: "Commit only",
  deploy: "Commit, then deploy exactly this commit",
  resize: "Commit, then resize the running container",
});

/**
 * The plan step's fields: the commit subject, a note, what follows.
 * @param {string[]} followUps what the plan offers (`deploy`, `resize`)
 * @param {string} subject the plan's subject
 * @returns {EditField[]}
 */
export function commitFields(followUps, subject) {
  const offered = ["none", ...followUps.filter((f) => f in FOLLOW)];
  return [
    {
      name: "subject",
      id: "edit-subject",
      kind: "text",
      label: "Commit subject",
      help: "The first line of the commit; a feature id in brackets is added when it names none.",
      required: true,
      current: subject,
    },
    {
      name: "note",
      id: "edit-note",
      kind: "textarea",
      label: "Why (optional)",
      help: "A sentence for the commit body: why this changes.",
    },
    {
      name: "follow",
      id: "edit-follow",
      kind: "choice",
      label: "After the commit",
      help: "A deploy goes through the Jobs queue with its progress; it deploys this very commit.",
      choices: offered.map((f) => ({
        value: f,
        label: FOLLOW[/** @type {keyof typeof FOLLOW} */ (f)],
      })),
      current: followUps.includes("deploy") ? "deploy" : "none",
    },
  ];
}

/**
 * feat-stacks-2: the settings form of one stack.
 * @param {string} stack
 * @param {ManifestView} m
 * @param {Record<string, string>} images `<app>/<service>` → image
 * @returns {{id: string, stack: string, title: string, steps: EditStep[]}}
 */
export function settingsForm(stack, m, images) {
  /** @type {EditField[]} */
  const fields = [
    num(
      "cores",
      "Cores",
      "CPU cores the container may use.",
      1,
      64,
      "",
      m.resources.cores,
    ),
    num(
      "memory_mb",
      "Memory",
      "Memory limit; raising it takes a Resize, lowering it a rebuild.",
      128,
      262144,
      "MB",
      m.resources.memory_mb,
    ),
    num(
      "swap_mb",
      "Swap",
      "Swap limit; 0 is valid.",
      0,
      65536,
      "MB",
      m.resources.swap_mb,
    ),
    num(
      "disk_gb",
      "Disk",
      "Root disk size; Proxmox can grow a disk, never shrink it.",
      2,
      4096,
      "GB",
      m.resources.disk_gb,
    ),
    {
      name: "onboot",
      id: "edit-onboot",
      kind: "check",
      label: "Start on boot",
      help: "Whether the container starts when pve starts (the deploy puts it back, W3).",
      current: m.boot.onboot,
    },
    num(
      "order",
      "Boot order",
      "Lower starts earlier; everything behind the edge waits for Traefik.",
      0,
      9999,
      "",
      m.boot.order ?? "",
    ),
    {
      name: "protection",
      id: "edit-protection",
      kind: "check",
      label: "Proxmox protection",
      help: "Refuses a destroy at the hypervisor; a rebuild applies a change.",
      current: m.protection,
    },
  ];
  for (const [key, image] of Object.entries(images).sort())
    fields.push({
      name: `image:${key}`,
      id: `edit-image-${key.replace(/[^a-z0-9]+/gi, "-")}`,
      kind: "text",
      label: `Image of ${key}`,
      help: "The image reference in the app's compose file, e.g. name:1.2.3.",
      required: true,
      pattern: "[A-Za-z0-9._/:@\\-]{1,255}",
      current: image,
    });
  return {
    id: `edit:settings:${stack}`,
    stack,
    title: `Settings · ${stack}`,
    steps: [{ id: "settings", label: "Settings", fields }],
  };
}

/**
 * @param {string} name @param {string} label @param {string} help
 * @param {number} min @param {number} max @param {string} unit
 * @param {number | string} current
 * @returns {EditField}
 */
function num(name, label, help, min, max, unit, current) {
  return {
    name,
    id: `edit-${name.replace(/_/g, "-")}`,
    kind: "number",
    label: unit ? `${label} (${unit})` : label,
    help,
    min,
    max,
    unit,
    required: name !== "order",
    current: String(current),
  };
}

/**
 * The values a form starts with: each field's current value.
 * @param {{steps: EditStep[]}} form
 * @returns {Values}
 */
export function startValues(form) {
  /** @type {Values} */
  const v = {};
  for (const f of form.steps.flatMap((s) => s.fields))
    v[f.name] = f.current ?? (f.kind === "check" ? false : "");
  return v;
}

/**
 * What is wrong with the values, by field name.
 * @param {{steps: EditStep[]}} form
 * @param {Values} values
 * @returns {Record<string, string>}
 */
export function checkFields(form, values) {
  /** @type {Record<string, string>} */
  const errors = {};
  for (const f of form.steps.flatMap((s) => s.fields)) {
    const v = values[f.name];
    if (f.kind === "check") continue;
    const text = typeof v === "string" ? v.trim() : "";
    if (text === "") {
      if (f.required) errors[f.name] = `${f.label} is needed.`;
      continue;
    }
    if (f.kind === "number") {
      const n = Number(text);
      if (!/^\d+$/.test(text) || n < (f.min ?? 0) || n > (f.max ?? Infinity))
        errors[f.name] =
          `${f.label} must be a whole number from ${f.min} to ${f.max}.`;
    } else if (f.kind === "typed" && text !== f.expect)
      errors[f.name] = `Type ${f.expect} exactly to confirm.`;
    else if (f.pattern && !new RegExp(`^(?:${f.pattern})$`).test(text))
      errors[f.name] = `"${text}" is not a valid ${f.label.toLowerCase()}.`;
  }
  return errors;
}

/**
 * The settings edit the server takes: only what differs from now.
 * @param {ReturnType<typeof settingsForm>} form
 * @param {Values} values
 * @returns {{kind: "settings"} & Record<string, unknown>}
 */
export function settingsBody(form, values) {
  /** @type {{kind: "settings"} & Record<string, unknown>} */
  const out = { kind: "settings" };
  /** @type {Record<string, string>} */
  const images = {};
  for (const f of form.steps[0].fields) {
    const v = values[f.name];
    if (f.kind === "check") {
      if (v !== f.current) out[f.name] = v === true;
    } else if (typeof v === "string" && v.trim() !== String(f.current ?? "")) {
      if (f.name.startsWith("image:")) images[f.name.slice(6)] = v.trim();
      else if (v.trim() !== "") out[f.name] = Number(v.trim());
    }
  }
  if (Object.keys(images).length) out.images = images;
  return out;
}

/**
 * Whether an edit body changes anything.
 * @param {Record<string, unknown>} body
 */
export const changesSomething = (body) =>
  Object.keys(body).some((k) => k !== "kind");

// ── feat-firewall-1 ─────────────────────────────────────────────────────

/**
 * The firewall as the editor holds it; a stack without one starts closed:
 * inbound DROP, outbound ACCEPT, not yet enabled.
 * @param {FirewallSpec | null} fw
 * @returns {FirewallModel}
 */
export function firewallModel(fw) {
  return {
    enabled: fw?.enabled ?? false,
    comment: fw?.comment ?? "",
    policy_in: fw?.policy_in ?? "DROP",
    policy_out: fw?.policy_out ?? "ACCEPT",
    management_open: fw?.management_open ?? "",
    rules: (fw?.rules ?? []).map((rule, i) => ({ origin: i, rule })),
  };
}

/**
 * The firewall edit the server takes.
 * @param {FirewallModel} m
 */
export function firewallBody(m) {
  return {
    kind: "firewall",
    enabled: m.enabled,
    comment: m.comment.trim() ? m.comment : null,
    policy_in: m.policy_in,
    policy_out: m.policy_out,
    management_open: m.management_open.trim() ? m.management_open.trim() : null,
    rules: m.rules.map((r) => ({ origin: r.origin, rule: cleanRule(r.rule) })),
  };
}

/** @param {Rule} r @returns {Rule} */
function cleanRule(r) {
  /** @type {Rule} */
  const out = { dir: r.dir, action: r.action };
  for (const k of /** @type {const} */ ([
    "source",
    "dest",
    "proto",
    "dport",
    "comment",
    "note",
  ])) {
    const v = r[k];
    if (v != null && String(v).trim() !== "")
      /** @type {Record<string, unknown>} */ (out)[k] =
        k === "comment" ? v : String(v).trim();
  }
  return out;
}

/**
 * Whether the model differs from the file's firewall.
 * @param {FirewallModel} m
 * @param {FirewallSpec | null} fw
 */
export function firewallChanged(m, fw) {
  return (
    JSON.stringify(firewallBody(m)) !==
    JSON.stringify(firewallBody(firewallModel(fw)))
  );
}

/**
 * One rule in words, for a table row and a screen reader.
 * @param {Rule} r
 */
export function ruleSummary(r) {
  const peer = r.dir === "in" ? r.source : r.dest;
  const where = r.dir === "in" ? "from" : "to";
  const parts = [r.dir.toUpperCase(), r.action];
  parts.push(peer ? `${where} ${peer}` : `${where} anywhere`);
  if (r.proto) parts.push(r.proto);
  if (r.dport) parts.push(`port ${r.dport}`);
  return parts.join(" ");
}

/**
 * The rule form (the dialog behind Add and Edit).
 * @param {Rule | null} r the rule being edited, or null for a new one
 * @returns {EditField[]}
 */
export function ruleFields(r) {
  const peer = r ? (r.dir === "in" ? r.source : r.dest) : "";
  return [
    {
      name: "dir",
      id: "rule-dir",
      kind: "choice",
      label: "Direction",
      help: "In: who may reach this container. Out: where it may go.",
      choices: [
        { value: "in", label: "In (to this container)" },
        { value: "out", label: "Out (from this container)" },
      ],
      current: r?.dir ?? "in",
    },
    {
      name: "action",
      id: "rule-action",
      kind: "choice",
      label: "Action",
      help: "The first rule that matches decides; the policy applies when none does.",
      choices: [
        { value: "ACCEPT", label: "Accept" },
        { value: "DROP", label: "Drop" },
        { value: "REJECT", label: "Reject" },
      ],
      current: r?.action ?? "ACCEPT",
    },
    {
      name: "peer",
      id: "rule-peer",
      kind: "text",
      label: "Other side",
      help: "One address (10.10.10.4) or a network (10.10.10.0/24); empty: anywhere.",
      pattern: "\\d{1,3}(\\.\\d{1,3}){3}(/\\d{1,2})?",
      placeholder: "10.10.10.4",
      current: peer ?? "",
    },
    {
      name: "proto",
      id: "rule-proto",
      kind: "choice",
      label: "Protocol",
      help: "A port needs tcp or udp; icmp has no ports.",
      choices: [
        { value: "", label: "Any" },
        { value: "tcp", label: "tcp" },
        { value: "udp", label: "udp" },
        { value: "icmp", label: "icmp" },
      ],
      current: r?.proto ?? "tcp",
    },
    {
      name: "dport",
      id: "rule-dport",
      kind: "text",
      label: "Ports",
      help: "8080, 8080,8787 or 5000:5003; empty: every port.",
      pattern: "\\d{1,5}([:,]\\d{1,5})*",
      placeholder: "8080",
      current: r?.dport ?? "",
    },
    {
      name: "note",
      id: "rule-note",
      kind: "text",
      label: "Note",
      help: "One line, written after the rule in pve's file (who this is for).",
      current: r?.note ?? "",
    },
    {
      name: "comment",
      id: "rule-comment",
      kind: "textarea",
      label: "Comment above the rule",
      help: "Optional lines written above the rule: why it exists.",
      current: r?.comment ?? "",
    },
  ];
}

/**
 * The rule from its form's values.
 * @param {Values} v
 * @returns {Rule}
 */
export function ruleFromValues(v) {
  const s = (/** @type {string} */ k) => String(v[k] ?? "").trim();
  const dir = s("dir") === "out" ? "out" : "in";
  /** @type {Rule} */
  const r = {
    dir,
    action: /** @type {Rule["action"]} */ (s("action") || "ACCEPT"),
  };
  if (s("peer")) r[dir === "in" ? "source" : "dest"] = s("peer");
  if (s("proto")) r.proto = /** @type {Rule["proto"]} */ (s("proto"));
  if (s("dport")) r.dport = s("dport").replace(/\s+/g, "");
  if (s("note")) r.note = s("note");
  const c = String(v.comment ?? "").trimEnd();
  if (c.trim()) r.comment = c;
  return r;
}

/**
 * What Proxmox would reject or misread, by field (the server checks again
 * with homelab-core).
 * @param {Values} v
 * @returns {Record<string, string>}
 */
export function ruleProblems(v) {
  /** @type {Record<string, string>} */
  const out = {};
  const peer = String(v.peer ?? "").trim();
  if (peer) {
    const m =
      /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})(?:\/(\d{1,2}))?$/.exec(peer);
    if (!m || m.slice(1, 5).some((o) => Number(o) > 255))
      out.peer = `${peer} is not an address like 10.10.10.4 or a network like 10.10.10.0/24.`;
    else if (m[5] !== undefined) {
      const bits = Number(m[5]);
      const ip =
        ((Number(m[1]) << 24) >>> 0) +
        (Number(m[2]) << 16) +
        (Number(m[3]) << 8) +
        Number(m[4]);
      const mask = bits === 0 ? 0 : (0xffffffff << (32 - bits)) >>> 0;
      if (bits > 32) out.peer = `${peer} has a prefix longer than 32.`;
      else if ((ip & mask) >>> 0 !== ip)
        out.peer = `${peer} has host bits set: Proxmox reads it as the whole network. Write one address, or the network.`;
    }
  }
  const ports = String(v.dport ?? "").replace(/\s+/g, "");
  const proto = String(v.proto ?? "");
  if (ports) {
    if (proto === "" || proto === "icmp")
      out.dport =
        proto === "icmp"
          ? "icmp has no ports; clear them."
          : "A port needs tcp or udp.";
    else
      for (const part of ports.split(",")) {
        const [a, b] = part.split(":").map(Number);
        const ok = (/** @type {number} */ n) =>
          Number.isInteger(n) && n >= 1 && n <= 65535;
        if (!ok(a) || (part.includes(":") && (!ok(b) || b <= a))) {
          out.dport = `${part} is not a port 1 to 65535 or a forward range.`;
          break;
        }
      }
  }
  if (String(v.note ?? "").includes("\n")) out.note = "A note is one line.";
  return out;
}

/**
 * Move a rule up (-1) or down (+1); rules are read top to bottom.
 * @param {FirewallModel} m
 * @param {number} i
 * @param {number} delta
 * @returns {FirewallModel}
 */
export function moveRule(m, i, delta) {
  const j = i + delta;
  if (j < 0 || j >= m.rules.length) return m;
  const rules = [...m.rules];
  [rules[i], rules[j]] = [rules[j], rules[i]];
  return { ...m, rules };
}

// ── feat-stacks-3: the new-stack wizard ──────────────────────────────────

/**
 * @typedef {{name: string, description: string, ram_mb: number,
 *   cores: number, disk_gb: number, apps: string[], gpu: boolean,
 *   vpn: boolean}} Preset
 */

/**
 * The wizard, as data: its steps in order, every field with a stable id.
 * The data step's fields depend on the preset and are made by
 * `dataFields` when that step opens.
 * @param {Preset[]} presets
 * @param {number | null} suggestVmid
 * @returns {{id: string, title: string, steps: EditStep[]}}
 */
export function newStackWizard(presets, suggestVmid) {
  const first = presets[0];
  return {
    id: "new-stack",
    title: "New stack",
    steps: [
      {
        id: "preset",
        label: "Preset",
        fields: [
          {
            name: "preset",
            id: "new-preset",
            kind: "choice",
            label: "Start from",
            help: "A preset from the repository's presets/ directory: its apps, their compose files and a size.",
            required: true,
            choices: presets.map((p) => ({
              value: p.name,
              label: `${p.name} · ${p.description}`,
            })),
            current: first?.name ?? "",
          },
        ],
      },
      {
        id: "identity",
        label: "Name",
        fields: [
          {
            name: "name",
            id: "new-name",
            kind: "text",
            label: "Stack name",
            help: "Lowercase letters, digits and dashes; it names the directory, the container and its data.",
            required: true,
            pattern: "[a-z0-9][a-z0-9\\-]{0,31}",
            placeholder: "recipes",
          },
          {
            name: "vmid",
            id: "new-vmid",
            kind: "number",
            label: "Container number",
            help: "The vmid; its address becomes 10.10.10.<number − 100>.",
            required: true,
            min: 102,
            max: 354,
            current: suggestVmid == null ? "" : String(suggestVmid),
          },
        ],
      },
      {
        id: "size",
        label: "Size",
        fields: [
          num(
            "ram_mb",
            "Memory",
            "The container's memory limit.",
            128,
            262144,
            "MB",
            first?.ram_mb ?? 1024,
          ),
          num("cores", "Cores", "CPU cores.", 1, 64, "", first?.cores ?? 2),
          num(
            "disk_gb",
            "Disk",
            "Root disk size.",
            2,
            4096,
            "GB",
            first?.disk_gb ?? 32,
          ),
          {
            ...num(
              "swap_mb",
              "Swap",
              "Empty: a quarter of the memory, 512 MB to 2 GB.",
              0,
              65536,
              "MB",
              "",
            ),
            id: "new-swap-mb",
            required: false,
          },
        ].map((f) => ({ ...f, id: f.id.replace(/^edit-/, "new-") })),
      },
      { id: "data", label: "Data", fields: [] },
      { id: PLAN, label: "Plan and commit", fields: [] },
    ],
  };
}

/**
 * The size fields follow the chosen preset.
 * @param {Preset | undefined} p
 * @returns {Values}
 */
export const presetSize = (p) =>
  p
    ? {
        ram_mb: String(p.ram_mb),
        cores: String(p.cores),
        disk_gb: String(p.disk_gb),
      }
    : {};

/**
 * The data step: one check per `/appdata` folder the preset's apps bind,
 * ticked when that app keeps nothing of its own.
 * @param {string[]} paths
 * @returns {EditField[]}
 */
export const dataFields = (paths) =>
  paths.map((p, i) => ({
    name: `nodata:${p}`,
    id: `new-nodata-${i}`,
    kind: /** @type {const} */ ("check"),
    label: `${p} keeps nothing`,
    help: "Ticked: the app keeps no files of its own here, so it gets no backup repository (no_data). Leave it off when in doubt.",
    current: false,
  }));

/**
 * What the wizard sends.
 * @param {Values} v
 */
export function newStackBody(v) {
  const n = (/** @type {string} */ k) => Number(String(v[k] ?? "").trim());
  const swap = String(v.swap_mb ?? "").trim();
  return {
    name: String(v.name ?? "").trim(),
    vmid: n("vmid"),
    preset: String(v.preset ?? ""),
    ram_mb: n("ram_mb"),
    cores: n("cores"),
    disk_gb: n("disk_gb"),
    ...(swap ? { swap_mb: Number(swap) } : {}),
    no_data: Object.entries(v)
      .filter(([k, x]) => k.startsWith("nodata:") && x === true)
      .map(([k]) => k.slice(7)),
  };
}

/**
 * The wizard's own checks of one step, before the server's.
 * @param {ReturnType<typeof newStackWizard>} w
 * @param {string} step
 * @param {Values} v
 * @param {{names: string[], vmids: number[]}} taken
 * @returns {Record<string, string>}
 */
export function checkNewStep(w, step, v, taken) {
  const s = w.steps.find((x) => x.id === step);
  if (!s) return {};
  const errors = checkFields({ steps: [s] }, v);
  if (step === "identity") {
    const name = String(v.name ?? "").trim();
    if (!errors.name && taken.names.includes(name))
      errors.name = `There is a stack called ${name} already.`;
    const vmid = Number(v.vmid);
    if (!errors.vmid && taken.vmids.includes(vmid))
      errors.vmid = `CT ${vmid} exists already.`;
    if (!errors.vmid && [100, 101, 102, 103].includes(vmid))
      errors.vmid = `${vmid} is on the no-touch list.`;
  }
  return errors;
}

// ── feat-settings-1 ─────────────────────────────────────────────────────

/**
 * @typedef {{type: "int", min: number, max: number} | {type: "bool"} |
 *   {type: "text"} | {type: "url"} | {type: "vmid"} | {type: "vmid_list"} |
 *   {type: "text_list"} | {type: "window"} | {type: "table"}} KeyKind
 * @typedef {{key: string, group: string, label: string, help: string,
 *   default: string, kind: KeyKind,
 *   access: "browser" | "confirm" | "locked" | "ssh_only" | "secret",
 *   apply: "live" | "restart", set: boolean, value: unknown,
 *   toml: string | null}} HostField
 */

/** Who changes a key, in words. */
export const ACCESS = /** @type {const} */ ({
  browser: "Here",
  confirm: "Here with the name typed",
  locked: "ssh only (can cut the dashboard off)",
  ssh_only: "ssh only (safety policy)",
  secret: "ssh only (secret)",
});

/** @param {HostField} f */
export const editable = (f) => f.access === "browser" || f.access === "confirm";

/**
 * A key's value as the table shows it.
 * @param {HostField} f
 */
export function valueText(f) {
  if (f.access === "secret") return f.set ? "set (hidden)" : "not set";
  if (!f.set) return `default: ${f.default}`;
  const v = f.value;
  if (f.kind.type === "table") {
    const n = Array.isArray(v) ? v.length : 1;
    return Array.isArray(v) ? `${n} ${n === 1 ? "entry" : "entries"}` : "set";
  }
  if (Array.isArray(v)) return v.join(", ");
  if (typeof v === "boolean") return v ? "on" : "off";
  if (f.kind.type === "int" && /_s$/.test(f.key) && typeof v === "number")
    return seconds(v);
  return String(v);
}

/** @param {number} s */
function seconds(s) {
  if (s % 86400 === 0 && s >= 86400) return `${s / 86400} d`;
  if (s % 3600 === 0 && s >= 3600) return `${s / 3600} h`;
  if (s % 60 === 0 && s >= 60) return `${s / 60} min`;
  return `${s} s`;
}

/**
 * The text a key's field starts with.
 * @param {HostField} f
 */
export function fieldText(f) {
  if (f.kind.type === "table") return f.toml ?? "";
  if (!f.set || f.value == null) return "";
  if (Array.isArray(f.value)) return f.value.join(", ");
  return String(f.value);
}

/**
 * A typed text as the key's JSON value; empty removes the key.
 * @param {HostField} f
 * @param {string | boolean} input
 * @returns {{ok: true, value: unknown} | {ok: false, why: string}}
 */
export function parseKey(f, input) {
  const k = f.kind;
  if (k.type === "bool")
    return { ok: true, value: input === true || input === "true" };
  const t = String(input).trim();
  if (t === "") return { ok: true, value: null };
  switch (k.type) {
    case "int": {
      const n = Number(t);
      return /^\d+$/.test(t) && n >= k.min && n <= k.max
        ? { ok: true, value: n }
        : { ok: false, why: `A whole number from ${k.min} to ${k.max}.` };
    }
    case "vmid": {
      const n = Number(t);
      return /^\d+$/.test(t) && n >= 100
        ? { ok: true, value: n }
        : { ok: false, why: "A container number, 100 or more." };
    }
    case "url":
      return /^https?:\/\/\S+$/.test(t)
        ? { ok: true, value: t }
        : { ok: false, why: "An http:// or https:// address." };
    case "window":
      return /^\d+[smhdw]$/.test(t)
        ? { ok: true, value: t }
        : { ok: false, why: "A number and a unit, e.g. 24h." };
    case "vmid_list": {
      const parts = t
        .split(/[\s,]+/)
        .filter(Boolean)
        .map(Number);
      return parts.every((n) => Number.isInteger(n) && n >= 100)
        ? { ok: true, value: parts }
        : { ok: false, why: "Container numbers separated by commas." };
    }
    case "text_list":
      return {
        ok: true,
        value: t
          .split(",")
          .map((x) => x.trim())
          .filter(Boolean),
      };
    case "table":
      return { ok: true, value: t };
    default:
      return t.includes("\n")
        ? { ok: false, why: "One line." }
        : { ok: true, value: t };
  }
}

/**
 * The staged changes as the server takes them.
 * @param {string} sha256 the file's hash as the page read it
 * @param {Map<string, {field: HostField, value: unknown}>} staged
 * @param {Set<string>} confirmed keys whose name was typed
 */
export function hostSettingsBody(sha256, staged, confirmed) {
  /** @type {Record<string, unknown>} */
  const values = {};
  /** @type {Record<string, string>} */
  const fragments = {};
  for (const [key, { field, value }] of staged) {
    if (field.kind.type === "table") fragments[key] = String(value ?? "");
    else values[key] = value;
  }
  return {
    expect_sha256: sha256,
    values,
    fragments,
    confirms: [...confirmed].filter((k) => staged.has(k)),
  };
}
