// redesign-integrate-8: one control's catalog entry as the Live view sweep
// pressed it — what it is, where it lives and how it is reached; its words
// (`what`) left out, a reworded description does not move a control. The
// sweep (invariants.e2e.js) stamps these into sweep-stamp.json when it
// passes; admin/web/test/drivecatalog.test.js refuses a catalog control
// whose key is not stamped.

/** @param {any} c a catalog control (js/drivecatalog.json `controls`) */
export const sweepKey = (c) =>
  JSON.stringify([
    c.id,
    c.page,
    c.opens,
    c.row ?? null,
    c.href ?? null,
    c.was ?? [],
    c.shows ?? null,
    c.reach ?? [],
    !!c.twins,
  ]);
