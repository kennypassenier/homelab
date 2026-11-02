// Drawing the working copy's rows and alerts from one read of /data/repo.

import { agoEl } from "../../ago.js";
import { h } from "../../dom.js";
import { repoState, shortRemote } from "../../settingsview.js";
import { chip, dot } from "../../ui.js";
import { resolveButton } from "./repoactions.js";

/** @typedef {import("./ctx.js").CommitRef} CommitRef */
/**
 * @typedef {{repo: {present: boolean, remote: string, branch: string,
 *   head: CommitRef | null, unpushed: CommitRef[], behind: number,
 *   dirty: string[], key_present: boolean | null, fetched_at: number | null,
 *   commits_via: "dashboard" | "host",
 *   error: string | null}, deployed_unpushed: {stack: string, commit: string}[]}} RepoView
 */

/** @param {{label: string, value: Node | string, tone?: string}[]} rows */
const repoLines = (rows) =>
  rows.map((r) =>
    h(
      "div",
      { class: "kp-formgrid__row" },
      h("div", { class: "kp-formgrid__label" }, h("span", null, r.label)),
      h(
        "div",
        { class: "kp-formgrid__control", "data-kp-formgrid-span": "wide" },
        r.value,
      ),
    ),
  );

/**
 * The warnings and the unpushed-commits alert.
 * @param {RepoView} v @param {() => Promise<void>} reload
 * @returns {Node[]}
 */
function repoAlerts(v, reload) {
  const r = v.repo;
  /** @type {Node[]} */
  const alerts = [];
  if (r.error)
    alerts.push(
      h(
        "div",
        {
          class: "kp-alert kp-alert--warning",
          role: "status",
          "data-kp-formgrid-span": "all",
        },
        `The last fetch or clone failed: ${r.error}`,
      ),
    );
  if (r.dirty.length)
    alerts.push(
      h(
        "div",
        {
          class: "kp-alert kp-alert--destructive",
          role: "alert",
          "data-kp-formgrid-span": "all",
        },
        `Files differ from the last commit: ${r.dirty.join(", ")}. Edits are refused until that is looked at by hand.`,
      ),
    );
  if (r.unpushed.length) {
    const deployed = v.deployed_unpushed.map((d) => d.stack);
    alerts.push(
      h(
        "div",
        {
          class: "kp-alert kp-alert--warning",
          role: "status",
          "data-kp-formgrid-span": "all",
        },
        h(
          "div",
          { class: "kp-alert__body" },
          h(
            "strong",
            null,
            `${r.unpushed.length} commit(s) here are not on the remote. `,
          ),
          deployed.length
            ? `The host runs ${deployed.join(", ")} from one of them: push keeps the repository and the fleet in step.`
            : "Push them, rebase them onto the remote, or drop them.",
          h(
            "ul",
            null,
            r.unpushed.map((c) =>
              h(
                "li",
                { class: "cf-mono" },
                `${c.commit.slice(0, 10)} · ${c.subject}`,
              ),
            ),
          ),
          h(
            "div",
            { class: "kp-formgrid__buttons" },
            /** @type {const} */ (["push", "rebase", "drop"]).map((choice) =>
              resolveButton(choice, reload),
            ),
          ),
        ),
      ),
    );
  }
  return alerts;
}

/** The "At" row: head commit, branch and remote. @param {RepoView["repo"]} r */
function atValue(r) {
  return h(
    "span",
    { class: "st-wc__at" },
    h(
      "strong",
      null,
      r.head ? r.head.subject : r.present ? "—" : "not cloned yet",
    ),
    r.head
      ? h(
          "span",
          { class: "cf-chip cf-chip--mono", title: r.head.commit },
          r.head.commit.slice(0, 10),
        )
      : null,
    chip(r.branch),
    h(
      "span",
      { class: "cf-mono cf-muted st-remote", title: r.remote },
      shortRemote(r.remote),
    ),
  );
}

/** The "State" row: tone and the counts. @param {RepoView["repo"]} r */
function stateValue(r) {
  const st = repoState(r);
  return h(
    "span",
    { class: "st-wc__chips" },
    dot(st.tone, st.word),
    h(
      "span",
      {
        class: "cf-chip",
        title: "Commits on the remote the host has not fetched",
      },
      `${r.behind} behind`,
    ),
    h(
      "span",
      {
        class: "cf-chip",
        title: "Commits here the remote lacks: push, rebase or drop them",
      },
      `${r.unpushed.length} unpushed`,
    ),
    h(
      "span",
      { class: "cf-chip", title: "Files changed but not committed" },
      `${r.dirty.length} uncommitted`,
    ),
    r.commits_via === "host"
      ? h(
          "span",
          {
            class: "cf-chip",
            title:
              "The host commits and pushes every edit with its own deploy key; this copy only reads the remote",
          },
          "pushed by the host",
        )
      : null,
    r.key_present === null
      ? null
      : h(
          "span",
          {
            class: `cf-chip${r.key_present ? "" : " cf-chip--warn"}`,
            title: "The deploy key the dashboard pushes with",
          },
          r.key_present ? "deploy key present" : "deploy key missing",
        ),
  );
}

/**
 * The rows and alerts of the working copy card.
 * @param {RepoView} v @param {HTMLElement} fetchNow
 * @param {() => Promise<void>} reload
 * @returns {Node[]}
 */
export function repoNodes(v, fetchNow, reload) {
  const r = v.repo;
  const fetched = agoEl("Last fetched", r.fetched_at ?? null);
  return [
    ...repoLines([
      { label: "At", value: atValue(r) },
      { label: "State", value: stateValue(r) },
    ]),
    h(
      "div",
      { class: "kp-formgrid__row" },
      h(
        "div",
        { class: "kp-formgrid__label" },
        h("span", null, "Remote"),
        h("small", null, r.fetched_at ? fetched : "Not fetched yet"),
      ),
      h(
        "div",
        {
          class: "kp-formgrid__control st-versions",
          "data-kp-formgrid-span": "wide",
        },
        fetchNow,
        h(
          "small",
          { class: "cf-hint" },
          "Push, rebase or drop appear here when the remote lacks commits.",
        ),
      ),
    ),
    ...repoAlerts(v, reload),
  ];
}
