// feat-kp11demo-5 (Settings on kp-themes 11): one row of the host.toml form,
// the editor that fits its key, and the bridge between an editor and the
// hidden text mirror the Save bar tracks (settingsrows.js says what a mirror
// holds). Every editor is kp-themes' own: a switch, a stepper that shows its
// unit, a 24-hour time field, tags, a code view with a command to copy, and
// the key typed to confirm.
//
// A row is a `.kp-formrow` of a `.kp-formgrid`: the key's name and its words
// in the label track, the editor in the control track, what the host uses
// when the key is absent (and when a change takes effect) in the value track,
// so every row of every group stands on the same four tracks.

import { accessWords } from "./settingsview.js";
import {
  boundsOf,
  editorId,
  editorKind,
  mirrorOf,
  shownValue,
  unitOf,
} from "./settingsrows.js";
import { attachAll } from "/static/kp/dist/kp-themes.min.js";
import { combobox } from "/static/kp/dist/kp-themes.min.js";
import { setQuantity } from "/static/kp/dist/kp-themes.min.js";
import { setTime } from "/static/kp/dist/kp-themes.min.js";

/** @typedef {import("./editforms.js").HostField} HostField */

/** @param {unknown} text */
export const esc = (text) =>
  String(text).replace(
    /[&<>"']/g,
    (c) =>
      /** @type {Record<string, string>} */ ({
        "&": "&amp;",
        "<": "&lt;",
        ">": "&gt;",
        '"': "&quot;",
        "'": "&#39;",
      })[c] ?? c,
  );

/** The attribute that marks a row's mirror. */
export const MIRROR = "data-st-mirror";

/**
 * The ssh line that changes a key, written for the terminal the person uses.
 * @param {HostField} f
 * @param {string} path
 */
export const sshLine = (f, path) =>
  `ssh -t root@pve sudoedit ${path.replace(/ \(.*\)$/, "")}  # set ${f.key}`;

/** The terminals the how-to is written for (kp's command block). */
export const TERMINALS = /** @type {const} */ (["garuda", "wsl", "powershell"]);

/**
 * The editor's markup, with its mirror.
 * @param {HostField} f
 * @param {string} mirror the mirror's starting text
 */
function editorHtml(f, mirror) {
  const kind = editorKind(f);
  const id = editorId(f.key);
  const label = esc(f.label);
  const hid = (/** @type {string} */ what) =>
    `<input class="kp-sr-only" id="${id}" ${MIRROR}="${what}" name="${f.key}" value="${esc(mirror)}" tabindex="-1" aria-hidden="true" />`;
  switch (kind) {
    case "switch":
      return `<label class="kp-switch"><input class="kp-switch__input" type="checkbox" role="switch" id="${id}" name="${f.key}"${mirror === "on" ? " checked" : ""} /><span>${mirror === "on" ? "On" : "Off"}</span></label>`;
    case "hour":
      return `<div class="kp-time" data-kp-time data-kp-value="${esc(mirror)}" data-kp-label="${label}" data-kp-time-step="60" data-kp-time-presets="02:00,03:00,04:00" data-kp-time-clear></div>${hid("time")}`;
    case "quantity": {
      const u = unitOf(f);
      const b = boundsOf(f, u);
      return `<div class="kp-quantity" data-kp-quantity data-kp-quantity-min="${b.min}" data-kp-quantity-max="${b.max}" data-kp-quantity-step="${u.step}"${u.unit ? ` data-kp-quantity-unit="${esc(u.unit)}" data-kp-quantity-unit-one="${esc(u.unitOne)}"` : ""} data-kp-quantity-label="${label}"><input class="kp-quantity__input" value="${esc(mirror)}" /></div>${hid("number")}`;
    }
    case "number":
      return `<input class="kp-field__input" type="text" inputmode="numeric" id="${id}" name="${f.key}" value="${esc(mirror)}" placeholder="${esc(f.default)}" aria-label="${label}" autocomplete="off" />`;
    case "tags": {
      const chosen = mirror.split(",").filter(Boolean);
      const lid = `${id}-tags`;
      return `<div class="kp-combobox" data-kp-combobox data-kp-tags data-kp-creatable><label class="kp-sr-only" for="${lid}">${label}</label><ul class="kp-tag-list" data-kp-tag-list>${chosen.map((v) => `<li class="kp-tag" data-value="${esc(v)}"><span>${esc(v)}</span><button type="button" class="kp-tag__remove" aria-label="Remove ${esc(v)}">×</button></li>`).join("")}</ul><input class="kp-combobox__input" id="${lid}" type="text" role="combobox" aria-expanded="false" aria-controls="${lid}-list" autocomplete="off" /><ul class="kp-combobox__list" id="${lid}-list" role="listbox" hidden></ul></div>${hid("tags")}`;
    }
    case "toml":
      return `<textarea class="kp-field__input st-toml" id="${id}" name="${f.key}" rows="6" spellcheck="false" aria-label="${esc(f.key)} as TOML" data-kp-autosize>${esc(mirror)}</textarea>`;
    case "secret":
      return `<input class="kp-field__input" type="password" id="${id}" name="${f.key}" value="" placeholder="${f.set ? "set (hidden): type to replace it" : "not set"}" aria-label="${label}" autocomplete="new-password" />`;
    default:
      return `<input class="kp-field__input" type="${f.kind.type === "url" ? "url" : "text"}" id="${id}" name="${f.key}" value="${esc(mirror)}" placeholder="${esc(f.default)}" aria-label="${label}" autocomplete="off" spellcheck="false"${f.kind.type === "vmid" ? ' inputmode="numeric"' : ""} />`;
  }
}

/**
 * What a key that is kept over ssh shows: its value to read, and the line
 * that changes it, in a how-to that opens under the row.
 * @param {HostField} f
 * @param {string} path host.toml's path
 * @param {string} term the terminal the line is written for
 * @param {boolean} open
 */
function readHtml(f, path, term, open) {
  const words = accessWords(f);
  const value =
    f.kind.type === "table" && f.set && (f.toml ?? "") !== ""
      ? `<figure class="kp-code" data-kp-code data-kp-code-title="${esc(f.key)} in host.toml" data-kp-code-language="TOML" data-kp-code-lines><pre class="kp-code__body"><code>${esc(f.toml ?? "")}</code></pre></figure>`
      : `<p class="st-read">${esc(readText(f))}</p>`;
  const how =
    f.access === "secret" || f.access === "host_held"
      ? `<p class="st-line">${esc(words ? words[1] : "")}</p>`
      : `<div class="kp-accordion"><details class="kp-accordion__item" data-st-howto="${f.key}"${open ? " open" : ""}><summary class="kp-accordion__summary" id="how-${f.key}">How to change this</summary><div class="kp-accordion__body"><p class="st-line">${esc(words ? words[1] : "")} It is edited in the file on pve; the host reads it again at its next start.</p><div class="kp-command" data-kp-command="${term}" data-kp-command-where="pve" data-kp-command-elevated><pre class="kp-command__code"><code>${esc(sshLine(f, path))}</code></pre></div></div></details></div>`;
  return value + how;
}

/**
 * A kept key's value in words.
 * @param {HostField} f
 */
export function readText(f) {
  if (f.access === "secret" || f.access === "host_held")
    return f.set ? "set (hidden)" : "not set";
  if (!f.set) return `default: ${f.default}`;
  const v = f.value;
  if (Array.isArray(v)) return v.join(", ");
  if (typeof v === "boolean") return v ? "on" : "off";
  return String(v);
}

/**
 * One row of the form.
 * @param {HostField} f
 * @param {{path: string, term: string, howOpen: boolean}} o
 * @returns {string}
 */
export function rowHtml(f, o) {
  const kind = editorKind(f);
  const words = accessWords(f);
  const mirror = mirrorOf(f);
  const confirm = f.access === "confirm";
  const badge = confirm
    ? ' <span class="kp-badge kp-badge--info">type the key to confirm</span>'
    : kind === "read" && words
      ? ` <span class="kp-badge">${esc(words[0])}</span>`
      : kind === "secret"
        ? ' <span class="kp-badge">write-only</span>'
        : "";
  const editor =
    kind === "read"
      ? readHtml(f, o.path, o.term, o.howOpen)
      : editorHtml(f, mirror);
  const typed = confirm
    ? `<div class="st-confirm" data-st-confirm hidden><template id="loses-${f.key}"><ul class="kp-type-confirm__list"><li>${esc(f.help)}</li><li>${f.apply === "restart" ? "The change is used from the host's next start." : "The host uses the change at once."}</li></ul></template><div data-kp-type-confirm="${f.key}" data-kp-type-confirm-loses="loses-${f.key}"></div></div>`
    : "";
  const when =
    f.apply === "live" ? "applies at once" : "at the host's next start";
  return `<div class="kp-formrow" id="k-${f.key}" data-key="${f.key}" data-kp-name="${esc(f.label)}" data-kp-anchor="${f.key}" data-st-kind="${kind}"${kind === "read" ? " data-kp-locked" : ""}><div class="kp-formrow__label"><span class="st-name" data-st-name>${esc(f.label)}</span> <code class="st-key">${f.key}</code>${badge}<small>${esc(f.help)}</small></div><div class="kp-formrow__control">${editor}${typed}</div><div class="kp-formrow__aside"><span>default ${esc(f.default)}</span><span class="st-when">${when}</span></div></div>`;
}

/**
 * Attach every component a subtree carries and mark each editor's inner
 * fields as not tracked, so the Save bar counts the mirror (one value per
 * row) and nothing twice.
 * @param {HTMLElement} scope
 * @returns {Promise<() => void>} detach
 */
export async function wire(scope) {
  const off = attachAll(scope);
  await off.ready;
  for (const input of scope.querySelectorAll(
    "[data-kp-quantity] input, [data-kp-time] input, [data-kp-combobox] input, [data-kp-type-confirm] input, .kp-code input, .kp-command input",
  ))
    input.setAttribute("data-kp-untracked", "");
  return off;
}

/**
 * Keep every editor and its mirror in step, both ways, inside `scope`:
 * an editor's change writes its mirror (which the bar tracks); the bar
 * reverting a row writes the mirror, which writes the editor back. The
 * bar's list says "was" and "now" in the key's own words.
 * @param {HTMLElement} scope
 * @param {(key: string) => HostField | undefined} fieldOf
 * @returns {() => void}
 */
export function bridge(scope, fieldOf) {
  /** @param {Element} row @param {string} value */
  const push = (row, value) => {
    const m = /** @type {HTMLInputElement | null} */ (
      row.querySelector(`[${MIRROR}]`)
    );
    if (!m || m.value === value) return;
    m.value = value;
    m.dispatchEvent(new Event("input", { bubbles: true }));
    m.dispatchEvent(new Event("change", { bubbles: true }));
  };
  /** The words in the bar's list, once the bar has marked the row. @param {Element} row */
  const words = (row) =>
    queueMicrotask(() => {
      const f = fieldOf(row.getAttribute("data-key") ?? "");
      const m = /** @type {HTMLInputElement | null} */ (
        row.querySelector(`[${MIRROR}]`)
      );
      if (!f || !m || !row.hasAttribute("data-kp-changed")) return;
      row.setAttribute("data-kp-now", shownValue(f, m.value));
      row.setAttribute(
        "data-kp-was",
        shownValue(f, m.getAttribute("value") ?? ""),
      );
    });
  const rowOf = (/** @type {Event} */ e) =>
    /** @type {Element} */ (e.target).closest("[data-key]");
  /** @type {[string, (e: Event, row: Element) => void][]} */
  const handlers = [
    [
      "kp-quantity-change",
      (e, row) =>
        push(row, String(/** @type {CustomEvent} */ (e).detail.value)),
    ],
    [
      "kp-time-change",
      (e, row) =>
        push(row, String(/** @type {CustomEvent} */ (e).detail.value)),
    ],
    [
      "kp-combobox-change",
      (e, row) =>
        push(row, /** @type {CustomEvent} */ (e).detail.values.join(",")),
    ],
  ];
  /** @type {[string, (e: Event) => void][]} */
  const bound = handlers.map(([name, f]) => [
    name,
    (e) => {
      const row = rowOf(e);
      if (row) f(e, row);
    },
  ]);
  for (const [name, f] of bound) scope.addEventListener(name, f);
  // The bar reverts a row by writing its mirror: the editor follows.
  const onInput = (/** @type {Event} */ e) => {
    const m = /** @type {HTMLInputElement} */ (e.target);
    if (!(m instanceof HTMLInputElement) || !m.hasAttribute(MIRROR)) {
      const row = rowOf(e);
      if (row) words(row);
      return;
    }
    const row = m.closest("[data-key]");
    if (!row) return;
    const kind = m.getAttribute(MIRROR);
    if (kind === "number") {
      const q = row.querySelector("[data-kp-quantity]");
      if (q && m.value !== "") setQuantity(q, Number(m.value));
    } else if (kind === "time") {
      const t = /** @type {HTMLElement | null} */ (
        row.querySelector("[data-kp-time]")
      );
      if (t) setTime(t, m.value, { fire: false });
    } else if (kind === "tags") {
      const box = row.querySelector("[data-kp-combobox]");
      const handle = box ? combobox(box) : null;
      if (handle && handle.values().join(",") !== m.value)
        handle.set(m.value.split(",").filter(Boolean));
    }
    words(row);
  };
  scope.addEventListener("input", onInput);
  // A switch's word follows its state.
  const onChange = (/** @type {Event} */ e) => {
    const box = /** @type {HTMLInputElement} */ (e.target);
    if (box instanceof HTMLInputElement && box.matches(".kp-switch__input")) {
      const word = box.parentElement?.querySelector("span");
      if (word) word.textContent = box.checked ? "On" : "Off";
    }
    const row = rowOf(e);
    if (row) words(row);
  };
  scope.addEventListener("change", onChange);
  return () => {
    for (const [name, f] of bound) scope.removeEventListener(name, f);
    scope.removeEventListener("input", onInput);
    scope.removeEventListener("change", onChange);
  };
}
