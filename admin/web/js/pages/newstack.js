// ux-stacknew-2 (Kenny, 2026-11-01: "Keuze in paneel, wizard op pagina"):
// New stack's wizard on a page of its own, like Restore. The Stacks drawer
// keeps the three-way choice; each choice is an address of its own that the
// browser's Back button leaves again:
//   /stacks/new/preset[?preset=<name>]   from one of the repository's presets
//   /stacks/new/bundle                   from an exported bundle
//   /stacks/new/empty                    a blank stack, apps added later
// The three share the review (newreview.js) and the wizard's frame
// (newwizard.js); only their first steps differ.

import { ensureStyle, h } from "../dom.js";
import { mountBundleWizard } from "../importstack.js";
import { mountPresetWizard } from "../newstack.js";
import { pageHeader } from "../ui.js";

/** @typedef {"preset" | "bundle" | "empty"} Kind */

/** @type {Record<Kind, {title: string, desc: string}>} */
export const KINDS = {
  preset: {
    title: "New stack from a preset",
    desc: "From one of the repository's presets; committed under stacks/<name>/ and pushed, then deployed if you choose.",
  },
  bundle: {
    title: "New stack from a bundle",
    desc: "A bundle an export wrote (Export bundle on a stack's page, or homelab export) becomes a new stack: its name, number, address and /appdata paths are replaced by the new ones. Secrets are never in a bundle: add them through latch before the first deploy.",
  },
  empty: {
    title: "New empty stack",
    desc: "A stack with no apps yet: its container and address, committed under stacks/<name>/ and pushed, then deployed if you choose. Add its first app from its hub.",
  },
};

/** @param {string} pathname @returns {Kind} */
export const kindOf = (pathname) =>
  pathname.replace(/\/+$/, "").endsWith("/bundle")
    ? "bundle"
    : pathname.replace(/\/+$/, "").endsWith("/empty")
      ? "empty"
      : "preset";

/**
 * @param {HTMLElement} root
 * @param {{navigate: (href: string) => void}} ctx
 * @returns {() => void}
 */
export function mount(root, ctx) {
  ensureStyle("/css/pages/newstack.css");
  root.classList.add("ns-page");
  const kind = kindOf(location.pathname);
  const head = pageHeader({ title: KINDS[kind].title, desc: KINDS[kind].desc });
  const holder = h("div", { class: "ns-holder", "aria-busy": "true" });
  root.replaceChildren(head.el, holder);
  let stopped = false;
  /** @type {() => void} */
  let stop = () => {};
  const ready = (/** @type {() => void} */ s) => {
    holder.removeAttribute("aria-busy");
    if (stopped) s();
    else stop = s;
  };
  if (kind === "bundle")
    ready(mountBundleWizard(holder, { navigate: ctx.navigate }));
  else
    void mountPresetWizard(holder, {
      navigate: ctx.navigate,
      empty: kind === "empty",
      preset: new URLSearchParams(location.search).get("preset"),
    }).then(ready);
  return () => {
    stopped = true;
    stop();
    root.classList.remove("ns-page");
  };
}
