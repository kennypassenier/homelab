// Milestone edit: every edit form described once, as data, like
// actionforms.js. The stack settings form (feat-stacks-2), the firewall
// rule form and the firewall model (feat-firewall-1), the new-stack wizard
// (feat-stacks-3) and the host settings fields (feat-settings-1). Each
// field has a stable id and each step a name, so milestone `follow`
// (feat-platform-10) can replay a wizard step by step; the `*Body`
// functions are the one place typed values become the request the server
// takes. The fields' words live in formspec.json's `edit` section, which
// the dashboard's server reads too (core::driveedit): a driven step and a
// click are checked against the same description.
//
// Pure: no DOM, no fetch, no clock.

import SPEC from "./formspec.json" with { type: "json" };
import { fill } from "./actionforms.js";

/** The edit section of the one form description. */
const E = SPEC.edit;
/** @param {string} key @param {Record<string, string | number>} w */
const say = (key, w = {}) =>
  fill(
    /** @type {Record<string, string>} */ (E.messages)[key],
    Object.fromEntries(Object.entries(w).map(([k, v]) => [k, String(v)])),
  );

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
 * @typedef {{watch_every?: number | null, down_after?: number | null}} TileView
 * @typedef {{ip: string, gateway: string, bridge: string,
 *   vlan?: number | null}} NetworkView
 * @typedef {{template: string, unprivileged: boolean, features: string,
 *   protection: boolean, gpu: boolean, vpn: boolean}} LxcView
 * @typedef {{host_path: string, mount_point: string, no_data: boolean,
 *   no_backup?: string | null, host_owner_uid?: number | null,
 *   app?: string | null}} StorageView
 * @typedef {{files: string, keep?: number, reopen?:
 *   {container: string, signal?: string} | null}} RotateView
 * @typedef {{host_path: string, mount_point: string, note?: string | null,
 *   rotate?: RotateView | null}} DataMountView
 * @typedef {{path: string, job: string}} LogFileView
 * @typedef {{every_days: number, span_days?: number | null}} RetentionTierView
 * @typedef {{from: string, dest: string, mode: string, owner?: string | null,
 *   restarts?: string | null}} LatchFileView
 * @typedef {{latch_secrets: string[], latch_files: LatchFileView[]}} LatchView
 * @typedef {{vmid: number, hostname: string, ip: string,
 *   resources: Resources, boot: {onboot: boolean, order?: number | null},
 *   protection: boolean, apps: string[], natives: string[],
 *   native_only?: boolean, firewall: FirewallSpec | null,
 *   tiles?: Record<string, TileFieldsView>, network?: NetworkView, lxc?: LxcView,
 *   on_demand?: boolean, storage?: StorageView[],
 *   data_mounts?: DataMountView[], log_files?: LogFileView[],
 *   retention?: RetentionTierView[] | null,
 *   latch?: LatchView}} ManifestView
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
 * @typedef {{name: string, command: string,
 *   expect: "never_decreases" | "must_match" | "must_be_present",
 *   layer: "network" | "process" | "application" | "user_visible",
 *   blind_spot?: string | null}} CheckView
 * @typedef {{healthy: {equals: string} | {at_least: number} | {at_most: number}}
 *   & Omit<CheckView, "expect">} ProbeView
 * @typedef {{checks: CheckView[], manual: ({text: string, once?: boolean} | string)[],
 *   probes: ProbeView[], busy_check?: {command: string} | null,
 *   url?: string | null}} ChecksReadView
 * @typedef {ChecksReadView | {error: string}} ChecksView
 * @typedef {{name: string, group: string, order?: number | null,
 *   description?: string | null, url?: string | null, reading?: string | null,
 *   watch_every?: number | null, down_after?: number | null}} TileFieldsView
 */

/** The step every edit ends on: the plan, then the commit. */
export const PLAN = /** @type {const} */ ("plan");

/** What can follow a commit, in the order the plan offers it. */
export const FOLLOW = E.follow;

/**
 * The plan step's fields: the commit subject, a note, what follows.
 * @param {string[]} followUps what the plan offers (`deploy`, `resize`)
 * @param {string} subject the plan's subject
 * @returns {EditField[]}
 */
export function commitFields(followUps, subject) {
  const offered = ["none", ...followUps.filter((f) => f in FOLLOW)];
  const [s, note, follow] = /** @type {EditField[]} */ (E.commit);
  return [
    { ...s, current: subject },
    { ...note },
    {
      ...follow,
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
  /** @type {Record<string, string | boolean>} */
  const now = {
    cores: String(m.resources.cores),
    memory_mb: String(m.resources.memory_mb),
    swap_mb: String(m.resources.swap_mb),
    disk_gb: String(m.resources.disk_gb),
    onboot: m.boot.onboot,
    order: String(m.boot.order ?? ""),
    protection: m.protection,
  };
  /** @type {EditField[]} */
  const fields = /** @type {EditField[]} */ (E.settings).map((f) => ({
    ...f,
    current: now[f.name],
  }));
  for (const [key, image] of Object.entries(images).sort())
    fields.push(imageField(key, image));
  for (const [key, tile] of Object.entries(m.tiles ?? {}).sort())
    fields.push(...tileFields(key, tile));
  return {
    id: `edit:settings:${stack}`,
    stack,
    title: `Settings · ${stack}`,
    steps: [{ id: "settings", label: "Settings", fields }],
  };
}

/**
 * The field of one app's image, `<app>/<service>` = `key`.
 * @param {string} key
 * @param {string} image
 * @returns {EditField}
 */
export function imageField(key, image) {
  const w = { key, slug: key.replace(/[^a-z0-9]+/gi, "-") };
  /** @type {Record<string, unknown>} */
  const f = {};
  for (const [k, v] of Object.entries(E.image))
    f[k] = typeof v === "string" && k !== "pattern" ? fill(v, w) : v;
  return /** @type {EditField} */ ({ ...f, current: image });
}

/** The field name prefixes of a tile's two watch fields (feat-stacks-3
 * owner remark 2026-09-30, "de uptime-check tijd … in de wizard"). */
export const TILE_WATCH_PREFIX = "tile_watch_every:";
export const TILE_DOWN_PREFIX = "tile_down_after:";

/**
 * The two optional watch fields of one tile, `<host>` = `key`.
 * @param {string} key
 * @param {TileView} tile
 * @returns {EditField[]}
 */
export function tileFields(key, tile) {
  const w = { key, slug: key.replace(/[^a-z0-9]+/gi, "-") };
  const build = (
    /** @type {Record<string, unknown>} */ tpl,
    /** @type {string} */ current,
  ) => {
    /** @type {Record<string, unknown>} */
    const f = {};
    for (const [k, v] of Object.entries(tpl))
      f[k] = typeof v === "string" && k !== "pattern" ? fill(v, w) : v;
    return /** @type {EditField} */ ({ ...f, current });
  };
  return [
    build(
      E.tile_watch,
      tile.watch_every != null ? String(tile.watch_every) : "",
    ),
    build(E.tile_down, tile.down_after != null ? String(tile.down_after) : ""),
  ];
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
      if (f.required) errors[f.name] = say("needed", { label: f.label });
      continue;
    }
    if (f.kind === "number") {
      const n = Number(text);
      if (!/^\d+$/.test(text) || n < (f.min ?? 0) || n > (f.max ?? Infinity))
        errors[f.name] = say("number", {
          label: f.label,
          min: String(f.min),
          max: String(f.max),
        });
    } else if (f.kind === "typed" && text !== f.expect)
      errors[f.name] = say("typed", { expect: f.expect ?? "" });
    else if (f.pattern && !new RegExp(`^(?:${f.pattern})$`).test(text))
      errors[f.name] = say("pattern", { text, lower: f.label.toLowerCase() });
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
  /** @type {Record<string, {watch_every?: number, down_after?: number}>} */
  const tiles = {};
  for (const f of form.steps[0].fields) {
    const v = values[f.name];
    if (f.kind === "check") {
      if (v !== f.current) out[f.name] = v === true;
    } else if (typeof v === "string" && v.trim() !== String(f.current ?? "")) {
      if (f.name.startsWith("image:")) images[f.name.slice(6)] = v.trim();
      else if (f.name.startsWith(TILE_WATCH_PREFIX)) {
        if (v.trim() !== "")
          (tiles[f.name.slice(TILE_WATCH_PREFIX.length)] ??= {}).watch_every =
            Number(v.trim());
      } else if (f.name.startsWith(TILE_DOWN_PREFIX)) {
        if (v.trim() !== "")
          (tiles[f.name.slice(TILE_DOWN_PREFIX.length)] ??= {}).down_after =
            Number(v.trim());
      } else if (v.trim() !== "") out[f.name] = Number(v.trim());
    }
  }
  if (Object.keys(images).length) out.images = images;
  if (Object.keys(tiles).length) out.tiles = tiles;
  return out;
}

/**
 * What is wrong across a tile's two watch fields together (the per-field
 * checks only see one field at a time): down after must be at least check
 * every, when both are typed.
 * @param {Values} values
 * @returns {Record<string, string>}
 */
export function tileProblems(values) {
  /** @type {Record<string, string>} */
  const errors = {};
  /** @type {Set<string>} */
  const keys = new Set();
  for (const name of Object.keys(values)) {
    if (name.startsWith(TILE_WATCH_PREFIX))
      keys.add(name.slice(TILE_WATCH_PREFIX.length));
    else if (name.startsWith(TILE_DOWN_PREFIX))
      keys.add(name.slice(TILE_DOWN_PREFIX.length));
  }
  for (const key of keys) {
    const w = String(values[`${TILE_WATCH_PREFIX}${key}`] ?? "").trim();
    const d = String(values[`${TILE_DOWN_PREFIX}${key}`] ?? "").trim();
    if (w === "" || d === "") continue;
    const wn = Number(w);
    const dn = Number(d);
    if (Number.isFinite(wn) && Number.isFinite(dn) && dn < wn)
      errors[`${TILE_DOWN_PREFIX}${key}`] = say("tile_down_low", {
        key,
        watch: wn,
        down: dn,
      });
  }
  return errors;
}

/**
 * Whether an edit body changes anything.
 * @param {Record<string, unknown>} body
 */
export const changesSomething = (body) =>
  Object.keys(body).some((k) => k !== "kind");

// ── feat-stacks-9: network, lxc flags, storage, on_demand, retention ────

/**
 * The settings-extension form: a second page next to Settings, so neither
 * grows past what the dashboard shows at once.
 * @param {string} stack
 * @param {ManifestView} m
 * @returns {{id: string, stack: string, title: string, steps: EditStep[]}}
 */
export function settingsExtForm(stack, m) {
  /** @type {Record<string, string | boolean>} */
  const now = {
    ip: m.network?.ip ?? "",
    gateway: m.network?.gateway ?? "",
    bridge: m.network?.bridge ?? "",
    vlan: m.network?.vlan != null ? String(m.network.vlan) : "",
    unprivileged: m.lxc?.unprivileged ?? true,
    gpu: m.lxc?.gpu ?? false,
    vpn: m.lxc?.vpn ?? false,
    storage: m.resources?.storage ?? "",
    on_demand: m.on_demand ?? false,
  };
  /** @type {EditField[]} */
  const fields = /** @type {EditField[]} */ (E.settings_ext).map((f) => ({
    ...f,
    current: now[f.name],
  }));
  return {
    id: `edit:settings-ext:${stack}`,
    stack,
    title: `Network & hardware · ${stack}`,
    steps: [{ id: "settings_ext", label: "Network & hardware", fields }],
  };
}

/**
 * @param {ReturnType<typeof settingsExtForm>} form
 * @param {Values} values
 * @param {RowEdit[] | null} retention rows if the retention table changed, else null (unchanged)
 * @returns {{kind: "settings_ext"} & Record<string, unknown>}
 */
export function settingsExtBody(form, values, retention) {
  /** @type {{kind: "settings_ext"} & Record<string, unknown>} */
  const out = { kind: "settings_ext" };
  for (const f of form.steps[0].fields) {
    const v = values[f.name];
    if (f.kind === "check") {
      if (v !== f.current) out[f.name] = v === true;
      continue;
    }
    const text = typeof v === "string" ? v.trim() : "";
    if (text === "" || text === String(f.current ?? "")) continue;
    out[f.name] = f.kind === "number" ? Number(text) : text;
  }
  // W2: a plain list, not origin-tracked like storage/data_mounts/log_files
  // — the server takes the whole end state (`SettingsExtEdit.retention`,
  // `Option<Vec<RetentionTierEdit>>`); an empty list clears this stack's
  // own tiers and goes back to the fleet-wide policy.
  if (retention)
    out.retention = retention.map((r) => retentionRowFromRow(r.row));
  return out;
}

/**
 * @param {Record<string, unknown> | null} row
 * @returns {EditField[]}
 */
export function retentionRowFields(row) {
  const from = row ?? {};
  return /** @type {EditField[]} */ (E.retention_tier).map((f) => ({
    ...f,
    current:
      from[f.name] != null ? String(/** @type {unknown} */ (from[f.name])) : "",
  }));
}

/** @param {Values} v */
export function retentionRowFromValues(v) {
  /** @type {Record<string, unknown>} */
  const out = { every_days: Number(String(v.every_days ?? "").trim()) };
  const span = String(v.span_days ?? "").trim();
  if (span !== "") out.span_days = Number(span);
  return out;
}

/** @param {Record<string, unknown>} row */
function retentionRowFromRow(row) {
  return retentionRowFromValues(
    /** @type {Values} */ ({
      every_days: row.every_days,
      span_days: row.span_days,
    }),
  );
}

/** @param {Values} v */
export function retentionRowProblems(v) {
  /** @type {Record<string, string>} */
  const errors = {};
  const every = String(v.every_days ?? "").trim();
  const span = String(v.span_days ?? "").trim();
  if (every !== "" && span !== "" && Number(span) < Number(every))
    errors.span_days = "Must be at least the keep-one-every span.";
  return errors;
}

/** @param {Record<string, unknown>} row */
export function retentionRowSummary(row) {
  return row.span_days != null
    ? `every ${row.every_days}d, kept ${row.span_days}d`
    : `every ${row.every_days}d, kept forever`;
}

// ── feat-checks-1: origin-tracked JSON list fields (checks.yml) ─────────
//
// `checks.yml`'s own lists (checks/manual/probes, Area B) are all "send
// the end state, origins track the old rows" lists on the server (the
// same shape `FirewallEdit.rules` uses, `yamledit::Item::{Keep,Retext,New}`
// under it) and are drawn as one JSON textarea per list, pre-filled with
// the current rows and their index as `origin` so a row left untouched
// keeps its file comments. Storage entries, data mounts, log files and
// latch files (feat-stacks-10/11, below) used to be drawn the same way;
// they now have their own row table + dialog (`rowModel`/`rowBody` and
// the per-list field functions further down), the same shape the
// Firewall tab's rule table uses.

/**
 * @param {Record<string, unknown>[]} rows
 */
export function originJson(rows) {
  return JSON.stringify(
    rows.map((r, i) => ({ origin: i, ...r })),
    null,
    2,
  );
}

/**
 * @param {string} text
 * @param {string[]} keys the fields every row must have, besides `origin`
 * @returns {{ok: true, rows: Record<string, unknown>[]} | {ok: false, error: string}}
 */
export function parseJsonList(text, keys) {
  /** @type {unknown} */
  let parsed;
  try {
    parsed = JSON.parse(text.trim() === "" ? "[]" : text);
  } catch {
    return { ok: false, error: "Not valid JSON." };
  }
  if (!Array.isArray(parsed))
    return { ok: false, error: "Must be a JSON list." };
  for (const row of parsed) {
    if (!row || typeof row !== "object" || Array.isArray(row))
      return { ok: false, error: "Every row must be a JSON object." };
    for (const k of keys)
      if (!(k in /** @type {Record<string, unknown>} */ (row)))
        return { ok: false, error: `Every row needs "${k}".` };
  }
  return {
    ok: true,
    rows: /** @type {Record<string, unknown>[]} */ (parsed),
  };
}

// ── feat-checks-1: check/probe/manual rows, tile rows ────────────────────
//
// checks/manual/probes are "send the end state, origin tracks the old
// row" lists like storage/data_mounts/log_files below — `rowModel`/
// `rowBody` there apply unchanged. Tiles are the odd one out:
// `stackedit_tiles::TilesEdit.tiles` is a SPARSE change list keyed by the
// tile's hostname (`origin: Option<String>`, not an array index) — a tile
// not mentioned is left alone, and deleting an EXISTING one needs an
// explicit `delete: true` tombstone rather than just being missing. So the
// tile row table still uses the generic `rowTable`/`rowModel` (its
// `origin` is simply a string instead of a number, which the generic
// machinery never inspects), but `tilesBody` below — not `rowBody` — turns
// the displayed rows into that sparse body.

/**
 * @param {Record<string, unknown> | null} row
 * @returns {EditField[]}
 */
export function checkRowFields(row) {
  const from = row ?? {};
  return /** @type {EditField[]} */ (E.check_row).map((f) => ({
    ...f,
    current: /** @type {string} */ (from[f.name] ?? f.current),
  }));
}

/** @param {Values} v */
export function checkRowFromValues(v) {
  /** @type {Record<string, unknown>} */
  const out = {
    name: String(v.name ?? "").trim(),
    command: String(v.command ?? "").trim(),
    expect: String(v.expect ?? "never_decreases"),
    layer: String(v.layer ?? "network"),
  };
  const bs = String(v.blind_spot ?? "").trim();
  if (bs) out.blind_spot = bs;
  return out;
}

/** @param {Values} v */
export function checkRowProblems(v) {
  /** @type {Record<string, string>} */
  const errors = {};
  const layer = String(v.layer ?? "");
  const bs = String(v.blind_spot ?? "").trim();
  if (layer !== "application" && layer !== "user_visible" && !bs)
    errors.blind_spot = "A check below Application layer needs a blind spot.";
  return errors;
}

/** @param {Record<string, unknown>} row */
export function checkRowSummary(row) {
  return `${row.name} (${row.layer})`;
}

/**
 * @param {Record<string, unknown> | null} row
 * @returns {EditField[]}
 */
export function probeRowFields(row) {
  const from = /** @type {Record<string, unknown>} */ (row ?? {});
  let healthyKind = "equals";
  let healthyValue = "";
  const h = /** @type {Record<string, unknown> | undefined} */ (from.healthy);
  if (h && "equals" in h) {
    healthyKind = "equals";
    healthyValue = String(h.equals);
  } else if (h && "at_least" in h) {
    healthyKind = "at_least";
    healthyValue = String(h.at_least);
  } else if (h && "at_most" in h) {
    healthyKind = "at_most";
    healthyValue = String(h.at_most);
  }
  const merged = /** @type {Record<string, unknown>} */ ({
    ...from,
    healthy_kind: healthyKind,
    healthy_value: healthyValue,
  });
  return /** @type {EditField[]} */ (E.probe_row).map((f) => ({
    ...f,
    current: /** @type {string} */ (merged[f.name] ?? f.current),
  }));
}

/** @param {Values} v */
export function probeRowFromValues(v) {
  const kind = String(v.healthy_kind ?? "equals");
  const val = String(v.healthy_value ?? "").trim();
  const healthy =
    kind === "equals"
      ? { equals: val }
      : kind === "at_most"
        ? { at_most: Number(val) }
        : { at_least: Number(val) };
  /** @type {Record<string, unknown>} */
  const out = {
    name: String(v.name ?? "").trim(),
    command: String(v.command ?? "").trim(),
    healthy,
    layer: String(v.layer ?? "network"),
  };
  const bs = String(v.blind_spot ?? "").trim();
  if (bs) out.blind_spot = bs;
  return out;
}

/** @param {Values} v */
export function probeRowProblems(v) {
  /** @type {Record<string, string>} */
  const errors = {};
  const kind = String(v.healthy_kind ?? "");
  const val = String(v.healthy_value ?? "").trim();
  if (
    (kind === "at_least" || kind === "at_most") &&
    val !== "" &&
    !/^-?\d+$/.test(val)
  )
    errors.healthy_value = "A whole number.";
  const layer = String(v.layer ?? "");
  const bs = String(v.blind_spot ?? "").trim();
  if (layer !== "application" && layer !== "user_visible" && !bs)
    errors.blind_spot = "A probe below Application layer needs a blind spot.";
  return errors;
}

/** @param {Record<string, unknown>} row */
export function probeRowSummary(row) {
  const h = /** @type {Record<string, unknown>} */ (row.healthy ?? {});
  const word =
    "equals" in h
      ? `= ${h.equals}`
      : "at_least" in h
        ? `>= ${h.at_least}`
        : "at_most" in h
          ? `<= ${h.at_most}`
          : "";
  return `${row.name} (${word})`;
}

/**
 * A manual check row normalized to an object: `checks.yml`'s `manual:`
 * list is a bare string unless the question has `once: true`.
 * @param {{text: string, once?: boolean} | string} row
 */
export function manualRow(row) {
  return typeof row === "string"
    ? { text: row, once: false }
    : { text: row.text, once: !!row.once };
}

/**
 * @param {Record<string, unknown> | null} row
 * @returns {EditField[]}
 */
export function manualRowFields(row) {
  const from = /** @type {Record<string, unknown> | null} */ (
    row ? manualRow(/** @type {any} */ (row)) : null
  );
  return /** @type {EditField[]} */ (E.manual_row).map((f) => ({
    ...f,
    current: /** @type {string | boolean} */ (from?.[f.name] ?? f.current),
  }));
}

/** @param {Values} v */
export function manualRowFromValues(v) {
  return { text: String(v.text ?? "").trim(), once: v.once === true };
}

/** @param {Record<string, unknown>} row */
export function manualRowSummary(row) {
  const r = manualRow(/** @type {any} */ (row));
  return r.once ? `${r.text} (once)` : r.text;
}

/**
 * @param {string[]} groups
 * @param {Record<string, unknown> | null} row
 * @returns {EditField[]}
 */
export function tileRowFields(groups, row) {
  const from = /** @type {Record<string, unknown>} */ (row ?? {});
  return /** @type {EditField[]} */ (E.tile_row).map((f) => {
    /** @type {EditField} */
    const out = {
      ...f,
      current: /** @type {string} */ (from[f.name] ?? f.current),
    };
    for (const k of ["order", "watch_every", "down_after"])
      if (f.name === k) out.current = from[k] != null ? String(from[k]) : "";
    return out;
  });
}

/** @param {Values} v */
export function tileRowFromValues(v) {
  /** @type {Record<string, unknown>} */
  const out = {
    key: String(v.key ?? "").trim(),
    name: String(v.name ?? "").trim(),
    group: String(v.group ?? "").trim(),
  };
  const order = String(v.order ?? "").trim();
  if (order !== "") out.order = Number(order);
  for (const k of /** @type {const} */ (["description", "url", "reading"])) {
    const t = String(v[k] ?? "").trim();
    if (t) out[k] = t;
  }
  const we = String(v.watch_every ?? "").trim();
  if (we !== "") out.watch_every = Number(we);
  const da = String(v.down_after ?? "").trim();
  if (da !== "") out.down_after = Number(da);
  return out;
}

/** @param {Values} v */
export function tileRowProblems(v) {
  /** @type {Record<string, string>} */
  const errors = {};
  const we = String(v.watch_every ?? "").trim();
  const da = String(v.down_after ?? "").trim();
  if (we !== "" && da !== "" && Number(da) < Number(we))
    errors.down_after = "Must be at least check every (seconds).";
  return errors;
}

/** @param {Record<string, unknown>} row */
export function tileRowSummary(row) {
  return `${row.key} · ${row.name} (${row.group})`;
}

/**
 * Turns the tile row table's displayed rows (`RowEdit[]`, `origin` the old
 * hostname or `null`) into `TilesEdit.tiles`: only the rows that actually
 * changed, plus a `delete: true` tombstone for every original tile whose
 * hostname no longer appears among the rows (deleting a row that was
 * itself new, `origin: null`, needs no tombstone — it never existed).
 * @param {RowEdit[]} rows
 * @param {Record<string, Record<string, unknown>>} originals hostname → its fields, as read
 */
export function tilesBody(rows, originals) {
  const present = new Set(
    rows
      .map((r) => (typeof r.origin === "string" ? r.origin : null))
      .filter((k) => k != null),
  );
  /** @type {Record<string, unknown>[]} */
  const out = [];
  for (const r of rows) {
    const origin = typeof r.origin === "string" ? r.origin : null;
    if (origin == null) {
      out.push({
        origin: null,
        key: r.row.key,
        delete: false,
        tile: withoutKey(r.row),
      });
      continue;
    }
    const was = originals[origin];
    const changed =
      !was || JSON.stringify(withoutKey(r.row)) !== JSON.stringify(was);
    if (changed || r.row.key !== origin) {
      out.push({
        origin,
        key: r.row.key,
        delete: false,
        tile: withoutKey(r.row),
      });
    }
  }
  for (const key of Object.keys(originals)) {
    if (!present.has(key))
      out.push({ origin: key, key, delete: true, tile: {} });
  }
  return out;
}

/** @param {Record<string, unknown>} row */
function withoutKey(row) {
  const { key, ...rest } = row;
  return rest;
}

// ── feat-stacks-10 / feat-stacks-11: storage, data_mounts, log_files,
// latch_files row tables ─────────────────────────────────────────────────
//
// Each is a kp datatable + add/edit dialog, the same shape the Firewall
// tab's rule table uses: `rowModel`/`rowBody` carry the generic
// "origin tracks the old row, index i" bookkeeping (any row not
// referenced by its old index drops out, matching `yamledit::Op::Seq`);
// the per-list `*Fields`/`*FromValues`/`*Problems`/`*Summary` functions are
// the one place each list's own shape lives, the same split
// `ruleFields`/`ruleFromValues`/`ruleProblems`/`ruleSummary` uses for rules.

/**
 * @typedef {{origin: number | string | null, row: Record<string, unknown>}} RowEdit
 */

/**
 * @param {Record<string, unknown>[]} rows
 * @returns {RowEdit[]}
 */
export function rowModel(rows) {
  return (rows ?? []).map((row, i) => ({ origin: i, row }));
}

/**
 * @param {RowEdit[]} rows
 */
export function rowBody(rows) {
  return rows.map((r) => ({ origin: r.origin, ...r.row }));
}

/**
 * Whether the list changed from its starting rows.
 * @param {RowEdit[]} rows
 * @param {RowEdit[]} start
 */
export const rowsChanged = (rows, start) =>
  JSON.stringify(rowBody(rows)) !== JSON.stringify(rowBody(start));

/**
 * @param {{app: string, checks: RowEdit[] | null, manual: RowEdit[] | null,
 *   probes: RowEdit[] | null, busyCheck: string, url: string}} parts
 */
export function checksBody(parts) {
  /** @type {{kind: "checks"} & Record<string, unknown>} */
  const out = { kind: "checks", app: parts.app };
  if (parts.checks) out.checks = rowBody(parts.checks);
  if (parts.manual) out.manual = rowBody(parts.manual);
  if (parts.probes) out.probes = rowBody(parts.probes);
  out.busy_check = parts.busyCheck.trim() || null;
  out.url = parts.url.trim() || null;
  return out;
}

/**
 * @param {RowEdit[]} rows
 * @param {Record<string, Record<string, unknown>>} originals
 */
export function tilesEditBody(rows, originals) {
  return { kind: "tiles", tiles: tilesBody(rows, originals) };
}

/** @param {string} app */
export function appsRemoveField(app) {
  const w = { app };
  /** @type {Record<string, unknown>} */
  const f = {};
  for (const [k, v] of Object.entries(E.apps_remove))
    f[k] = typeof v === "string" ? fill(v, w) : v;
  return /** @type {EditField} */ ({ ...f, current: false });
}

/** @returns {EditField} */
export function appsAddBlankField() {
  return /** @type {EditField} */ ({ ...E.apps_add_blank, current: "" });
}

/** @param {string} app */
export function latchSecretField(app) {
  const w = { app };
  /** @type {Record<string, unknown>} */
  const f = {};
  for (const [k, v] of Object.entries(E.latch_secret))
    f[k] = typeof v === "string" ? fill(v, w) : v;
  return /** @type {EditField} */ ({ ...f, current: false });
}

/**
 * feat-stacks-3/feat-tiles-3: a preset's app(s) added to this stack, and
 * each app's own optional tile hostname, applied to the SAME staged
 * manifest as the app itself (one commit, not the app's commit followed
 * by a second, best-effort `StackEdit::Tiles` one).
 * @param {string} preset
 * @param {Record<string, string>} tiles app → hostname, only apps that got one
 */
export function addAppBody(preset, tiles) {
  /** @type {{kind: "add_app"} & Record<string, unknown>} */
  const out = { kind: "add_app", preset };
  if (Object.keys(tiles).length) out.tiles = tiles;
  return out;
}

/**
 * The apps & storage edit the server takes: only the parts touched.
 * @param {{remove: string[], addBlank: string[], storage: RowEdit[] | null,
 *   dataMounts: RowEdit[] | null, logFiles: RowEdit[] | null}} parts
 */
export function appsBody(parts) {
  /** @type {{kind: "apps"} & Record<string, unknown>} */
  const out = { kind: "apps" };
  if (parts.remove.length) out.remove = parts.remove;
  if (parts.addBlank.length) out.add_blank = parts.addBlank;
  if (parts.storage) out.storage = rowBody(parts.storage);
  if (parts.dataMounts) out.data_mounts = rowBody(parts.dataMounts);
  if (parts.logFiles) out.log_files = rowBody(parts.logFiles);
  return out;
}

/**
 * @param {{secrets: string[] | null, files: RowEdit[] | null}} parts
 */
export function latchBody(parts) {
  /** @type {{kind: "latch"} & Record<string, unknown>} */
  const out = { kind: "latch" };
  if (parts.secrets) out.secrets = parts.secrets;
  if (parts.files) out.files = rowBody(parts.files);
  return out;
}

/**
 * @param {ManifestView} m
 * @param {Record<string, unknown> | null} row
 * @returns {EditField[]}
 */
export function storageFields(m, row) {
  const from = row ?? {};
  return /** @type {EditField[]} */ (E.storage_entry).map((f) => {
    /** @type {EditField} */
    const out = {
      ...f,
      current: /** @type {string | boolean} */ (from[f.name] ?? f.current),
    };
    if (f.name === "no_data") out.current = from.no_data === true;
    if (f.name === "host_owner_uid")
      out.current =
        from.host_owner_uid != null ? String(from.host_owner_uid) : "";
    if (f.name === "app")
      out.choices = [
        { value: "", label: "(the stack itself)" },
        ...(m.apps ?? []).map((a) => ({ value: a, label: a })),
      ];
    return out;
  });
}

/** @param {Values} v */
export function storageFromValues(v) {
  /** @type {Record<string, unknown>} */
  const out = {
    host_path: String(v.host_path ?? "").trim(),
    mount_point: String(v.mount_point ?? "").trim(),
    no_data: v.no_data === true,
  };
  const app = String(v.app ?? "").trim();
  if (app) out.app = app;
  const noBackup = String(v.no_backup ?? "").trim();
  if (noBackup) out.no_backup = noBackup;
  const uid = String(v.host_owner_uid ?? "").trim();
  if (uid !== "") out.host_owner_uid = Number(uid);
  return out;
}

/** @param {Record<string, unknown>} row */
export function storageSummary(row) {
  const app = row.app ? ` (${row.app})` : "";
  return `${row.host_path} → ${row.mount_point}${app}`;
}

/**
 * @param {Record<string, unknown> | null} row
 * @returns {EditField[]}
 */
export function dataMountFields(row) {
  const from = row ?? {};
  return /** @type {EditField[]} */ (E.data_mount).map((f) => ({
    ...f,
    current: /** @type {string | boolean} */ (from[f.name] ?? f.current),
  }));
}

/** @param {Values} v */
export function dataMountFromValues(v) {
  /** @type {Record<string, unknown>} */
  const out = {
    host_path: String(v.host_path ?? "").trim(),
    mount_point: String(v.mount_point ?? "").trim(),
  };
  const note = String(v.note ?? "").trim();
  if (note) out.note = note;
  const files = String(v.rotate_files ?? "").trim();
  if (files) {
    /** @type {Record<string, unknown>} */
    const rotate = { files };
    const keep = String(v.rotate_keep ?? "").trim();
    if (keep !== "") rotate.keep = Number(keep);
    const container = String(v.rotate_container ?? "").trim();
    if (container) {
      /** @type {Record<string, unknown>} */
      const reopen = { container };
      const signal = String(v.rotate_signal ?? "").trim();
      if (signal) reopen.signal = signal;
      rotate.reopen = reopen;
    }
    out.rotate = rotate;
  }
  return out;
}

/** @param {Values} v */
export function dataMountProblems(v) {
  /** @type {Record<string, string>} */
  const errors = {};
  if (String(v.rotate_files ?? "").trim() === "") {
    for (const k of ["rotate_keep", "rotate_container", "rotate_signal"])
      if (String(v[k] ?? "").trim() !== "")
        errors[k] = "Needs a rotate file name/glob above.";
  }
  return errors;
}

/** @param {Record<string, unknown>} row */
export function dataMountSummary(row) {
  return `${row.host_path} → ${row.mount_point}`;
}

/**
 * @param {Record<string, unknown> | null} row
 * @returns {EditField[]}
 */
export function logFileFields(row) {
  const from = row ?? {};
  return /** @type {EditField[]} */ (E.log_file).map((f) => ({
    ...f,
    current: /** @type {string} */ (from[f.name] ?? f.current),
  }));
}

/** @param {Values} v */
export function logFileFromValues(v) {
  return {
    path: String(v.path ?? "").trim(),
    job: String(v.job ?? "").trim(),
  };
}

/** @param {Record<string, unknown>} row */
export function logFileSummary(row) {
  return `${row.path} (${row.job})`;
}

/**
 * @param {ManifestView} m
 * @param {Record<string, unknown> | null} row
 * @returns {EditField[]}
 */
export function latchFileFields(m, row) {
  const from = row ?? {};
  return /** @type {EditField[]} */ (E.latch_file).map((f) => {
    /** @type {EditField} */
    const out = {
      ...f,
      current: /** @type {string} */ (from[f.name] ?? f.current),
    };
    if (f.name === "restarts")
      out.choices = [
        { value: "", label: "(none)" },
        ...(m.natives ?? []).map((u) => ({ value: u, label: u })),
      ];
    return out;
  });
}

/** @param {Values} v */
export function latchFileFromValues(v) {
  /** @type {Record<string, unknown>} */
  const out = {
    from: String(v.from ?? "").trim(),
    dest: String(v.dest ?? "").trim(),
    mode: String(v.mode ?? "").trim(),
  };
  const owner = String(v.owner ?? "").trim();
  if (owner) out.owner = owner;
  const restarts = String(v.restarts ?? "").trim();
  if (restarts) out.restarts = restarts;
  return out;
}

/**
 * The latch --expand trap: refused before anything is sent, on every
 * field a latch_files row carries.
 * @param {Values} v
 */
export function latchFileProblems(v) {
  /** @type {Record<string, string>} */
  const errors = {};
  for (const k of ["from", "dest", "mode", "owner", "restarts"]) {
    if (String(v[k] ?? "").includes("${")) errors[k] = say("dollar_expand");
  }
  return errors;
}

/** @param {Record<string, unknown>} row */
export function latchFileSummary(row) {
  return `${row.from} → ${row.dest}`;
}

/**
 * The latch_secrets checkboxes: refuse '${' the same way a latch_files
 * row does, although the app-name charset already excludes it.
 * @param {string[]} apps
 */
export function latchSecretsProblems(apps) {
  /** @type {Record<string, string>} */
  const errors = {};
  for (const a of apps)
    if (a.includes("${")) errors[`latch-secret:${a}`] = say("dollar_expand");
  return errors;
}

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
  /** @type {Record<string, string | null | undefined>} */
  const from = {
    dir: r?.dir,
    action: r?.action,
    peer,
    proto: r?.proto,
    dport: r?.dport,
    note: r?.note,
    comment: r?.comment,
  };
  return /** @type {EditField[]} */ (E.rule).map((f) => ({
    ...f,
    current: from[f.name] ?? f.current,
  }));
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
      out.peer = say("peer_form", { peer });
    else if (m[5] !== undefined) {
      const bits = Number(m[5]);
      const ip =
        ((Number(m[1]) << 24) >>> 0) +
        (Number(m[2]) << 16) +
        (Number(m[3]) << 8) +
        Number(m[4]);
      const mask = bits === 0 ? 0 : (0xffffffff << (32 - bits)) >>> 0;
      if (bits > 32) out.peer = say("peer_prefix", { peer });
      else if ((ip & mask) >>> 0 !== ip)
        out.peer = say("peer_host_bits", { peer });
    }
  }
  const ports = String(v.dport ?? "").replace(/\s+/g, "");
  const proto = String(v.proto ?? "");
  if (ports) {
    if (proto === "" || proto === "icmp")
      out.dport = say(proto === "icmp" ? "icmp_ports" : "port_proto");
    else
      for (const part of ports.split(",")) {
        const [a, b] = part.split(":").map(Number);
        const ok = (/** @type {number} */ n) =>
          Number.isInteger(n) && n >= 1 && n <= 65535;
        if (!ok(a) || (part.includes(":") && (!ok(b) || b <= a))) {
          out.dport = say("port_range", { part });
          break;
        }
      }
  }
  if (String(v.note ?? "").includes("\n")) out.note = say("note_line");
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
  const d = E.new_defaults;
  /** @type {Record<string, string>} */
  const now = {
    preset: first?.name ?? "",
    vmid: suggestVmid == null ? "" : String(suggestVmid),
    ram_mb: String(first?.ram_mb ?? d.ram_mb),
    cores: String(first?.cores ?? d.cores),
    disk_gb: String(first?.disk_gb ?? d.disk_gb),
    swap_mb: "",
  };
  return {
    id: "new-stack",
    title: "New stack",
    steps: E.new_stack.map((s) => ({
      id: s.id,
      label: s.label,
      fields: /** @type {EditField[]} */ (s.fields).map((f) => ({
        ...f,
        ...(f.name === "preset"
          ? {
              choices: presets.map((p) => ({
                value: p.name,
                label: `${p.name} · ${p.description}`,
              })),
            }
          : {}),
        ...(f.name in now && f.name !== "name" ? { current: now[f.name] } : {}),
      })),
    })),
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
  paths.map((path, i) => ({
    name: fill(E.nodata.name, { path }),
    id: fill(E.nodata.id, { i: String(i) }),
    kind: /** @type {const} */ ("check"),
    label: fill(E.nodata.label, { path }),
    help: E.nodata.help,
    current: false,
  }));

/**
 * feat-tiles-3: the wizard's optional Tile step, folded into the SAME
 * `NewStack` body the stack itself is scaffolded from (`m.tile` on the
 * Rust side, applied to the staged manifest before it is ever written —
 * one commit, not the stack's commit followed by a second, best-effort
 * `StackEdit::Tiles` one). `null` when the hostname is blank, which is the
 * step's "no tile" answer. A blank name/group falls back to the stack's
 * own name / a generic "Own", so the one required choice is the hostname.
 * @param {Values} v
 */
function newStackTile(v) {
  const hostname = String(v.tile_hostname ?? "").trim();
  if (!hostname) return null;
  const num = (/** @type {string} */ k) => {
    const s = String(v[k] ?? "").trim();
    return s ? Number(s) : null;
  };
  /** @type {Record<string, unknown>} */
  const tile = {
    hostname,
    name: String(v.tile_name ?? "").trim() || String(v.name ?? "").trim(),
    group: String(v.tile_group ?? "").trim() || "Own",
  };
  const description = String(v.tile_description ?? "").trim();
  if (description) tile.description = description;
  const watchEvery = num("tile_watch_every");
  if (watchEvery != null) tile.watch_every = watchEvery;
  const downAfter = num("tile_down_after");
  if (downAfter != null) tile.down_after = downAfter;
  return tile;
}

/**
 * What the wizard sends.
 * @param {Values} v
 */
export function newStackBody(v) {
  const n = (/** @type {string} */ k) => Number(String(v[k] ?? "").trim());
  const swap = String(v.swap_mb ?? "").trim();
  const tile = newStackTile(v);
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
    ...(tile ? { tile } : {}),
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
      errors.name = say("name_taken", { name });
    const vmid = Number(v.vmid);
    if (!errors.vmid && taken.vmids.includes(vmid))
      errors.vmid = say("vmid_taken", { vmid });
    if (!errors.vmid && E.no_touch.includes(vmid))
      errors.vmid = say("vmid_no_touch", { vmid });
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
        : { ok: false, why: say("key_int", { min: k.min, max: k.max }) };
    }
    case "vmid": {
      const n = Number(t);
      return /^\d+$/.test(t) && n >= 100
        ? { ok: true, value: n }
        : { ok: false, why: say("key_vmid") };
    }
    case "url":
      return /^https?:\/\/\S+$/.test(t)
        ? { ok: true, value: t }
        : { ok: false, why: say("key_url") };
    case "window":
      return /^\d+[smhdw]$/.test(t)
        ? { ok: true, value: t }
        : { ok: false, why: say("key_window") };
    case "vmid_list": {
      const parts = t
        .split(/[\s,]+/)
        .filter(Boolean)
        .map(Number);
      return parts.every((n) => Number.isInteger(n) && n >= 100)
        ? { ok: true, value: parts }
        : { ok: false, why: say("key_vmid_list") };
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
        ? { ok: false, why: say("key_line") }
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

// ── TUI parity: import a bundle as a new stack ──────────────────────────

/** The import form's first step, from the one form description. */
export const IMPORT_FIELDS = /** @type {EditField[]} */ (E.import);

/**
 * The plan request: the bundle as typed, the name trimmed, the number as a
 * number (the server's `ImportReq`).
 * @param {Values} v
 */
export const importBody = (v) => ({
  bundle: String(v.bundle ?? ""),
  name: String(v.name ?? "").trim(),
  vmid: Number(String(v.vmid ?? "").trim() || 0),
});

/**
 * What is wrong with the first step, in the new-stack wizard's words (the
 * server holds a driven step to the same).
 * @param {Values} v
 * @param {{names: string[], vmids: number[]}} taken
 */
export const importErrors = (v, taken) =>
  checkNewStep(
    {
      id: "import",
      title: "Import a stack",
      steps: [{ id: "identity", label: "Bundle", fields: IMPORT_FIELDS }],
    },
    "identity",
    v,
    taken,
  );
