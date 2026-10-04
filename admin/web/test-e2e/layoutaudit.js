// redesign-final-gen (3.71.0's final review, 2026-10-04): the generic
// whole-screen checks behind the review's finding classes, run in the page
// (`page.evaluate(layoutAudit, root)`) over every page and every action
// dialog or drawer, at 1894 and 390 px:
//   a  no word wraps per letter, no text is cut or spills out of its box
//      (an ellipsis only where the element carries the whole text as its
//      title);
//   b  no row's content runs into the next row (a table row's chips into
//      the row below, a list row into the next one);
//   c  one date format, dd/mm/yyyy HH:MM: never a weekday or month name;
//   d  one page-level header: at most one heading of the h1's size;
//   e  a "problems" counter that reads 0 draws no red chip beside it.
// Not a test file itself (no `.e2e.js`): invariants.e2e.js imports it.

/**
 * The page-side audit. Self-contained: Playwright serialises it.
 * @param {string | null} rootSel the dialog to audit, or null for the page
 * @returns {{a: string[], b: string[], c: string[], d: string[], e: string[]}}
 */
export function layoutAudit(rootSel) {
  /** @type {{a: string[], b: string[], c: string[], d: string[], e: string[]}} */
  const out = { a: [], b: [], c: [], d: [], e: [] };
  const root = /** @type {HTMLElement | null} */ (
    rootSel
      ? document.querySelector(rootSel)
      : (document.querySelector("main") ?? document.body)
  );
  if (!root) return out;
  const say = (/** @type {Element} */ el) => {
    const t = (el.textContent ?? "").trim().replace(/\s+/g, " ");
    const cls = el.getAttribute("class")?.split(/\s+/)[0] ?? "";
    return `${el.tagName.toLowerCase()}${cls ? `.${cls}` : ""} "${t.slice(0, 50)}"`;
  };
  const shown = (/** @type {Element} */ el) => {
    const he = /** @type {HTMLElement} */ (el);
    if (he.closest("[hidden],[aria-hidden='true'],template,svg")) return false;
    // Inside a shut <details> or a hidden box: never painted.
    if (
      !he.checkVisibility({
        contentVisibilityAuto: true,
        visibilityProperty: true,
      })
    )
      return false;
    const cs = getComputedStyle(he);
    if (cs.visibility === "hidden" || cs.display === "none") return false;
    // Screen-reader-only on it or on an ancestor (a card table's head).
    for (
      let a = he;
      a && a !== document.body;
      a = /** @type {HTMLElement} */ (a.parentElement)
    ) {
      const s = a === he ? cs : getComputedStyle(a);
      if (
        s.position === "absolute" &&
        (s.clip !== "auto" || s.clipPath !== "none")
      )
        return false;
      const b = a.getBoundingClientRect();
      if (
        s.display !== "contents" &&
        s.overflow !== "visible" &&
        (b.width <= 2 || b.height <= 2)
      )
        return false;
    }
    const r = he.getBoundingClientRect();
    return r.width > 2 && r.height > 2;
  };
  /** Skeletons, screen-reader-only and decorative pieces. */
  const skip = (/** @type {Element} */ el) =>
    !!el.closest(
      ".nx-skel,.skeleton,[class*='skel'],.sr-only,.visually-hidden,.nx-sr,input,textarea,select,option,pre,code,.cm-editor",
    );

  // ---- a: per-letter wraps, cut and spilling text ----
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  /** @type {Text[]} */
  const texts = [];
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const t = /** @type {Text} */ (n);
    if (!t.data.trim() || !t.parentElement) continue;
    if (skip(t.parentElement) || !shown(t.parentElement)) continue;
    texts.push(t);
  }
  const range = document.createRange();
  for (const t of texts) {
    const el = /** @type {HTMLElement} */ (t.parentElement);
    // A word broken over lines with a line holding at most two of its
    // letters: "DEPLO / Y".
    for (const m of t.data.matchAll(/\S{4,}/g)) {
      range.setStart(t, m.index ?? 0);
      range.setEnd(t, (m.index ?? 0) + m[0].length);
      const tops = new Set(
        [...range.getClientRects()]
          .filter((r) => r.width > 0)
          .map((r) => Math.round(r.top)),
      );
      if (tops.size < 2) continue;
      /** @type {Map<number, number>} */
      const per = new Map();
      for (let i = 0; i < m[0].length; i++) {
        range.setStart(t, (m.index ?? 0) + i);
        range.setEnd(t, (m.index ?? 0) + i + 1);
        const r = range.getClientRects()[0];
        if (!r) continue;
        const k = Math.round(r.top);
        per.set(k, (per.get(k) ?? 0) + 1);
      }
      if ([...per.values()].some((c) => c <= 2))
        out.a.push(
          `word "${m[0].slice(0, 30)}" wraps per letter in ${say(el)}`,
        );
    }
    // Cut or spilling: the text's box against every clipping ancestor and
    // against its own block.
    range.selectNodeContents(t);
    const tr = range.getBoundingClientRect();
    if (tr.width === 0) continue;
    for (
      let a = el;
      a && a !== root.parentElement;
      a = /** @type {HTMLElement} */ (a.parentElement)
    ) {
      const cs = getComputedStyle(a);
      const clipX = cs.overflowX === "hidden" || cs.overflowX === "clip";
      const clipY = cs.overflowY === "hidden" || cs.overflowY === "clip";
      const scroller = /auto|scroll/.test(cs.overflowX + cs.overflowY);
      if (scroller) break;
      if (!clipX && !clipY) continue;
      const b = a.getBoundingClientRect();
      const outX = clipX && (tr.right > b.right + 1 || tr.left < b.left - 1);
      const outY =
        clipY &&
        cs.webkitLineClamp === "none" &&
        (tr.bottom > b.bottom + 2 || tr.top < b.top - 2);
      if (!outX && !outY) continue;
      const marked =
        cs.textOverflow === "ellipsis" &&
        !!(a.title || a.closest("[title]") || a.getAttribute("aria-label"));
      if (!marked) out.a.push(`text cut by ${say(a)}: ${say(el)}`);
      break;
    }
    // Spilling past its own block (overflow visible).
    let blk = el;
    while (
      blk &&
      getComputedStyle(blk).display.startsWith("inline") &&
      blk.parentElement
    )
      blk = blk.parentElement;
    const bs = getComputedStyle(blk);
    if (
      bs.overflowX === "visible" &&
      !/^(table|table-row-group|table-row)$/.test(bs.display)
    ) {
      const b = blk.getBoundingClientRect();
      if (tr.right > b.right + 2 || tr.left < b.left - 2)
        out.a.push(
          `text spills out of ${say(blk)} by ${Math.round(Math.max(tr.right - b.right, b.left - tr.left))} px`,
        );
    }
  }

  // ---- b: a row's content running into the next row ----
  const leaves = [...root.querySelectorAll("*")].filter(
    (el) =>
      (el.childElementCount === 0 ||
        [...el.childNodes].some(
          (n) => n.nodeType === 3 && n.textContent?.trim(),
        )) &&
      !skip(el) &&
      shown(el) &&
      !["fixed", "absolute", "sticky"].includes(getComputedStyle(el).position),
  );
  /** @type {Set<string>} */
  const seen = new Set();
  for (const leaf of leaves) {
    let bottom = leaf.getBoundingClientRect().bottom;
    const lr = leaf.getBoundingClientRect();
    for (
      let a = /** @type {HTMLElement} */ (leaf);
      a && a !== root;
      a = /** @type {HTMLElement} */ (a.parentElement)
    ) {
      const cs = getComputedStyle(a);
      if (["fixed", "absolute", "sticky"].includes(cs.position) && a !== leaf)
        break;
      // A `display: contents` box has no box of its own to compare.
      if (cs.display === "contents") continue;
      const ar = a.getBoundingClientRect();
      if (cs.overflowY !== "visible") bottom = Math.min(bottom, ar.bottom);
      const s = a.nextElementSibling;
      if (
        !s ||
        !shown(s) ||
        ["fixed", "absolute", "sticky"].includes(getComputedStyle(s).position)
      )
        continue;
      const sr = s.getBoundingClientRect();
      const nextRow = sr.top > ar.top + 2;
      const sideBySide = sr.left < lr.right && sr.right > lr.left;
      if (nextRow && sideBySide && bottom > sr.top + 2) {
        const key = `${say(a)}>${say(s)}`;
        if (!seen.has(key)) {
          seen.add(key);
          out.b.push(
            `${say(leaf)} in ${say(a)} runs ${Math.round(bottom - sr.top)} px into the next row ${say(s)}`,
          );
        }
        break;
      }
    }
  }

  // ---- c: one date format (Kenny's rule, REGISTER fix-216: dd/mm/yyyy
  // HH:MM): never a weekday or a month's name ("Sat 3 Oct", "30 Sep 12:14")
  for (const t of texts) {
    const m = t.data.match(
      /\b(?:(?:Mon|Tue|Wed|Thu|Fri|Sat|Sun) \d{1,2} (?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\b|\d{1,2} (?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)(?: \d{4})?,? \d{2}:\d{2})/,
    );
    if (m)
      out.c.push(
        `date "${m[0]}" not in dd/mm/yyyy in ${say(/** @type {Element} */ (t.parentElement))}`,
      );
  }

  if (rootSel) return out;

  // ---- d: one page-level header ----
  const h1s = [...document.querySelectorAll("h1")].filter(
    (h) => shown(h) && !h.closest("dialog"),
  );
  if (h1s.length > 1)
    out.d.push(`${h1s.length} h1s: ${h1s.map((h) => say(h)).join(", ")}`);
  if (h1s.length) {
    const big = parseFloat(getComputedStyle(h1s[0]).fontSize) * 0.85;
    const rivals = [...root.querySelectorAll("h2,h3,h4,[role=heading]")].filter(
      (h) =>
        shown(h) &&
        !h.closest("dialog") &&
        parseFloat(getComputedStyle(h).fontSize) >= big,
    );
    for (const r of rivals) out.d.push(`a heading of the h1's size: ${say(r)}`);
  }

  // ---- e: a problems counter of 0 beside red chips ----
  const bad =
    ".nx-chip--bad,.cf-chip--bad,.sk-chip--bad,.kp-badge--destructive,[data-tone='bad']";
  const counters = [...root.querySelectorAll(".nx-kpi,button")].filter(
    (c) => shown(c) && !c.closest("dialog") === !rootSel,
  );
  for (const k of counters) {
    const label = k.classList.contains("nx-kpi")
      ? (k.querySelector(".nx-kpi__label")?.textContent ?? "")
      : (k.textContent ?? "").replace(/[\d\s·]+$/, "");
    if (!/^\s*(problems?|failing|failures?|broken)\s*$/i.test(label)) continue;
    const num = k.classList.contains("nx-kpi")
      ? (k.querySelector(".nx-kpi__value")?.textContent ?? "")
      : ((k.textContent ?? "").match(/(\d+)\s*$/)?.[1] ?? "");
    if (parseInt(num, 10) !== 0) continue;
    const red = [...root.querySelectorAll(bad)].filter(
      (c) => shown(c) && !c.closest(".nx-kpi"),
    );
    if (red.length)
      out.e.push(
        `"${label.trim()}" reads 0 beside ${red.length} red chips (${say(red[0])})`,
      );
  }
  return out;
}
