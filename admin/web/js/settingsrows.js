// feat-kp11demo-5 (Settings on kp-themes 11): which editor fits a host.toml
// key, and how a key's value becomes the text the Save bar tracks and back.
// Pure, so it is tested without a browser.
//
// A row's editor keeps its value in its own parts (a switch's box, a
// stepper's field, a time field's input, tags), which the Save bar cannot
// read (kp-gaps/homelab.md 37). So each row also holds a hidden text mirror of
// its value: the bar tracks the mirror, `settingsform.js` keeps the two in
// step. This module owns what the mirror says.
//
// Kinds of editor, by the key's own kind (core/src/hostconfig.rs `Kind`):
//   switch    a yes/no key                       mirror "on" | "off"
//   hour      `backup_hour` (0-23)               mirror "HH:00" | ""
//   quantity  a whole number with its unit       mirror the number as text
//   number    a whole number with no default      the field itself
//   text      one line of text, url, vmid        the field itself
//   tags      a list of vmids or of texts        mirror "a,b,c"
//   toml      a table, edited as a TOML fragment the textarea itself
//   secret    a write-only value                 the password field itself
//   read      kept over ssh: a value to read, no editor here

/**
 * @typedef {import("./editforms.js").HostField} HostField
 * @typedef {{unit: string, unitOne: string, per: number, step: number}} Unit
 * @typedef {"switch" | "hour" | "quantity" | "number" | "text" | "tags" |
 *   "toml" | "secret" | "read"} EditorKind
 */

/** Seconds, and the bigger units a `_s` key reads in. */
const SECONDS = /** @type {const} */ ([
  { unit: "days", unitOne: "day", per: 86400 },
  { unit: "h", unitOne: "h", per: 3600 },
  { unit: "min", unitOne: "min", per: 60 },
  { unit: "s", unitOne: "s", per: 1 },
]);

/**
 * The editor a key gets.
 * @param {HostField} f
 * @returns {EditorKind}
 */
export function editorKind(f) {
  if (f.access === "dashboard_secret") return "secret";
  if (f.access !== "browser" && f.access !== "confirm") return "read";
  switch (f.kind.type) {
    case "bool":
      return "switch";
    case "int":
      if (f.key === "backup_hour") return "hour";
      // A stepper needs a number to start from: the saved one or the
      // default's. A key whose default is words ("not asked") is a field.
      return f.set || defaultNumber(f) != null ? "quantity" : "number";
    case "window":
      return "quantity";
    case "vmid_list":
    case "text_list":
      return "tags";
    case "table":
      return "toml";
    default:
      return "text";
  }
}

/**
 * The unit a number key is shown in, chosen from the saved value (or the
 * default) so what is saved reads as a whole number of it: 14400 s is "4 h".
 * `per` is how many of the stored unit make one shown unit.
 * @param {HostField} f
 * @returns {Unit}
 */
export function unitOf(f) {
  const k = f.kind;
  if (k.type === "window") {
    const raw = f.set ? String(f.value) : f.default;
    const m = /^(\d+)([smhdw])/.exec(raw);
    const u = m ? m[2] : "h";
    return { unit: u, unitOne: u, per: 1, step: 1 };
  }
  if (f.key.endsWith("_s")) {
    const v = f.set && typeof f.value === "number" ? f.value : defaultNumber(f);
    const hit =
      SECONDS.find((u) => v != null && v >= u.per && v % u.per === 0) ??
      SECONDS[SECONDS.length - 1];
    const min = k.type === "int" ? k.min : 1;
    // A step that cannot go below the host's own minimum.
    const step = hit.per >= 3600 ? 1 : hit.per === 60 ? 5 : 10;
    void min;
    return { ...hit, step };
  }
  if (f.key.endsWith("_mib"))
    return { unit: "MiB", unitOne: "MiB", per: 1, step: 1 };
  if (f.key.endsWith("_bytes"))
    return { unit: "bytes", unitOne: "byte", per: 1, step: 65536 };
  if (f.key.endsWith("_days"))
    return { unit: "days", unitOne: "day", per: 1, step: 1 };
  if (f.key === "backup_concurrency")
    return { unit: "stacks", unitOne: "stack", per: 1, step: 1 };
  if (f.key === "recent_lines")
    return { unit: "lines", unitOne: "line", per: 1, step: 100 };
  if (f.key.endsWith("_count"))
    return { unit: "bundles", unitOne: "bundle", per: 1, step: 1 };
  return { unit: "", unitOne: "", per: 1, step: 1 };
}

/**
 * The number the host takes when the key is absent, read from the words of
 * its default ("14400 (4 h)" is 14400); null when the default is words.
 * @param {HostField} f
 * @returns {number | null}
 */
export function defaultNumber(f) {
  const m = /^(\d+)/.exec(f.default);
  return m ? Number(m[1]) : null;
}

/**
 * The bounds of a quantity, in the shown unit.
 * @param {HostField} f
 * @param {Unit} u
 * @returns {{min: number, max: number}}
 */
export function boundsOf(f, u) {
  const k = f.kind;
  if (k.type === "window") return { min: 1, max: 9999 };
  if (k.type !== "int") return { min: 0, max: Number.MAX_SAFE_INTEGER };
  return {
    min: Math.ceil(k.min / u.per),
    max: Math.min(Math.floor(k.max / u.per), 99_999_999),
  };
}

/**
 * What the mirror holds for a key as the host has it now (the effective
 * value: the saved one, else the default when that is a number, else empty).
 * @param {HostField} f
 * @returns {string}
 */
export function mirrorOf(f) {
  switch (editorKind(f)) {
    case "switch": {
      const v = f.set
        ? f.value === true
        : f.default === "false"
          ? false
          : false;
      return v ? "on" : "off";
    }
    case "hour":
      return f.set && typeof f.value === "number" ? hourText(f.value) : "";
    case "quantity": {
      const u = unitOf(f);
      if (f.kind.type === "window") {
        const raw = f.set ? String(f.value) : f.default;
        const m = /^(\d+)/.exec(raw);
        return m ? m[1] : "";
      }
      const v =
        f.set && typeof f.value === "number" ? f.value : defaultNumber(f);
      return v == null ? "" : String(Math.round(v / u.per));
    }
    case "tags":
      return f.set && Array.isArray(f.value) ? f.value.join(",") : "";
    case "toml":
      return f.set ? (f.toml ?? "") : "";
    case "secret":
      return "";
    default:
      return f.set && f.value != null ? String(f.value) : "";
  }
}

/** @param {number} h */
export const hourText = (h) => `${String(h).padStart(2, "0")}:00`;

/**
 * The key's value from what its mirror (or its field) holds: the shape
 * `PUT /data/host-settings` takes (editforms.js `parseKey`'s answer).
 * @param {HostField} f
 * @param {string} text
 * @returns {{ok: true, value: unknown} | {ok: false, why: string}}
 */
export function valueFromMirror(f, text) {
  switch (editorKind(f)) {
    case "switch":
      return { ok: true, value: text === "on" };
    case "hour": {
      if (text.trim() === "") return { ok: true, value: null };
      const m = /^(\d\d):(\d\d)$/.exec(text.trim());
      if (!m)
        return {
          ok: false,
          why: "Pick a time of day, or clear it for no nightly round.",
        };
      return { ok: true, value: Number(m[1]) };
    }
    case "quantity": {
      if (text.trim() === "") return { ok: true, value: null };
      const n = Number(text);
      if (!Number.isFinite(n) || n < 0)
        return { ok: false, why: "A whole number is needed." };
      const u = unitOf(f);
      if (f.kind.type === "window") return { ok: true, value: `${n}${u.unit}` };
      return { ok: true, value: Math.round(n * u.per) };
    }
    case "tags": {
      const parts = text
        .split(",")
        .map((x) => x.trim())
        .filter(Boolean);
      if (parts.length === 0) return { ok: true, value: null };
      if (f.kind.type === "vmid_list") {
        const nums = parts.map(Number);
        if (!nums.every((n) => Number.isInteger(n) && n >= 100))
          return {
            ok: false,
            why: "Each entry is a container or VM number, 100 or more.",
          };
        return { ok: true, value: nums };
      }
      return { ok: true, value: parts };
    }
    case "toml":
      return { ok: true, value: text.trim() === "" ? "" : text };
    case "number": {
      const t = text.trim();
      if (t === "") return { ok: true, value: null };
      const k = f.kind;
      const n = Number(t);
      return /^\d+$/.test(t) && k.type === "int" && n >= k.min && n <= k.max
        ? { ok: true, value: n }
        : {
            ok: false,
            why:
              k.type === "int"
                ? `A whole number, ${k.min} to ${k.max}.`
                : "A whole number.",
          };
    }
    default:
      return parseText(f, text);
  }
}

/**
 * A plain field's text as the key's value (empty removes the key).
 * @param {HostField} f
 * @param {string} text
 * @returns {{ok: true, value: unknown} | {ok: false, why: string}}
 */
function parseText(f, text) {
  const t = text.trim();
  if (t === "") return { ok: true, value: null };
  if (f.kind.type === "vmid") {
    return /^\d+$/.test(t) && Number(t) >= 100
      ? { ok: true, value: Number(t) }
      : { ok: false, why: "A container or VM number, 100 or more." };
  }
  if (f.kind.type === "url")
    return /^https?:\/\/\S+$/.test(t)
      ? { ok: true, value: t }
      : { ok: false, why: "An http:// or https:// address." };
  return t.includes("\n")
    ? { ok: false, why: "One line only." }
    : { ok: true, value: t };
}

/**
 * The words a row's old and new value get in the Save bar's list and the
 * review sentence.
 * @param {HostField} f
 * @param {string} text the mirror's text
 */
export function shownValue(f, text) {
  const kind = editorKind(f);
  if (kind === "secret")
    return text === "" ? "not changed" : "a new value (hidden)";
  if (text === "") return "the default";
  if (kind === "hour") return text;
  if (kind === "quantity") {
    const u = unitOf(f);
    return u.unit ? `${text} ${Number(text) === 1 ? u.unitOne : u.unit}` : text;
  }
  if (kind === "tags") return text.split(",").join(", ");
  if (kind === "toml") return "a new table";
  return text;
}

/**
 * Which groups open on a first visit: the first two (as before 3.75).
 * @param {string[]} groups
 * @param {string} group
 */
export const openFirst = (groups, group) => groups.indexOf(group) < 2;

/**
 * The Live view field id of a key's editor (`key-backup-concurrency`).
 * @param {string} key
 */
export const editorId = (key) => `key-${key.replace(/_/g, "-")}`;
