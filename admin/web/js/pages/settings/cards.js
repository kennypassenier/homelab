// The Settings page's cards: the generic data card and the four that read
// nothing from the host themselves (This browser, Account, Sign-in, Home
// address).

import { h } from "../../dom.js";
import { mountAccount, mountThisBrowser } from "../../thisbrowser.js";
import { mountSignin } from "../../signin.js";
import { mountHomeAddress } from "../../homeaddress.js";
import { chip } from "../../ui.js";

/**
 * A card with data: a heading with its one sentence, the tools, and the
 * stage the package fills with loading, empty and failed (`data-kp-state`).
 * A card that reads nothing (`state: "static"`, This browser) has no state to
 * draw, so it carries none.
 * @param {{id: string, title: string, desc: string, tools?: Node[],
 *   data: Node[], state?: "loading" | "ready" | "static"}} spec
 */
export function dataCard(spec) {
  const data = h("div", { class: "kp-card__data" }, ...spec.data);
  const card = h(
    "section",
    {
      class: "kp-card nx-card cf-card",
      id: spec.id,
      ...(spec.state === "static"
        ? {}
        : { "data-kp-state": spec.state ?? "loading" }),
      "aria-labelledby": `${spec.id}-h`,
    },
    h(
      "div",
      { class: "kp-card__header" },
      h("h2", { class: "kp-card__title", id: `${spec.id}-h` }, spec.title),
      spec.tools?.length
        ? h("div", { class: "kp-card__actions" }, ...spec.tools)
        : null,
    ),
    h("p", { class: "kp-lede" }, spec.desc),
    h("div", { class: "kp-card__stage" }, data),
  );
  return { card, data };
}

/**
 * This browser, Account, Sign-in and Home address: each mounts its own
 * content and registers its cleanup.
 * @param {import("./ctx.js").Ctx} ctx
 */
export function buildStaticCards(ctx) {
  const browserCard = dataCard({
    id: "browser",
    title: "This browser",
    desc: "Choices that belong to this device and never reach the host: the theme, what Live view opens in, the tour, and a way to forget it all. Each choice applies the moment you make it, with an Undo.",
    tools: [chip("Saves at once")],
    data: [],
    state: "static",
  });
  ctx.stops.push(mountThisBrowser(browserCard.data));

  const accountCard = dataCard({
    id: "account",
    title: "Account",
    desc: "Choices that follow you to every device you sign in on: calm mode. A choice applies the moment you make it, with an Undo.",
    tools: [chip("Saves at once")],
    data: [],
    state: "static",
  });
  ctx.stops.push(mountAccount(accountCard.data));

  const signinBody = h("div", { class: "si-grid" });
  const signin = dataCard({
    id: "signin",
    title: "Sign-in",
    desc: "Who can reach this dashboard and the host: your passkeys for the browser, one token per machine for the CLI and TUI.",
    data: [signinBody],
    state: "static",
  });
  ctx.stops.push(
    mountSignin(signinBody, (what, e, retry) => ctx.failed(what, e, retry)),
  );

  const homeBody = h("div", { class: "ha-body" });
  const home = dataCard({
    id: "home-address",
    title: "Home address",
    desc: "The house has no fixed public address. Cloudflare lets it in without a login by that address; when the provider changes it, update it here.",
    data: [homeBody],
    state: "static",
  });
  ctx.stops.push(
    mountHomeAddress(homeBody, (what, e, retry) => ctx.failed(what, e, retry)),
  );
  return { browserCard, accountCard, signin, home };
}
