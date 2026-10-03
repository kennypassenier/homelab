// redesign-final-h4 (3.71.0's final review, 2026-10-04): the System
// landing as the approved flows/system.html draws it — four headed groups
// (The host · The fleet as a whole · Set up · Tools), the host's live line
// on the Host card and the disaster runbook among the tools — instead of
// one flat grid of nine cards. Pure, so `node --test` holds the groups and
// the live line; system.js draws them.

/**
 * @typedef {{id: string, label: string, what: string, href?: string,
 *   download?: string}} SystemCard
 * `id`: a page of areas.js `SUB_PAGES` (its address), or "runbook".
 * @typedef {{title: string, desc: string, cards: SystemCard[]}} SystemGroup
 */

/** @type {SystemGroup[]} the demo's groups, cards and words, in order */
export const SYSTEM_GROUPS = [
  {
    title: "The host",
    desc: "The Proxmox machine every stack runs on.",
    cards: [
      {
        id: "host",
        label: "Host",
        what: "Load, disk, the biggest folders, and host-wide actions: update the host, patch every container, restart",
      },
      {
        id: "metrics",
        label: "Metrics",
        what: "CPU, memory, disk and traffic over time, per stack, with deploys and backups marked on the charts",
      },
    ],
  },
  {
    title: "The fleet as a whole",
    desc: "How the stacks fit together.",
    cards: [
      {
        id: "fleetview",
        label: "Map",
        what: "Which stack talks to which, how much room each takes, and how fast disks grow (was “Fleet view”)",
      },
      {
        id: "firewall",
        label: "Firewall",
        what: "Every rule of every stack in one table; one stack's rules are also in its hub",
      },
    ],
  },
  {
    title: "Set up",
    desc: "Settings you choose once and change rarely.",
    cards: [
      {
        id: "settings",
        label: "Host settings",
        what: "host.toml: backup targets, retention, thresholds, maintenance window",
      },
      {
        id: "presets",
        label: "Presets",
        what: "Ready-made stacks the New stack wizard starts from",
      },
      {
        id: "notifications",
        label: "Notification rules",
        what: "When the dashboard pushes to your phone, the daily digest, muted stacks",
      },
      {
        id: "sign-in",
        label: "Sign-in",
        what: "Passkeys and machine tokens that can sign in to this dashboard (the Sign-in section of Host settings)",
      },
    ],
  },
  {
    title: "Tools",
    desc: "For when you need to go under the hood.",
    cards: [
      {
        id: "shell",
        label: "Console",
        what: "A shell on the host or in one container, and “run a command”",
      },
      {
        id: "runbook",
        label: "Disaster runbook",
        what: "Download the step-by-step to rebuild everything from backups",
        href: "/data/download/runbook",
        download: "DR_RUNBOOK.md",
      },
    ],
  },
];

/**
 * The header's state and the Host card's live line: "CPU 7% · disk 31% ·
 * 3.63.0 (3.64.0 available)".
 * @param {{host?: {cpu_pct?: number | null, disk_pct?: number | null}} | null | undefined} fleet
 * @param {{host?: string | null, latest?: string | null,
 *   update_available?: boolean} | null | undefined} versions
 * @param {boolean} failed the fleet read failed
 * @returns {{tone: "ok" | "bad" | "", word: string, line: string}}
 */
export function hostLine(fleet, versions, failed) {
  const h = fleet?.host;
  const parts = [];
  if (h?.cpu_pct != null) parts.push(`CPU ${Math.round(h.cpu_pct)}%`);
  if (h?.disk_pct != null) parts.push(`disk ${Math.round(h.disk_pct)}%`);
  if (versions?.host)
    parts.push(
      versions.update_available && versions.latest
        ? `${versions.host} (${versions.latest} available)`
        : versions.host,
    );
  if (failed) return { tone: "bad", word: "host not answering", line: "" };
  if (!fleet) return { tone: "", word: "reading the host…", line: "" };
  return { tone: "ok", word: "host healthy", line: parts.join(" · ") };
}
