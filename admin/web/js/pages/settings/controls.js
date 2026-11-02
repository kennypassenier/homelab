// The Live view controls of the Settings page: every control it marks, declared
// once (drivable.js) so the catalog and the page agree.

import { declare } from "../../drivable.js";

// fix-239: Live view reaches every control here: host.toml's rows through
// the host-settings form (`homelab ui open host-settings`, `row edit
// <key>`), the rest by `homelab ui click …`.
export const FETCH_NOW = declare({
  id: "fetch-now",
  page: "settings",
  opens: "run",
  what: "fetch the repository's working copy now",
});
export const TRY_AGAIN = declare({
  id: "settings-try-again",
  page: "settings",
  opens: "view",
  what: "read what failed again",
  shows: "after the settings could not be read",
});
export const REPO_CHOICE = declare({
  id: "repo-choice",
  page: "settings",
  opens: "run",
  row: "push|rebase|drop",
  what: "resolve unpushed commits: push them, rebase and push, or drop them",
  shows: "when the working copy holds commits that were not pushed",
});
export const SHOW = declare({
  id: "settings-show",
  page: "settings",
  opens: "view",
  row: "all|changed|here",
  what: "show every host setting, only those changed from the default, or only those editable here",
});
export const FOLD_ALL = declare({
  id: "settings-fold-all",
  page: "settings",
  opens: "view",
  what: "open or close every host.toml group",
});
export const HOW_TO = declare({
  id: "settings-how-to",
  page: "settings",
  opens: "view",
  row: "<key>",
  what: "show how a key the dashboard may not change is changed over ssh",
});
export const DISCARD = declare({
  id: "settings-discard",
  page: "settings",
  opens: "run",
  what: "drop every staged host.toml change (an Undo follows)",
  shows: "while a change is staged on this page",
});
export const CHECK_WRITE = declare({
  id: "settings-check-and-write",
  page: "settings",
  opens: "run",
  what: "write the staged host.toml changes (Save)",
  shows: "while a change is staged on this page",
});
export const SEARCH = declare({
  id: "settings-search",
  page: "settings",
  opens: "view",
  what: "the search over every setting (name, key or what it does)",
});
export const NAV = declare({
  id: "settings-nav",
  page: "settings",
  opens: "view",
  row: "wc|signin|browser|<group slug>",
  what: "jump to a section from the side list",
});
export const GROUP = declare({
  id: "settings-group",
  page: "settings",
  opens: "view",
  row: "<group slug>",
  what: "open or close one host.toml group",
});
export const EDIT_KEY = declare({
  id: "settings-edit",
  page: "settings",
  opens: "view",
  row: "<key>",
  what: "go to one host.toml key and put the cursor in its editor",
  // The editors stand in their rows now: staging is typing, and a key goes
  // back to what it was from the Save bar's list.
  was: ["settings-stage", "settings-use-default", "settings-cancel-edit"],
});
export const STAGED = declare({
  id: "settings-staged",
  page: "settings",
  opens: "view",
  what: "the Save bar's count of staged changes; it lists them, each with its own Revert",
  was: ["settings-undo"],
});
export const CLEAR = declare({
  id: "settings-clear-search",
  page: "settings",
  opens: "view",
  what: "clear the search and show every setting",
  shows: "when the search matches no setting",
});
