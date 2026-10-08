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
//   e  a "problems" counter that reads 0 draws no red chip beside it;
//   f  no internal id in a person's words: a working reference ("fix-193"),
//      an incident bundle's file name, a test fixture's words;
//   g  the page's freshness ("updated 4 s ago") in one slot: the title row,
//      after the title, never with the actions or in a meta row;
//   h  exact counts: a count linked to its rows (`data-count-of`, ui.js
//      countOf) says how many rows it describes ("N" or "N of M");
//   uni  uniform siblings: the tiles of one KPI strip or one Apps board
//      share one height, cards side by side in a row share one height,
//      and a card's foot line sits at the card's bottom.
// Not a test file itself (no `.e2e.js`): invariants.e2e.js imports it.

/**
 * The page-side audit. Self-contained: Playwright serialises it.
 * @param {string | null} rootSel the dialog to audit, or null for the page
 * @returns {{a: string[], b: string[], c: string[], d: string[], e: string[], f: string[], g: string[], h: string[], hn: string[], uni: string[]}}
 */
export function layoutAudit(rootSel) {
  /** @type {{a: string[], b: string[], c: string[], d: string[], e: string[], f: string[], g: string[], h: string[], hn: string[], uni: string[]}} */
  const out = {
    a: [],
    b: [],
    c: [],
    d: [],
    e: [],
    f: [],
    g: [],
    h: [],
    hn: [],
    uni: [],
  };
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

  // ---- h: a count is the rows it describes (Kenny: exact counts) ----
  // A count of a list's rows — "N of M shown", "· N" on a fold or a
  // heading, "N items|rows|operations|…" in a card's head, foot or toolbar
  // state — must name its list (ui.js countOf); an unlinked one fails.
  const COUNT =
    /(?:^|\s)·\s*\d+(?![\d/:.\w-])|\b\d+ of \d+\b|\b\d+ (?:items?|rows?|operations?|lines?|stacks?|rules?|repositor(?:y|ies)|snapshots?|apps?|entries)\b/;
  for (const t of texts) {
    const el = /** @type {HTMLElement} */ (t.parentElement);
    if (!COUNT.test(t.data)) continue;
    if (
      !el.closest(
        ".nx-card__head, .nx-card__foot, summary, .nx-tb__state, .section-head, h2, h3",
      )
    )
      continue;
    if (el.closest("[data-count-of], .nx-kpi, [class*='chip'], button, a"))
      continue;
    out.h.push(`count ${say(el)} names no list (ui.js countOf)`);
  }
  let checked = 0;
  for (const c of root.querySelectorAll("[data-count-of]")) {
    const el = /** @type {HTMLElement} */ (c);
    if (!shown(el)) continue;
    const list = document.getElementById(el.dataset.countOf ?? "");
    const n = /(\d+)/.exec(el.textContent ?? "");
    if (!list || !n) continue;
    const rows = [
      ...list.querySelectorAll(el.dataset.countRows || ":scope > *"),
    ].filter(
      (r) =>
        !(/** @type {HTMLElement} */ (r).hidden) &&
        getComputedStyle(r).display !== "none",
    ).length;
    checked += 1;
    if (Number(n[1]) !== rows)
      out.h.push(`count ${say(el)} says ${n[1]}, its list holds ${rows} rows`);
  }
  out.hn.push(String(checked));

  // ---- f: internal ids in user text (redesign-final M8) ----
  for (const t of texts) {
    const m = t.data.match(
      /\b(?:fix|gap|redesign(?:-[a-z0-9]+)?)-\d+\b|\b\d{10}-[a-z][\w-]*|\bfixture\b/i,
    );
    if (m)
      out.f.push(
        `internal id "${m[0]}" in ${say(/** @type {Element} */ (t.parentElement))}`,
      );
  }

  if (rootSel) return out;

  // ---- g: the page's freshness next to its title (redesign-final X1) ----
  for (const live of root.querySelectorAll(".nx-head .nx-live")) {
    if (!shown(live)) continue;
    const row = live.closest(".title-row");
    if (
      live.closest(".nx-head-meta, .nx-head-actions") ||
      !row?.querySelector(":scope > :is(h1, h2)")
    )
      out.g.push(`freshness ${say(live)} is not in the title's row`);
  }

  // ---- uni: uniform siblings (fix-371-1, Kenny 2026-10-04) ----
  const tall = (/** @type {Element} */ e) => e.getBoundingClientRect().height;
  for (const set of [
    ...[...root.querySelectorAll(".nx-kpis")].map((s) => [
      ...s.querySelectorAll(":scope > .nx-kpi"),
    ]),
    ...[...root.querySelectorAll(".ap-board")].map((b) => [
      ...b.querySelectorAll(".ap-tile:not(.ap-tile--skeleton)"),
    ]),
  ]) {
    const hs = set.filter(shown).map(tall);
    if (hs.length > 1 && Math.max(...hs) - Math.min(...hs) > 1)
      out.uni.push(
        `${say(/** @type {Element} */ (set[0].parentElement)).slice(0, 50)}: tiles ${Math.round(Math.min(...hs))} to ${Math.round(Math.max(...hs))} px tall`,
      );
  }
  /** @type {Map<string, number[]>} cards of one grid row, by grid and top */
  const rowsOf = new Map();
  const grids = new Map();
  for (const c of root.querySelectorAll(".nx-card")) {
    const p = c.parentElement;
    if (!p || !shown(c) || getComputedStyle(p).display !== "grid") continue;
    if (!grids.has(p)) grids.set(p, grids.size);
    const key = `${grids.get(p)}@${Math.round(c.getBoundingClientRect().top)}`;
    rowsOf.set(key, [...(rowsOf.get(key) ?? []), tall(c)]);
  }
  for (const [key, hs] of rowsOf)
    if (hs.length > 1 && Math.max(...hs) - Math.min(...hs) > 1)
      out.uni.push(
        `cards side by side (grid ${key}) are ${Math.round(Math.min(...hs))} to ${Math.round(Math.max(...hs))} px tall`,
      );
  for (const f of root.querySelectorAll(".nx-card > .nx-card__foot")) {
    if (!shown(f)) continue;
    const card = /** @type {Element} */ (f.parentElement);
    const cs = getComputedStyle(card);
    const gap =
      card.getBoundingClientRect().bottom -
      parseFloat(cs.paddingBlockEnd) -
      parseFloat(cs.borderBlockEndWidth) -
      f.getBoundingClientRect().bottom;
    if (gap > 1)
      out.uni.push(
        `${say(card).slice(0, 50)}: its foot sits ${Math.round(gap)} px above the card's bottom`,
      );
  }

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

/**
 * redesign-final-45 (Kenny, 2026-10-04: one walk, not a walker per rule):
 * the page-level checks that each had a walk of their own, run in the
 * same walk as `layoutAudit`, on the page (never a dialog). Self-contained:
 * Playwright serialises it. The width is the page's own.
 *   desc      a one-sentence description right under the title (1894 px)
 *   descgap   that description sits directly under the title, not a
 *             section gap away
 *   titlerow  the title row's controls end at its right edge (1894 px),
 *             sit in one row on a desktop, and fold under the title
 *             left-aligned on a phone
 *   mono      monospace reads at nearly its row's own size (1894 px)
 *   sideways  no sideways scroll and no table wider than its box (390 px)
 *   links     a link names an absolute http(s) address or a same-site
 *             path, a section link a section this page has, and a
 *             repository address shown as text is a link (1894 px; the
 *             same-site paths come back in `hrefs`, resolved by the caller)
 * @returns {{desc: string[], descgap: string[], titlerow: string[],
 *   mono: string[], sideways: string[], links: string[], hrefs: string[]}}
 */
export function pageChecks() {
  /** @type {{desc: string[], descgap: string[], titlerow: string[], mono: string[], sideways: string[], links: string[], hrefs: string[]}} */
  const out = {
    desc: [],
    descgap: [],
    titlerow: [],
    mono: [],
    sideways: [],
    links: [],
    hrefs: [],
  };
  const wide = window.innerWidth > 800;
  const h1 = document.querySelector("main h1");

  // ---- desc: a description under the title ----
  if (wide && h1) {
    const top = h1.getBoundingClientRect().bottom;
    const has = [...document.querySelectorAll("main p")].some((p) => {
      const r = p.getBoundingClientRect();
      return (
        r.height > 0 &&
        r.top >= top - 4 &&
        r.top - top < 120 &&
        (p.textContent ?? "").trim().length > 20
      );
    });
    if (!has) out.desc.push("no one-sentence description under the title");
  }

  // ---- descgap: directly under it ----
  if (h1) {
    const head = h1.closest(".title-row") ?? h1;
    const desc = head.nextElementSibling;
    if (desc && desc.tagName === "P") {
      const parts =
        head === h1
          ? [h1]
          : [...head.children].filter(
              (c) => /** @type {HTMLElement} */ (c).offsetParent,
            );
      const bottom = Math.max(
        ...parts.map((p) => p.getBoundingClientRect().bottom),
      );
      const gap = Math.round(desc.getBoundingClientRect().top - bottom);
      if (gap > 12 || gap < 0)
        out.descgap.push(`the description sits ${gap}px under the title`);
    }
  }

  // ---- titlerow ----
  if (wide)
    for (const r of document.querySelectorAll("main .title-row")) {
      if (!(/** @type {HTMLElement} */ (r).offsetParent)) continue;
      if (!r.querySelector(":scope > h1") || r.children.length < 2) continue;
      // The freshness after the title (redesign-final X1) and the chips
      // beside it (pageHeader titleMeta) are no controls; since kp-themes
      // 9.2.0's page header the actions sit outside the title row.
      // A title's own identity (the stack hub's mark before the name, its
      // state word after it) is no control either: only what can be used
      // counts (kp-themes 10's page header keeps the actions in
      // `.kp-page-header__actions`, outside the row).
      const CONTROL =
        "button, a[href], input, select, textarea, summary, [role=button], [tabindex]:not([tabindex='-1'])";
      const kids = [...r.children].filter(
        (c) =>
          /** @type {HTMLElement} */ (c).offsetParent &&
          !c.matches(":is(h1, h2), .nx-live, .nx-head-titlemeta") &&
          (c.matches(CONTROL) || !!c.querySelector(CONTROL)),
      );
      if (!kids.length) continue;
      const short = Math.round(
        r.getBoundingClientRect().right -
          kids[kids.length - 1].getBoundingClientRect().right,
      );
      if (short > 2)
        out.titlerow.push(
          `the controls stop ${short}px short of the right edge`,
        );
    }
  const row = h1?.closest(".title-row");
  if (h1 && row) {
    const t = h1.getBoundingClientRect();
    const ctls = [
      ...row.querySelectorAll("button, a.kp-button, .state, .badge"),
    ]
      .filter((e) => /** @type {HTMLElement} */ (e).offsetParent)
      .map((e) => e.getBoundingClientRect())
      .filter((b) => b.width > 0);
    if (ctls.length) {
      const stacked = ctls.some((a) => ctls.some((b) => b.top >= a.bottom - 1));
      const below = ctls.filter((b) => b.top >= t.bottom - 2);
      const left = below.length
        ? Math.round(Math.min(...below.map((b) => b.left)) - t.left)
        : null;
      if (wide && stacked) out.titlerow.push("controls stacked in a column");
      if (left !== null && Math.abs(left) > 2)
        out.titlerow.push(
          `folded controls start ${left}px from the title's edge`,
        );
    }
  }

  // ---- mono ----
  if (wide)
    for (const e of document.querySelectorAll("main .mono")) {
      if (!(/** @type {HTMLElement} */ (e).offsetParent)) continue;
      const own = parseFloat(getComputedStyle(e).fontSize);
      const parent = parseFloat(
        getComputedStyle(/** @type {Element} */ (e.parentElement)).fontSize,
      );
      if (own / parent < 0.8 || own < 12)
        out.mono.push(
          `"${(e.textContent ?? "").trim().slice(0, 30)}" at ${own}px`,
        );
    }

  // ---- sideways ----
  if (!wide) {
    const over =
      document.documentElement.scrollWidth -
      document.documentElement.clientWidth;
    if (over > 0) out.sideways.push(`the page scrolls sideways by ${over} px`);
    for (const w of document.querySelectorAll(
      "main .kp-table-wrap, main .bk-table-wrap",
    )) {
      // A table in a shut fold or a hidden view is not on screen.
      if (!w.checkVisibility()) continue;
      const x = w.scrollWidth - w.clientWidth;
      if (x > 1) out.sideways.push(`a table overflows its box by ${x} px`);
    }
  }

  // ---- links ----
  if (wide) {
    for (const a of document.querySelectorAll("a[href]")) {
      const href = a.getAttribute("href") ?? "";
      if (/^https?:\/\/[^/\s]+/.test(href)) continue;
      if (href.startsWith("/") && !href.startsWith("//")) {
        out.hrefs.push(href);
        continue;
      }
      out.links.push(`${JSON.stringify(href)} has no scheme and no leading /`);
    }
    for (const a of document.querySelectorAll("a[href*='#']")) {
      const u = new URL(/** @type {HTMLAnchorElement} */ (a).href);
      if (
        u.origin === location.origin &&
        u.pathname === location.pathname &&
        u.hash.length > 1 &&
        !document.getElementById(decodeURIComponent(u.hash.slice(1)))
      )
        out.links.push(`${u.hash} points at a section this page does not have`);
    }
    const walk = document.createTreeWalker(
      document.getElementById("page") ?? document.body,
      NodeFilter.SHOW_TEXT,
    );
    for (let n = walk.nextNode(); n; n = walk.nextNode()) {
      const m =
        /\b(?:github\.com|gitlab\.com|codeberg\.org)\/[\w.-]+\/[\w.-]+/.exec(
          n.textContent ?? "",
        );
      if (!m) continue;
      const a = n.parentElement?.closest("a[href]");
      if (!a || !/^https?:\/\//.test(a.getAttribute("href") ?? ""))
        out.links.push(`"${m[0]}" is shown as text, not as a working link`);
    }
  }
  return out;
}

/**
 * redesign-final-45: the action dialog's field grid (folded from its own
 * case): labels on one edge, controls on another, each control beside its
 * label, at desktop width. Run on the audited dialog.
 * @param {string} rootSel the dialog
 * @returns {{faults: string[], fields: number}}
 */
export function dialogGrid(rootSel) {
  /** @type {string[]} */
  const out = [];
  const d = document.querySelector(rootSel);
  if (!d || window.innerWidth <= 800) return { faults: out, fields: 0 };
  const step = [...d.querySelectorAll("[data-kp-step]")].find(
    (s) => !(/** @type {HTMLElement} */ (s).hidden),
  );
  if (!step) return { faults: out, fields: 0 };
  const edges = [...step.querySelectorAll(":scope > .kp-field")]
    .filter((f) => /** @type {HTMLElement} */ (f).offsetParent)
    .map((f) => {
      const label = f.querySelector(".kp-field__label");
      const ctl = f.querySelector(".kp-field__input, .kp-field__check");
      if (!label || !ctl) return null;
      const l = label.getBoundingClientRect();
      const c = ctl.getBoundingClientRect();
      if (c.width === 0 || c.height === 0) return null;
      return { label: Math.round(l.left), ctl: Math.round(c.left) };
    })
    .filter((e) => e != null);
  if (!edges.length) return { faults: out, fields: 0 };
  const labels = new Set(edges.map((e) => e.label));
  const ctls = new Set(edges.map((e) => e.ctl));
  if (labels.size > 1) out.push(`labels start at ${[...labels].join(", ")}`);
  if (ctls.size > 1) out.push(`controls start at ${[...ctls].join(", ")}`);
  if (edges.some((e) => e.ctl <= e.label))
    out.push("a control sits under its label instead of beside it");
  return { faults: out, fields: edges.length };
}
