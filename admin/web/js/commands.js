// feat-overview-3: the command palette's entries, as a registry. Each
// provider hands back the commands it knows for the moment the palette
// opens; the built-in ones are the areas and their pages, every stack and
// its tabs, every action, the Inbox's open items and the themes.
//
// feat-shell-2 (redesign 3.71.0, FLOWS.md §1.2): every command belongs to
// one of four sections, shown in this order — Inbox (open items), Do
// (actions: the open stack's first), Go to (areas, pages, stacks, tabs),
// Theme — and carries the hidden `words` a person may type for it, so
// intent.js can find "update jellyfin" or "gateway logs" in any word order.

import { AREAS, SUB_PAGES } from "./areas.js";
import { pages } from "./pages.js";
import { STACK_TABS, stackHref } from "./router.js";

/**
 * @typedef {{fleet: import("./fleet.js").Fleet | null,
 *   themes: readonly {name: string, label: string, dark: boolean}[],
 *   theme: string | null}} CommandContext
 * @typedef {import("./intent.js").Command} Command
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

/**
 * The six areas (the registry's `nav: true` pages, so their titles are the
 * server's) and every page inside them (areas.js), as places to go. Empty
 * of areas until the registry has answered; the sub-pages are always
 * there.
 * @type {Provider}
 */
export const pageCommands = () => {
  const reg = pages()?.pages ?? [];
  /** @type {Command[]} */
  const out = [];
  for (const a of AREAS) {
    const p = reg.find((x) => x.id === a.id && x.nav);
    if (!p) continue;
    out.push({
      id: `page:${a.id}`,
      group: "Go to",
      label: p.title,
      href: p.path,
      hint: a.what,
      keys: a.keys,
      words: `${a.id} area`,
    });
  }
  if (reg.length === 0) return out;
  for (const s of SUB_PAGES) {
    const area = AREAS.find((a) => a.id === s.area);
    out.push({
      id: `page:${s.id}`,
      group: "Go to",
      label: s.label,
      href: s.href,
      hint: `${area?.label ?? ""} · ${s.what}`,
      words: s.words,
    });
  }
  return out;
};

/** What else a person may type for a stack tab. */
const TAB_WORDS = /** @type {Record<string, string>} */ ({
  overview: "hub status state health checks healthy",
  logs: "log journal errors read",
  apps: "versions containers images",
  backups: "snapshots restore",
  history: "past who what happened",
  settings: "secrets firewall size network files",
});

/**
 * Every stack's hub, and each of its tabs, as places to go.
 * @type {Provider}
 */
export const stackCommands = (ctx) =>
  (ctx.fleet?.stacks ?? []).flatMap((s) =>
    STACK_TABS.map((t) => ({
      id: `stack:${s.name}:${t.tab}`,
      group: "Go to",
      label: t.tab === "overview" ? s.name : `${s.name} · ${t.label}`,
      href: stackHref(s.name, t.tab),
      hint: t.tab === "overview" ? `stack · CT ${s.vmid}` : undefined,
      words: `stack ${TAB_WORDS[t.tab] ?? ""}`,
      stack: s.name,
      featured: t.tab === "overview",
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
    words: "theme colour color look",
    run: () => apply(t.name),
  }));

/**
 * What else a person may type for an action, by the words of its slug
 * (generic verbs, never an app's name).
 * @param {string} action
 */
export function actionWords(action) {
  /** @type {[RegExp, string][]} */
  const table = [
    [/^update|release-update/, "upgrade pull image new version newer"],
    [/^backup/, "back up snapshot save"],
    [/^restore/, "recover bring back snapshot undo data"],
    [/^verify-restore/, "drill test snapshot"],
    [/^deploy/, "apply reconcile files"],
    [/^change-secret/, "secret password token env latch"],
    [/^disable/, "park pause disable"],
    [/^enable/, "unpark resume enable"],
    [/^guards/, "guards log caps"],
    [/^destroy|^forget|^wipe/, "delete remove"],
    [/^rollback/, "roll back undo previous"],
    [/^apply$/, "apply repository fleet plan"],
  ];
  return table
    .filter(([re]) => re.test(action))
    .map(([, w]) => w)
    .join(" ");
}

/**
 * Whether an action applies to a stack: a `*-native` action only to a
 * stack of adopted services (FLOWS.md §4: the palette listed Restore
 * (native) for stacks that have no native unit).
 * @param {string} action
 * @param {{native?: boolean}} stack
 */
export const appliesTo = (action, stack) =>
  !/-native$/.test(action) || stack.native === true;

/**
 * feat-stacks-4: every action, as a command in Do. The stack whose hub is
 * open comes first; then the host-wide actions; then every other stack's.
 * An action the dashboard never does to its own stack (arch-self), or one
 * that does not apply to a stack, is left out for that stack. `open`
 * starts the action's dialog (nothing runs without its confirm), so this
 * module stays free of the DOM.
 * @param {() => import("./actionforms.js").Catalog | null} catalog
 * @param {() => string | null} currentStack the stack whose hub is open
 * @param {(stack: string, action: string) => void} open
 * @returns {Provider}
 */
export const actionCommands = (catalog, currentStack, open) => (ctx) => {
  const c = catalog();
  if (!c) return [];
  const stacks = ctx.fleet?.stacks ?? [];
  const here = currentStack();
  /** @param {{name: string, native?: boolean}} s @returns {Command[]} */
  const forStack = (s) =>
    c.actions
      .filter(
        (a) =>
          a.target === "stack" &&
          !(s.name === c.self_stack && a.refused_for_self) &&
          appliesTo(a.action, s),
      )
      .map((a) => ({
        id: `action:${s.name}:${a.action}`,
        group: "Do",
        label: `${a.label} · ${s.name}`,
        hint: a.what,
        words: actionWords(a.action),
        stack: s.name,
        run: () => open(s.name, a.action),
      }));
  /** @type {Command[]} */
  const out = [];
  const open1 = stacks.find((s) => s.name === here);
  if (open1) out.push(...forStack(open1));
  out.push(
    ...c.actions
      .filter((a) => a.target === "host")
      .map((a) => ({
        id: `action:${c.host_target}:${a.action}`,
        group: "Do",
        label: a.label,
        hint: a.what,
        words: `host ${actionWords(a.action)}`,
        run: () => open(c.host_target, a.action),
      })),
  );
  for (const s of stacks) if (s.name !== here) out.push(...forStack(s));
  return out;
};

/**
 * Milestone edit: a new stack, deploying every change, and each stack's
 * settings (its secrets and firewall live there since 3.71.0), as
 * commands in Do. `openNew` starts the wizard, so this module stays free
 * of the DOM.
 * @param {() => string | null} currentStack the stack whose hub is open
 * @param {() => void} openNew
 * @param {() => void} [openImportBundle] TUI parity: `homelab import`
 * @returns {Provider}
 */
export const editCommands =
  (currentStack, openNew, openImportBundle) => (ctx) => {
    const here = currentStack();
    const names = (ctx.fleet?.stacks ?? []).map((s) => s.name);
    const order =
      here && names.includes(here)
        ? [here, ...names.filter((n) => n !== here)]
        : names;
    /** @type {Command[]} */
    const out = [
      {
        id: "edit:new-stack",
        group: "Do",
        label: "New stack…",
        hint: "from a preset, a bundle or empty, with the wizard",
        words: "add create stack app wizard",
        featured: true,
        run: openNew,
      },
      {
        id: "edit:deploy-all",
        group: "Do",
        label: "Deploy all changes",
        hint: "every stack against the repository: see the plan first",
        words: "apply repository fleet plan",
        featured: true,
        href: "/stacks?deploy-all=1",
      },
      {
        // redesign-flows-1: the one Update flow over every app with a
        // newer version (a stack's own "Update · <stack>" opens it too).
        id: "edit:update-all",
        group: "Do",
        label: "Update apps with a newer version…",
        hint: "see what is newer and what it changes, then back up, update and verify",
        words: "update upgrade newer version stale images all apps",
        href: "/inbox?update=all",
      },
      ...(openImportBundle
        ? [
            {
              id: "edit:import",
              group: "Do",
              label: "Import a stack…",
              hint: "from an export bundle",
              words: "bundle import",
              run: openImportBundle,
            },
          ]
        : []),
    ];
    for (const s of order)
      out.push(
        {
          id: `edit:settings:${s}`,
          group: "Do",
          label: `Edit settings · ${s}`,
          href: stackHref(s, "settings"),
          words: "size network files change",
          stack: s,
        },
        {
          id: `edit:secrets:${s}`,
          group: "Do",
          label: `Change a secret in ${s}`,
          href: `${stackHref(s, "settings")}?section=secrets`,
          words: "secret password token env latch reveal",
          stack: s,
        },
        {
          id: `edit:firewall:${s}`,
          group: "Do",
          label: `Edit firewall · ${s}`,
          href: `${stackHref(s, "settings")}?section=firewall`,
          words: "rules ports",
          stack: s,
        },
      );
    return out;
  };

/**
 * The Inbox's open items, worst first, as the palette's first section.
 * @param {() => import("./inbox.js").InboxItem[]} items
 * @returns {Provider}
 */
export const inboxCommands = (items) => () =>
  items().map((i) => ({
    id: `inbox:${i.key}`,
    group: "Inbox",
    label: i.title,
    hint: i.why,
    words: `inbox ${i.stack ?? ""}`,
    href: i.href,
    stack: i.stack ?? undefined,
  }));
