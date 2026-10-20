// feat-kpadopt-2 (kp-themes 11.1.1): the filter column of a log explorer is
// kp's facet list (`facets.js`): checkbox rows with a count and "only".
// The host log and the stack hub's Logs tab draw their Source / Apps and
// Level groups with this one builder.

import { h } from "./dom.js";

/**
 * One facet group, busy (kp's own loading look) until its first counts are
 * written by `setFacetCounts`.
 * @param {string} label what the group filters, as its group name
 * @param {string} all the words of the "everything" row
 * @param {string} none what an empty group says
 * @returns {HTMLElement}
 */
export const facetGroup = (label, all, none) =>
  h(
    "div",
    {
      class: "kp-facets hl-facets",
      "data-kp-facets": "",
      "data-kp-facets-all": all,
      role: "group",
      "aria-label": label,
      "aria-busy": "true",
    },
    h("ul", { class: "kp-facets__list" }),
    h("p", { class: "kp-facets__empty", hidden: "" }, none),
  );
