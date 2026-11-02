// The Stacks list's Live view controls (invariant 39): every control of the
// list, declared once here so the modules that mark elements share them.

import { declare } from "./drivable.js";

export const VIEW = declare({
  id: "stacks-view",
  page: "overview",
  opens: "view",
  row: "cards|table",
  what: "show the stacks as cards or as a table (in the table's own toolbar)",
});
export const GROUP = declare({
  id: "stacks-group",
  page: "overview",
  opens: "view",
  row: "state|none",
  what: "group the table's rows by state (needs attention, running, parked) or not at all (in the table's own toolbar)",
  shows: "in the table view",
  reach: [{ do: "click", control: "stacks-view", row: "table" }],
});
export const SHOW = declare({
  id: "stacks-show",
  page: "overview",
  opens: "view",
  row: "all|attention|parked",
  what: "show every card, only the stacks that need attention, or only the parked ones",
  shows: "in the cards view",
  reach: [{ do: "click", control: "stacks-view", row: "cards" }],
});
export const ASKS = declare({
  id: "stacks-deploy-asks",
  page: "overview",
  opens: "view",
  row: "undo|confirm",
  what: "choose how a batch Deploy asks for safety: ten seconds to undo it, or a dialog that asks first",
  shows: "in the selection bar, while a stack is ticked",
  reach: [{ do: "click", control: "stacks-tick", row: "*" }],
});
export const CLEAR = declare({
  id: "stacks-untick",
  page: "overview",
  opens: "view",
  what: "untick every ticked stack",
  shows: "while a stack is ticked",
  reach: [{ do: "click", control: "stacks-tick", row: "*" }],
});
export const SEARCH = declare({
  id: "stacks-search",
  page: "overview",
  opens: "view",
  what: "the table's search box: find a stack by name, state, backup or any column",
  shows: "in the table view",
  reach: [{ do: "click", control: "stacks-view", row: "table" }],
});
export const TICK = declare({
  id: "stacks-tick",
  page: "overview",
  opens: "view",
  row: "<stack>",
  what: "tick or untick one stack for a batch action",
  selects: true,
});
export const TICK_ALL = declare({
  id: "stacks-tick-all",
  page: "overview",
  opens: "view",
  what: "tick every stack shown (click again to untick them)",
  shows: "in the table view",
  reach: [{ do: "click", control: "stacks-view", row: "table" }],
});
export const BATCH = declare({
  id: "stacks-batch",
  page: "overview",
  opens: "dialog",
  row: "backup|update|deploy",
  what: "act on the ticked stacks: deploy them (ten seconds to undo, or a batch dialog that asks first), back them up in one batch dialog, or update them (the Update flow)",
  shows: "while a stack is ticked",
  reach: [{ do: "click", control: "stacks-tick", row: "*" }],
});
export const OPEN = declare({
  id: "stacks-open",
  page: "overview",
  opens: "view",
  row: "<stack>",
  what: "open one stack's hub (its name in the table or on its card)",
});
export const ROW_ACTION = declare({
  id: "stacks-row-action",
  page: "overview",
  opens: "dialog",
  row: "<stack>",
  what: "a card's Deploy or Back up now: open that stack's action dialog",
  shows: "in the cards view",
  reach: [{ do: "click", control: "stacks-view", row: "cards" }],
});
export const ROW_UPDATE = declare({
  id: "stacks-row-update",
  page: "overview",
  opens: "view",
  row: "<stack>",
  what: "a card's Update: open the Update flow for that stack",
  shows: "in the cards view",
  reach: [{ do: "click", control: "stacks-view", row: "cards" }],
});
export const ROW_LOGS = declare({
  id: "stacks-row-logs",
  page: "overview",
  opens: "view",
  row: "<stack>",
  what: "a card's Logs: open the stack hub's Logs tab",
  shows: "in the cards view",
  reach: [{ do: "click", control: "stacks-view", row: "cards" }],
});
export const SHOW_ALL = declare({
  id: "stacks-show-all",
  page: "overview",
  opens: "view",
  what: "clear the table's search and filters (the empty table's Clear the search and filters)",
  shows: "when the search and the filters match no stack",
});
export const EMPTY_NEW = declare({
  id: "stacks-empty-new",
  page: "overview",
  opens: "dialog",
  what: "the empty fleet's New stack",
  shows: "while the fleet has no stack",
});
