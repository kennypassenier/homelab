// @ts-nocheck — a test shim; its own correctness is what the tests over it show.
// A small DOM for the node tests of ui.js's blocks (redesign-kit-16): just
// enough of the browser's document — elements, attributes, classes,
// dataset, events with bubbling, focus, <dialog>, and querySelector over
// simple selectors (tag, #id, .class, [attr], [attr="v"], :scope, :focus,
// descendant and child combinators, comma lists) — to build a block and
// read what it drew. No layout: every box measures 0. `install()` puts it
// on globalThis and returns the document; tests call it before importing
// ui.js. Nothing here is installed from npm (the dashboard has no DOM
// package and adds none).

class Event_ {
  /** @param {string} type @param {{bubbles?: boolean, detail?: unknown, key?: string, shiftKey?: boolean}} [o] */
  constructor(type, o = {}) {
    this.type = type;
    this.bubbles = !!o.bubbles;
    this.detail = o.detail;
    this.key = o.key;
    this.shiftKey = !!o.shiftKey;
    this.ctrlKey = false;
    this.metaKey = false;
    this.altKey = false;
    this.defaultPrevented = false;
    this.propagationStopped = false;
    /** @type {any} */ this.target = null;
    /** @type {any} */ this.currentTarget = null;
  }
  preventDefault() {
    this.defaultPrevented = true;
  }
  stopPropagation() {
    this.propagationStopped = true;
  }
}

class Node_ {
  constructor() {
    /** @type {Node_[]} */ this.childNodes = [];
    /** @type {Element_ | null} */ this.parentNode = null;
    /** @type {Map<string, Function[]>} */ this._ls = new Map();
  }
  get parentElement() {
    return this.parentNode;
  }
  /** kp-themes' modules build through their element's own document. */
  get ownerDocument() {
    return doc ?? null;
  }
  get firstChild() {
    return this.childNodes[0] ?? null;
  }
  get isConnected() {
    /** @type {any} */ let n = this;
    while (n.parentNode) n = n.parentNode;
    return n === doc;
  }
  /** @param {string} t @param {Function} f */
  addEventListener(t, f) {
    const l = this._ls.get(t) ?? [];
    if (!l.includes(f)) l.push(f);
    this._ls.set(t, l);
  }
  /** @param {string} t @param {Function} f */
  removeEventListener(t, f) {
    this._ls.set(
      t,
      (this._ls.get(t) ?? []).filter((x) => x !== f),
    );
  }
  /** Listeners on `t`, for a test that counts them. @param {string} t */
  listeners(t) {
    return (this._ls.get(t) ?? []).length;
  }
  /** @param {Event_} e */
  dispatchEvent(e) {
    e.target ??= this;
    /** @type {any} */ let n = this;
    while (n) {
      e.currentTarget = n;
      for (const f of [...(n._ls.get(e.type) ?? [])]) f.call(n, e);
      if (!e.bubbles || e.propagationStopped) break;
      n = n.parentNode ?? (n === doc ? win : null);
    }
    return !e.defaultPrevented;
  }
  /** @param {Node_} n */
  contains(n) {
    for (let x = /** @type {Node_ | null} */ (n); x; x = x.parentNode)
      if (x === this) return true;
    return false;
  }
  /** @param {...(Node_ | string)} kids */
  append(...kids) {
    for (const k of kids) this._insert(k, this.childNodes.length);
  }
  /** @param {...(Node_ | string)} kids */
  prepend(...kids) {
    kids.forEach((k, i) => this._insert(k, i));
  }
  /** @param {...(Node_ | string)} kids */
  replaceChildren(...kids) {
    for (const c of [...this.childNodes]) c.remove();
    this.append(...kids);
  }
  /** @param {Node_ | string} k @param {number} at */
  _insert(k, at) {
    const n = typeof k === "string" ? new Text_(k) : k;
    if (n instanceof Fragment_) {
      const kids = [...n.childNodes];
      kids.forEach((c, i) => this._insert(c, at + i));
      return;
    }
    n.remove();
    n.parentNode = /** @type {any} */ (this);
    this.childNodes.splice(at, 0, n);
  }
  remove() {
    const p = this.parentNode;
    if (!p) return;
    p.childNodes.splice(p.childNodes.indexOf(this), 1);
    this.parentNode = null;
    if (doc.activeElement && !doc.activeElement.isConnected)
      doc.activeElement = doc.body;
  }
  get textContent() {
    return this.childNodes.map((c) => c.textContent).join("");
  }
  set textContent(v) {
    this.replaceChildren(...(v ? [String(v)] : []));
  }
}

class Text_ extends Node_ {
  /** @param {string} t */
  constructor(t) {
    super();
    this.nodeType = 3;
    this.data = t;
  }
  get textContent() {
    return this.data;
  }
  set textContent(v) {
    this.data = String(v);
  }
}

class Fragment_ extends Node_ {}

const camel = (/** @type {string} */ s) =>
  s.replace(/-([a-z])/g, (_, c) => c.toUpperCase());
const kebab = (/** @type {string} */ s) =>
  s.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);

class Element_ extends Node_ {
  /** @param {string} tag @param {string} [ns] */
  constructor(tag, ns) {
    super();
    this.nodeType = 1;
    this.namespaceURI = ns ?? "http://www.w3.org/1999/xhtml";
    this.tagName = ns ? tag : tag.toUpperCase();
    this.localName = tag;
    /** @type {Map<string, string>} */ this._a = new Map();
    /** @type {Record<string, string>} */ this._style = {};
    const self = this;
    this.style = new Proxy(this._style, {
      get(t, k) {
        if (k === "setProperty")
          return (/** @type {string} */ n, /** @type {string} */ v) => {
            t[n] = v;
          };
        if (k === "getPropertyValue")
          return (/** @type {string} */ n) => t[n] ?? "";
        if (k === "removeProperty")
          return (/** @type {string} */ n) => delete t[n];
        return t[/** @type {string} */ (k)] ?? "";
      },
      set(t, k, v) {
        t[/** @type {string} */ (k)] = String(v);
        return true;
      },
    });
    this.dataset = new Proxy(
      {},
      {
        get: (_, k) =>
          self.getAttribute(`data-${kebab(String(k))}`) ?? undefined,
        set: (_, k, v) => {
          self.setAttribute(`data-${kebab(String(k))}`, String(v));
          return true;
        },
        deleteProperty: (_, k) => {
          self.removeAttribute(`data-${kebab(String(k))}`);
          return true;
        },
        has: (_, k) => self.hasAttribute(`data-${kebab(String(k))}`),
        ownKeys: () =>
          [...self._a.keys()]
            .filter((a) => a.startsWith("data-"))
            .map((a) => camel(a.slice(5))),
        getOwnPropertyDescriptor: (_, k) =>
          self.hasAttribute(`data-${kebab(String(k))}`)
            ? {
                enumerable: true,
                configurable: true,
                value: self.getAttribute(`data-${kebab(String(k))}`),
              }
            : undefined,
      },
    );
    this.classList = {
      add: (/** @type {string[]} */ ...c) =>
        this._cls(new Set([...this._clsSet(), ...c])),
      remove: (/** @type {string[]} */ ...c) =>
        this._cls(new Set([...this._clsSet()].filter((x) => !c.includes(x)))),
      contains: (/** @type {string} */ c) => this._clsSet().has(c),
      toggle: (/** @type {string} */ c, /** @type {boolean} */ on) => {
        const want = on ?? !this._clsSet().has(c);
        if (want) this.classList.add(c);
        else this.classList.remove(c);
        return want;
      },
    };
    if (this.localName === "input" || this.localName === "textarea") {
      this.value = "";
      this.checked = false;
      this.disabled = false;
    }
  }
  _clsSet() {
    return new Set((this._a.get("class") ?? "").split(/\s+/).filter(Boolean));
  }
  /** @param {Set<string>} s */
  _cls(s) {
    this.setAttribute("class", [...s].join(" "));
  }
  get className() {
    return this._a.get("class") ?? "";
  }
  set className(v) {
    this.setAttribute("class", String(v));
  }
  get id() {
    return this._a.get("id") ?? "";
  }
  set id(v) {
    this.setAttribute("id", v);
  }
  get hidden() {
    return this._a.has("hidden");
  }
  set hidden(v) {
    if (v) this.setAttribute("hidden", "");
    else this.removeAttribute("hidden");
  }
  get open() {
    return this._a.has("open");
  }
  set open(v) {
    if (v) this.setAttribute("open", "");
    else this.removeAttribute("open");
  }
  get title() {
    return this._a.get("title") ?? "";
  }
  set title(v) {
    this.setAttribute("title", v);
  }
  get href() {
    return this._a.get("href") ?? "";
  }
  set href(v) {
    this.setAttribute("href", v);
  }
  get tabIndex() {
    return Number(this._a.get("tabindex") ?? -1);
  }
  set tabIndex(v) {
    this.setAttribute("tabindex", String(v));
  }
  /** @param {string} k @param {unknown} v */
  setAttribute(k, v) {
    this._a.set(k.toLowerCase(), String(v));
  }
  /** @param {string} k */
  getAttribute(k) {
    return this._a.get(k.toLowerCase()) ?? null;
  }
  /** @param {string} k */
  hasAttribute(k) {
    return this._a.has(k.toLowerCase());
  }
  /** @param {string} k */
  removeAttribute(k) {
    this._a.delete(k.toLowerCase());
  }
  get children() {
    return /** @type {Element_[]} */ (
      this.childNodes.filter((c) => c instanceof Element_)
    );
  }
  get firstElementChild() {
    return this.children[0] ?? null;
  }
  get tHead() {
    return this.children.find((c) => c.localName === "thead") ?? null;
  }
  get tBodies() {
    return this.children.filter((c) => c.localName === "tbody");
  }
  get rows() {
    return this.children.filter((c) => c.localName === "tr");
  }
  get cells() {
    return this.children.filter(
      (c) => c.localName === "td" || c.localName === "th",
    );
  }
  get childElementCount() {
    return this.children.length;
  }
  get nextElementSibling() {
    const sib = this.parentNode?.children ?? [];
    return sib[sib.indexOf(this) + 1] ?? null;
  }
  get previousElementSibling() {
    const sib = this.parentNode?.children ?? [];
    return sib[sib.indexOf(this) - 1] ?? null;
  }
  get offsetWidth() {
    return 0;
  }
  get offsetHeight() {
    return 0;
  }
  get offsetParent() {
    return this.isConnected && !this.hidden ? doc.body : null;
  }
  getBoundingClientRect() {
    return {
      left: 0,
      top: 0,
      right: 0,
      bottom: 0,
      width: 0,
      height: 0,
      x: 0,
      y: 0,
    };
  }
  /** @param {{preventScroll?: boolean}} [_o] */
  focus(_o) {
    doc.activeElement = this;
  }
  blur() {
    if (doc.activeElement === this) doc.activeElement = doc.body;
  }
  click() {
    this.dispatchEvent(new Event_("click", { bubbles: true }));
  }
  show() {
    this.open = true;
  }
  showModal() {
    this.open = true;
  }
  close() {
    if (!this.open) return;
    this.open = false;
    this.dispatchEvent(new Event_("close"));
  }
  /** @param {string} sel */
  matches(sel) {
    return parse(sel).some((chain) => matchChain(this, chain, null));
  }
  /** @param {string} sel */
  closest(sel) {
    for (let n = /** @type {Element_ | null} */ (this); n; n = n.parentNode)
      if (n instanceof Element_ && n.matches(sel)) return n;
    return null;
  }
  /** @param {string} sel @returns {Element_[]} */
  querySelectorAll(sel) {
    const chains = parse(sel);
    /** @type {Element_[]} */ const out = [];
    const walk = (/** @type {Node_} */ n) => {
      for (const c of n.childNodes)
        if (c instanceof Element_) {
          if (chains.some((ch) => matchChain(c, ch, this))) out.push(c);
          walk(c);
        }
    };
    walk(this);
    return out;
  }
  /** @param {string} sel */
  querySelector(sel) {
    return this.querySelectorAll(sel)[0] ?? null;
  }
}

/**
 * A selector as chains of compounds joined by " " or ">".
 * @param {string} sel
 * @returns {{comb: string, parts: string[]}[][]}
 */
function parse(sel) {
  return sel.split(",").map((one) => {
    const toks =
      one
        .trim()
        .replace(/\s*>\s*/g, " > ")
        .match(/(?:\[[^\]]*\]|[^\s\[])+|>/g) ?? [];
    /** @type {{comb: string, parts: string[]}[]} */ const chain = [];
    let comb = " ";
    for (const t of toks) {
      if (t === ">") {
        comb = ">";
        continue;
      }
      chain.push({
        comb,
        parts: t.match(/\[[^\]]*\]|[.#:]?[^.#:\[]+/g) ?? [],
      });
      comb = " ";
    }
    return chain;
  });
}

/** @param {Element_} e @param {string[]} parts @param {Element_ | null} scope */
function compound(e, parts, scope) {
  return parts.every((p) => {
    if (p === "*") return true;
    if (p === ":scope") return e === scope;
    if (p === ":focus") return doc.activeElement === e;
    if (p.startsWith("#")) return e.id === p.slice(1);
    if (p.startsWith(".")) return e._clsSet().has(p.slice(1));
    if (p.startsWith("[")) {
      const m = p.match(/^\[([^=\]]+)(?:="?([^"\]]*)"?)?\]$/);
      if (!m) return false;
      const v = e.getAttribute(m[1]);
      return m[2] === undefined ? v != null : v === m[2];
    }
    return e.localName === p.toLowerCase();
  });
}

/** @param {Element_} e @param {{comb: string, parts: string[]}[]} chain @param {Element_ | null} scope */
function matchChain(e, chain, scope) {
  const at = (/** @type {Element_} */ el, /** @type {number} */ i) => {
    if (!compound(el, chain[i].parts, scope)) return false;
    if (i === 0) return true;
    if (chain[i].comb === ">") {
      const p = el.parentNode;
      return p instanceof Element_ && at(p, i - 1);
    }
    for (let p = el.parentNode; p instanceof Element_; p = p.parentNode)
      if (at(p, i - 1)) return true;
    return false;
  };
  return at(e, chain.length - 1);
}

class Document_ extends Element_ {
  constructor() {
    super("#document");
    this.documentElement = new Element_("html");
    this.head = new Element_("head");
    this.body = new Element_("body");
    this.documentElement.append(this.head, this.body);
    this.append(this.documentElement);
    /** @type {Element_} */ this.activeElement = this.body;
  }
  get defaultView() {
    return win;
  }
  /** @param {string} t */
  createElement(t) {
    return new Element_(t);
  }
  /** @param {string} ns @param {string} t */
  createElementNS(ns, t) {
    return new Element_(t, ns);
  }
  /** @param {string} t */
  createTextNode(t) {
    return new Text_(t);
  }
  createDocumentFragment() {
    return new Fragment_();
  }
  /** @param {string} id */
  getElementById(id) {
    return this.querySelector(`#${id}`);
  }
  createRange() {
    return { selectNodeContents() {} };
  }
}

/** @type {Document_} */
let doc;
/** @type {any} */
let win;

/**
 * Installs a fresh document on globalThis (with `#page` in its body) and
 * returns it.
 * @returns {any}
 */
export function install() {
  doc = new Document_();
  const page = new Element_("main");
  page.id = "page";
  doc.body.append(page);
  win = new Node_();
  const g = /** @type {any} */ (globalThis);
  Object.assign(g, {
    document: doc,
    window: g,
    Event: Event_,
    CustomEvent: Event_,
    KeyboardEvent: Event_,
    Node: Node_,
    Element: Element_,
    HTMLElement: Element_,
    HTMLAnchorElement: Element_,
    HTMLDetailsElement: class {},
    HTMLDialogElement: Element_,
    HTMLInputElement: Element_,
    HTMLTableElement: Element_,
    SVGElement: Element_,
    CSS: { escape: (/** @type {string} */ s) => s },
    innerWidth: 1894,
    innerHeight: 1000,
    scrollX: 0,
    scrollY: 0,
    scrollTo() {},
    matchMedia: () => ({
      matches: false,
      addEventListener() {},
      removeEventListener() {},
    }),
    getComputedStyle: () => ({ getPropertyValue: () => "" }),
    requestAnimationFrame: (/** @type {Function} */ f) => setTimeout(f, 0),
  });
  if (!g.localStorage) {
    /** @type {Map<string, string>} */ const m = new Map();
    g.localStorage = {
      getItem: (/** @type {string} */ k) => m.get(k) ?? null,
      setItem: (/** @type {string} */ k, /** @type {string} */ v) =>
        m.set(k, String(v)),
      removeItem: (/** @type {string} */ k) => m.delete(k),
      clear: () => m.clear(),
    };
  }
  g.addEventListener = win.addEventListener.bind(win);
  g.removeEventListener = win.removeEventListener.bind(win);
  return doc;
}

/** A key press on `target`, bubbling. @param {any} target @param {string} key @param {{shiftKey?: boolean}} [o] */
export function press(target, key, o = {}) {
  const e = new Event_("keydown", { bubbles: true, key, ...o });
  target.dispatchEvent(e);
  return e;
}

/** A click with Shift held, bubbling. @param {any} target */
export function shiftClick(target) {
  target.dispatchEvent(new Event_("click", { bubbles: true, shiftKey: true }));
}

/** A pointer press on `target`, bubbling. @param {any} target */
export function pointerDown(target) {
  target.dispatchEvent(new Event_("pointerdown", { bubbles: true }));
}
