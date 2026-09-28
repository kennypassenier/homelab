// feat-overview-8: what a page shows is in its address. The pure half: a
// table's search and filters to query parameters and back, and changing a
// query string without losing what other parts of the page put there.
//
// A table named `fleet` writes `fleet.q=<search>` and one `fleet.f<column>`
// per filter: a choice filter repeats the key once per value
// (`fleet.f2=running&fleet.f2=degraded`), a range or date filter writes
// `~from..to` (either side may be empty).

/** @typedef {string[] | {from?: string, to?: string}} FilterValue */
/** @typedef {{query: string, filters: Record<number, FilterValue>}} TableState */

/**
 * @param {string} name the table's remember name
 * @param {TableState} view
 * @returns {[string, string][]}
 */
export function tableToParams(name, view) {
  /** @type {[string, string][]} */
  const out = [];
  if (view.query) out.push([`${name}.q`, view.query]);
  const cols = Object.keys(view.filters)
    .map(Number)
    .sort((a, b) => a - b);
  for (const col of cols) {
    const v = view.filters[col];
    const key = `${name}.f${col}`;
    if (Array.isArray(v)) for (const x of v) out.push([key, x]);
    else if (v && (v.from || v.to))
      out.push([key, `~${v.from ?? ""}..${v.to ?? ""}`]);
  }
  return out;
}

/**
 * @param {string} name
 * @param {URLSearchParams} params
 * @returns {TableState}
 */
export function tableFromParams(name, params) {
  const prefix = `${name}.f`;
  /** @type {Record<number, FilterValue>} */
  const filters = {};
  for (const [k, v] of params) {
    if (!k.startsWith(prefix)) continue;
    const col = Number(k.slice(prefix.length));
    if (!Number.isInteger(col) || col < 0) continue;
    const range = /^~(.*)\.\.(.*)$/.exec(v);
    if (range) {
      /** @type {{from?: string, to?: string}} */
      const r = {};
      if (range[1]) r.from = range[1];
      if (range[2]) r.to = range[2];
      filters[col] = r;
    } else {
      const prev = filters[col];
      filters[col] = Array.isArray(prev) ? [...prev, v] : [v];
    }
  }
  return { query: params.get(`${name}.q`) ?? "", filters };
}

/**
 * A query string with every key of `name`'s table replaced by `entries`,
 * the rest kept in order.
 * @param {string} search e.g. location.search
 * @param {string} name
 * @param {[string, string][]} entries
 * @returns {string} "" or "?…"
 */
export function replaceTableParams(search, name, entries) {
  const p = new URLSearchParams(search);
  for (const k of [...new Set(p.keys())])
    if (k === `${name}.q` || k.startsWith(`${name}.f`)) p.delete(k);
  for (const [k, v] of entries) p.append(k, v);
  const s = p.toString();
  return s ? `?${s}` : "";
}

/**
 * A query string with some keys set (a null or empty value removes it).
 * @param {string} search
 * @param {Record<string, string | null | undefined>} updates
 * @returns {string} "" or "?…"
 */
export function setParams(search, updates) {
  const p = new URLSearchParams(search);
  for (const [k, v] of Object.entries(updates)) {
    if (v == null || v === "") p.delete(k);
    else p.set(k, v);
  }
  const s = p.toString();
  return s ? `?${s}` : "";
}

/**
 * One of a fixed set of choices from the query string, or the default.
 * @template {string} T
 * @param {URLSearchParams} params
 * @param {string} key
 * @param {readonly T[]} choices
 * @param {T} fallback
 * @returns {T}
 */
export function choice(params, key, choices, fallback) {
  const v = params.get(key);
  return choices.find((c) => c === v) ?? fallback;
}
