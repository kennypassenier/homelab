// feat-overview-3: the command palette's entries, as a registry. Each
// provider hands back the commands it knows for the moment the palette
// opens; the built-in ones are the pages, every stack and its tabs, and the
// themes. Later milestones register their actions here (a deploy, a backup)
// without the palette knowing them.

import { NAV, STACK_TABS, stackHref } from "./router.js";

/**
 * @typedef {{fleet: import("./fleet.js").Fleet | null,
 *   themes: readonly {name: string, label: string, dark: boolean}[],
 *   theme: string | null}} CommandContext
 * @typedef {{id: string, group: string, label: string, href?: string,
 *   run?: () => void, hint?: string, keys?: string}} Command
 * @typedef {(ctx: CommandContext) => Command[]} Provider
 */

/** @type {Map<string, Provider>} */
const providers = new Map();

/**
 * Add a provider under a name; the same name replaces the earlier one.
 * @param {string} name
 * @param {Provider} provider
 * @returns {() => void} take it out again
 */
export function registerCommands(name, provider) {
  providers.set(name, provider);
  return () => {
    if (providers.get(name) === provider) providers.delete(name);
  };
}

/**
 * Every command for `ctx`, in provider order, the first of a repeated id
 * kept. A provider that throws is skipped, so one broken entry cannot empty
 * the palette.
 * @param {CommandContext} ctx
 * @returns {Command[]}
 */
export function allCommands(ctx) {
  /** @type {Map<string, Command>} */
  const out = new Map();
  for (const p of providers.values()) {
    /** @type {Command[]} */
    let list = [];
    try {
      list = p(ctx);
    } catch {
      list = [];
    }
    for (const c of list) if (!out.has(c.id)) out.set(c.id, c);
  }
  return [...out.values()];
}

/**
 * The palette's groups, in the order the commands arrived.
 * @param {Command[]} commands
 * @returns {{group: string, commands: Command[]}[]}
 */
export function grouped(commands) {
  /** @type {Map<string, Command[]>} */
  const m = new Map();
  for (const c of commands) {
    const g = m.get(c.group);
    if (g) g.push(c);
    else m.set(c.group, [c]);
  }
  return [...m].map(([group, list]) => ({ group, commands: list }));
}

/** @type {Record<string, string>} */
const PAGE_KEYS = {
  overview: "g o",
  host: "g h",
  activity: "g a",
  timeline: "g t",
  checks: "g c",
  doctor: "g d",
};

/** @type {Provider} */
export const pageCommands = () =>
  NAV.map((n) => ({
    id: `page:${n.page}`,
    group: "Pages",
    label: n.label,
    href: n.href,
    keys: PAGE_KEYS[n.page],
  }));

/**
 * Every stack, and each of its tabs, as places to go.
 * @type {Provider}
 */
export const stackCommands = (ctx) =>
  (ctx.fleet?.stacks ?? []).flatMap((s) =>
    STACK_TABS.map((t) => ({
      id: `stack:${s.name}:${t.tab}`,
      group: "Stacks",
      label: t.tab === "overview" ? s.name : `${s.name} · ${t.label}`,
      href: stackHref(s.name, t.tab),
      hint: t.tab === "overview" ? `CT ${s.vmid}` : undefined,
    })),
  );

/**
 * feat-settings-2: every theme, as a command. `apply` does the choosing,
 * so this module stays free of the DOM.
 * @param {(name: string) => void} apply
 * @returns {Provider}
 */
export const themeCommands = (apply) => (ctx) =>
  ctx.themes.map((t) => ({
    id: `theme:${t.name}`,
    group: "Theme",
    label: `Theme: ${t.label}`,
    hint: t.name === ctx.theme ? "current" : t.dark ? "dark" : "light",
    run: () => apply(t.name),
  }));
