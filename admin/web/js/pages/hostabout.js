// The Host page's About card (what the machine is) and Templates card (the
// golden templates a new stack is cloned from).

import { openAction } from "../actiondialog.js";
import { fetchJson, h } from "../dom.js";
import { viaForm } from "../drivable.js";
import { daemonSignature, gib, hostUptime, versionText } from "../host.js";
import { current } from "../store.js";
import { dot, section } from "../ui.js";

export function createAbout() {
  const about = h("dl", { class: "kp-kv", id: "host-about" });
  const aboutCard = section({
    id: "host-about-card",
    title: "About this host",
    desc: "What the machine is.",
  });
  aboutCard.body.append(about);
  const paintAbout = () => {
    const s = current();
    const f = s.fleet;
    if (!f) return;
    const d = f.host.disk_detail;
    const sig = daemonSignature(f.host);
    const up = hostUptime(f.host);
    /** @type {[string, Node | string][]} */
    const rows = [
      ["Name", f.host.name],
      [
        "Daemon",
        h(
          "span",
          { id: "host-daemon-signature", title: sig.title },
          `${versionText(s.hostVersion, s.hostBuild)} `,
          h(
            "span",
            { class: sig.tone === "ok" ? null : "hk-muted" },
            `(${sig.text})`,
          ),
        ),
      ],
      [
        "Hardware",
        `${f.host.cores_total ? `${f.host.cores_total} cores · ` : ""}${gib(f.host.ram_total_mb).replace(/\.0$/, "")} GiB RAM`,
      ],
      [
        "Root disk",
        d
          ? `${d.root_disk_device || "unknown"} · ${d.root_disk_total_gb.toFixed(0)} GB`
          : "not read yet",
      ],
      [
        "Up for",
        up ??
          h("span", { class: "hk-muted" }, "not reported by this host version"),
      ],
    ];
    about.replaceChildren(
      ...rows.map(([k, v]) =>
        h("div", { class: "kp-kv__row" }, h("dt", null, k), h("dd", null, v)),
      ),
    );
  };
  return { card: aboutCard, paint: paintAbout };
}

/** @param {import("./hostenv.js").HostEnv} env */
export function createTemplates(env) {
  const { abort } = env;
  const tplList = h(
    "ul",
    { class: "hk-tpl", id: "host-templates" },
    h(
      "li",
      { role: "status" },
      h("span", { class: "hk-sk kp-skeleton" }),
      "Asking the host (pveam and the template containers)…",
    ),
  );
  const tplBuild = viaForm(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm",
        title: "Bake a fresh golden template: opens the dialog first",
        onclick: () => void openAction("_host", "template-build"),
      },
      "Build a template",
    ),
    "template-build",
  );
  const tplCard = section({
    id: "host-templates-card",
    title: "Templates",
    desc: "The golden templates a new stack is cloned from.",
    foot: [tplBuild, "read with the page"],
  });
  tplCard.body.append(tplList);
  const loadTemplates = async () => {
    const r = await fetchJson("/data/templates", "the templates", abort.signal);
    if (!r.ok) {
      tplList.replaceChildren(
        h(
          "li",
          { class: "kp-alert kp-alert--destructive error", role: "alert" },
          `Could not read ${r.error.what}: ${r.error.why}`,
        ),
      );
      return;
    }
    const t = r.body.templates;
    tplList.replaceChildren(
      ...t.clones.map((/** @type {[number, string]} */ c) =>
        h(
          "li",
          null,
          dot("ok"),
          h(
            "span",
            null,
            h("span", { class: "mono" }, `CT ${c[0]}`),
            ` · ${c[1]}`,
          ),
        ),
      ),
      ...t.os.map((/** @type {string} */ o) =>
        h(
          "li",
          null,
          dot(""),
          h("span", { class: "mono" }, o.replace("local:vztmpl/", "")),
        ),
      ),
      ...(t.clones.length
        ? []
        : [
            h(
              "li",
              { class: "hk-muted" },
              "No golden template yet: build one.",
            ),
          ]),
    );
  };
  return { card: tplCard, load: loadTemplates };
}
